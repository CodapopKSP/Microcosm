//! The field notebook: species the viewer has recorded, with the specimens they sketched.
//!
//! It's saved with the world but never affects the simulation.

use serde::{Deserialize, Serialize};

use crate::World;
use crate::genome::Genome;

/// Most specimens sketched in one entry (more don't fit on its half page).
pub const MAX_SPECIMENS: usize = 4;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Notebook {
    /// In the order they were first recorded.
    pub entries: Vec<Entry>,
}

/// One species' entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub species: u32,
    /// The species' name when it was recorded.
    pub name: String,
    /// Sim seconds when it was first recorded.
    pub time: f64,
    /// Oldest first.
    pub specimens: Vec<Specimen>,
}

/// One recorded organism.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Specimen {
    pub organism: u64,
    pub genome: Genome,
    /// Sim seconds when it was recorded.
    pub time: f64,
    pub generation: u32,
    /// Its radius then (it may not have been fully grown).
    pub radius: f32,
}

/// What recording an organism did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recorded {
    /// A new entry for a species not in the notebook yet.
    NewEntry,
    /// Another specimen added to an existing entry.
    Specimen,
    /// This organism is already in the notebook.
    AlreadyRecorded,
    /// Its species' entry already has `MAX_SPECIMENS` sketches.
    Full,
    /// No such organism (it died).
    Missing,
}

impl Notebook {
    /// Whether this organism has been recorded.
    pub fn has_organism(&self, id: u64) -> bool {
        self.entries
            .iter()
            .any(|e| e.specimens.iter().any(|s| s.organism == id))
    }

    /// The entry for a species, if it has one.
    pub fn entry(&self, species: u32) -> Option<&Entry> {
        self.entries.iter().find(|e| e.species == species)
    }

    /// Tear out a species' entry.
    pub fn remove(&mut self, species: u32) {
        self.entries.retain(|e| e.species != species);
    }
}

impl World {
    /// The species an organism's notebook entry is for: its named species (a variant goes
    /// under the named species it came from).
    fn notebook_species(&self, species: u32) -> u32 {
        self.species.established_ancestor(species).unwrap_or(species)
    }

    /// The notebook entry organism `id` is in, or else its species' entry.
    pub fn notebook_entry(&self, id: u64) -> Option<usize> {
        let entries = &self.notebook.entries;
        entries
            .iter()
            .position(|e| e.specimens.iter().any(|s| s.organism == id))
            .or_else(|| {
                let species = self.notebook_species(self.find(id)?.species);
                entries.iter().position(|e| e.species == species)
            })
    }

    /// Whether recording organism `id` would sketch it: it's alive, not in the notebook yet,
    /// and its species' entry has room.
    pub fn can_record(&self, id: u64) -> bool {
        let Some(o) = self.find(id) else {
            return false;
        };
        !self.notebook.has_organism(id)
            && self
                .notebook
                .entry(self.notebook_species(o.species))
                .is_none_or(|e| e.specimens.len() < MAX_SPECIMENS)
    }

    /// Record organism `id` in the field notebook, under its named species (a variant goes
    /// under the named species it came from).
    pub fn record_specimen(&mut self, id: u64) -> Recorded {
        let Some(o) = self.find(id) else {
            return Recorded::Missing;
        };
        if self.notebook.has_organism(id) {
            return Recorded::AlreadyRecorded;
        }
        let species = self.notebook_species(o.species);
        let time = self.time();
        let specimen = Specimen {
            organism: id,
            genome: o.genome,
            time,
            generation: o.generation,
            radius: o.radius,
        };
        if let Some(entry) = self.notebook.entries.iter_mut().find(|e| e.species == species) {
            if entry.specimens.len() >= MAX_SPECIMENS {
                return Recorded::Full;
            }
            entry.specimens.push(specimen);
            return Recorded::Specimen;
        }
        let name = self.species.name(species).to_string();
        self.notebook.entries.push(Entry {
            species,
            name,
            time,
            specimens: vec![specimen],
        });
        Recorded::NewEntry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;

    fn world() -> World {
        let mut config = Config::default();
        config.population.initial = 12;
        config.events.enabled = false;
        World::new(config)
    }

    #[test]
    fn recording_adds_entries_and_specimens() {
        let mut w = world();
        let ids: Vec<u64> = w.organisms.iter().map(|o| o.id).collect();
        assert_eq!(w.record_specimen(ids[0]), Recorded::NewEntry);
        assert_eq!(w.record_specimen(ids[0]), Recorded::AlreadyRecorded);
        // The founders are all one species: more of them become more sketches in that entry.
        assert_eq!(w.record_specimen(ids[1]), Recorded::Specimen);
        assert_eq!(w.notebook.entries.len(), 1);
        assert_eq!(w.notebook.entries[0].specimens.len(), 2);
        assert_eq!(w.notebook.entries[0].name, w.species.name(0));
        assert!(w.notebook.has_organism(ids[1]));
        assert_eq!(w.record_specimen(u64::MAX), Recorded::Missing);
    }

    #[test]
    fn a_full_entry_takes_no_more() {
        let mut w = world();
        let ids: Vec<u64> = w.organisms.iter().map(|o| o.id).collect();
        for &id in ids.iter().take(MAX_SPECIMENS) {
            assert!(w.can_record(id));
            w.record_specimen(id);
        }
        let extra = ids[MAX_SPECIMENS];
        assert!(!w.can_record(extra));
        assert_eq!(w.record_specimen(extra), Recorded::Full);
        let specimens = &w.notebook.entries[0].specimens;
        assert_eq!(specimens.len(), MAX_SPECIMENS);
        assert_eq!(specimens.last().unwrap().organism, ids[MAX_SPECIMENS - 1]);
        // It still leads to its species' entry.
        assert_eq!(w.notebook_entry(extra), Some(0));
        w.notebook.remove(0);
        assert!(w.notebook.entries.is_empty());
        assert!(w.can_record(extra));
    }

    #[test]
    fn the_notebook_survives_a_save() {
        let mut w = world();
        let id = w.organisms[0].id;
        w.record_specimen(id);
        let dir = std::env::temp_dir().join(format!("evo_notebook_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("nb.bin");
        w.save(&path).unwrap();
        let back = World::load(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(back.notebook.entries.len(), 1);
        assert_eq!(back.notebook.entries[0].specimens[0].organism, id);
    }
}

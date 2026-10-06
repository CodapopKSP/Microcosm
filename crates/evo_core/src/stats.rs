//! Time series statistics and the event log.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::organism::DeathCause;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StatSample {
    pub tick: u64,
    pub time: f64,
    pub population: u32,
    /// Established species alive right now.
    pub species: u32,
    pub biomass: f32,
    /// Organisms by main diet: photo, chemo, scavenge, hunt.
    pub diets: [u32; 4],
    pub motile: u32,
    pub mean_radius: f32,
    pub max_radius: f32,
    pub births: u32,
    pub deaths: u32,
    pub sun: f32,
    pub surface_temp: f32,
    /// Air: oxygen and CO₂ (1.0 = today).
    pub air_oxygen: f32,
    pub air_co2: f32,
    /// Mean oxygen and nutrients in the sunlit top of the ocean, and the share of water
    /// that is nearly oxygen-free (dead zones).
    pub surface_oxygen: f32,
    pub surface_nutrients: f32,
    pub anoxic_fraction: f32,
    /// Organisms with at least one body part, and the mean number of parts.
    pub with_parts: u32,
    pub mean_parts: f32,
    /// (species id, population) for established living species.
    pub species_populations: Vec<(u32, u32)>,
    /// Organisms by depth: surface (< 300), sunlit (300–900), twilight (900–2500), deep.
    #[serde(default)]
    pub depth_bands: [u32; 4],
    /// Deaths since the last sample by cause (in `DeathCause::ALL` order).
    #[serde(default)]
    pub death_causes: [u32; 10],
    /// Organisms carrying each kind of body part (in `PartKind::ALL` order).
    #[serde(default)]
    pub part_carriers: [u32; 15],
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Stats {
    pub samples: Vec<StatSample>,
    pub total_births: u64,
    pub total_deaths: u64,
    pub deaths_by_cause: [u64; 10],
    pub births_since_sample: u32,
    pub deaths_since_sample: u32,
    /// Deaths since the last sample by cause.
    pub causes_since_sample: [u32; 10],
    pub since_sample: f32,
}

impl Stats {
    pub fn record_death(&mut self, cause: DeathCause) {
        self.total_deaths += 1;
        self.deaths_since_sample += 1;
        let i = DeathCause::ALL.iter().position(|&c| c == cause).unwrap();
        self.deaths_by_cause[i] += 1;
        self.causes_since_sample[i] += 1;
    }

    pub fn record_birth(&mut self) {
        self.total_births += 1;
        self.births_since_sample += 1;
    }

    /// Push a sample, thinning older history (every other sample) when over `max`.
    pub fn push(&mut self, sample: StatSample, max: usize) {
        self.samples.push(sample);
        if self.samples.len() > max.max(16) {
            let mut keep = true;
            self.samples.retain(|_| {
                keep = !keep;
                keep
            });
        }
        self.births_since_sample = 0;
        self.deaths_since_sample = 0;
        self.since_sample = 0.0;
    }

    pub fn latest(&self) -> Option<&StatSample> {
        self.samples.last()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    Species,
    Extinction,
    Environment,
    Recolonization,
    Disaster,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub tick: u64,
    pub time: f64,
    pub kind: EventKind,
    pub text: String,
    /// Organism or species the event is about, if any (for "follow" links).
    pub organism: Option<u64>,
    pub species: Option<u32>,
    /// Where it happened, if it has a place (vents, walls, impacts).
    #[serde(default)]
    pub pos: Option<glam::Vec2>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(from = "SavedEventLog")]
pub struct EventLog {
    pub entries: VecDeque<Event>,
    /// Total events ever logged (entries may be trimmed).
    pub count: u64,
    /// Every event that changed the world (environment, disasters, recolonizations), kept for
    /// the whole run: they're rare, and the family tree marks them.
    pub world: Vec<Event>,
}

/// An event log as saved. Events of kinds that no longer exist (older saves had milestones)
/// are dropped when loading.
#[derive(Default, Deserialize)]
#[serde(default)]
struct SavedEventLog {
    entries: VecDeque<SavedEvent>,
    count: u64,
    world: Vec<SavedEvent>,
}

#[derive(Deserialize)]
struct SavedEvent {
    tick: u64,
    time: f64,
    #[serde(deserialize_with = "known_kind")]
    kind: Option<EventKind>,
    text: String,
    organism: Option<u64>,
    species: Option<u32>,
    #[serde(default)]
    pos: Option<glam::Vec2>,
}

fn known_kind<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<EventKind>, D::Error> {
    use serde::de::IntoDeserializer;
    let name = String::deserialize(d)?;
    let kind: Result<EventKind, serde::de::value::Error> =
        EventKind::deserialize(name.as_str().into_deserializer());
    Ok(kind.ok())
}

impl From<SavedEventLog> for EventLog {
    fn from(saved: SavedEventLog) -> Self {
        let known = |e: SavedEvent| {
            Some(Event {
                tick: e.tick,
                time: e.time,
                kind: e.kind?,
                text: e.text,
                organism: e.organism,
                species: e.species,
                pos: e.pos,
            })
        };
        let total = saved.entries.len();
        let entries: VecDeque<Event> = saved.entries.into_iter().filter_map(known).collect();
        let dropped = (total - entries.len()) as u64;
        EventLog {
            entries,
            count: saved.count.saturating_sub(dropped),
            world: saved.world.into_iter().filter_map(known).collect(),
        }
    }
}

/// Most world events kept (a cap far beyond weeks of running).
const MAX_WORLD_EVENTS: usize = 50_000;

impl EventLog {
    pub fn push(&mut self, event: Event, max: usize) {
        if matches!(
            event.kind,
            EventKind::Environment | EventKind::Disaster | EventKind::Recolonization
        ) && self.world.len() < MAX_WORLD_EVENTS
        {
            self.world.push(event.clone());
        }
        self.entries.push_back(event);
        self.count += 1;
        while self.entries.len() > max.max(1) {
            self.entries.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_of_removed_kinds_are_dropped_on_load() {
        #[derive(Serialize)]
        struct OldEvent {
            tick: u64,
            time: f64,
            kind: &'static str,
            text: String,
            organism: Option<u64>,
            species: Option<u32>,
        }
        #[derive(Serialize)]
        struct OldLog {
            entries: Vec<OldEvent>,
            count: u64,
            world: Vec<OldEvent>,
        }
        let event = |kind| OldEvent {
            tick: 1,
            time: 0.5,
            kind,
            text: kind.into(),
            organism: None,
            species: Some(3),
        };
        let old = OldLog {
            entries: vec![event("Milestone"), event("Species"), event("Environment")],
            count: 10,
            world: vec![event("Environment")],
        };
        let bytes = rmp_serde::to_vec_named(&old).unwrap();
        let log: EventLog = rmp_serde::from_slice(&bytes).unwrap();
        let kinds: Vec<EventKind> = log.entries.iter().map(|e| e.kind).collect();
        assert_eq!(kinds, [EventKind::Species, EventKind::Environment]);
        assert_eq!(log.count, 9);
        assert_eq!(log.world.len(), 1);
        // And today's logs round-trip.
        let again: EventLog = rmp_serde::from_slice(&rmp_serde::to_vec_named(&log).unwrap()).unwrap();
        assert_eq!(again.entries.len(), 2);
        assert_eq!(again.count, 9);
    }
}

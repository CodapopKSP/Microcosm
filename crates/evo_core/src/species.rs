//! Species and genera: founding, naming, population tracking, phylogeny.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::genome::{Genome, drive, gene};
use crate::parts::{self, PartKind};
use crate::rng::{Rng, mix64};
use crate::traits::Traits;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Species {
    pub id: u32,
    pub name: String,
    pub genus: u32,
    pub parent: Option<u32>,
    pub founder: Genome,
    pub founded: u64,
    pub extinct: Option<u64>,
    pub population: u32,
    pub peak: u32,
    /// Named and listed once it reaches `species.established` members.
    pub established: bool,
    pub established_at: Option<u64>,
    pub color: [f32; 3],
    pub total_born: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Genus {
    pub id: u32,
    pub name: String,
    pub founder: Genome,
}

/// What happened to species this tick, for the event log.
#[derive(Clone, Debug)]
pub enum SpeciesEvent {
    Established(u32),
    Extinct(u32),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SpeciesRegistry {
    pub species: BTreeMap<u32, Species>,
    pub genera: BTreeMap<u32, Genus>,
    next_species: u32,
    next_genus: u32,
    #[serde(skip)]
    pub events: Vec<SpeciesEvent>,
}

const COLOR_PREFIX: [(&str, f32); 8] = [
    ("Rubro", 0.0),
    ("Auranti", 30.0),
    ("Xantho", 55.0),
    ("Chloro", 110.0),
    ("Thalasso", 165.0),
    ("Cyano", 200.0),
    ("Ianthino", 255.0),
    ("Rhodo", 315.0),
];

fn genus_name(t: &Traits, seed: u64) -> String {
    let mut rng = Rng::new(seed);
    let hue = t.hue as f32 / 256.0 * 360.0;
    let color = if t.saturation < 50 {
        if rng.chance(0.5) { "Leuco" } else { "Glauco" }
    } else {
        COLOR_PREFIX
            .iter()
            .min_by(|a, b| {
                let da = (hue - a.1).abs().min(360.0 - (hue - a.1).abs());
                let db = (hue - b.1).abs().min(360.0 - (hue - b.1).abs());
                da.partial_cmp(&db).unwrap()
            })
            .unwrap()
            .0
    };
    let diet: &[&str] = match t.main_diet() {
        0 => &["phyta", "coccus", "phycus", "chloris", "plasma"],
        1 => &["lithus", "thermus", "sulfa", "ferrum", "chemon"],
        2 => &["phagus", "sapros", "necrus", "limax", "detrix"],
        _ => &["vorax", "odon", "raptor", "lamia", "phagon"],
    };
    let size_suffix: &[&str] = if t.adult_radius < 4.0 {
        &["", "ella", "ula", "ina"]
    } else if t.adult_radius < 24.0 {
        &["", "is", "ops", "ella"]
    } else {
        &["", "ops", "us", "odon"]
    };
    let stem = diet[rng.below(diet.len() as u32) as usize];
    let suffix = size_suffix[rng.below(size_suffix.len() as u32) as usize];
    let mut name = format!("{color}{stem}");
    if !suffix.is_empty() && !name.ends_with(suffix) {
        // Drop a trailing vowel/"us" before adding a suffix.
        let trimmed = name
            .trim_end_matches(['a', 'e', 'i', 'o', 'u', 's'])
            .to_string();
        name = format!("{trimmed}{suffix}");
    }
    name
}

/// Epithets that describe traits, with how strongly each applies.
fn descriptive_epithets(t: &Traits, genome: &Genome) -> Vec<(&'static str, f32)> {
    let d = &t.drives;
    // How much of each part kind the body carries (copies × size).
    let mut amount = [0.0f32; PartKind::ALL.len()];
    let segments = t.segments as f32;
    for p in parts::decode_parts(genome) {
        let repeats = if p.anchor == parts::Anchor::EverySegment {
            segments
        } else {
            1.0
        };
        amount[p.kind as usize - 1] += p.count as f32 * repeats * p.size;
    }
    let part = |k: PartKind| 1.0 - (-1.5 * amount[k as usize - 1]).exp();
    vec![
        ("magnus", (t.adult_radius / 24.0).min(2.0) - 0.4),
        ("gigas", (t.adult_radius / 60.0).min(2.0) - 0.5),
        ("minimus", 1.0 - t.adult_radius / 3.0),
        ("velox", t.swim * 1.5 - 0.3),
        ("agilis", t.turn_rate / 4.0 - 0.3),
        ("ferox", t.hunt * 1.2 + t.jaws * 0.5 - 0.2),
        ("flagellatus", part(PartKind::Flagellum) * 1.2 - 0.2),
        ("ciliatus", part(PartKind::Cilia) * 1.2 - 0.2),
        ("tentaculatus", part(PartKind::Tentacle) * 1.4 - 0.1),
        ("frondosus", part(PartKind::Frond) * 1.4 - 0.1),
        ("pinnatus", part(PartKind::Fin) * 1.3 - 0.1),
        ("caudatus", part(PartKind::Fluke) * 1.3 - 0.1),
        ("pedatus", part(PartKind::Leg) * 1.4 - 0.1),
        ("affixus", part(PartKind::Holdfast) * 1.3 - 0.1),
        ("stipitatus", part(PartKind::Stalk) * 1.2 - 0.1),
        ("oculatus", part(PartKind::Eye) * 1.3 - 0.1),
        ("antennatus", part(PartKind::Antenna) * 1.3 - 0.1),
        ("lucifer", part(PartKind::Lure) * 1.6),
        ("spinosus", part(PartKind::Spine) * 1.4),
        ("dentatus", part(PartKind::Jaw) * 1.3),
        ("loricatus", part(PartKind::Shell) * 1.4),
        ("elongatus", t.elongation * 1.4 - 0.3),
        ("segmentatus", (t.segments as f32 - 2.0) / 3.0),
        ("toxicus", t.toxin * 1.4 - 0.1),
        ("anaerobius", (0.35 - t.aerobic) * 3.0),
        ("azotifer", t.fixation * 1.3 - 0.2),
        ("sulfureus", (0.5 - t.oxygenic) * 2.0 * t.photo),
        ("gregarius", d[drive::KIN] * 1.2),
        ("solitarius", -d[drive::KIN] * 1.2),
        ("profundus", -d[drive::RISE] * 1.1),
        ("superus", d[drive::RISE] * 1.1),
        ("thermophilus", (t.temp_pref - 26.0) / 10.0),
        ("frigidus", (10.0 - t.temp_pref) / 8.0),
        ("timidus", -d[drive::LARGER] * 1.1),
        ("audax", d[drive::LARGER] * 1.1),
        ("longaevus", genome.frac(gene::LONGEVITY) * 1.2 - 0.2),
        ("fugax", 0.6 - genome.frac(gene::LONGEVITY) * 3.0 - 0.3),
        ("prolificus", (t.brood as f32 - 2.0) / 3.0),
        ("photophilus", d[drive::LIGHT]),
        ("lucifugus", -d[drive::LIGHT]),
        ("vagans", t.restlessness * 1.2 - 0.2),
        ("robustus", genome.frac(gene::TOUGHNESS) * 1.2 - 0.2),
        ("tenax", genome.frac(gene::REGEN) * 1.2 - 0.2),
        ("natans", genome.signed(gene::BUOYANCY) * 1.5 - 0.3),
        ("mergens", -genome.signed(gene::BUOYANCY) * 1.2),
    ]
}

/// Neutral epithets used when nothing is distinctive (or distinctive ones are taken).
const PLAIN: [&str; 48] = [
    "vulgaris",
    "communis",
    "simplex",
    "primus",
    "modestus",
    "placidus",
    "errans",
    "tenuis",
    "pallidus",
    "novus",
    "obscurus",
    "parvus",
    "rotundus",
    "gracilis",
    "lentus",
    "mollis",
    "tardus",
    "levis",
    "serenus",
    "clarus",
    "fidelis",
    "humilis",
    "pulcher",
    "rarus",
    "sollers",
    "tacitus",
    "varius",
    "vividus",
    "comptus",
    "delicatus",
    "exiguus",
    "fulgens",
    "hilaris",
    "insignis",
    "laetus",
    "lucidus",
    "mirabilis",
    "nitidus",
    "ornatus",
    "perpusillus",
    "quietus",
    "rutilus",
    "splendens",
    "subtilis",
    "tranquillus",
    "umbrosus",
    "venustus",
    "zephyrus",
];

/// Candidate epithets in order of preference.
fn epithet_candidates(t: &Traits, genome: &Genome, seed: u64) -> Vec<&'static str> {
    let mut rng = Rng::new(seed);
    let mut descriptive = descriptive_epithets(t, genome);
    descriptive.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let mut out: Vec<&'static str> = Vec::new();
    // Shuffle the top few distinctive ones so siblings don't all get the same name.
    let mut strong: Vec<&'static str> = descriptive
        .iter()
        .take(3)
        .filter(|c| c.1 > 0.25)
        .map(|c| c.0)
        .collect();
    while !strong.is_empty() {
        let i = rng.below(strong.len() as u32) as usize;
        out.push(strong.swap_remove(i));
    }
    let mut plain: Vec<&'static str> = PLAIN.to_vec();
    while !plain.is_empty() {
        let i = rng.below(plain.len() as u32) as usize;
        out.push(plain.swap_remove(i));
    }
    out
}

const ROMAN: [&str; 10] = [
    "", " II", " III", " IV", " V", " VI", " VII", " VIII", " IX", " X",
];

impl SpeciesRegistry {
    /// Create the root species for the ancestor genome.
    pub fn new(ancestor: &Genome, config: &Config) -> Self {
        let mut reg = Self::default();
        let genus = reg.new_genus(ancestor, config);
        let id = reg.insert_species(ancestor, genus, None, 0, config);
        let s = reg.species.get_mut(&id).unwrap();
        s.established = true;
        s.established_at = Some(0);
        reg
    }

    /// Add another founding lineage (its own genus and established species) and return its id.
    pub fn add_root(&mut self, founder: &Genome, config: &Config, tick: u64) -> u32 {
        let genus = self.new_genus(founder, config);
        let id = self.insert_species(founder, genus, None, tick, config);
        let s = self.species.get_mut(&id).unwrap();
        s.established = true;
        s.established_at = Some(tick);
        id
    }

    fn new_genus(&mut self, founder: &Genome, config: &Config) -> u32 {
        let id = self.next_genus;
        self.next_genus += 1;
        let t = Traits::decode(founder, config);
        let mut name = genus_name(&t, mix64(id as u64 ^ 0x6E6E));
        let mut attempt = 1;
        while self.genera.values().any(|g| g.name == name) && attempt < 12 {
            name = genus_name(&t, mix64(id as u64 ^ 0x6E6E ^ attempt << 20));
            attempt += 1;
        }
        self.genera.insert(
            id,
            Genus {
                id,
                name,
                founder: *founder,
            },
        );
        id
    }

    fn insert_species(
        &mut self,
        founder: &Genome,
        genus: u32,
        parent: Option<u32>,
        tick: u64,
        config: &Config,
    ) -> u32 {
        let id = self.next_species;
        self.next_species += 1;
        let t = Traits::decode(founder, config);
        self.species.insert(
            id,
            Species {
                id,
                name: String::new(),
                genus,
                parent,
                founder: *founder,
                founded: tick,
                extinct: None,
                population: 0,
                peak: 0,
                established: false,
                established_at: None,
                color: t.color,
                total_born: 0,
            },
        );
        self.assign_name(id, config);
        id
    }

    fn assign_name(&mut self, id: u32, config: &Config) {
        let s = &self.species[&id];
        let genus_id = s.genus;
        let genus = self.genera[&genus_id].name.clone();
        let t = Traits::decode(&s.founder, config);
        // Only named (established) species matter to viewers; transient variants don't
        // reserve names.
        let taken = |name: &str| {
            self.species
                .values()
                .any(|o| o.id != id && o.established && o.name == name)
        };
        let candidates = epithet_candidates(&t, &s.founder, mix64(id as u64 ^ 0x5EED));
        let mut name = None;
        for e in &candidates {
            let n = format!("{genus} {e}");
            if !taken(&n) {
                name = Some(n);
                break;
            }
        }
        let name = name.unwrap_or_else(|| {
            let base = format!("{genus} {}", candidates[0]);
            (1..)
                .map(|n| {
                    if n < ROMAN.len() {
                        format!("{base}{}", ROMAN[n])
                    } else {
                        format!("{base} {}", n + 1)
                    }
                })
                .find(|n| !taken(n))
                .unwrap()
        });
        self.species.get_mut(&id).unwrap().name = name;
    }

    /// Species for a newborn of `parent_species` with `genome`: the parent's, or a new one.
    pub fn classify(
        &mut self,
        parent_species: u32,
        genome: &Genome,
        tick: u64,
        config: &Config,
    ) -> u32 {
        let sc = &config.species;
        let Some(parent) = self.species.get(&parent_species) else {
            return parent_species;
        };
        if genome.distance(&parent.founder, sc.color_weight, sc.gene_cap) <= sc.threshold {
            return parent_species;
        }
        let parent_genus = parent.genus;
        let genus = match self.genera.get(&parent_genus) {
            Some(g)
                if genome.distance(&g.founder, sc.color_weight, sc.gene_cap)
                    <= sc.genus_threshold =>
            {
                parent_genus
            }
            _ => self.new_genus(genome, config),
        };
        self.insert_species(genome, genus, Some(parent_species), tick, config)
    }

    pub fn on_birth(&mut self, species: u32, config: &Config, tick: u64) {
        let threshold = config.species.established;
        if let Some(s) = self.species.get_mut(&species) {
            s.population += 1;
            s.total_born += 1;
            s.peak = s.peak.max(s.population);
            if s.extinct.is_some() {
                s.extinct = None;
            }
            if !s.established && s.population >= threshold {
                s.established = true;
                s.established_at = Some(tick);
                // Pick the name now, so it's unique among the named species.
                self.assign_name(species, config);
                self.events.push(SpeciesEvent::Established(species));
            }
        }
    }

    pub fn on_death(&mut self, species: u32, tick: u64) {
        let Some(s) = self.species.get_mut(&species) else {
            return;
        };
        s.population = s.population.saturating_sub(1);
        if s.population == 0 {
            if s.established {
                s.extinct = Some(tick);
                self.events.push(SpeciesEvent::Extinct(species));
            } else {
                // Never got going; forget it. Genera without species are kept (cheap).
                self.species.remove(&species);
            }
        }
    }

    pub fn get(&self, id: u32) -> Option<&Species> {
        self.species.get(&id)
    }

    pub fn name(&self, id: u32) -> &str {
        self.species
            .get(&id)
            .map(|s| s.name.as_str())
            .unwrap_or("?")
    }

    /// Established species currently alive.
    pub fn living(&self) -> impl Iterator<Item = &Species> {
        self.species
            .values()
            .filter(|s| s.established && s.population > 0)
    }

    /// Nearest established ancestor (for unestablished variants).
    pub fn established_ancestor(&self, mut id: u32) -> Option<u32> {
        for _ in 0..10_000 {
            let s = self.species.get(&id)?;
            if s.established {
                return Some(id);
            }
            id = s.parent?;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Config, Genome, SpeciesRegistry) {
        let config = Config::default();
        let g = Genome::parse(&config.population.ancestor).unwrap();
        let reg = SpeciesRegistry::new(&g, &config);
        (config, g, reg)
    }

    #[test]
    fn root_species_is_named() {
        let (_, _, reg) = setup();
        let root = reg.get(0).unwrap();
        assert!(root.established);
        assert!(root.name.starts_with("Chloro"), "{}", root.name);
        assert!(root.name.contains(' '));
    }

    #[test]
    fn small_changes_stay_in_species_large_changes_found_new_ones() {
        let (config, g, mut reg) = setup();
        let mut close = g;
        close.0[gene::SIZE] += 2;
        assert_eq!(reg.classify(0, &close, 10, &config), 0);
        let mut far = g;
        far.0[gene::SIZE] = 0x90;
        far.0[gene::HUNT] = 0xC0;
        far.0[gene::TOXIN] = 0x80;
        far.0[gene::FIXATION] = 0x80;
        far.0[parts::slot_offset(0)] = PartKind::Fin.code();
        // A single huge jump alone is not enough.
        let mut one = g;
        one.0[gene::SIZE] = 0xFF;
        assert_eq!(reg.classify(0, &one, 10, &config), 0);
        let id = reg.classify(0, &far, 10, &config);
        assert_ne!(id, 0);
        let s = reg.get(id).unwrap();
        assert_eq!(s.parent, Some(0));
        assert!(!s.established);
        assert_ne!(s.name, reg.get(0).unwrap().name);
    }

    #[test]
    fn species_lifecycle() {
        let (config, g, mut reg) = setup();
        let mut far = g;
        far.0[gene::HUNT] = 0xFF;
        far.0[gene::PHOTO] = 0x00;
        far.0[gene::RESPIRATION] = 0x00;
        far.0[gene::SIZE] = 0x80;
        far.0[parts::slot_offset(0)] = PartKind::Jaw.code();
        far.0[parts::slot_offset(1)] = PartKind::Fin.code();
        let id = reg.classify(0, &far, 5, &config);
        assert_ne!(id, 0);
        for _ in 0..config.species.established {
            reg.on_birth(id, &config, 6);
        }
        assert!(reg.get(id).unwrap().established);
        for _ in 0..config.species.established {
            reg.on_death(id, 9);
        }
        assert_eq!(reg.get(id).unwrap().extinct, Some(9));
        // An unestablished variant is forgotten when it dies out.
        let mut other = g;
        other.0[gene::CHEMO] = 0xFF;
        other.0[gene::PHOTO] = 0x00;
        other.0[gene::BUOYANCY] = 0x00;
        other.0[gene::OXYGENIC] = 0x00;
        other.0[gene::TEMP_PREF] = 0xFF;
        other.0[parts::slot_offset(0)] = PartKind::Holdfast.code();
        let id2 = reg.classify(0, &other, 5, &config);
        assert_ne!(id2, 0);
        reg.on_birth(id2, &config, 6);
        reg.on_death(id2, 7);
        assert!(reg.get(id2).is_none());
    }
}

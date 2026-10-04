//! Time series statistics, the event log, and milestones.

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
    Milestone,
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
#[serde(default)]
pub struct EventLog {
    pub entries: VecDeque<Event>,
    /// Total events ever logged (entries may be trimmed).
    pub count: u64,
    /// Every event that changed the world (environment, disasters, recolonizations), kept for
    /// the whole run: they're rare, and the family tree marks them.
    pub world: Vec<Event>,
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

/// One-time "firsts" worth announcing.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Milestones {
    pub first_swimmer: bool,
    pub first_kill: bool,
    pub first_chemotroph: bool,
    pub first_scavenger: bool,
    pub first_hunter: bool,
    pub first_eyes: bool,
    pub first_armor: bool,
    pub first_walker: bool,
    pub first_anchored: bool,
    pub first_anaerobe: bool,
    pub first_sulfur: bool,
    pub first_fixer: bool,
    pub first_toxic: bool,
    /// Part kinds that have appeared in a reproducing lineage (bit = kind code).
    pub parts_seen: u32,
    /// Largest adult radius ever born.
    pub size_record: f32,
}

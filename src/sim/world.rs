//! Simulation-wide resources (tick counter, ids, RNG, bounds).

use bevy::prelude::*;
use rand::prelude::*;
use rand::rngs::StdRng;

use crate::config::SimConfig;

/// Fixed simulation timestep in seconds ([SIMULATION.md](../../docs/SIMULATION.md)).
pub const SIM_DT: f32 = 1.0 / 60.0;

/// Bevy resource for loaded simulation config (`config` module stays Bevy-free).
#[derive(Resource, Clone)]
pub struct SimConfigResource(pub SimConfig);

const STREAM_SPAWN: u64 = 0x5A50_4E5F;
const STREAM_MUTATION: u64 = 0x4D55_5410;
const STREAM_REPRO_RISK: u64 = 0x5250_524B;
const STREAM_MOVEMENT: u64 = 0x4D4F_5645;

/// World half-extents in sim units (1 unit = 1 pixel in visual mode).
#[derive(Resource, Clone, Copy)]
pub struct WorldBounds {
    pub half_x: f32,
    pub half_y: f32,
}

impl WorldBounds {
    pub fn from_world_size(width: f32, height: f32) -> Self {
        Self {
            half_x: width / 2.0,
            half_y: height / 2.0,
        }
    }
}

/// Monotonic simulation tick (incremented once per pipeline pass).
#[derive(Resource, Clone, Copy, Default)]
pub struct SimTick {
    pub tick: u64,
}

/// When true, reproduction skips spawning children (Phase 1J failsafe stub).
#[derive(Resource, Clone, Copy, Default)]
pub struct SpawnBlocked(pub bool);

/// Next entity id for [`Lineage`](super::components::Lineage).
#[derive(Resource, Clone, Copy)]
pub struct NextEntityId(pub u64);

impl NextEntityId {
    pub fn new(start: u64) -> Self {
        Self(start)
    }

    pub fn next(&mut self) -> u64 {
        let id = self.0;
        self.0 = self.0.saturating_add(1);
        id
    }
}

/// Seeded RNG sub-streams (see [SIMULATION.md](../../docs/SIMULATION.md)).
///
/// With `master_seed`, [`stream_spawn`](Self::spawn_rng) is sequential; mutation and repro-risk
/// draws use per-event sub-streams so unrelated rolls do not share entropy.
#[derive(Resource)]
pub struct SimRng {
    master_seed: Option<u64>,
    /// `stream_spawn` — initial placement, child offsets, spawn jitter.
    spawn: StdRng,
}

impl SimRng {
    pub fn from_config(config: &SimConfig) -> Self {
        match config.run.master_seed {
            Some(master) => Self {
                master_seed: Some(master),
                spawn: StdRng::seed_from_u64(mix_seed(master, STREAM_SPAWN)),
            },
            None => Self {
                master_seed: None,
                spawn: StdRng::from_rng(rand::thread_rng()).expect("thread_rng seeds StdRng"),
            },
        }
    }

    pub fn is_seeded(&self) -> bool {
        self.master_seed.is_some()
    }

    /// Mutable `stream_spawn` RNG (position / velocity jitter at spawn).
    pub fn spawn_rng(&mut self) -> &mut StdRng {
        &mut self.spawn
    }

    /// Deterministic per-entity mutation stream when `master_seed` is set.
    pub fn stream_mutation(&self, entity_id: u64, generation: u32) -> StdRng {
        let seed = self.master_seed.map(|master| {
            mix_seed(
                master,
                STREAM_MUTATION
                    ^ entity_id
                    ^ (generation as u64).wrapping_mul(0xD134_2543_DE82_BF95),
            )
        });
        seed_from_opt_or_thread(seed)
    }

    /// Deterministic reproduction-risk stream when `master_seed` is set.
    pub fn stream_repro_risk(&self, entity_id: u64, tick: u64) -> StdRng {
        let seed = self.master_seed.map(|master| {
            mix_seed(
                master,
                STREAM_REPRO_RISK ^ entity_id ^ tick.wrapping_mul(0x94D0_49BB_1331_11EB),
            )
        });
        seed_from_opt_or_thread(seed)
    }

    /// Per-tick movement-intent rolls (`desire_to_move` gene).
    pub fn stream_movement(&self, entity_id: u64, tick: u64) -> StdRng {
        let seed = self.master_seed.map(|master| {
            mix_seed(
                master,
                STREAM_MOVEMENT ^ entity_id ^ tick.wrapping_mul(0xC2B2_AE3D_27D4_EB4F),
            )
        });
        seed_from_opt_or_thread(seed)
    }
}

fn seed_from_opt_or_thread(seed: Option<u64>) -> StdRng {
    match seed {
        Some(s) => StdRng::seed_from_u64(s),
        None => StdRng::from_rng(rand::thread_rng()).expect("thread_rng seeds StdRng"),
    }
}

/// SplitMix64-style mixing for sub-stream seeds.
pub fn mix_seed(master: u64, salt: u64) -> u64 {
    let mut z = master.wrapping_add(salt.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

pub fn advance_sim_tick(mut tick: ResMut<SimTick>) {
    tick.tick = tick.tick.saturating_add(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::headless::run_headless_sim;

    #[test]
    fn seeded_rng_is_reproducible() {
        let config = SimConfig::load_default().unwrap();
        let mut a = SimRng::from_config(&config);
        let mut b = SimRng::from_config(&config);
        assert!(a.is_seeded());
        assert_eq!(a.spawn_rng().gen::<u32>(), b.spawn_rng().gen::<u32>());
    }

    #[test]
    fn mutation_stream_varies_by_entity() {
        let config = SimConfig::load_default().unwrap();
        let rng = SimRng::from_config(&config);
        let a = rng.stream_mutation(1, 0).gen::<u32>();
        let b = rng.stream_mutation(2, 0).gen::<u32>();
        assert_ne!(a, b);
    }

    #[test]
    fn repro_risk_stream_stable_for_same_key() {
        let config = SimConfig::load_default().unwrap();
        let rng = SimRng::from_config(&config);
        let a = rng.stream_repro_risk(7, 100).gen::<f32>();
        let b = rng.stream_repro_risk(7, 100).gen::<f32>();
        assert_eq!(a, b);
    }

    #[test]
    fn repro_risk_stream_varies_by_tick() {
        let config = SimConfig::load_default().unwrap();
        let rng = SimRng::from_config(&config);
        let a = rng.stream_repro_risk(7, 100).gen::<f32>();
        let b = rng.stream_repro_risk(7, 101).gen::<f32>();
        assert_ne!(a, b);
    }

    #[test]
    fn next_entity_id_increments() {
        let mut ids = NextEntityId::new(10);
        assert_eq!(ids.next(), 10);
        assert_eq!(ids.next(), 11);
    }

    #[test]
    fn seeded_reproduction_digest_identical_over_500_ticks() {
        let mut config = SimConfig::load_default().unwrap();
        assert!(config.run.master_seed.is_some());
        config.world.initial_population = 10;

        let a = run_headless_sim(&config, 500);
        let b = run_headless_sim(&config, 500);
        assert_eq!(
            (
                a.alive,
                a.births,
                a.repro_fatalities,
                a.stillborns,
                a.child_genome_fold
            ),
            (
                b.alive,
                b.births,
                b.repro_fatalities,
                b.stillborns,
                b.child_genome_fold
            ),
            "same master_seed and config must yield identical repro outcomes"
        );

        let long = run_headless_sim(&config, 3000);
        assert!(
            long.births > 0,
            "expected births over 3k ticks (got {long:?})"
        );
    }

    #[test]
    fn different_master_seed_changes_reproduction_digest() {
        let mut config_a = SimConfig::load_default().unwrap();
        config_a.run.master_seed = Some(1);
        config_a.world.initial_population = 25;
        let mut config_b = config_a.clone();
        config_b.run.master_seed = Some(2);

        let a = run_headless_sim(&config_a, 3000);
        let b = run_headless_sim(&config_b, 3000);
        assert_ne!(
            (
                a.alive,
                a.births,
                a.repro_fatalities,
                a.stillborns,
                a.child_genome_fold
            ),
            (
                b.alive,
                b.births,
                b.repro_fatalities,
                b.stillborns,
                b.child_genome_fold
            )
        );
        assert!(a.births > 0 && b.births > 0);
    }
}

//! Simulation tick systems ([SIMULATION.md](../../docs/SIMULATION.md)).
//!
//! Phase 4+ pipeline: `environment_tick` → `sense_exposure` → `sense_rays` (interval) → `behavior_v2` →
//! `feeding_v2` → `metabolism_and_gains` → `behavior_movement_v2` → `passive_physics` →
//! `interactions` → `healing_v2` → `reproduction` → `advance_age` → `death_and_despawn` →
//! `performance_failsafe` → `metrics_sample`.
//!
//! Most non-movement systems run on [`RunConfig::logic_interval_ticks`](crate::config::RunConfig::logic_interval_ticks)
//! (default 6); movement (`behavior_movement_v2`, `passive_physics`) runs every tick. Sensor rays and
//! biomass feeding keep their own longer intervals.

mod age;
mod death;
mod environment_tick;
mod failsafe;
mod headless;
pub mod healing;
mod interactions;
mod metabolism;
mod metabolism_v2;
mod movement;
mod movement_v2;
mod passive_physics;
mod healing_v2;
mod reproduction;
mod reproduction_v2;
mod behavior_v2;
mod feeding_v2;
mod sense;
mod sense_rays;

pub use crate::sim::metrics::metrics_sample;

pub use age::*;
pub use death::*;
pub use failsafe::*;
pub use headless::*;
pub use healing::{repair_energy_cost, repair_hp_cap_per_tick};
pub use interactions::{interactions, v2_damage_mitigation};
pub use metabolism::*;
pub use metabolism_v2::{
    apply_metabolism_tick_v2, metabolism_tick_delta_v2, metabolism_tick_cost_v2,
    photosynthetic_gain_per_tick_v2,
};
pub use movement::*;
pub use behavior_v2::behavior_v2;
pub use environment_tick::environment_tick;
pub use feeding_v2::feeding_v2;
pub use movement_v2::{behavior_movement_v2, movement_energy_for_distance_v2};
pub use passive_physics::passive_physics;
pub use sense_rays::{evaluate_ray_slot_matches, evaluate_slot_matches, SlotMatchResult};
pub use healing_v2::{apply_heal_tick_v2, healing_v2};
pub use reproduction::*;
pub use reproduction_v2::{
    attempt_reproduction_v2, collect_vitality_payment, endowment_budget_cap,
    repro_threshold_energy, split_endowment_to_child,
};
pub use sense::*;
pub use sense_rays::sense_rays;

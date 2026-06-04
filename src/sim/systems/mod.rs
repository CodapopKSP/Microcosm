//! Simulation tick systems ([SIMULATION.md](../../docs/SIMULATION.md)).
//!
//! Phase 4+ pipeline: `sense_exposure` (light, hazards, vents) → `metabolism_and_gains` → `behavior_movement` →
//! `interactions` → `healing` → `reproduction` → `advance_age` → `death_and_despawn` →
//! `performance_failsafe` → `metrics_sample`.
//! Metabolism may skip photosynthetic gain on overlap when
//! `[environment].crowding_blocks_photosynthesis` is true.

mod age;
mod death;
mod failsafe;
mod headless;
pub mod healing;
mod interactions;
mod metabolism;
mod movement;
mod reproduction;
mod sense;

pub use crate::sim::metrics::metrics_sample;

pub use age::*;
pub use death::*;
pub use failsafe::*;
pub use headless::*;
pub use healing::*;
pub use interactions::*;
pub use metabolism::*;
pub use movement::*;
pub use reproduction::*;
pub use sense::*;

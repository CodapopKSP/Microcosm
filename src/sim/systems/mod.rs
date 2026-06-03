//! Simulation tick systems ([SIMULATION.md](../../docs/SIMULATION.md)).
//!
//! Phase 2 pipeline: `sense_exposure` → `metabolism_and_gains` → `behavior_movement` →
//! `reproduction` → `death_and_despawn` → `performance_failsafe` (environment, interactions,
//! healing, metrics are no-ops). Metabolism may skip photosynthetic gain on overlap when
//! `[environment].crowding_blocks_photosynthesis` is true.

mod death;
mod failsafe;
mod metabolism;
mod movement;
mod reproduction;
mod sense;

pub use death::*;
pub use failsafe::*;
pub use metabolism::*;
pub use movement::*;
pub use reproduction::*;
pub use sense::*;

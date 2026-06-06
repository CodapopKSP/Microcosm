//! Per-tick age advancement (Phase 3D).

use bevy::prelude::*;

use super::super::components::Age;
use super::super::interval::{logic_active_this_tick, logic_elapsed_ticks};
use super::super::world::{SimConfigResource, SimTick};

/// Increment `Age.ticks` for every living agent after reproduction, before death checks.
pub fn advance_age(
    tick: Res<SimTick>,
    config: Res<SimConfigResource>,
    mut query: Query<&mut Age>,
) {
    let run = &config.0.run;
    if !logic_active_this_tick(tick.tick, run) {
        return;
    }
    let steps = logic_elapsed_ticks(run);
    for mut age in &mut query {
        age.ticks = age.ticks.saturating_add(steps);
    }
}

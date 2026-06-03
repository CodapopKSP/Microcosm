//! Headless exit when `SimTick` reaches `run.max_ticks` (Phase 3A/3B).

use bevy::app::AppExit;
use bevy::prelude::*;

use super::super::world::{SimConfigResource, SimTick};

/// Exit the app after the configured tick count (headless batch runs).
pub fn exit_headless_when_done(
    tick: Res<SimTick>,
    config: Res<SimConfigResource>,
    mut exit: EventWriter<AppExit>,
) {
    if tick.tick >= config.0.run.max_ticks {
        eprintln!("headless: finished at tick {}", tick.tick);
        exit.send(AppExit::Success);
    }
}

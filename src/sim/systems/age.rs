//! Per-tick age advancement (Phase 3D).

use bevy::prelude::*;

use super::super::components::Age;

/// Increment `Age.ticks` for every living agent after reproduction, before death checks.
pub fn advance_age(mut query: Query<&mut Age>) {
    for mut age in &mut query {
        age.ticks = age.ticks.saturating_add(1);
    }
}

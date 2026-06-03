//! Performance failsafe when `population > max_entities` ([SIMULATION.md](../../docs/SIMULATION.md), Phase 1J).
//!
//! Emergency CPU relief only — not a fitness mechanic. Runs after death each tick.

use bevy::prelude::*;

use crate::config::FailsafePolicy;

use super::super::components::Genome;
use super::super::world::{SimConfigResource, SimTick, SpawnBlocked};

/// Suppresses repeated stderr warnings while population stays above `max_entities`.
#[derive(Resource, Default)]
pub struct FailsafeWarnLatch {
    over_cap: bool,
}

/// Apply failsafe policy for the current population count (pure logic for tests).
pub fn apply_failsafe(
    population: usize,
    max_entities: usize,
    policy: FailsafePolicy,
    spawn_blocked: &mut bool,
) -> FailsafeAction {
    if population > max_entities {
        match policy {
            FailsafePolicy::StopSpawns => {
                let newly_active = !*spawn_blocked;
                *spawn_blocked = true;
                FailsafeAction::StopSpawnsActive { newly_active }
            }
            FailsafePolicy::DespawnLowestEnergy => FailsafeAction::StubDespawnLowestEnergy,
            FailsafePolicy::Pause => FailsafeAction::StubPause,
        }
    } else {
        *spawn_blocked = false;
        FailsafeAction::WithinCap
    }
}

/// What the failsafe did this tick (for logging in the Bevy system).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailsafeAction {
    WithinCap,
    StopSpawnsActive { newly_active: bool },
    StubDespawnLowestEnergy,
    StubPause,
}

/// Bevy system: enforce `max_entities` policy after death & despawn.
pub fn performance_failsafe(
    config: Res<SimConfigResource>,
    tick: Res<SimTick>,
    mut spawn_blocked: ResMut<SpawnBlocked>,
    mut warn_latch: ResMut<FailsafeWarnLatch>,
    agents: Query<(), With<Genome>>,
) {
    let performance = &config.0.performance;
    let population = agents.iter().count();
    let over_cap = population > performance.max_entities;
    let should_warn = over_cap && !warn_latch.over_cap;
    warn_latch.over_cap = over_cap;

    let action = apply_failsafe(
        population,
        performance.max_entities,
        performance.failsafe_policy,
        &mut spawn_blocked.0,
    );

    if !should_warn {
        return;
    }

    match action {
        FailsafeAction::WithinCap => {}
        FailsafeAction::StopSpawnsActive { .. } => {
            eprintln!(
                "failsafe (stop_spawns): population {population} > max_entities {} at tick {}",
                performance.max_entities,
                tick.tick
            );
        }
        FailsafeAction::StubDespawnLowestEnergy => {
            eprintln!(
                "failsafe (despawn_lowest_energy): not implemented — population {population} > max_entities {} at tick {} (spawns not blocked)",
                performance.max_entities,
                tick.tick
            );
        }
        FailsafeAction::StubPause => {
            eprintln!(
                "failsafe (pause): not implemented — population {population} > max_entities {} at tick {}",
                performance.max_entities,
                tick.tick
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FailsafePolicy;

    #[test]
    fn stop_spawns_blocks_when_over_cap() {
        let mut blocked = false;
        assert_eq!(
            apply_failsafe(101, 100, FailsafePolicy::StopSpawns, &mut blocked),
            FailsafeAction::StopSpawnsActive { newly_active: true }
        );
        assert!(blocked);

        assert_eq!(
            apply_failsafe(101, 100, FailsafePolicy::StopSpawns, &mut blocked),
            FailsafeAction::StopSpawnsActive { newly_active: false }
        );
        assert!(blocked);
    }

    #[test]
    fn stop_spawns_clears_when_within_cap() {
        let mut blocked = true;
        assert_eq!(
            apply_failsafe(100, 100, FailsafePolicy::StopSpawns, &mut blocked),
            FailsafeAction::WithinCap
        );
        assert!(!blocked);
    }

    #[test]
    fn stub_policies_do_not_block_spawns() {
        let mut blocked = false;
        assert_eq!(
            apply_failsafe(200, 100, FailsafePolicy::DespawnLowestEnergy, &mut blocked),
            FailsafeAction::StubDespawnLowestEnergy
        );
        assert!(!blocked);

        assert_eq!(
            apply_failsafe(200, 100, FailsafePolicy::Pause, &mut blocked),
            FailsafeAction::StubPause
        );
        assert!(!blocked);
    }
}

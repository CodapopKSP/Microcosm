//! Performance failsafe when `population > max_entities` ([SIMULATION.md](../../docs/SIMULATION.md), Phase 1J).
//!
//! Emergency CPU relief only — not a fitness mechanic. Runs after death each tick.

use bevy::prelude::*;

use crate::config::FailsafePolicy;

use super::super::components::{Energy, Genome};
use super::super::interval::logic_active_this_tick;
use super::super::world::{SimConfigResource, SimTick, SpawnBlocked};

/// Suppresses repeated stderr warnings while population stays above `max_entities`.
#[derive(Resource, Default)]
pub struct FailsafeWarnLatch {
    over_cap: bool,
}

/// How many lowest-energy agents to remove when over cap (0 if policy does not cull).
pub fn failsafe_cull_count(
    population: usize,
    max_entities: usize,
    policy: FailsafePolicy,
    cull_count: usize,
) -> usize {
    if population <= max_entities {
        return 0;
    }
    match policy {
        FailsafePolicy::DespawnLowestEnergy => cull_count.min(population),
        _ => 0,
    }
}

/// How many corpses to remove when over cap (0 if culling disabled or within cap).
pub fn corpse_cull_count(population: usize, max_corpses: usize, cull_count: usize) -> usize {
    if max_corpses == 0 || cull_count == 0 || population <= max_corpses {
        0
    } else {
        cull_count.min(population)
    }
}

/// Apply failsafe policy for the current population count (pure logic for tests).
pub fn apply_failsafe(
    population: usize,
    max_entities: usize,
    policy: FailsafePolicy,
    cull_count: usize,
    spawn_blocked: &mut bool,
) -> FailsafeAction {
    if population > max_entities {
        match policy {
            FailsafePolicy::StopSpawns => {
                let newly_active = !*spawn_blocked;
                *spawn_blocked = true;
                FailsafeAction::StopSpawnsActive { newly_active }
            }
            FailsafePolicy::DespawnLowestEnergy => {
                *spawn_blocked = false;
                FailsafeAction::DespawnLowestEnergy {
                    count: failsafe_cull_count(population, max_entities, policy, cull_count),
                }
            }
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
    DespawnLowestEnergy { count: usize },
    StubPause,
}

/// Remove the `count` agents with the lowest `internal_energy` (stable tie-break on entity index).
/// Returns the lowest-energy entries to despawn (length ≤ `count`).
pub fn cull_lowest_energy_entities(
    agents: &mut Vec<(Entity, f32)>,
    count: usize,
) -> usize {
    if count == 0 || agents.is_empty() {
        return 0;
    }
    agents.sort_by(|(e_a, e_a_n), (e_b, e_b_n)| {
        e_a_n
            .partial_cmp(e_b_n)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| e_a.index().cmp(&e_b.index()))
    });
    let n = count.min(agents.len());
    agents.truncate(n);
    n
}

/// Bevy system: enforce `max_entities` policy after death & despawn.
pub fn performance_failsafe(
    config: Res<SimConfigResource>,
    tick: Res<SimTick>,
    mut commands: Commands,
    mut spawn_blocked: ResMut<SpawnBlocked>,
    mut warn_latch: ResMut<FailsafeWarnLatch>,
    agents: Query<(Entity, &Energy), With<Genome>>,
) {
    if !logic_active_this_tick(tick.tick, &config.0.run) {
        return;
    }
    let performance = &config.0.performance;
    let population = agents.iter().count();
    let over_cap = population > performance.max_entities;
    let should_warn = over_cap && !warn_latch.over_cap;
    warn_latch.over_cap = over_cap;

    let action = apply_failsafe(
        population,
        performance.max_entities,
        performance.failsafe_policy,
        performance.failsafe_cull_count,
        &mut spawn_blocked.0,
    );

    if let FailsafeAction::DespawnLowestEnergy { count } = action {
        if count > 0 {
            let mut ranked: Vec<(Entity, f32)> = agents
                .iter()
                .map(|(entity, energy)| (entity, energy.internal_energy))
                .collect();
            let culled = cull_lowest_energy_entities(&mut ranked, count);
            for (entity, _) in ranked {
                commands.entity(entity).despawn_recursive();
            }
            eprintln!(
                "failsafe (despawn_lowest_energy): culled {culled} lowest-energy agents \
                 (population {population} > max_entities {} at tick {})",
                performance.max_entities,
                tick.tick
            );
        }
        return;
    }

    if !should_warn {
        return;
    }

    match action {
        FailsafeAction::WithinCap | FailsafeAction::DespawnLowestEnergy { .. } => {}
        FailsafeAction::StopSpawnsActive { .. } => {
            eprintln!(
                "failsafe (stop_spawns): population {population} > max_entities {} at tick {}",
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
            apply_failsafe(101, 100, FailsafePolicy::StopSpawns, 3000, &mut blocked),
            FailsafeAction::StopSpawnsActive { newly_active: true }
        );
        assert!(blocked);

        assert_eq!(
            apply_failsafe(101, 100, FailsafePolicy::StopSpawns, 3000, &mut blocked),
            FailsafeAction::StopSpawnsActive { newly_active: false }
        );
        assert!(blocked);
    }

    #[test]
    fn stop_spawns_clears_when_within_cap() {
        let mut blocked = true;
        assert_eq!(
            apply_failsafe(100, 100, FailsafePolicy::StopSpawns, 3000, &mut blocked),
            FailsafeAction::WithinCap
        );
        assert!(!blocked);
    }

    #[test]
    fn despawn_lowest_energy_does_not_block_spawns() {
        let mut blocked = false;
        assert_eq!(
            apply_failsafe(6000, 5000, FailsafePolicy::DespawnLowestEnergy, 3000, &mut blocked),
            FailsafeAction::DespawnLowestEnergy { count: 3000 }
        );
        assert!(!blocked);
    }

    #[test]
    fn cull_count_caps_at_population() {
        assert_eq!(
            failsafe_cull_count(200, 100, FailsafePolicy::DespawnLowestEnergy, 3000),
            200
        );
    }

    #[test]
    fn cull_lowest_energy_entities_picks_lowest() {
        let mut ranked = vec![
            (Entity::from_raw(1), 50.0),
            (Entity::from_raw(2), 10.0),
            (Entity::from_raw(3), 30.0),
        ];
        let n = cull_lowest_energy_entities(&mut ranked, 2);
        assert_eq!(n, 2);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].1, 10.0);
        assert_eq!(ranked[1].1, 30.0);
    }

    #[test]
    fn corpse_cull_count_caps_at_population() {
        assert_eq!(corpse_cull_count(1200, 1000, 500), 500);
        assert_eq!(corpse_cull_count(1000, 1000, 500), 0);
        assert_eq!(corpse_cull_count(200, 1000, 500), 0);
        assert_eq!(corpse_cull_count(0, 0, 500), 0);
    }

    #[test]
    fn pause_stub_does_not_block_spawns() {
        let mut blocked = false;
        assert_eq!(
            apply_failsafe(200, 100, FailsafePolicy::Pause, 3000, &mut blocked),
            FailsafeAction::StubPause
        );
        assert!(!blocked);
    }
}

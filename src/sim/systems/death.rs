//! Death and despawn (Phase 1I). Entity–entity overlap never despawns here (Phase 4+ damage).
//!
//! Canonical tick order ([SIMULATION.md](../../docs/SIMULATION.md)): metabolism → movement →
//! reproduction → **death** → failsafe. Reproduction marks `ReproFatality` via [`PendingDeath`];
//! this system is the only place that despawns living agents for lifecycle reasons.

use bevy::prelude::*;

use super::super::components::{DeathReason, Energy, Health, PendingDeath};

/// Metabolism / movement death check (repro fatality uses [`PendingDeath`]).
pub fn death_reason_from_vitals(energy: &Energy, health: &Health) -> Option<DeathReason> {
    if energy.internal_energy <= 0.0 {
        return Some(DeathReason::EnergyDepleted);
    }
    if health.core_hp <= 0.0 {
        return Some(DeathReason::CoreDestroyed);
    }
    None
}

/// Bevy system: despawn agents that died this tick.
pub fn death_and_despawn(
    mut commands: Commands,
    query: Query<(Entity, &Energy, &Health, Option<&PendingDeath>)>,
) {
    for (entity, energy, health, pending) in &query {
        let reason = pending
            .map(|p| p.0)
            .or_else(|| death_reason_from_vitals(energy, health));

        if reason.is_some() {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::components::{Energy, Health};

    #[test]
    fn energy_depleted_triggers_death() {
        let energy = Energy {
            internal_energy: 0.0,
        };
        let health = Health {
            core_hp: 50.0,
            core_max_hp: 100.0,
            part_hp: vec![],
        };
        assert_eq!(
            death_reason_from_vitals(&energy, &health),
            Some(DeathReason::EnergyDepleted)
        );
    }

    #[test]
    fn core_destroyed_triggers_death() {
        let energy = Energy {
            internal_energy: 10.0,
        };
        let health = Health {
            core_hp: 0.0,
            core_max_hp: 100.0,
            part_hp: vec![],
        };
        assert_eq!(
            death_reason_from_vitals(&energy, &health),
            Some(DeathReason::CoreDestroyed)
        );
    }

    #[test]
    fn healthy_agent_not_flagged() {
        let energy = Energy {
            internal_energy: 1.0,
        };
        let health = Health {
            core_hp: 1.0,
            core_max_hp: 100.0,
            part_hp: vec![],
        };
        assert_eq!(death_reason_from_vitals(&energy, &health), None);
    }

    /// Production code in movement/metabolism/reproduction must not despawn agents (Phase 1I).
    fn production_src_without_tests(path: &str) -> String {
        let src = std::fs::read_to_string(path).expect("read source");
        src.split("#[cfg(test)]")
            .next()
            .unwrap_or(&src)
            .to_string()
    }

    #[test]
    fn no_collision_or_tick_despawn_outside_death_system() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for file in ["movement.rs", "metabolism.rs", "reproduction.rs"] {
            let path = root.join("src/sim/systems").join(file);
            let src = production_src_without_tests(path.to_str().unwrap());
            assert!(
                !src.contains(".despawn"),
                "{file} must not despawn entities"
            );
        }
    }

    #[test]
    fn negative_energy_counts_as_depleted() {
        let energy = Energy {
            internal_energy: -0.01,
        };
        let health = Health {
            core_hp: 100.0,
            core_max_hp: 100.0,
            part_hp: vec![],
        };
        assert_eq!(
            death_reason_from_vitals(&energy, &health),
            Some(DeathReason::EnergyDepleted)
        );
    }
}

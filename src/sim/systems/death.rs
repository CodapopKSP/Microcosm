//! Death and despawn (Phase 1I). Entity–entity overlap never despawns here (Phase 4+ damage).
//!
//! Canonical tick order ([SIMULATION.md](../../docs/SIMULATION.md)): metabolism → movement →
//! reproduction → **death** → failsafe. Reproduction marks `ReproFatality` via [`PendingDeath`];
//! this system is the only place that despawns living agents for lifecycle reasons.

use bevy::prelude::*;

use super::super::components::{Age, DeathReason, Energy, Health, PendingDeath, Phenotype};
use super::super::metrics::MetricsPeriodCounters;
use crate::genome::Phenotype as PhenotypeData;

/// Old-age hard cap removed; aging costs metabolism instead ([GENOME.md](../../../docs/GENOME.md)).
pub fn death_reason_from_age(_age: &Age, _phenotype: &PhenotypeData) -> Option<DeathReason> {
    None
}

/// Vitals death check (repro fatality uses [`PendingDeath`]).
///
/// Zero energy alone is not fatal; metabolic deficit drains `core_hp` until destroyed.
pub fn death_reason_from_vitals(_energy: &Energy, health: &Health) -> Option<DeathReason> {
    if health.core_hp <= 0.0 {
        Some(DeathReason::CoreDestroyed)
    } else {
        None
    }
}

/// Bevy system: despawn agents that died this tick.
pub fn death_and_despawn(
    mut commands: Commands,
    mut counters: ResMut<MetricsPeriodCounters>,
    query: Query<(Entity, &Age, &Phenotype, &Energy, &Health, Option<&PendingDeath>)>,
) {
    for (entity, age, phenotype, energy, health, pending) in &query {
        let reason = pending
            .map(|p| p.0)
            .or_else(|| death_reason_from_age(age, &phenotype.0))
            .or_else(|| death_reason_from_vitals(energy, health));

        if let Some(reason) = reason {
            counters.record_death(reason);
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::sim::components::{Age, Energy, Health};

    #[test]
    fn zero_energy_with_hp_is_not_fatal() {
        let energy = Energy {
            internal_energy: 0.0,
        };
        let health = Health {
            core_hp: 50.0,
            core_max_hp: 100.0,
            part_hp: vec![],
        };
        assert_eq!(death_reason_from_vitals(&energy, &health), None);
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
        for file in ["movement.rs", "metabolism.rs", "reproduction.rs", "interactions.rs"] {
            let path = root.join("src/sim/systems").join(file);
            let src = production_src_without_tests(path.to_str().unwrap());
            assert!(
                !src.contains(".despawn"),
                "{file} must not despawn entities"
            );
        }
    }

    #[test]
    fn age_no_longer_triggers_death() {
        let config = SimConfig::load_default().unwrap();
        let genome = crate::genome::Genome::ancestor_from_config(&config).unwrap();
        let phenotype = crate::genome::decode(&genome, &config).unwrap();
        let old = Age {
            ticks: 1_000_000,
        };
        assert_eq!(death_reason_from_age(&old, &phenotype), None);
    }

    #[test]
    fn negative_energy_field_is_not_fatal_if_hp_positive() {
        let energy = Energy {
            internal_energy: -0.01,
        };
        let health = Health {
            core_hp: 100.0,
            core_max_hp: 100.0,
            part_hp: vec![],
        };
        assert_eq!(death_reason_from_vitals(&energy, &health), None);
    }
}

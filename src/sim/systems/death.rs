//! Death and despawn (Phase 1I). Entity–entity overlap never despawns here (Phase 4+ damage).
//!
//! Canonical tick order ([SIMULATION.md](../../docs/SIMULATION.md)): metabolism → movement →
//! reproduction → **death** → failsafe. Reproduction marks `ReproFatality` via [`PendingDeath`];
//! this system is the only place that despawns living agents for lifecycle reasons.

use bevy::prelude::*;
use bevy_hierarchy::DespawnRecursiveExt;

use super::feeding_v2::corpse_energy_from_max_health;
use super::super::agent_material::AgentColorMaterial;
use super::super::components::{
    Agent, Age, Corpse, DeathReason, Energy, Health, Lineage, PendingDeath, Phenotype,
};
use super::super::metrics::MetricsPeriodCounters;
use super::super::interval::logic_active_this_tick;
use super::super::spawn::{corpse_color_from_phenotype, spawn_corpse_entity};
use super::super::world::{SimConfigResource, SimTick};
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

/// Bevy system: despawn agents that died this tick; optionally leave corpse markers (G7).
pub fn death_and_despawn(
    tick: Res<SimTick>,
    mut commands: Commands,
    config: Res<SimConfigResource>,
    mut counters: ResMut<MetricsPeriodCounters>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<AgentColorMaterial>>,
    query: Query<(
        Entity,
        &Transform,
        &Agent,
        &Age,
        &Phenotype,
        &Energy,
        &Health,
        &Lineage,
        Option<&PendingDeath>,
    )>,
) {
    if !logic_active_this_tick(tick.tick, &config.0.run) {
        return;
    }
    let env = &config.0.environment;
    let feeding = &env.feeding;
    let corpses_on = env.corpses_enabled && config.0.genetics.genome_version == 2;

    for (entity, transform, agent, age, phenotype, energy, health, lineage, pending) in &query {
        let _ = age;
        let reason = pending
            .map(|p| p.0)
            .or_else(|| death_reason_from_vitals(energy, health));

        if let Some(reason) = reason {
            counters.record_death(reason);
            if corpses_on {
                let ttl = feeding.corpse_ttl_ticks;
                spawn_corpse_entity(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    transform.translation.truncate(),
                    agent.radius,
                    corpse_color_from_phenotype(&phenotype.0),
                    lineage.id,
                    Corpse {
                        remaining_energy: corpse_energy_from_max_health(health.core_max_hp),
                        radius: agent.radius,
                        ticks_remaining: if ttl > 0 { ttl } else { u32::MAX },
                    },
                );
            }
            commands.entity(entity).despawn_recursive();
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
        let energy = Energy::new_v2(0.0, 100.0);
        let health = Health {
            core_hp: 50.0,
            core_max_hp: 100.0,
            part_hp: vec![],
        };
        assert_eq!(death_reason_from_vitals(&energy, &health), None);
    }

    #[test]
    fn core_destroyed_triggers_death() {
        let energy = Energy::new_v2(10.0, 100.0);
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
        let energy = Energy::new_v2(1.0, 100.0);
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
    fn no_overlap_or_tick_despawn_outside_death_system() {
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
    fn negative_energy_field_is_not_fatal_if_hp_positive() {
        let energy = Energy::new_v2(-0.01, 100.0);
        let health = Health {
            core_hp: 100.0,
            core_max_hp: 100.0,
            part_hp: vec![],
        };
        assert_eq!(death_reason_from_vitals(&energy, &health), None);
    }

    /// Living agents spawn a black [`AgentOutline`] child; plain `despawn` orphans it in Bevy 0.15.
    #[test]
    fn death_uses_recursive_despawn_for_outline_children() {
        let src = production_src_without_tests(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/sim/systems/death.rs"
        ));
        assert!(
            src.contains("despawn_recursive"),
            "death must despawn the outline child mesh, not leave a black ghost ring"
        );
    }
}

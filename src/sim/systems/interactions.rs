//! Hazard zone damage (Phase 4B; entity overlap does not damage HP).

use bevy::prelude::*;
use std::collections::HashMap;

use super::super::components::{Agent, HazardExposure, Health, Phenotype, Velocity};
use super::super::interval::{logic_active_this_tick, logic_elapsed_ticks};
use super::super::world::{SimConfigResource, SimTick};
use crate::genome::{mitigated_damage, PhenotypeV2};

const K_MITIGATE: f32 = 0.5;

/// Hazard damage mitigation from armor gene (`h / (h + k_mitigate)`).
pub fn v2_damage_mitigation(phenotype: &PhenotypeV2, stroke_max: f32) -> f32 {
    let armor_t = if stroke_max > 0.0 {
        (phenotype.armor_stroke / stroke_max).clamp(0.0, 1.0)
    } else {
        0.0
    };
    armor_t / (armor_t + K_MITIGATE)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HazardSnapshot {
    entity: Entity,
    mitigation: f32,
    hazard_raw: f32,
}

/// Accumulate per-entity damage from hazard exposure only (pure logic for tests).
pub(crate) fn accumulate_hazard_damage(agents: &[HazardSnapshot]) -> HashMap<Entity, f32> {
    let mut damage = HashMap::new();

    for agent in agents {
        let hazard_applied = mitigated_damage(agent.hazard_raw, agent.mitigation);
        if hazard_applied > 0.0 {
            *damage.entry(agent.entity).or_default() += hazard_applied;
        }
    }

    damage
}

/// Bevy system: apply hazard exposure damage after movement.
pub fn interactions(
    tick: Res<SimTick>,
    config: Res<SimConfigResource>,
    mut query: Query<(
        Entity,
        &Transform,
        &Agent,
        &Velocity,
        &Phenotype,
        &HazardExposure,
        &mut Health,
    )>,
) {
    let run = &config.0.run;
    if !logic_active_this_tick(tick.tick, run) {
        return;
    }
    let steps = logic_elapsed_ticks(run) as f32;

    let stroke_max = config
        .0
        .genetics
        .decode_v2
        .as_ref()
        .map(|d| d.stroke_max)
        .unwrap_or(1.0);
    let agents: Vec<HazardSnapshot> = query
        .iter()
        .map(|(entity, _transform, _agent, _velocity, phenotype, hazard, _)| HazardSnapshot {
            entity,
            mitigation: v2_damage_mitigation(&phenotype.0, stroke_max),
            hazard_raw: hazard.damage_raw,
        })
        .collect();

    let damage = accumulate_hazard_damage(&agents);

    for (entity, _, _, _, _, _, mut health) in &mut query {
        if let Some(amount) = damage.get(&entity) {
            health.apply_damage_to_core(*amount * steps);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::sim::components::{
        HazardExposure, LifeBundle, LightExposure, MineralExposure, VentExposure, Velocity,
    };
    use crate::sim::hazards::HazardField;
    use crate::sim::minerals::MineralField;
    use crate::sim::vents::VentField;
    use crate::sim::light::LightField;
    use crate::sim::systems::sense_exposure;
    use crate::sim::world::{SimConfigResource, SimTick, WorldBounds};

    fn snapshot(entity: Entity, mitigation: f32, hazard_raw: f32) -> HazardSnapshot {
        HazardSnapshot {
            entity,
            mitigation,
            hazard_raw,
        }
    }

    #[test]
    fn hazard_damage_applies_mitigation() {
        let mut app = App::new();
        let e1 = app.world_mut().spawn_empty().id();
        let agents = vec![snapshot(e1, 0.5, 10.0)];
        let damage = accumulate_hazard_damage(&agents);
        let applied = damage.get(&e1).copied().unwrap_or(0.0);
        assert!(applied > 0.0 && applied < 10.0);
    }

    /// EMR-F3: v2 agents overlapping at speed must not lose HP (hazard-only interactions).
    #[test]
    fn overlapping_v2_agents_do_not_lose_hp_from_collision() {
        use crate::genome::{decode_v2, v2_core, GenomeV2, V2_HEADER_LEN};

        let mut config = SimConfig::load_default().unwrap();
        config.run.logic_interval_ticks = 1;
        assert_eq!(config.genetics.genome_version, 2);
        let bounds = WorldBounds::from_world_size(config.world.width, config.world.height);
        let light = LightField::from_environment(&config.environment, &bounds);
        let hazards = HazardField::from_environment(&config.environment);
        let vents = VentField::from_environment(&config.environment);

        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        let max_speed_off = V2_HEADER_LEN + v2_core::MAX_SPEED;
        bytes[max_speed_off] = 0xFF;
        let fast_genome = GenomeV2::from_bytes(bytes, &config.genetics).unwrap();
        let phenotype = decode_v2(&fast_genome, &config).unwrap();
        let max_hp = phenotype.max_health;

        let mut app = App::new();
        app.insert_resource(SimConfigResource(config.clone()));
        app.insert_resource(SimTick::default());
        app.insert_resource(light);
        app.insert_resource(hazards.clone());
        app.insert_resource(vents.clone());
        app.insert_resource(MineralField::from_vent_field(
            &vents,
            &hazards,
            &config.environment.feeding,
            config.environment.minerals_enabled,
        ));

        let life_a = LifeBundle::from_any_parts_with_vitals(
            fast_genome.clone_genome(),
            phenotype.clone(),
            &config,
            1,
            None,
            0,
            config.economy.spawn_energy,
            max_hp,
            max_hp,
        );
        let life_b = LifeBundle::from_any_parts_with_vitals(
            fast_genome,
            phenotype,
            &config,
            2,
            None,
            0,
            config.economy.spawn_energy,
            max_hp,
            max_hp,
        );

        app.world_mut().spawn((
            life_a,
            Velocity(Vec2::new(80.0, 0.0)),
            LightExposure(0.0),
            HazardExposure::default(),
            VentExposure::default(),
            MineralExposure::default(),
            Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)),
        ));
        app.world_mut().spawn((
            life_b,
            Velocity(Vec2::ZERO),
            LightExposure(0.0),
            HazardExposure::default(),
            VentExposure::default(),
            MineralExposure::default(),
            Transform::from_translation(Vec3::new(5.0, 0.0, 0.0)),
        ));

        app.add_systems(Update, (sense_exposure, interactions).chain());
        app.update();

        let world = app.world_mut();
        let hps: Vec<f32> = world
            .query::<&Health>()
            .iter(world)
            .map(|h| h.core_hp)
            .collect();
        assert_eq!(hps.len(), 2);
        assert!(
            hps.iter().all(|hp| (*hp - max_hp).abs() < 1e-3),
            "v2 overlap at speed must not reduce core_hp (max_hp={max_hp}, hps={hps:?})"
        );
    }
}

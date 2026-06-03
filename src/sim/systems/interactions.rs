//! Hazard zone and collision damage (Phase 4B).
//!
//! Runs after movement; overlap applies symmetric collision damage from relative speed
//! (`|v_a - v_b|`). Pair iteration is sorted by [`Entity::to_bits`] for determinism.
//! O(n²) broad phase — acceptable while `performance.max_entities` ≤ 5000; upgrade to
//! spatial hash if profiling requires it.

use bevy::prelude::*;
use std::collections::HashMap;

use super::metabolism::circles_overlap;
use super::super::components::{Agent, HazardExposure, Health, Phenotype, Velocity};
use super::super::world::SimConfigResource;
use crate::genome::mitigated_damage;

#[derive(Debug, Clone, Copy)]
pub(crate) struct AgentSnapshot {
    entity: Entity,
    pos: Vec2,
    radius: f32,
    velocity: Vec2,
    mitigation: f32,
    hazard_raw: f32,
}

/// Accumulate per-entity damage from hazards and pairwise collisions (pure logic for tests).
pub(crate) fn accumulate_interaction_damage(
    agents: &mut [AgentSnapshot],
    collision_damage_scale: f32,
) -> HashMap<Entity, f32> {
    let mut damage = HashMap::new();

    for agent in agents.iter() {
        let hazard_applied = mitigated_damage(agent.hazard_raw, agent.mitigation);
        if hazard_applied > 0.0 {
            *damage.entry(agent.entity).or_default() += hazard_applied;
        }
    }

    agents.sort_by_key(|a| a.entity.to_bits());

    let n = agents.len();
    for i in 0..n {
        for j in (i + 1)..n {
            let a = &agents[i];
            let b = &agents[j];
            if !circles_overlap(a.pos, a.radius, b.pos, b.radius) {
                continue;
            }

            // Relative speed magnitude (documented choice for Phase 4).
            let relative_speed = (a.velocity - b.velocity).length();
            let raw_collision = collision_damage_scale * relative_speed;
            if raw_collision <= 0.0 {
                continue;
            }

            let dmg_a = mitigated_damage(raw_collision, a.mitigation);
            let dmg_b = mitigated_damage(raw_collision, b.mitigation);
            *damage.entry(a.entity).or_default() += dmg_a;
            *damage.entry(b.entity).or_default() += dmg_b;
        }
    }

    damage
}

/// Bevy system: hazard exposure + collision damage after movement.
pub fn interactions(
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
    let collision_scale = config.0.economy.collision_damage_scale;

    let mut agents: Vec<AgentSnapshot> = query
        .iter()
        .map(|(entity, transform, agent, velocity, phenotype, hazard, _)| AgentSnapshot {
            entity,
            pos: transform.translation.truncate(),
            radius: agent.radius,
            velocity: **velocity,
            mitigation: phenotype.0.damage_mitigation,
            hazard_raw: hazard.damage_raw,
        })
        .collect();

    let damage = accumulate_interaction_damage(&mut agents, collision_scale);

    for (entity, _, _, _, _, _, mut health) in &mut query {
        if let Some(amount) = damage.get(&entity) {
            health.apply_damage_to_core(*amount);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::genome::{decode, Genome};
    use crate::sim::components::{HazardExposure, LifeBundle, LightExposure, Velocity};
    use crate::sim::hazards::HazardField;
    use crate::sim::light::LightField;
    use crate::sim::systems::sense_exposure;
    use crate::sim::world::WorldBounds;

    fn snapshot(
        entity: Entity,
        pos: Vec2,
        radius: f32,
        velocity: Vec2,
        mitigation: f32,
        hazard_raw: f32,
    ) -> AgentSnapshot {
        AgentSnapshot {
            entity,
            pos,
            radius,
            velocity,
            mitigation,
            hazard_raw,
        }
    }

    #[test]
    fn collision_reduces_core_hp() {
        let mut app = App::new();
        let e1 = app.world_mut().spawn_empty().id();
        let e2 = app.world_mut().spawn_empty().id();
        let mut agents = vec![
            snapshot(e1, Vec2::ZERO, 10.0, Vec2::new(20.0, 0.0), 0.0, 0.0),
            snapshot(e2, Vec2::new(15.0, 0.0), 10.0, Vec2::ZERO, 0.0, 0.0),
        ];
        let damage = accumulate_interaction_damage(&mut agents, 0.15);
        assert!(damage.get(&e1).copied().unwrap_or(0.0) > 0.0);
        assert!(damage.get(&e2).copied().unwrap_or(0.0) > 0.0);

        let mut health = Health {
            core_hp: 100.0,
            core_max_hp: 100.0,
            part_hp: vec![],
        };
        health.apply_damage_to_core(damage.get(&e1).copied().unwrap_or(0.0));
        assert!(health.core_hp < 100.0);
        assert!(health.core_hp > 0.0);
    }

    #[test]
    fn deterministic_collision_same_positions() {
        let mut app = App::new();
        let e1 = app.world_mut().spawn_empty().id();
        let e2 = app.world_mut().spawn_empty().id();
        let e3 = app.world_mut().spawn_empty().id();
        let mut agents_a = vec![
            snapshot(e1, Vec2::ZERO, 10.0, Vec2::new(30.0, 0.0), 0.0, 0.0),
            snapshot(e2, Vec2::new(12.0, 0.0), 10.0, Vec2::ZERO, 0.0, 0.0),
            snapshot(e3, Vec2::new(100.0, 0.0), 10.0, Vec2::new(50.0, 0.0), 0.0, 0.0),
        ];
        let mut agents_b = agents_a.clone();

        let dmg_a = accumulate_interaction_damage(&mut agents_a, 0.15);
        let dmg_b = accumulate_interaction_damage(&mut agents_b, 0.15);
        assert_eq!(dmg_a, dmg_b);
    }

    #[test]
    fn overlapping_agents_lose_hp_without_despawn() {
        let config = SimConfig::load_default().unwrap();
        let bounds = WorldBounds::from_world_size(config.world.width, config.world.height);
        let light = LightField::from_environment(&config.environment, &bounds);
        let hazards = HazardField::from_environment(&config.environment);
        let genome = Genome::ancestor_from_config(&config).unwrap();
        let mut bytes = genome.bytes.clone();
        let i = crate::genome::HEADER_LEN + crate::genome::core::MAX_SPEED;
        bytes[i] = 0x80;
        bytes[i + 1] = 0x00;
        let fast_genome =
            Genome::from_bytes(bytes, config.genetics.genome_version, config.genetics.core_length)
                .unwrap();
        let phenotype = decode(&fast_genome, &config).unwrap();

        let mut app = App::new();
        app.insert_resource(SimConfigResource(config.clone()));
        app.insert_resource(light);
        app.insert_resource(hazards);

        let life_a = LifeBundle::from_parts(fast_genome.clone_genome(), phenotype.clone(), &config, 1, None, 0);
        let life_b = LifeBundle::from_parts(fast_genome, phenotype, &config, 2, None, 0);

        app.world_mut().spawn((
            life_a,
            Velocity(Vec2::new(40.0, 0.0)),
            LightExposure(0.0),
            HazardExposure::default(),
            Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)),
        ));
        app.world_mut().spawn((
            life_b,
            Velocity(Vec2::ZERO),
            LightExposure(0.0),
            HazardExposure::default(),
            Transform::from_translation(Vec3::new(20.0, 0.0, 0.0)),
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
        assert!(hps.iter().all(|hp| *hp < 100.0), "both should take collision damage");
        assert!(hps.iter().all(|hp| *hp > 0.0), "no despawn until death system");
        assert_eq!(world.query::<&Health>().iter(world).count(), 2);
    }
}

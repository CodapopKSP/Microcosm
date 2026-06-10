//! Passive world forces: Brownian jitter for small bodies, gravity for large ones.

use bevy::prelude::*;
use rand::Rng;

use crate::config::WorldPhysicsConfig;

use super::movement::apply_world_bounds;
use super::super::components::{Agent, Corpse, Lineage, Phenotype};
use super::super::structures::WorldStructures;
use super::super::world::{SimConfigResource, SimRng, SimTick, WorldBounds};

/// Displacement from passive physics this tick (no energy cost).
pub fn passive_physics_delta(
    body_radius: f32,
    physics: &WorldPhysicsConfig,
    dt: f32,
    rng: &mut impl Rng,
) -> Vec2 {
    if body_radius < physics.radius_threshold {
        brownian_delta(physics.brownian_speed, dt, rng)
    } else {
        Vec2::new(0.0, -physics.gravity_speed * dt)
    }
}

fn brownian_delta(speed: f32, dt: f32, rng: &mut impl Rng) -> Vec2 {
    if speed <= 0.0 || dt <= 0.0 {
        return Vec2::ZERO;
    }
    let angle = rng.gen_range(0.0..std::f32::consts::TAU);
    let magnitude = speed * dt * rng.gen::<f32>();
    Vec2::from_angle(angle) * magnitude
}

fn apply_passive_delta(
    transform: &mut Transform,
    body_radius: f32,
    hull_radius: f32,
    physics: &WorldPhysicsConfig,
    dt: f32,
    rng: &mut impl Rng,
    bounds: &WorldBounds,
    structures: &WorldStructures,
    boundary: crate::config::BoundaryMode,
) {
    let delta = passive_physics_delta(body_radius, physics, dt, rng);
    if delta.length_squared() <= 0.0 {
        return;
    }

    let mut pos = transform.translation.truncate() + delta;
    let mut vel = Vec2::ZERO;
    apply_world_bounds(&mut pos, &mut vel, hull_radius, bounds, boundary);
    structures.resolve_circle_collisions(&mut pos, &mut vel, hull_radius);
    transform.translation = pos.extend(transform.translation.z);
}

/// Apply Brownian motion or gravity based on body radius (living agents and corpses).
pub fn passive_physics(
    tick: Res<SimTick>,
    config: Res<SimConfigResource>,
    bounds: Res<WorldBounds>,
    structures: Res<WorldStructures>,
    sim_rng: Res<SimRng>,
    mut agents: Query<(&mut Transform, &Phenotype, &Agent, &Lineage)>,
    mut corpses: Query<(Entity, &mut Transform, &Corpse), Without<Agent>>,
) {
    let dt = super::super::world::SIM_DT;
    let physics = &config.0.world.physics;
    let boundary = config.0.world.boundary;
    let sim_tick = tick.tick;

    for (mut transform, phenotype, agent, lineage) in &mut agents {
        let mut rng = sim_rng.stream_physics(lineage.id, sim_tick);
        apply_passive_delta(
            &mut transform,
            phenotype.body_radius(),
            agent.radius,
            physics,
            dt,
            &mut rng,
            &bounds,
            &structures,
            boundary,
        );
    }

    for (entity, mut transform, corpse) in &mut corpses {
        let mut rng = sim_rng.stream_physics(entity.index() as u64, sim_tick);
        apply_passive_delta(
            &mut transform,
            corpse.radius,
            corpse.radius,
            physics,
            dt,
            &mut rng,
            &bounds,
            &structures,
            boundary,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BoundaryMode, WorldPhysicsConfig};
    use crate::sim::world::WorldBounds;
    use rand::SeedableRng;

    fn default_physics() -> WorldPhysicsConfig {
        WorldPhysicsConfig::default()
    }

    #[test]
    fn small_radius_gets_brownian_not_gravity() {
        let physics = default_physics();
        let dt = 1.0 / 60.0;
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let delta = passive_physics_delta(4.0, &physics, dt, &mut rng);
        assert!(delta.length() > 0.0);
        assert_ne!(delta, Vec2::new(0.0, -physics.gravity_speed * dt));
    }

    #[test]
    fn large_radius_gets_gravity_not_lateral_drift() {
        let physics = default_physics();
        let dt = 1.0 / 60.0;
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let delta = passive_physics_delta(12.0, &physics, dt, &mut rng);
        assert_eq!(delta.x, 0.0);
        assert!(delta.y < 0.0);
    }

    #[test]
    fn threshold_boundary_uses_gravity_at_five() {
        let physics = default_physics();
        let dt = 1.0 / 60.0;
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let delta = passive_physics_delta(5.0, &physics, dt, &mut rng);
        assert_eq!(delta.x, 0.0);
        assert!(delta.y < 0.0);
    }

    #[test]
    fn gravity_sinks_toward_bottom() {
        let physics = WorldPhysicsConfig {
            gravity_speed: 60.0,
            ..default_physics()
        };
        let bounds = WorldBounds::from_world_size(200.0, 200.0);
        let radius = 10.0;
        let mut pos = Vec2::new(0.0, bounds.half_y - radius);
        let mut vel = Vec2::ZERO;
        let dt = 1.0 / 60.0;

        for _ in 0..120 {
            pos.y += passive_physics_delta(radius, &physics, dt, &mut rand::thread_rng()).y;
            apply_world_bounds(
                &mut pos,
                &mut vel,
                radius,
                &bounds,
                BoundaryMode::Bounce,
            );
        }

        assert!(pos.y < bounds.half_y - radius - 1.0);
    }

    #[test]
    fn small_corpse_radius_uses_brownian_not_gravity() {
        let physics = default_physics();
        let dt = 1.0 / 60.0;
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let delta = passive_physics_delta(4.0, &physics, dt, &mut rng);
        assert!(delta.length() > 0.0);
        assert_ne!(delta, Vec2::new(0.0, -physics.gravity_speed * dt));
    }
}

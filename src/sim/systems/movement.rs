//! Agent movement: speed cap, move cost, bounds, moving metabolism (Phase 1G).
//!
//! Entity–entity overlap applies collision damage in [`super::interactions`] (Phase 4B).

use bevy::prelude::*;
use rand::Rng;

use crate::config::BoundaryMode;
use crate::genome::{
    movement_energy_for_distance, wants_to_move_this_tick, Phenotype as PhenotypeData,
};

use super::super::components::{Agent, Energy, Lineage, Phenotype, Velocity};
use super::super::world::{SimConfigResource, SimRng, SimTick, WorldBounds};

/// Speed above this counts as moving.
pub const MOVE_SPEED_EPS: f32 = 1e-3;

/// Energy spent when traveling `distance` world units (see [`movement_energy_for_distance`]).
pub fn movement_energy_cost(phenotype: &PhenotypeData, distance: f32) -> f32 {
    if distance <= MOVE_SPEED_EPS {
        return 0.0;
    }
    movement_energy_for_distance(
        distance,
        phenotype.body_radius,
        phenotype.movement_pace,
    )
}

/// Clamp velocity magnitude to `phenotype.max_speed`.
pub fn clamp_velocity_to_max_speed(velocity: Vec2, max_speed: f32) -> Vec2 {
    let max_speed = max_speed.max(0.0);
    let speed = velocity.length();
    if speed > max_speed && speed > 0.0 {
        velocity * (max_speed / speed)
    } else {
        velocity
    }
}

/// Resolve wall contact for one axis (bounce or wrap).
pub fn resolve_axis_bounds(
    pos: &mut f32,
    vel: &mut f32,
    radius: f32,
    half_extent: f32,
    boundary: BoundaryMode,
) {
    let min = -half_extent + radius;
    let max = half_extent - radius;
    if min > max {
        *pos = 0.0;
        return;
    }

    match boundary {
        BoundaryMode::Bounce => {
            if *pos < min {
                *vel = -(*vel);
                *pos = min;
            } else if *pos > max {
                *vel = -(*vel);
                *pos = max;
            }
        }
        BoundaryMode::Wrap => {
            let span = max - min;
            if span <= 0.0 {
                return;
            }
            while *pos < min {
                *pos += span;
            }
            while *pos > max {
                *pos -= span;
            }
        }
    }
}

/// Apply bounds to position and velocity.
pub fn apply_world_bounds(
    pos: &mut Vec2,
    vel: &mut Vec2,
    radius: f32,
    bounds: &WorldBounds,
    boundary: BoundaryMode,
) {
    resolve_axis_bounds(&mut pos.x, &mut vel.x, radius, bounds.half_x, boundary);
    resolve_axis_bounds(&mut pos.y, &mut vel.y, radius, bounds.half_y, boundary);
}

/// Integrate motion for one agent. Returns `false` if movement was skipped (insufficient energy).
pub fn apply_movement_tick(
    pos: &mut Vec2,
    vel: &mut Vec2,
    energy: &mut Energy,
    phenotype: &PhenotypeData,
    radius: f32,
    bounds: &WorldBounds,
    boundary: BoundaryMode,
    dt: f32,
    attempt_move: bool,
) -> bool {
    if !attempt_move {
        *vel = Vec2::ZERO;
        return true;
    }

    *vel = clamp_velocity_to_max_speed(*vel, phenotype.max_speed);
    let delta = *vel * dt;
    let distance = delta.length();

    if distance > MOVE_SPEED_EPS {
        let cost = movement_energy_cost(phenotype, distance);
        if energy.internal_energy < cost {
            return false;
        }
        energy.internal_energy = (energy.internal_energy - cost).max(0.0);
    }

    *pos += delta;
    apply_world_bounds(pos, vel, radius, bounds, boundary);
    true
}

/// Bevy system: movement after metabolism (see [SIMULATION.md](../../docs/SIMULATION.md)).
pub fn behavior_movement(
    time: Res<Time>,
    tick: Res<SimTick>,
    config: Res<SimConfigResource>,
    bounds: Res<WorldBounds>,
    sim_rng: Res<SimRng>,
    mut query: Query<(
        &mut Transform,
        &mut Velocity,
        &Phenotype,
        &mut Energy,
        &Agent,
        &Lineage,
    )>,
) {
    let dt = time.delta_secs();
    let boundary = config.0.world.boundary;
    let sim_tick = tick.tick;

    for (mut transform, mut velocity, phenotype, mut energy, agent, lineage) in &mut query {
        let mut rng = sim_rng.stream_movement(lineage.id, sim_tick);
        let attempt_move =
            wants_to_move_this_tick(phenotype.0.desire_to_move, rng.gen::<f32>());
        let mut pos = transform.translation.truncate();
        apply_movement_tick(
            &mut pos,
            &mut **velocity,
            &mut energy,
            &phenotype.0,
            agent.radius,
            &bounds,
            boundary,
            dt,
            attempt_move,
        );
        transform.translation = pos.extend(transform.translation.z);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::genome::{self, decode, movement_energy_for_distance, Genome};
    use crate::genome::Phenotype as PhenotypeData;
    use crate::sim::components::Energy;
    use crate::sim::world::WorldBounds;

    fn config() -> SimConfig {
        SimConfig::load_default().unwrap()
    }

    fn set_u16_locus(bytes: &mut [u8], core_offset: usize, raw: u16) {
        let i = genome::HEADER_LEN + core_offset;
        bytes[i] = (raw >> 8) as u8;
        bytes[i + 1] = (raw & 0xFF) as u8;
    }

    fn set_u8_locus(bytes: &mut [u8], core_offset: usize, raw: u8) {
        bytes[genome::HEADER_LEN + core_offset] = raw;
    }

    fn phenotype_with_loci_and_desire(loci: &[(usize, u16)], desire_raw: u8) -> PhenotypeData {
        let config = config();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        set_u8_locus(&mut bytes, genome::core::DESIRE_TO_MOVE, desire_raw);
        for &(off, raw) in loci {
            set_u16_locus(&mut bytes, off, raw);
        }
        let genome =
            Genome::from_bytes(bytes, config.genetics.genome_version, config.genetics.core_length)
                .unwrap();
        decode(&genome, &config).unwrap()
    }

    fn phenotype_default_movable() -> PhenotypeData {
        phenotype_with_loci_and_desire(&[], 0xFF)
    }

    #[test]
    fn movement_calibration_at_reference_radius() {
        let p = phenotype_default_movable();
        let distance = 10.0 * p.body_radius;
        let cost = movement_energy_for_distance(distance, p.body_radius, 1.0);
        assert!(
            (cost - 1.0).abs() < 1e-4,
            "120 units at r=12 should cost ~1 energy, got {cost}"
        );
    }

    #[test]
    fn faster_pace_doubles_energy_per_distance() {
        let slow = phenotype_with_loci_and_desire(&[(genome::core::MOVEMENT_PACE, 0x5555)], 0xFF);
        let fast = phenotype_with_loci_and_desire(&[(genome::core::MOVEMENT_PACE, 0xFFFF)], 0xFF);
        assert!(fast.movement_pace > slow.movement_pace);
        let d = 50.0;
        let slow_cost = movement_energy_cost(&slow, d);
        let fast_cost = movement_energy_cost(&fast, d);
        assert!((fast_cost / slow_cost - fast.movement_pace / slow.movement_pace).abs() < 0.05);
    }

    #[test]
    fn movement_deducts_distance_scaled_cost() {
        let phenotype = phenotype_default_movable();
        let dt = 1.0 / 60.0;
        let mut vel = clamp_velocity_to_max_speed(
            Vec2::new(phenotype.max_speed, 0.0),
            phenotype.max_speed,
        );
        let distance = vel.length() * dt;

        let mut energy = Energy {
            internal_energy: 10_000.0,
        };
        let mut pos = Vec2::ZERO;
        let bounds = WorldBounds::from_world_size(500.0, 500.0);
        let start = energy.internal_energy;

        apply_movement_tick(
            &mut pos,
            &mut vel,
            &mut energy,
            &phenotype,
            phenotype.body_radius,
            &bounds,
            BoundaryMode::Bounce,
            dt,
            true,
        );

        let drain = start - energy.internal_energy;
        let expected = movement_energy_cost(&phenotype, distance);
        assert!(
            (drain - expected).abs() < expected.max(1e-6) * 0.15 + 1e-6,
            "drain={drain} expected={expected}"
        );
        assert!(drain > 0.0);
    }

    #[test]
    fn zero_desire_zeros_velocity_and_skips_cost() {
        let config = config();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        set_u8_locus(&mut bytes, genome::core::DESIRE_TO_MOVE, 0);
        let phenotype = decode(
            &Genome::from_bytes(
                bytes,
                config.genetics.genome_version,
                config.genetics.core_length,
            )
            .unwrap(),
            &config,
        )
        .unwrap();
        assert_eq!(phenotype.desire_to_move, 0.0);

        let mut pos = Vec2::ZERO;
        let mut vel = Vec2::new(phenotype.max_speed, 0.0);
        let start_energy = 1000.0;
        let mut energy = Energy {
            internal_energy: start_energy,
        };
        apply_movement_tick(
            &mut pos,
            &mut vel,
            &mut energy,
            &phenotype,
            phenotype.body_radius,
            &WorldBounds::from_world_size(200.0, 200.0),
            BoundaryMode::Bounce,
            1.0 / 60.0,
            false,
        );
        assert_eq!(pos, Vec2::ZERO);
        assert_eq!(vel, Vec2::ZERO);
        assert_eq!(energy.internal_energy, start_energy);
    }

    #[test]
    fn default_ancestor_can_move_when_desire_maxed() {
        let p = phenotype_default_movable();
        assert!(p.max_speed > 0.0);
        assert!((p.movement_pace - 1.0).abs() < 0.02);
    }

    #[test]
    fn insufficient_energy_skips_movement() {
        let bounds = WorldBounds::from_world_size(200.0, 200.0);
        let phenotype = phenotype_default_movable();
        let start = Vec2::new(10.0, 5.0);
        let mut pos = start;
        let mut vel = clamp_velocity_to_max_speed(Vec2::new(50.0, 0.0), phenotype.max_speed);
        let dt = 0.1;
        let cost = movement_energy_cost(&phenotype, vel.length() * dt);
        let mut energy = Energy {
            internal_energy: cost - 0.01,
        };

        let moved = apply_movement_tick(
            &mut pos,
            &mut vel,
            &mut energy,
            &phenotype,
            phenotype.body_radius,
            &bounds,
            BoundaryMode::Bounce,
            dt,
            true,
        );

        assert!(!moved);
        assert_eq!(pos, start);
    }
}

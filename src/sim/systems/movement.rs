//! Agent movement: speed cap, move cost, bounds, moving metabolism (Phase 1G).
//!
//! Entity–entity overlap is ignored until collision damage (Phase 4).

use bevy::prelude::*;

use crate::config::BoundaryMode;
use crate::genome::Phenotype as PhenotypeData;

use super::super::components::{Agent, Energy, Phenotype, Velocity};
use super::super::world::{SimConfigResource, WorldBounds};

/// Speed above this counts as moving for [`move_cost`](PhenotypeData::move_cost).
pub const MOVE_SPEED_EPS: f32 = 1e-3;

/// Extra metabolism beyond idle already charged in [`super::metabolism::metabolism_and_gains`].
pub fn moving_metabolism_extra(phenotype: &PhenotypeData, speed: f32) -> f32 {
    (phenotype.metabolism_moving(speed) - phenotype.metabolism_idle).max(0.0)
}

/// Energy spent on movement this tick (`move_cost` + moving metabolism extra).
pub fn movement_energy_cost(phenotype: &PhenotypeData, speed: f32) -> f32 {
    if speed <= MOVE_SPEED_EPS {
        return 0.0;
    }
    phenotype.move_cost + moving_metabolism_extra(phenotype, speed)
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
) -> bool {
    *vel = clamp_velocity_to_max_speed(*vel, phenotype.max_speed);
    let speed = vel.length();

    if speed > MOVE_SPEED_EPS {
        let cost = movement_energy_cost(phenotype, speed);
        if energy.internal_energy < phenotype.move_cost {
            return false;
        }
        energy.internal_energy = (energy.internal_energy - cost).max(0.0);
    }

    *pos += *vel * dt;
    apply_world_bounds(pos, vel, radius, bounds, boundary);
    true
}

/// Bevy system: movement after metabolism (see [SIMULATION.md](../../docs/SIMULATION.md)).
pub fn behavior_movement(
    time: Res<Time>,
    config: Res<SimConfigResource>,
    bounds: Res<WorldBounds>,
    mut query: Query<(
        &mut Transform,
        &mut Velocity,
        &Phenotype,
        &mut Energy,
        &Agent,
    )>,
) {
    let dt = time.delta_secs();
    let boundary = config.0.world.boundary;

    for (mut transform, mut velocity, phenotype, mut energy, agent) in &mut query {
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
        );
        transform.translation = pos.extend(transform.translation.z);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::genome::{self, Genome};
    use crate::genome::{decode, Phenotype as PhenotypeData};
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

    fn phenotype_with_locomotion_raw(raw: u16) -> PhenotypeData {
        let config = config();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        set_u16_locus(&mut bytes, genome::core::LOCOMOTION_INVESTMENT, raw);
        set_u16_locus(&mut bytes, genome::core::MAX_SPEED, 0x8000);
        let genome =
            Genome::from_bytes(bytes, config.genetics.genome_version, config.genetics.core_length)
                .unwrap();
        decode(&genome, &config).unwrap()
    }

    #[test]
    fn high_locomotion_drains_more_energy_while_moving() {
        let bounds = WorldBounds::from_world_size(200.0, 200.0);
        let low = phenotype_with_locomotion_raw(0x1000);
        let high = phenotype_with_locomotion_raw(0xE000);
        let speed = high.max_speed.max(low.max_speed) * 0.5;
        assert!(speed > MOVE_SPEED_EPS);

        let mut low_energy = Energy {
            internal_energy: 10_000.0,
        };
        let mut high_energy = Energy {
            internal_energy: 10_000.0,
        };
        let mut pos = Vec2::ZERO;
        let mut low_vel = Vec2::new(speed, 0.0);
        let mut high_vel = Vec2::new(speed, 0.0);

        apply_movement_tick(
            &mut pos,
            &mut low_vel,
            &mut low_energy,
            &low,
            low.body_radius,
            &bounds,
            BoundaryMode::Bounce,
            0.0,
        );
        let low_drain = 10_000.0 - low_energy.internal_energy;

        apply_movement_tick(
            &mut pos,
            &mut high_vel,
            &mut high_energy,
            &high,
            high.body_radius,
            &bounds,
            BoundaryMode::Bounce,
            0.0,
        );
        let high_drain = 10_000.0 - high_energy.internal_energy;

        assert!(
            high_drain > low_drain,
            "high locomotion should drain more per moving tick (low={low_drain}, high={high_drain})"
        );
    }

    #[test]
    fn movement_deducts_move_cost_and_moving_metabolism() {
        let phenotype = phenotype_with_locomotion_raw(0x8000);
        let mut vel = clamp_velocity_to_max_speed(
            Vec2::new(phenotype.max_speed * 0.8, 0.0),
            phenotype.max_speed,
        );
        assert!(vel.length() > MOVE_SPEED_EPS);

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
            0.0,
        );

        let drain = start - energy.internal_energy;
        let move_only = phenotype.move_cost;
        let at_full_speed = movement_energy_cost(&phenotype, phenotype.max_speed);
        assert!(
            drain > move_only,
            "moving tick should cost more than move_cost alone (drain={drain}, move_cost={move_only})"
        );
        assert!(
            drain <= at_full_speed,
            "drain {drain} should not exceed full-speed cost {at_full_speed}"
        );
    }

    #[test]
    fn high_locomotion_has_higher_move_cost() {
        let low = phenotype_with_locomotion_raw(0x1000);
        let high = phenotype_with_locomotion_raw(0xE000);
        assert!(high.locomotion_investment > low.locomotion_investment);
        assert!(high.move_cost > low.move_cost);
        assert!(
            movement_energy_cost(&high, high.max_speed * 0.5)
                > movement_energy_cost(&low, low.max_speed * 0.5)
        );
    }

    #[test]
    fn bounce_reverses_velocity_at_wall() {
        let bounds = WorldBounds::from_world_size(100.0, 100.0);
        let mut pos = Vec2::new(49.0, 0.0);
        let mut vel = Vec2::new(10.0, 0.0);
        apply_movement_tick(
            &mut pos,
            &mut vel,
            &mut Energy {
                internal_energy: 1000.0,
            },
            &phenotype_with_locomotion_raw(0x2800),
            1.0,
            &bounds,
            BoundaryMode::Bounce,
            1.0,
        );
        assert!(vel.x < 0.0);
        assert!(pos.x <= bounds.half_x - 1.0);
    }

    #[test]
    fn wrap_teleports_across_edge() {
        let bounds = WorldBounds::from_world_size(100.0, 100.0);
        let mut pos = Vec2::new(48.0, 0.0);
        let mut vel = Vec2::new(10.0, 0.0);
        let phenotype = phenotype_with_locomotion_raw(0x2800);
        apply_movement_tick(
            &mut pos,
            &mut vel,
            &mut Energy {
                internal_energy: 1000.0,
            },
            &phenotype,
            1.0,
            &bounds,
            BoundaryMode::Wrap,
            1.0,
        );
        assert!(pos.x < 0.0, "wrap should move agent to opposite side, got {}", pos.x);
    }

    #[test]
    fn insufficient_energy_skips_movement() {
        let bounds = WorldBounds::from_world_size(200.0, 200.0);
        let phenotype = phenotype_with_locomotion_raw(0x2800);
        let start = Vec2::new(10.0, 5.0);
        let mut pos = start;
        let mut vel = Vec2::new(50.0, 0.0);
        let mut energy = Energy {
            internal_energy: phenotype.move_cost - 0.01,
        };

        let moved = apply_movement_tick(
            &mut pos,
            &mut vel,
            &mut energy,
            &phenotype,
            phenotype.body_radius,
            &bounds,
            BoundaryMode::Bounce,
            0.1,
        );

        assert!(!moved);
        assert_eq!(pos, start);
    }

    #[test]
    fn velocity_clamped_to_max_speed() {
        let phenotype = phenotype_with_locomotion_raw(0x2800);
        let vel = clamp_velocity_to_max_speed(Vec2::new(10_000.0, 0.0), phenotype.max_speed);
        assert!((vel.length() - phenotype.max_speed).abs() < 1e-3);
    }
}

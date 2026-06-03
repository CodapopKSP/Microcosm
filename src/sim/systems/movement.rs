//! Agent movement: speed cap, move cost, bounds, moving metabolism (Phase 1G).
//!
//! Entity–entity overlap applies collision damage in [`super::interactions`] (Phase 4B).

use bevy::prelude::*;

use crate::config::{BoundaryMode, EconomyConfig};
use crate::genome::{movement_energy_per_tick, Phenotype as PhenotypeData};

use super::super::components::{Agent, Energy, Phenotype, Velocity};
use super::super::world::{SimConfigResource, WorldBounds};

/// Speed above this counts as moving.
pub const MOVE_SPEED_EPS: f32 = 1e-3;

/// Energy spent on movement this tick (size- and speed-scaled; see [`movement_energy_per_tick`]).
pub fn movement_energy_cost(
    phenotype: &PhenotypeData,
    speed: f32,
    economy: &EconomyConfig,
) -> f32 {
    if speed <= MOVE_SPEED_EPS {
        return 0.0;
    }
    movement_energy_per_tick(
        phenotype.photo_area_scale,
        phenotype.speed_ratio(speed),
        economy,
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
    economy: &EconomyConfig,
    dt: f32,
) -> bool {
    *vel = clamp_velocity_to_max_speed(*vel, phenotype.max_speed);
    let speed = vel.length();

    if speed > MOVE_SPEED_EPS {
        let cost = movement_energy_cost(phenotype, speed, economy);
        if energy.internal_energy < cost {
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
    let economy = &config.0.economy;

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
            economy,
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
    fn ancestor_movement_costs_one_energy_per_sec_at_half_speed() {
        let config = config();
        let phenotype = decode(
            &Genome::ancestor_from_config(&config).unwrap(),
            &config,
        )
        .unwrap();
        assert!(phenotype.max_speed > 0.0);

        let speed = phenotype.max_speed * 0.5;
        let per_sec = movement_energy_cost(&phenotype, speed, &config.economy) * 60.0;
        assert!(
            (per_sec - config.economy.move_energy_per_sec_at_half_speed).abs() < 0.15,
            "ancestor at half max speed should cost ~1 energy/s, got {per_sec}"
        );
    }

    #[test]
    fn smaller_body_pays_less_move_energy_at_same_speed_ratio() {
        let config = config();
        let mut small_bytes = config.genetics.parse_ancestor_bytes().unwrap();
        small_bytes[genome::HEADER_LEN + genome::core::BODY_RADIUS] = 0;
        let small = decode(
            &Genome::from_bytes(
                small_bytes,
                config.genetics.genome_version,
                config.genetics.core_length,
            )
            .unwrap(),
            &config,
        )
        .unwrap();
        let large = phenotype_with_locomotion_raw(0x8000);
        let speed_ratio = 0.5;
        let small_cost =
            movement_energy_cost(&small, small.max_speed * speed_ratio, &config.economy);
        let large_cost =
            movement_energy_cost(&large, large.max_speed * speed_ratio, &config.economy);
        assert!(small_cost < large_cost);
        assert!(small.photo_area_scale < large.photo_area_scale);
    }

    #[test]
    fn movement_deducts_scaled_cost_while_moving() {
        let config = config();
        let phenotype = decode(
            &Genome::ancestor_from_config(&config).unwrap(),
            &config,
        )
        .unwrap();
        let half_ratio = config.economy.move_energy_half_speed_ratio;
        let mut vel = clamp_velocity_to_max_speed(
            Vec2::new(phenotype.max_speed * half_ratio, 0.0),
            phenotype.max_speed,
        );

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
            &config.economy,
            0.0,
        );

        let drain = start - energy.internal_energy;
        assert!(
            (drain - phenotype.move_cost).abs() < 1e-4,
            "drain={drain} move_cost={}",
            phenotype.move_cost
        );
        assert!(drain > 0.0);
    }

    #[test]
    fn high_locomotion_increases_max_speed() {
        let low = phenotype_with_locomotion_raw(0x1000);
        let high = phenotype_with_locomotion_raw(0xE000);
        assert!(high.locomotion_investment > low.locomotion_investment);
        assert!(high.max_speed > low.max_speed);
    }

    #[test]
    fn bounce_reverses_velocity_at_wall() {
        let bounds = WorldBounds::from_world_size(100.0, 100.0);
        let mut pos = Vec2::new(49.0, 0.0);
        let mut vel = Vec2::new(10.0, 0.0);
        let config = config();
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
            &config.economy,
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
        let config = config();
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
            &config.economy,
            1.0,
        );
        assert!(pos.x < 0.0, "wrap should move agent to opposite side, got {}", pos.x);
    }

    #[test]
    fn insufficient_energy_skips_movement() {
        let config = config();
        let bounds = WorldBounds::from_world_size(200.0, 200.0);
        let phenotype = phenotype_with_locomotion_raw(0x2800);
        let start = Vec2::new(10.0, 5.0);
        let mut pos = start;
        let mut vel = Vec2::new(50.0, 0.0);
        let cost = movement_energy_cost(&phenotype, vel.length(), &config.economy);
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
            &config.economy,
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

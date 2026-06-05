//! Agent movement helpers: speed cap and world bounds.

use bevy::prelude::*;

use crate::config::BoundaryMode;

use super::super::world::WorldBounds;

/// Speed above this counts as moving.
pub const MOVE_SPEED_EPS: f32 = 1e-3;

/// Clamp velocity magnitude to `max_speed`.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_velocity_respects_max_speed() {
        let vel = clamp_velocity_to_max_speed(Vec2::new(100.0, 0.0), 30.0);
        assert!((vel.length() - 30.0).abs() < 1e-5);
    }

    #[test]
    fn bounce_reflects_at_wall() {
        let bounds = WorldBounds::from_world_size(200.0, 200.0);
        let radius = 5.0;
        let max_x = bounds.half_x - radius;
        let mut pos = Vec2::new(max_x + 1.0, 0.0);
        let mut vel = Vec2::new(10.0, 0.0);
        apply_world_bounds(&mut pos, &mut vel, radius, &bounds, BoundaryMode::Bounce);
        assert!(pos.x <= max_x + 1e-3);
        assert!(vel.x < 0.0);
    }
}

//! Light field sampling (Phase 2B).
//!
//! Samples are unitless multipliers in `[gradient_min, gradient_max]`. Global irradiance
//! scale is [`EnvironmentConfig::solar_constant`](crate::config::EnvironmentConfig::solar_constant)
//! applied in metabolism (Phase 2D).

use bevy::prelude::*;

use crate::config::{EnvironmentConfig, GradientBrightAt, LightMode};
use crate::sim::world::WorldBounds;

/// Static light field built from config and world bounds (centered origin).
#[derive(Debug, Clone, Resource)]
pub struct LightField {
    mode: LightMode,
    /// Level used for [`LightMode::Uniform`] (defaults to `gradient_max` from config).
    uniform_level: f32,
    gradient_min: f32,
    gradient_max: f32,
    bright_at: GradientBrightAt,
    half_y: f32,
}

impl LightField {
    pub fn from_environment(env: &EnvironmentConfig, bounds: &WorldBounds) -> Self {
        Self {
            mode: env.light_mode,
            uniform_level: env.gradient_max,
            gradient_min: env.gradient_min,
            gradient_max: env.gradient_max,
            bright_at: env.gradient_bright_at,
            half_y: bounds.half_y,
        }
    }

    /// Light multiplier at world position (same space as agent `Transform` translation XY).
    pub fn sample(&self, pos: Vec2) -> f32 {
        match self.mode {
            LightMode::Uniform => self.uniform_level,
            LightMode::GradientY => self.sample_gradient_y(pos.y),
        }
    }

    fn sample_gradient_y(&self, y: f32) -> f32 {
        let span = 2.0 * self.half_y;
        if span <= 0.0 {
            return self.gradient_min;
        }

        // y = -half_y → t = 0 (bottom edge), y = +half_y → t = 1 (top edge).
        let t = ((y + self.half_y) / span).clamp(0.0, 1.0);
        let t = match self.bright_at {
            GradientBrightAt::Top => t,
            GradientBrightAt::Bottom => 1.0 - t,
        };
        self.gradient_min + t * (self.gradient_max - self.gradient_min)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;

    fn light_field_with_mode(light_mode: LightMode) -> LightField {
        let config = SimConfig::load_default().unwrap();
        let bounds = WorldBounds::from_world_size(config.world.width, config.world.height);
        let mut env = config.environment.clone();
        env.light_mode = light_mode;
        LightField::from_environment(&env, &bounds)
    }

    #[test]
    fn uniform_is_constant_everywhere() {
        let field = light_field_with_mode(LightMode::Uniform);
        let config = SimConfig::load_default().unwrap();
        let expected = config.environment.gradient_max;
        let corners = [
            Vec2::new(-450.0, -350.0),
            Vec2::new(450.0, 350.0),
            Vec2::ZERO,
        ];
        for pos in corners {
            assert_eq!(field.sample(pos), expected);
        }
    }

    #[test]
    fn light_gradient_top_brighter_than_bottom() {
        let field = light_field_with_mode(LightMode::GradientY);
        let config = SimConfig::load_default().unwrap();
        let bounds = WorldBounds::from_world_size(config.world.width, config.world.height);
        let top = Vec2::new(0.0, bounds.half_y);
        let bottom = Vec2::new(0.0, -bounds.half_y);
        assert!(
            field.sample(top) > field.sample(bottom),
            "top {} should exceed bottom {}",
            field.sample(top),
            field.sample(bottom)
        );
        assert_eq!(field.sample(top), config.environment.gradient_max);
        assert_eq!(field.sample(bottom), config.environment.gradient_min);
    }

    #[test]
    fn light_gradient_y_corners_match_vertical_extremes() {
        let field = light_field_with_mode(LightMode::GradientY);
        let config = SimConfig::load_default().unwrap();
        let bounds = WorldBounds::from_world_size(config.world.width, config.world.height);
        let hx = bounds.half_x;
        let hy = bounds.half_y;
        let top_center = Vec2::new(0.0, hy);
        let top_left = Vec2::new(-hx, hy);
        let top_right = Vec2::new(hx, hy);
        let bottom_center = Vec2::new(0.0, -hy);
        let bottom_left = Vec2::new(-hx, -hy);
        let bottom_right = Vec2::new(hx, -hy);

        assert_eq!(field.sample(top_left), field.sample(top_center));
        assert_eq!(field.sample(top_right), field.sample(top_center));
        assert_eq!(field.sample(bottom_left), field.sample(bottom_center));
        assert_eq!(field.sample(bottom_right), field.sample(bottom_center));
        assert!(
            field.sample(top_left) > field.sample(bottom_left),
            "bright corners should exceed shaded corners"
        );
        assert_eq!(field.sample(top_center), config.environment.gradient_max);
        assert_eq!(field.sample(bottom_center), config.environment.gradient_min);
    }

    #[test]
    fn gradient_y_brighter_at_bottom_when_configured() {
        let config = SimConfig::load_default().unwrap();
        let bounds = WorldBounds::from_world_size(config.world.width, config.world.height);
        let mut env = config.environment.clone();
        env.light_mode = LightMode::GradientY;
        env.gradient_bright_at = GradientBrightAt::Bottom;
        let field = LightField::from_environment(&env, &bounds);
        let top = Vec2::new(0.0, bounds.half_y);
        let bottom = Vec2::new(0.0, -bounds.half_y);
        assert!(field.sample(bottom) > field.sample(top));
    }

    #[test]
    fn gradient_y_is_linear_midpoint() {
        let field = light_field_with_mode(LightMode::GradientY);
        let config = SimConfig::load_default().unwrap();
        let mid = field.sample(Vec2::ZERO);
        let expected =
            (config.environment.gradient_min + config.environment.gradient_max) / 2.0;
        assert!((mid - expected).abs() < 1e-5);
    }

    #[test]
    fn sample_clamps_y_outside_bounds() {
        let field = light_field_with_mode(LightMode::GradientY);
        let config = SimConfig::load_default().unwrap();
        let bounds = WorldBounds::from_world_size(config.world.width, config.world.height);
        let above = Vec2::new(0.0, bounds.half_y * 2.0);
        let top = Vec2::new(0.0, bounds.half_y);
        assert_eq!(field.sample(above), field.sample(top));
    }

}
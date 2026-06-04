//! Mineral concentration field for v2 signatures and feeding (G7).
//!
//! Until a dedicated minerals grid exists, overlapping vent flux is mapped to
//! local concentration ([CONFIG.md](../../docs/CONFIG.md) `environment.feeding`).

use bevy::prelude::*;

use crate::config::{EnvironmentConfig, FeedingConfig, VentZone};
use crate::sim::vents::VentField;

/// Static mineral field (vent bridge or future grid).
#[derive(Debug, Clone, Resource)]
pub struct MineralField {
    enabled: bool,
    zones: Vec<VentZone>,
    vent_flux_scale: f32,
}

impl MineralField {
    pub fn from_environment(env: &EnvironmentConfig) -> Self {
        Self {
            enabled: env.minerals_enabled,
            zones: env.vents.clone(),
            vent_flux_scale: env.feeding.minerals_vent_flux_scale,
        }
    }

    pub fn from_vent_field(vents: &VentField, feeding: &FeedingConfig, enabled: bool) -> Self {
        Self {
            enabled,
            zones: vents.zones().to_vec(),
            vent_flux_scale: feeding.minerals_vent_flux_scale,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Unitless concentration in `[0, 1]` at world position.
    pub fn concentration_at(&self, pos: Vec2) -> f32 {
        if !self.enabled {
            return 0.0;
        }
        let flux: f32 = self
            .zones
            .iter()
            .filter(|zone| Self::overlaps(zone, pos))
            .map(|zone| zone.vent_flux)
            .sum();
        if self.vent_flux_scale <= 0.0 {
            0.0
        } else {
            (flux * self.vent_flux_scale).clamp(0.0, 1.0)
        }
    }

    /// Band target for signature `0x0201` (low 16 bits, full-scale = concentration 1.0).
    pub fn minerals_raw_at(&self, pos: Vec2) -> u16 {
        (self.concentration_at(pos) * 65535.0).round() as u16
    }

    fn overlaps(zone: &VentZone, pos: Vec2) -> bool {
        let center = Vec2::from_array(zone.center);
        pos.distance(center) <= zone.radius
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::VentZone;

    #[test]
    fn vent_flux_maps_to_mineral_concentration() {
        let field = MineralField {
            enabled: true,
            zones: vec![VentZone {
                center: [0.0, 0.0],
                radius: 100.0,
                vent_flux: 0.04,
            }],
            vent_flux_scale: 25.0,
        };
        assert!((field.concentration_at(Vec2::ZERO) - 1.0).abs() < 1e-5);
        assert_eq!(field.concentration_at(Vec2::new(500.0, 0.0)), 0.0);
        assert_eq!(field.minerals_raw_at(Vec2::ZERO), 65535);
    }

    #[test]
    fn disabled_field_is_zero() {
        let field = MineralField {
            enabled: false,
            zones: vec![VentZone {
                center: [0.0, 0.0],
                radius: 100.0,
                vent_flux: 1.0,
            }],
            vent_flux_scale: 1.0,
        };
        assert_eq!(field.concentration_at(Vec2::ZERO), 0.0);
    }
}

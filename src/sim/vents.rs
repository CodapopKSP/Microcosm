//! Circular thermal vent zones (Phase 5A).
//!
//! Flux values are raw per-tick samples before phenotype scaling (applied in metabolism, Phase 5B).

use bevy::prelude::*;

use crate::config::{EnvironmentConfig, VentZone};

/// Static vent field built from config at startup.
#[derive(Debug, Clone, Resource)]
pub struct VentField {
    zones: Vec<VentZone>,
}

impl VentField {
    pub fn from_environment(env: &EnvironmentConfig) -> Self {
        Self {
            zones: env.vents.clone(),
        }
    }

    /// Sum of `vent_flux` for all zones overlapping `pos` (world XY).
    pub fn flux_at(&self, pos: Vec2) -> f32 {
        self.zones
            .iter()
            .filter(|zone| Self::overlaps(zone, pos))
            .map(|zone| zone.vent_flux)
            .sum()
    }

    /// Configured vent zones (for debug overlay and tests).
    pub fn zones(&self) -> &[VentZone] {
        &self.zones
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

    fn field_with_zones(zones: Vec<VentZone>) -> VentField {
        VentField { zones }
    }

    #[test]
    fn point_inside_vent_has_positive_flux() {
        let field = field_with_zones(vec![VentZone {
            center: [0.0, -450.0],
            radius: 150.0,
            vent_flux: 0.032,
        }]);
        assert!(field.flux_at(Vec2::new(0.0, -450.0)) > 0.0);
        assert_eq!(field.flux_at(Vec2::new(0.0, -450.0)), 0.032);
    }

    #[test]
    fn point_outside_vent_has_zero_flux() {
        let field = field_with_zones(vec![VentZone {
            center: [0.0, -450.0],
            radius: 150.0,
            vent_flux: 0.032,
        }]);
        assert_eq!(field.flux_at(Vec2::new(500.0, 500.0)), 0.0);
    }

    #[test]
    fn overlapping_vents_sum_flux() {
        let field = field_with_zones(vec![
            VentZone {
                center: [0.0, 0.0],
                radius: 100.0,
                vent_flux: 0.01,
            },
            VentZone {
                center: [50.0, 0.0],
                radius: 100.0,
                vent_flux: 0.02,
            },
        ]);
        assert_eq!(field.flux_at(Vec2::ZERO), 0.03);
    }

    #[test]
    fn empty_field_always_zero() {
        let field = field_with_zones(vec![]);
        assert_eq!(field.flux_at(Vec2::new(0.0, -450.0)), 0.0);
    }
}

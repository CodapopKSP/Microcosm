//! Mineral concentration field for v2 signatures and feeding (G7).
//!
//! Until a dedicated minerals grid exists, overlapping vent flux and hazard chem flux
//! are mapped to local concentration ([CONFIG.md](../../docs/CONFIG.md) `environment.feeding`).

use bevy::prelude::*;

use crate::config::{EnvironmentConfig, FeedingConfig, HazardZone, VentZone};
use crate::sim::hazards::HazardField;
use crate::sim::vents::VentField;

/// Static mineral field (vent + hazard bridge or future grid).
#[derive(Debug, Clone, Resource)]
pub struct MineralField {
    vents_enabled: bool,
    vent_zones: Vec<VentZone>,
    hazard_zones: Vec<HazardZone>,
    vent_flux_scale: f32,
}

impl MineralField {
    pub fn from_environment(env: &EnvironmentConfig) -> Self {
        Self {
            vents_enabled: env.minerals_enabled,
            vent_zones: env.vents.clone(),
            hazard_zones: env.hazards.clone(),
            vent_flux_scale: env.feeding.minerals_vent_flux_scale,
        }
    }

    pub fn from_vent_field(
        vents: &VentField,
        hazards: &HazardField,
        feeding: &FeedingConfig,
        vents_enabled: bool,
    ) -> Self {
        Self {
            vents_enabled,
            vent_zones: vents.zones().to_vec(),
            hazard_zones: hazards.zones().to_vec(),
            vent_flux_scale: feeding.minerals_vent_flux_scale,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.vents_enabled
    }

    /// Unitless concentration in `[0, 1]` at world position.
    pub fn concentration_at(&self, pos: Vec2) -> f32 {
        let vent_flux: f32 = if self.vents_enabled {
            self.vent_zones
                .iter()
                .filter(|zone| Self::overlaps_vent(zone, pos))
                .map(|zone| zone.vent_flux)
                .sum()
        } else {
            0.0
        };
        let hazard_flux: f32 = self
            .hazard_zones
            .iter()
            .filter(|zone| Self::overlaps_hazard(zone, pos))
            .map(|zone| zone.chem_flux)
            .sum();
        let flux = vent_flux + hazard_flux;
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

    fn overlaps_vent(zone: &VentZone, pos: Vec2) -> bool {
        let center = Vec2::from_array(zone.center);
        pos.distance(center) <= zone.radius
    }

    fn overlaps_hazard(zone: &HazardZone, pos: Vec2) -> bool {
        let center = Vec2::from_array(zone.center);
        pos.distance(center) <= zone.radius
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HazardZone, VentZone};

    #[test]
    fn vent_flux_maps_to_mineral_concentration() {
        let field = MineralField {
            vents_enabled: true,
            vent_zones: vec![VentZone {
                center: [0.0, 0.0],
                radius: 100.0,
                vent_flux: 0.04,
            }],
            hazard_zones: vec![],
            vent_flux_scale: 25.0,
        };
        assert!((field.concentration_at(Vec2::ZERO) - 1.0).abs() < 1e-5);
        assert_eq!(field.concentration_at(Vec2::new(500.0, 0.0)), 0.0);
        assert_eq!(field.minerals_raw_at(Vec2::ZERO), 65535);
    }

    #[test]
    fn hazard_chem_flux_maps_to_mineral_concentration() {
        let field = MineralField {
            vents_enabled: false,
            vent_zones: vec![],
            hazard_zones: vec![HazardZone {
                center: [100.0, 200.0],
                radius: 50.0,
                damage_per_tick: 0.01,
                chem_flux: 0.04,
            }],
            vent_flux_scale: 25.0,
        };
        assert!((field.concentration_at(Vec2::new(100.0, 200.0)) - 1.0).abs() < 1e-5);
        assert_eq!(field.concentration_at(Vec2::ZERO), 0.0);
    }

    #[test]
    fn disabled_vents_still_allow_hazard_chem() {
        let field = MineralField {
            vents_enabled: false,
            vent_zones: vec![VentZone {
                center: [0.0, 0.0],
                radius: 100.0,
                vent_flux: 1.0,
            }],
            hazard_zones: vec![HazardZone {
                center: [0.0, 0.0],
                radius: 100.0,
                damage_per_tick: 0.01,
                chem_flux: 0.04,
            }],
            vent_flux_scale: 25.0,
        };
        assert!((field.concentration_at(Vec2::ZERO) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn disabled_vents_without_hazard_chem_is_zero() {
        let field = MineralField {
            vents_enabled: false,
            vent_zones: vec![VentZone {
                center: [0.0, 0.0],
                radius: 100.0,
                vent_flux: 1.0,
            }],
            hazard_zones: vec![],
            vent_flux_scale: 1.0,
        };
        assert_eq!(field.concentration_at(Vec2::ZERO), 0.0);
    }
}

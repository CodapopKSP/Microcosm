//! Circular hazard zones (Phase 4A).
//!
//! Damage rates are raw per-tick values before armor mitigation (applied in `interactions`, Phase 4B).

use bevy::prelude::*;

use crate::config::{EnvironmentConfig, HazardZone};

/// Static hazard field built from config at startup.
#[derive(Debug, Clone, Resource)]
pub struct HazardField {
    zones: Vec<HazardZone>,
}

impl HazardField {
    pub fn from_environment(env: &EnvironmentConfig) -> Self {
        Self {
            zones: env.hazards.clone(),
        }
    }

    /// Sum of `damage_per_tick` for all zones overlapping `pos` (world XY).
    pub fn damage_at(&self, pos: Vec2) -> f32 {
        self.zones
            .iter()
            .filter(|zone| Self::overlaps(zone, pos))
            .map(|zone| zone.damage_per_tick)
            .sum()
    }

    /// Sum of `chem_flux` for all zones overlapping `pos` (world XY).
    pub fn chem_flux_at(&self, pos: Vec2) -> f32 {
        self.zones
            .iter()
            .filter(|zone| Self::overlaps(zone, pos))
            .map(|zone| zone.chem_flux)
            .sum()
    }

    /// Configured hazard zones (for debug overlay and tests).
    pub fn zones(&self) -> &[HazardZone] {
        &self.zones
    }

    fn overlaps(zone: &HazardZone, pos: Vec2) -> bool {
        let center = Vec2::from_array(zone.center);
        pos.distance(center) <= zone.radius
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HazardZone;

    fn field_with_zones(zones: Vec<HazardZone>) -> HazardField {
        HazardField { zones }
    }

    #[test]
    fn point_inside_zone_has_positive_damage() {
        let field = field_with_zones(vec![HazardZone {
            center: [0.0, -400.0],
            radius: 180.0,
            damage_per_tick: 2.5,
            chem_flux: 0.0,
        }]);
        assert!(field.damage_at(Vec2::new(0.0, -400.0)) > 0.0);
        assert_eq!(field.damage_at(Vec2::new(0.0, -400.0)), 2.5);
    }

    #[test]
    fn point_outside_zone_has_zero_damage() {
        let field = field_with_zones(vec![HazardZone {
            center: [0.0, -400.0],
            radius: 180.0,
            damage_per_tick: 2.5,
            chem_flux: 0.0,
        }]);
        assert_eq!(field.damage_at(Vec2::new(500.0, 500.0)), 0.0);
    }

    #[test]
    fn overlapping_zones_sum_damage() {
        let field = field_with_zones(vec![
            HazardZone {
                center: [0.0, 0.0],
                radius: 100.0,
                damage_per_tick: 1.0,
                chem_flux: 0.0,
            },
            HazardZone {
                center: [50.0, 0.0],
                radius: 100.0,
                damage_per_tick: 2.0,
                chem_flux: 0.0,
            },
        ]);
        // Origin is inside both circles.
        assert_eq!(field.damage_at(Vec2::ZERO), 3.0);
    }

    #[test]
    fn empty_field_always_zero() {
        let field = field_with_zones(vec![]);
        assert_eq!(field.damage_at(Vec2::new(0.0, -400.0)), 0.0);
        assert_eq!(field.chem_flux_at(Vec2::new(0.0, -400.0)), 0.0);
    }

    #[test]
    fn point_inside_zone_has_positive_chem_flux() {
        let field = field_with_zones(vec![HazardZone {
            center: [200.0, 600.0],
            radius: 20.0,
            damage_per_tick: 0.01,
            chem_flux: 0.032,
        }]);
        assert_eq!(field.chem_flux_at(Vec2::new(200.0, 600.0)), 0.032);
        assert_eq!(field.chem_flux_at(Vec2::new(500.0, 500.0)), 0.0);
    }
}

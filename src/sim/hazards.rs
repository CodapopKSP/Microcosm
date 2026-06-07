//! Circular hazard zones (Phase 4A) with optional ephemeral patches near the map top.
//!
//! Damage rates are raw per-tick values before armor mitigation (applied in `interactions`, Phase 4B).

use bevy::prelude::*;
use rand::Rng;

use crate::config::{DynamicHazardsConfig, EnvironmentConfig, HazardZone};
use crate::sim::interval::interval_active_this_tick;
use crate::sim::world::WorldBounds;

#[derive(Debug, Clone)]
struct EphemeralHazard {
    zone: HazardZone,
    expires_at_tick: u64,
}

/// Hazard field: static config zones plus randomly spawned ephemeral patches.
#[derive(Debug, Clone, Resource)]
pub struct HazardField {
    static_zones: Vec<HazardZone>,
    ephemeral: Vec<EphemeralHazard>,
}

impl HazardField {
    pub fn from_environment(env: &EnvironmentConfig) -> Self {
        Self {
            static_zones: env.hazards.clone(),
            ephemeral: Vec::new(),
        }
    }

    /// Advance ephemeral hazards: expire old patches and maybe spawn new ones.
    pub fn tick_dynamic(
        &mut self,
        cfg: &DynamicHazardsConfig,
        bounds: WorldBounds,
        tick: u64,
        rng: &mut impl Rng,
    ) {
        if !cfg.enabled {
            self.ephemeral.clear();
            return;
        }

        self.ephemeral.retain(|h| tick < h.expires_at_tick);

        if !interval_active_this_tick(tick, cfg.spawn_interval_ticks, 0) {
            return;
        }
        if self.ephemeral.len() >= cfg.max_active as usize {
            return;
        }

        let margin = cfg.radius;
        let x_min = -bounds.half_x + margin;
        let x_max = bounds.half_x - margin;
        let center_x = rng.gen_range(x_min..=x_max);
        let center_y = rng.gen_range(cfg.band_y_min..=cfg.band_y_max);
        let lifetime = rng.gen_range(cfg.lifetime_min_ticks..=cfg.lifetime_max_ticks);

        self.ephemeral.push(EphemeralHazard {
            zone: HazardZone {
                center: [center_x, center_y],
                radius: cfg.radius,
                damage_per_tick: cfg.damage_per_tick,
                chem_flux: cfg.chem_flux,
            },
            expires_at_tick: tick.saturating_add(u64::from(lifetime)),
        });
    }

    /// Sum of `damage_per_tick` for all zones overlapping `pos` (world XY).
    pub fn damage_at(&self, pos: Vec2) -> f32 {
        self.all_zones()
            .filter(|zone| Self::overlaps(zone, pos))
            .map(|zone| zone.damage_per_tick)
            .sum()
    }

    /// Sum of `chem_flux` for all zones overlapping `pos` (world XY).
    pub fn chem_flux_at(&self, pos: Vec2) -> f32 {
        self.all_zones()
            .filter(|zone| Self::overlaps(zone, pos))
            .map(|zone| zone.chem_flux)
            .sum()
    }

    /// All active hazard zones (static + ephemeral) for debug overlay and mineral bridge.
    pub fn zones(&self) -> Vec<HazardZone> {
        self.all_zones().cloned().collect()
    }

    fn all_zones(&self) -> impl Iterator<Item = &HazardZone> {
        self.static_zones
            .iter()
            .chain(self.ephemeral.iter().map(|h| &h.zone))
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
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn field_with_zones(zones: Vec<HazardZone>) -> HazardField {
        HazardField {
            static_zones: zones,
            ephemeral: Vec::new(),
        }
    }

    fn enabled_dynamic_cfg() -> DynamicHazardsConfig {
        DynamicHazardsConfig {
            enabled: true,
            band_y_min: 500.0,
            band_y_max: 700.0,
            radius: 30.0,
            damage_per_tick: 0.05,
            chem_flux: 0.0,
            max_active: 4,
            spawn_interval_ticks: 60,
            lifetime_min_ticks: 120,
            lifetime_max_ticks: 240,
        }
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

    #[test]
    fn dynamic_spawn_respects_max_active() {
        let cfg = enabled_dynamic_cfg();
        let bounds = WorldBounds::from_world_size(1000.0, 1500.0);
        let mut field = field_with_zones(vec![]);
        let mut rng = StdRng::seed_from_u64(42);

        for tick in 0..600 {
            field.tick_dynamic(&cfg, bounds, tick, &mut rng);
        }
        assert!(
            !field.zones().is_empty(),
            "expected at least one ephemeral hazard"
        );
        assert!(
            field.zones().len() <= cfg.max_active as usize,
            "ephemeral count {} exceeded max_active {}",
            field.zones().len(),
            cfg.max_active
        );
        for zone in field.zones() {
            assert!(zone.center[1] >= cfg.band_y_min && zone.center[1] <= cfg.band_y_max);
            assert!((zone.center[0].abs() + zone.radius) <= bounds.half_x + 1e-3);
        }
    }

    #[test]
    fn dynamic_hazards_expire() {
        let mut cfg = enabled_dynamic_cfg();
        cfg.spawn_interval_ticks = 9999;
        cfg.lifetime_min_ticks = 10;
        cfg.lifetime_max_ticks = 10;
        cfg.max_active = 1;

        let bounds = WorldBounds::from_world_size(1000.0, 1500.0);
        let mut field = field_with_zones(vec![]);
        let mut rng = StdRng::seed_from_u64(7);

        field.tick_dynamic(&cfg, bounds, 0, &mut rng);
        assert_eq!(field.zones().len(), 1);

        field.tick_dynamic(&cfg, bounds, 10, &mut rng);
        assert!(
            field.zones().is_empty(),
            "hazard should expire after lifetime"
        );
    }
}

//! Metabolism and photosynthetic gain (v2).

use bevy::prelude::*;

use crate::config::EconomyConfig;

use super::metabolism_v2::apply_metabolism_tick_v2;

use super::super::components::{Age, Energy, Health, LightExposure, Phenotype, VentExposure};
use super::super::interval::{logic_active_this_tick, logic_elapsed_ticks};
use super::super::world::{SimConfigResource, SimTick};

/// Logarithmic age multiplier on base metabolism (1.0 at birth → cap at old age).
///
/// Rises slowly at first (~negligible in the first minute at 60 Hz), reaching
/// `age_metabolism_cap_multiplier` at `age_metabolism_cap_ticks`, scaled by the gene.
pub fn age_metabolism_multiplier(
    age_ticks: u32,
    age_metabolism_scale: f32,
    economy: &EconomyConfig,
) -> f32 {
    let cap_ticks = economy.age_metabolism_cap_ticks.max(1.0);
    let ref_ticks = economy.age_metabolism_ref_ticks.max(1.0);
    let cap_mult = economy.age_metabolism_cap_multiplier;
    let age = age_ticks as f32;
    let num = (1.0 + age / ref_ticks).ln();
    let den = (1.0 + cap_ticks / ref_ticks).ln();
    if den <= 0.0 || cap_mult <= 1.0 {
        return 1.0;
    }
    let t = (num / den).clamp(0.0, 1.0);
    let scale = age_metabolism_scale.max(0.0);
    let extra = (cap_mult - 1.0) * scale;
    1.0 + t * extra
}

/// True when two agent circles have any area overlap (touching at exactly `r1 + r2` is not overlap).
pub fn circles_overlap(pos_a: Vec2, radius_a: f32, pos_b: Vec2, radius_b: f32) -> bool {
    let r_sum = radius_a + radius_b;
    if r_sum <= 0.0 {
        return false;
    }
    pos_a.distance_squared(pos_b) < r_sum * r_sum
}

/// Bevy system: photosynthetic grant minus metabolism and upkeeps for every living agent.
pub fn metabolism_and_gains(
    tick: Res<SimTick>,
    config: Res<SimConfigResource>,
    mut query: Query<(
        &Phenotype,
        &Age,
        &LightExposure,
        &VentExposure,
        &mut Energy,
        &mut Health,
    )>,
) {
    let run = &config.0.run;
    if !logic_active_this_tick(tick.tick, run) {
        return;
    }
    let steps = logic_elapsed_ticks(run);
    let environment = &config.0.environment;
    let economy = &config.0.economy;

    for (phenotype, age, exposure, _vent_exp, mut energy, mut health) in &mut query {
        apply_metabolism_tick_v2(
            &mut energy,
            &mut health,
            &phenotype.0,
            age.ticks,
            exposure.0,
            environment,
            economy,
            &config.0,
            steps,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circles_overlap_when_interiors_intersect() {
        assert!(circles_overlap(Vec2::ZERO, 10.0, Vec2::new(15.0, 0.0), 10.0));
        assert!(!circles_overlap(Vec2::ZERO, 10.0, Vec2::new(20.0, 0.0), 10.0));
    }

    fn test_economy() -> EconomyConfig {
        let mut economy = crate::config::SimConfig::load_default()
            .expect("default config")
            .economy;
        economy.age_metabolism_cap_multiplier = 1.5;
        economy.age_metabolism_cap_ticks = 18000.0;
        economy.age_metabolism_ref_ticks = 3600.0;
        economy
    }

    #[test]
    fn age_multiplier_is_one_at_birth() {
        assert_eq!(age_metabolism_multiplier(0, 1.0, &test_economy()), 1.0);
    }

    #[test]
    fn age_multiplier_reaches_cap_at_cap_ticks() {
        let economy = test_economy();
        let mult = age_metabolism_multiplier(18000, 1.0, &economy);
        assert!((mult - 1.5).abs() < 1e-5);
    }
}

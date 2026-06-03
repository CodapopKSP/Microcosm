//! Metabolism and photosynthetic gain (Phase 2D Model A + legacy flat grant).

use bevy::prelude::*;
use std::collections::HashSet;

use crate::config::{EconomyConfig, EnergyModel, EnvironmentConfig};
use crate::genome::Phenotype as PhenotypeData;

use super::super::components::{Agent, Energy, LightExposure, Phenotype};
use super::super::world::SimConfigResource;

/// Per-tick energy drain from idle metabolism and trait upkeeps (see [PHASE_1.md](../../docs/PHASE_1.md)).
pub fn metabolism_tick_cost(phenotype: &PhenotypeData) -> f32 {
    phenotype.metabolism_idle
        + phenotype.photo_upkeep
        + phenotype.armor_upkeep
        + phenotype.chemo_upkeep
}

/// Photosynthetic gain for one tick.
///
/// Model A (`energy_model = "local"`): `photo_strength * light * solar_constant`.
/// Legacy (`legacy_flat`): `flat_energy_grant_per_tick` (Phase 1 stand-in).
pub fn photosynthetic_gain_per_tick(
    phenotype: &PhenotypeData,
    light_sample: f32,
    environment: &EnvironmentConfig,
    economy: &EconomyConfig,
    blocked_by_overlap: bool,
) -> f32 {
    if blocked_by_overlap {
        return 0.0;
    }

    match environment.energy_model {
        EnergyModel::Local => phenotype.photo_strength * light_sample * environment.solar_constant,
        EnergyModel::LegacyFlat => economy.flat_energy_grant_per_tick,
        EnergyModel::Pool | EnergyModel::Spatial => 0.0,
    }
}

/// Net energy change for one metabolism tick (`photosynthetic gain - costs`).
pub fn metabolism_tick_delta(
    phenotype: &PhenotypeData,
    light_sample: f32,
    environment: &EnvironmentConfig,
    economy: &EconomyConfig,
    blocked_by_overlap: bool,
) -> f32 {
    photosynthetic_gain_per_tick(
        phenotype,
        light_sample,
        environment,
        economy,
        blocked_by_overlap,
    ) - metabolism_tick_cost(phenotype)
}

/// Apply one metabolism tick; clamps `internal_energy` to `>= 0`.
pub fn apply_metabolism_tick(
    energy: &mut Energy,
    phenotype: &PhenotypeData,
    light_sample: f32,
    environment: &EnvironmentConfig,
    economy: &EconomyConfig,
    blocked_by_overlap: bool,
) {
    let delta = metabolism_tick_delta(
        phenotype,
        light_sample,
        environment,
        economy,
        blocked_by_overlap,
    );
    energy.internal_energy = (energy.internal_energy + delta).max(0.0);
}

/// True when two agent circles have any area overlap (touching at exactly `r1 + r2` is not overlap).
pub fn circles_overlap(pos_a: Vec2, radius_a: f32, pos_b: Vec2, radius_b: f32) -> bool {
    let r_sum = radius_a + radius_b;
    if r_sum <= 0.0 {
        return false;
    }
    pos_a.distance_squared(pos_b) < r_sum * r_sum
}

/// Entities that overlap at least one other agent this tick (both parties included).
pub fn entities_blocked_by_overlap(agents: &[(Entity, Vec2, f32)]) -> HashSet<Entity> {
    let mut blocked = HashSet::new();
    let n = agents.len();
    for i in 0..n {
        let (e_i, pos_i, r_i) = agents[i];
        for j in (i + 1)..n {
            let (_, pos_j, r_j) = agents[j];
            if circles_overlap(pos_i, r_i, pos_j, r_j) {
                blocked.insert(e_i);
                blocked.insert(agents[j].0);
            }
        }
    }
    blocked
}

/// Bevy system: photosynthetic grant minus metabolism and upkeeps for every living agent.
pub fn metabolism_and_gains(
    config: Res<SimConfigResource>,
    mut query: Query<(Entity, &Transform, &Agent, &Phenotype, &LightExposure, &mut Energy)>,
) {
    let agents: Vec<_> = query
        .iter()
        .map(|(entity, transform, agent, _, _, _)| {
            (
                entity,
                transform.translation.truncate(),
                agent.radius,
            )
        })
        .collect();

    let environment = &config.0.environment;
    let economy = &config.0.economy;
    let crowding_blocks = environment.crowding_blocks_photosynthesis;
    let blocked = if crowding_blocks {
        entities_blocked_by_overlap(&agents)
    } else {
        HashSet::new()
    };

    for (entity, _, _, phenotype, exposure, mut energy) in &mut query {
        let blocked_by_overlap = crowding_blocks && blocked.contains(&entity);
        apply_metabolism_tick(
            &mut energy,
            &phenotype.0,
            exposure.0,
            environment,
            economy,
            blocked_by_overlap,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Entity;
    use crate::config::{EnergyModel, SimConfig};
    use crate::genome::{decode, Genome};
    use crate::sim::components::Energy;

    fn ancestor_phenotype(config: &SimConfig) -> PhenotypeData {
        let genome = Genome::ancestor_from_config(config).unwrap();
        decode(&genome, config).unwrap()
    }

    fn local_config() -> SimConfig {
        let mut config = SimConfig::load_default().unwrap();
        config.environment.energy_model = EnergyModel::Local;
        config
    }

    #[test]
    fn circles_overlap_when_interiors_intersect() {
        assert!(circles_overlap(Vec2::ZERO, 10.0, Vec2::new(15.0, 0.0), 10.0));
        assert!(!circles_overlap(Vec2::ZERO, 10.0, Vec2::new(20.0, 0.0), 10.0));
    }

    #[test]
    fn overlap_blocks_photosynthetic_gain_only_legacy_flat() {
        let mut config = SimConfig::load_default().unwrap();
        config.environment.energy_model = EnergyModel::LegacyFlat;
        config.economy.flat_energy_grant_per_tick = 0.8;
        let phenotype = ancestor_phenotype(&config);
        let environment = &config.environment;
        let economy = &config.economy;
        let light = 1.0;

        let open = metabolism_tick_delta(&phenotype, light, environment, economy, false);
        let crowded = metabolism_tick_delta(&phenotype, light, environment, economy, true);
        assert!(
            (open - crowded - economy.flat_energy_grant_per_tick).abs() < 1e-5,
            "blocked tick should differ only by the photosynthetic grant"
        );
        assert_eq!(
            photosynthetic_gain_per_tick(&phenotype, light, environment, economy, true),
            0.0
        );
    }

    #[test]
    fn overlap_blocks_local_gain_when_crowding_enabled() {
        let config = local_config();
        let phenotype = ancestor_phenotype(&config);
        let light = config.environment.gradient_max;
        let environment = &config.environment;
        let economy = &config.economy;

        let open_gain = photosynthetic_gain_per_tick(
            &phenotype, light, environment, economy, false,
        );
        let blocked_gain = photosynthetic_gain_per_tick(
            &phenotype, light, environment, economy, true,
        );

        assert!(open_gain > 0.0, "local gain should be positive in bright band");
        assert_eq!(blocked_gain, 0.0);

        let open_delta = metabolism_tick_delta(&phenotype, light, environment, economy, false);
        let crowded_delta = metabolism_tick_delta(&phenotype, light, environment, economy, true);
        assert!(
            (open_delta - crowded_delta - open_gain).abs() < 1e-5,
            "blocked tick should differ only by photosynthetic gain under local model"
        );
    }

    #[test]
    fn entities_blocked_by_overlap_marks_both() {
        let a = Entity::from_raw(1);
        let b = Entity::from_raw(2);
        let c = Entity::from_raw(3);
        let agents = vec![
            (a, Vec2::ZERO, 10.0),
            (b, Vec2::new(15.0, 0.0), 10.0),
            (c, Vec2::new(200.0, 0.0), 10.0),
        ];
        let blocked = entities_blocked_by_overlap(&agents);
        assert!(blocked.contains(&a) && blocked.contains(&b));
        assert!(!blocked.contains(&c));
    }

    #[test]
    fn zero_photo_strength_has_zero_local_gain() {
        let config = local_config();
        let mut phenotype = ancestor_phenotype(&config);
        phenotype.photo_strength = 0.0;
        let gain = photosynthetic_gain_per_tick(
            &phenotype,
            1.0,
            &config.environment,
            &config.economy,
            false,
        );
        assert_eq!(gain, 0.0);
    }

    #[test]
    fn local_gain_scales_with_solar_constant() {
        let mut config = local_config();
        let phenotype = ancestor_phenotype(&config);
        let light = 0.8;
        let base = photosynthetic_gain_per_tick(
            &phenotype,
            light,
            &config.environment,
            &config.economy,
            false,
        );
        config.environment.solar_constant *= 2.0;
        let doubled = photosynthetic_gain_per_tick(
            &phenotype,
            light,
            &config.environment,
            &config.economy,
            false,
        );
        assert!((doubled - base * 2.0).abs() < 1e-5);
    }

    #[test]
    fn local_gain_independent_of_other_agents() {
        let config = local_config();
        let phenotype = ancestor_phenotype(&config);
        let light = 0.6;
        let gain_a = photosynthetic_gain_per_tick(
            &phenotype,
            light,
            &config.environment,
            &config.economy,
            false,
        );
        let gain_b = photosynthetic_gain_per_tick(
            &phenotype,
            light,
            &config.environment,
            &config.economy,
            false,
        );
        assert_eq!(gain_a, gain_b);
    }

    #[test]
    fn local_gain_formula_matches_strength_light_solar() {
        let config = local_config();
        let phenotype = ancestor_phenotype(&config);
        let light = 0.75;
        let expected =
            phenotype.photo_strength * light * config.environment.solar_constant;
        let gain = photosynthetic_gain_per_tick(
            &phenotype,
            light,
            &config.environment,
            &config.economy,
            false,
        );
        assert!((gain - expected).abs() < 1e-5);
    }

    #[test]
    fn zero_local_solar_depletes_energy_over_ticks() {
        let mut config = local_config();
        config.environment.solar_constant = 0.0;
        config.economy.flat_energy_grant_per_tick = 0.0;
        let phenotype = ancestor_phenotype(&config);

        let mut energy = Energy {
            internal_energy: config.economy.spawn_energy,
        };
        let start = energy.internal_energy;

        for _ in 0..500 {
            apply_metabolism_tick(
                &mut energy,
                &phenotype,
                config.environment.gradient_max,
                &config.environment,
                &config.economy,
                false,
            );
        }

        assert!(energy.internal_energy < start);
        assert_eq!(energy.internal_energy, 0.0);
    }

    #[test]
    fn zero_legacy_grant_depletes_energy_over_ticks() {
        let mut config = SimConfig::load_default().unwrap();
        config.environment.energy_model = EnergyModel::LegacyFlat;
        let phenotype = ancestor_phenotype(&config);
        let mut economy = config.economy.clone();
        economy.flat_energy_grant_per_tick = 0.0;

        let mut energy = Energy {
            internal_energy: config.economy.spawn_energy,
        };
        let start = energy.internal_energy;

        for _ in 0..500 {
            apply_metabolism_tick(
                &mut energy,
                &phenotype,
                1.0,
                &config.environment,
                &economy,
                false,
            );
        }

        assert!(energy.internal_energy < start);
        assert_eq!(energy.internal_energy, 0.0);
    }

    #[test]
    fn local_ancestor_bright_band_net_positive_over_ticks() {
        let config = local_config();
        let phenotype = ancestor_phenotype(&config);
        let bright = config.environment.gradient_max;

        let mut energy = Energy {
            internal_energy: config.economy.spawn_energy,
        };
        let start = energy.internal_energy;

        for _ in 0..1000 {
            apply_metabolism_tick(
                &mut energy,
                &phenotype,
                bright,
                &config.environment,
                &config.economy,
                false,
            );
        }

        assert!(
            energy.internal_energy > start,
            "ancestor should gain net energy in bright band (start={start}, end={})",
            energy.internal_energy
        );
    }

    #[test]
    fn high_photo_shade_band_net_negative_over_ticks() {
        let config = local_config();
        let mut phenotype = ancestor_phenotype(&config);
        phenotype.photo_strength = config.genetics.decode_ranges.photo_max;
        phenotype.photo_upkeep =
            config.genetics.decode_ranges.k_photo * phenotype.photo_strength;
        let shade = config.environment.gradient_min;

        let mut energy = Energy {
            internal_energy: config.economy.spawn_energy,
        };
        let start = energy.internal_energy;

        for _ in 0..500 {
            apply_metabolism_tick(
                &mut energy,
                &phenotype,
                shade,
                &config.environment,
                &config.economy,
                false,
            );
        }

        assert!(
            energy.internal_energy < start,
            "high-photo genotype should lose energy in shade (start={start}, end={})",
            energy.internal_energy
        );
    }

    #[test]
    fn default_local_bright_band_stabilizes_above_repro_threshold() {
        let config = local_config();
        let phenotype = ancestor_phenotype(&config);

        let mut energy = Energy {
            internal_energy: config.economy.spawn_energy,
        };

        for _ in 0..1000 {
            apply_metabolism_tick(
                &mut energy,
                &phenotype,
                config.environment.gradient_max,
                &config.environment,
                &config.economy,
                false,
            );
        }

        assert!(
            energy.internal_energy > phenotype.repro_threshold,
            "energy {} should exceed repro threshold {} after 1000 ticks in bright band",
            energy.internal_energy,
            phenotype.repro_threshold
        );
    }

    #[test]
    fn tick_cost_includes_upkeeps() {
        let config = SimConfig::load_default().unwrap();
        let phenotype = ancestor_phenotype(&config);
        let cost = metabolism_tick_cost(&phenotype);
        assert!(cost >= phenotype.metabolism_idle);
        assert!(cost > 0.0);
    }
}

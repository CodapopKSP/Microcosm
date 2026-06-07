//! Genome v2 metabolism: radius-linked demand, photo triad (decoded), dual vitality caps ([Genome_Definition.md](../../../docs/Genome_Definition.md)).

use crate::config::{
    EconomyConfig, EnvironmentConfig, GeneticsBehaviorConfig, GeneticsDecodeV2, SimConfig,
};
use crate::genome::PhenotypeV2;
use crate::sim::components::{Energy, Health};

use super::metabolism::age_metabolism_multiplier;

/// Simulation tick rate for per-second genome formulas.
pub const TICKS_PER_SEC: f32 = 60.0;

/// Per-tick idle metabolism from decoded radius + armor demand.
pub fn metabolism_demand_per_tick(phenotype: &PhenotypeV2, economy: &EconomyConfig) -> f32 {
    let per_sec = phenotype.metabolism_demand;
    let scaled = per_sec / TICKS_PER_SEC;
    scaled.max(economy.metabolism_floor)
}

/// Sensor upkeep from enabled mask bits and decoded reach ([CONFIG.md](../../../docs/CONFIG.md) `[genetics.behavior]`).
pub fn sensor_upkeep_per_tick(
    phenotype: &PhenotypeV2,
    behavior: &GeneticsBehaviorConfig,
    decode: &GeneticsDecodeV2,
) -> f32 {
    let eyes = phenotype.sensor_eye_count;
    if eyes == 0 {
        return 0.0;
    }
    let reach_norm = if decode.sensor_distance_at_high_nibble_f > 0.0 {
        (phenotype.sensor_reach_per_eye * eyes as f32 / decode.sensor_distance_at_high_nibble_f)
            .clamp(0.0, 1.0)
    } else {
        0.0
    };
    let strength_mult = 1.0 + behavior.sensor_upkeep_strength_scale * reach_norm;
    eyes as f32 * behavior.sensor_upkeep_per_eye_per_tick * strength_mult
}

/// Per-tick energy drain (scaled base metabolism + sensor upkeep).
pub fn metabolism_tick_cost_v2(
    phenotype: &PhenotypeV2,
    age_ticks: u32,
    economy: &EconomyConfig,
    behavior: &GeneticsBehaviorConfig,
    decode: &GeneticsDecodeV2,
) -> f32 {
    let demand = metabolism_demand_per_tick(phenotype, economy);
    let age_mult = age_metabolism_multiplier(age_ticks, 1.0, economy);
    demand * age_mult + sensor_upkeep_per_tick(phenotype, behavior, decode)
}

/// Photosynthetic gain for one tick (`photo_rate × radius × photo_effective × light × solar`, per-second → per-tick).
pub fn photosynthetic_gain_per_tick_v2(
    phenotype: &PhenotypeV2,
    light_sample: f32,
    environment: &EnvironmentConfig,
    decode: &GeneticsDecodeV2,
) -> f32 {
    let per_sec = phenotype.photo_gain_per_sec(decode);
    per_sec * light_sample.clamp(0.0, 1.0) * environment.solar_constant / TICKS_PER_SEC
}

/// Net energy change for one v2 metabolism tick.
pub fn metabolism_tick_delta_v2(
    phenotype: &PhenotypeV2,
    age_ticks: u32,
    light_sample: f32,
    environment: &EnvironmentConfig,
    economy: &EconomyConfig,
    decode: &GeneticsDecodeV2,
    behavior: &GeneticsBehaviorConfig,
) -> f32 {
    photosynthetic_gain_per_tick_v2(phenotype, light_sample, environment, decode)
        - metabolism_tick_cost_v2(phenotype, age_ticks, economy, behavior, decode)
}

/// Clamp pools to decoded caps ([Genome_Definition.md](../../../docs/Genome_Definition.md) vitality).
pub fn clamp_vitality_pools(energy: &mut Energy, health: &mut Health, phenotype: &PhenotypeV2) {
    let max_e = phenotype.max_energy.max(0.0);
    energy.max_internal_energy = max_e;
    energy.internal_energy = energy.internal_energy.min(max_e);
    let max_hp = phenotype.max_health.max(0.0);
    health.core_max_hp = max_hp;
    health.core_hp = health.core_hp.min(max_hp);
}

/// Apply one or more v2 metabolism ticks; deficit drains `core_hp` after energy is empty.
pub fn apply_metabolism_tick_v2(
    energy: &mut Energy,
    health: &mut Health,
    phenotype: &PhenotypeV2,
    age_ticks: u32,
    light_sample: f32,
    environment: &EnvironmentConfig,
    economy: &EconomyConfig,
    config: &SimConfig,
    tick_count: u32,
) {
    let decode = config.genetics.require_decode_v2().expect("v2 metabolism requires decode_v2");
    let behavior = config.genetics.require_behavior().expect("v2 metabolism requires behavior");

    let delta = metabolism_tick_delta_v2(
        phenotype,
        age_ticks,
        light_sample,
        environment,
        economy,
        decode,
        behavior,
    ) * tick_count.max(1) as f32;
    energy.internal_energy += delta;
    if energy.internal_energy < 0.0 {
        let hp_deficit = -energy.internal_energy;
        energy.internal_energy = 0.0;
        health.apply_damage_to_core(hp_deficit);
    }
    clamp_vitality_pools(energy, health, phenotype);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::genome::{decode_v2, GenomeV2};
    use crate::sim::components::{Energy, Health};

    fn v2_config() -> SimConfig {
        SimConfig::load_from_path(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("configs/genome_v2.example.toml"),
        )
        .unwrap()
    }

    fn ancestor_phenotype(config: &SimConfig) -> PhenotypeV2 {
        let genome = GenomeV2::ancestor_from_config(config).unwrap();
        decode_v2(&genome, config).unwrap()
    }

    fn full_health(config: &SimConfig, phenotype: &PhenotypeV2) -> Health {
        Health {
            core_hp: phenotype.max_health,
            core_max_hp: phenotype.max_health,
            part_hp: vec![],
        }
    }

    fn energy_at_spawn(config: &SimConfig, phenotype: &PhenotypeV2) -> Energy {
        Energy::new_v2(
            config.economy.spawn_energy,
            phenotype.max_energy,
        )
    }

    #[test]
    fn ancestor_has_positive_photo_gain_in_bright_band() {
        let config = v2_config();
        let phenotype = ancestor_phenotype(&config);
        let decode = config.genetics.decode_v2.as_ref().unwrap();
        let gain = photosynthetic_gain_per_tick_v2(
            &phenotype,
            config.environment.gradient_max,
            &config.environment,
            decode,
        );
        assert!(gain > 0.0);
        assert!(phenotype.photo_effective > 0.9);
    }

    #[test]
    fn metabolism_cost_scales_with_radius_and_armor() {
        let config = v2_config();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        let body_idx =
            crate::genome::V2_HEADER_LEN + crate::genome::v2_core::BODY_RADIUS;
        bytes[body_idx] = 0xFF;
        bytes[body_idx + 1] = 0xFF;
        let large = decode_v2(
            &GenomeV2::from_bytes(bytes.clone(), &config.genetics).unwrap(),
            &config,
        )
        .unwrap();
        let small = ancestor_phenotype(&config);
        let economy = &config.economy;
        let behavior = config.genetics.behavior.as_ref().unwrap();
        let decode = config.genetics.decode_v2.as_ref().unwrap();
        assert!(
            metabolism_tick_cost_v2(&large, 0, economy, behavior, decode)
                > metabolism_tick_cost_v2(&small, 0, economy, behavior, decode)
        );
    }

    #[test]
    fn metabolic_deficit_drains_hp_when_energy_empty() {
        let config = v2_config();
        let phenotype = ancestor_phenotype(&config);
        let mut energy = Energy::new_v2(0.0, phenotype.max_energy);
        let mut health = Health {
            core_hp: 50.0,
            core_max_hp: phenotype.max_health,
            part_hp: vec![],
        };
        apply_metabolism_tick_v2(
            &mut energy,
            &mut health,
            &phenotype,
            0,
            0.0,
            &config.environment,
            &config.economy,
            &config,
            1,
        );
        assert_eq!(energy.internal_energy, 0.0);
        assert!(health.core_hp < 50.0);
    }

    #[test]
    fn bright_band_net_positive_per_tick() {
        let config = v2_config();
        let phenotype = ancestor_phenotype(&config);
        let decode = config.genetics.decode_v2.as_ref().unwrap();
        let behavior = config.genetics.behavior.as_ref().unwrap();
        let delta = metabolism_tick_delta_v2(
            &phenotype,
            0,
            config.environment.gradient_max,
            &config.environment,
            &config.economy,
            decode,
            behavior,
        );
        assert!(
            delta > 0.0,
            "sessile photo ancestor should have positive net metabolism in bright band (delta={delta})"
        );
    }

    #[test]
    fn zero_solar_depletes_then_drains_health() {
        let mut config = v2_config();
        config.environment.solar_constant = 0.0;
        let phenotype = ancestor_phenotype(&config);
        let mut energy = energy_at_spawn(&config, &phenotype);
        let mut health = full_health(&config, &phenotype);

        for _ in 0..5000 {
            apply_metabolism_tick_v2(
                &mut energy,
                &mut health,
                &phenotype,
                0,
                config.environment.gradient_max,
                &config.environment,
                &config.economy,
                &config,
                1,
            );
        }

        assert_eq!(energy.internal_energy, 0.0);
        assert!(health.core_hp < phenotype.max_health);
    }

    #[test]
    fn energy_clamped_to_max_after_gain() {
        let config = v2_config();
        let phenotype = ancestor_phenotype(&config);
        let mut energy = Energy::new_v2(phenotype.max_energy - 0.01, phenotype.max_energy);
        let mut health = full_health(&config, &phenotype);

        for _ in 0..5000 {
            apply_metabolism_tick_v2(
                &mut energy,
                &mut health,
                &phenotype,
                0,
                config.environment.gradient_max,
                &config.environment,
                &config.economy,
                &config,
                1,
            );
        }

        assert!(energy.internal_energy <= phenotype.max_energy + 1e-5);
    }

    #[test]
    fn per_tick_healing_restores_hp_from_available_energy_in_zero_light() {
        use crate::config::RepairPolicy;
        use crate::sim::systems::healing_v2::apply_heal_tick_v2;
        use crate::sim::world::SIM_DT;

        let config = SimConfig::load_default().unwrap();
        let phenotype = ancestor_phenotype(&config);
        let mut energy = Energy::new_v2(19.0, phenotype.max_energy);
        let mut health = Health {
            core_hp: 0.5,
            core_max_hp: phenotype.max_health,
            part_hp: vec![],
        };
        let env = config.environment.clone();

        for _ in 0..600 {
            apply_metabolism_tick_v2(
                &mut energy,
                &mut health,
                &phenotype,
                0,
                0.0,
                &env,
                &config.economy,
                &config,
                1,
            );
            apply_heal_tick_v2(
                &mut health,
                &mut energy,
                &phenotype,
                RepairPolicy::CoreFirst,
                &config.economy,
                SIM_DT,
                0,
            );
        }

        assert!(
            health.core_hp > 5.0,
            "expected healing to restore HP from energy; hp={} max_hp={} energy={}",
            health.core_hp,
            phenotype.max_health,
            energy.internal_energy
        );
        assert!(energy.internal_energy < 19.0);
        assert!(health.core_hp < phenotype.max_health);
    }

    #[test]
    fn ratio_locus_sets_distinct_caps_from_same_radius() {
        let config = v2_config();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        let idx = crate::genome::V2_HEADER_LEN + crate::genome::v2_core::ENERGY_HEALTH_RATIO;
        bytes[idx] = 0x00;
        let energy_heavy = decode_v2(
            &GenomeV2::from_bytes(bytes.clone(), &config.genetics).unwrap(),
            &config,
        )
        .unwrap();
        bytes[idx] = 0xFF;
        let health_heavy = decode_v2(
            &GenomeV2::from_bytes(bytes, &config.genetics).unwrap(),
            &config,
        )
        .unwrap();
        assert!(energy_heavy.max_energy > health_heavy.max_energy);
        assert!(health_heavy.max_health > energy_heavy.max_health);
        assert!(
            (energy_heavy.max_energy + energy_heavy.max_health
                - health_heavy.max_energy
                - health_heavy.max_health)
                .abs()
                < 1.0
        );
    }
}

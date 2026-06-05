//! Phase 1L / Phase 2E–2J acceptance tests (v2).

#![cfg(test)]

use bevy::prelude::Vec2;

use crate::config::{EnergyModel, SimConfig};
use crate::genome::{
    decode_v2, mitigated_damage, v2_core, GenomeV2, PhenotypeV2, V2_HEADER_LEN,
};

use super::components::{Energy, Health};
use super::hazards::HazardField;
use super::headless::run_headless_sim;
use super::{apply_heal_tick_v2, repro_threshold_energy, v2_damage_mitigation};
use super::world::SIM_DT;

fn repro_risk_ab_config(mut config: SimConfig) -> SimConfig {
    let genome = GenomeV2::ancestor_from_config(&config).unwrap();
    let phenotype = decode_v2(&genome, &config).unwrap();
    let threshold = repro_threshold_energy(&phenotype);
    config.economy.repro_safety_margin = 2.0;
    config.economy.spawn_energy = threshold + 50.0;
    config
}

fn phenotype_with_armor(config: &SimConfig, armor_raw: u16) -> PhenotypeV2 {
    let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
    let off = V2_HEADER_LEN + v2_core::ARMOR;
    bytes[off] = (armor_raw >> 8) as u8;
    bytes[off + 1] = (armor_raw & 0xFF) as u8;
    let genome = GenomeV2::from_bytes(bytes, &config.genetics).unwrap();
    decode_v2(&genome, config).unwrap()
}

fn armor_mitigation(phenotype: &PhenotypeV2, config: &SimConfig) -> f32 {
    let stroke_max = config
        .genetics
        .decode_v2
        .as_ref()
        .map(|d| d.stroke_max)
        .unwrap_or(1.0);
    v2_damage_mitigation(phenotype, stroke_max)
}

struct HazardHarnessAgent {
    health: Health,
    energy: Energy,
    phenotype: PhenotypeV2,
    position: Vec2,
}

fn hazard_harness_agent(config: &SimConfig, phenotype: PhenotypeV2, position: Vec2) -> HazardHarnessAgent {
    let hp_frac = config.economy.spawn_health_frac.clamp(0.0, 1.0);
    let max_hp = phenotype.max_health;
    HazardHarnessAgent {
        health: Health {
            core_hp: max_hp * hp_frac,
            core_max_hp: max_hp,
            part_hp: vec![],
        },
        energy: Energy::new_v2(config.economy.spawn_energy, phenotype.max_energy),
        phenotype,
        position,
    }
}

fn simulate_hazard_damage_ticks(
    config: &SimConfig,
    agents: &mut Vec<HazardHarnessAgent>,
    ticks: u32,
    repair_enabled: bool,
) {
    let hazards = HazardField::from_environment(&config.environment);
    for _ in 0..ticks {
        for agent in agents.iter_mut() {
            let raw = hazards.damage_at(agent.position);
            let applied = mitigated_damage(raw, armor_mitigation(&agent.phenotype, config));
            agent.health.apply_damage_to_core(applied);
            if repair_enabled && config.economy.repair_energy_per_hp > 0.0 {
                apply_heal_tick_v2(
                    &mut agent.health,
                    &mut agent.energy,
                    &agent.phenotype,
                    config.economy.repair_policy,
                    &config.economy,
                    SIM_DT,
                    0,
                );
            }
        }
        agents.retain(|a| a.health.core_hp > 0.0);
    }
}

fn ticks_until_hazard_death(
    config: &SimConfig,
    phenotype: PhenotypeV2,
    position: Vec2,
    repair_enabled: bool,
) -> u32 {
    let mut agent = hazard_harness_agent(config, phenotype, position);
    let hazards = HazardField::from_environment(&config.environment);
    let mut tick = 0u32;
    while agent.health.core_hp > 0.0 && tick < 20_000 {
        let raw = hazards.damage_at(agent.position);
        let applied = mitigated_damage(raw, armor_mitigation(&agent.phenotype, config));
        agent.health.apply_damage_to_core(applied);
        if repair_enabled && config.economy.repair_energy_per_hp > 0.0 {
            apply_heal_tick_v2(
                &mut agent.health,
                &mut agent.energy,
                &agent.phenotype,
                config.economy.repair_policy,
                &config.economy,
                SIM_DT,
                0,
            );
        }
        tick += 1;
    }
    tick
}

#[test]
fn population_viable_20k_ticks_with_age() {
    let config = SimConfig::load_default().unwrap();
    let summary = run_headless_sim(&config, 20_000);
    assert!(
        summary.alive > 0,
        "population extinct before 20k ticks with evolvable lifespan: {summary:?}"
    );
    assert!(
        summary.births > 0,
        "expected reproduction over 20k ticks with age upkeep: {summary:?}"
    );
}

#[test]
fn population_survives_5000_ticks() {
    let mut config = SimConfig::load_default().unwrap();
    config.world.initial_population = 20;

    let summary = run_headless_sim(&config, 5000);
    assert!(
        summary.alive > 0,
        "population extinct before 5k ticks: {summary:?}"
    );
    assert!(summary.births > 0, "expected reproduction over 5k ticks: {summary:?}");
}

#[test]
fn repro_risk_ab_same_seed_different_trajectories() {
    let base = repro_risk_ab_config(SimConfig::load_default().unwrap());
    assert!(base.run.master_seed.is_some());

    let mut low = base.clone();
    low.economy.p_death_on_repro_base = 0.05;
    low.world.initial_population = 20;

    let mut high = base.clone();
    high.economy.p_death_on_repro_base = 0.4;
    high.world.initial_population = 20;

    let low_run = run_headless_sim(&low, 5000);
    let high_run = run_headless_sim(&high, 5000);

    assert!(
        low_run.repro_fatalities != high_run.repro_fatalities
            || low_run.alive != high_run.alive
            || low_run.births != high_run.births,
        "p_death_on_repro_base A/B should diverge with same master_seed (low={low_run:?}, high={high_run:?})"
    );
}

#[test]
fn zero_solar_local_model_extinguishes_population() {
    let mut config = SimConfig::load_default().unwrap();
    config.environment.solar_constant = 0.0;
    config.world.initial_population = 20;

    let summary = run_headless_sim(&config, 10_000);
    assert_eq!(
        summary.alive, 0,
        "population should die with zero solar and no flat grant: {summary:?}"
    );
}

#[test]
fn default_config_survives_without_crowding_block() {
    let config = SimConfig::load_default().unwrap();
    assert!(
        !config.environment.crowding_blocks_photosynthesis,
        "default headless acceptance assumes crowding_blocks_photosynthesis = false"
    );

    let summary = run_headless_sim(&config, 3000);
    assert!(
        summary.alive > 0,
        "population should survive without overlap blocking: {summary:?}"
    );
}

#[test]
fn local_model_population_survives_3000_ticks() {
    let config = SimConfig::load_default().unwrap();
    assert_eq!(
        config.environment.energy_model,
        EnergyModel::Local,
        "default config should use local energy model"
    );
    assert_eq!(
        config.economy.flat_energy_grant_per_tick, 0.0,
        "flat grant should be zero under local model default"
    );

    let summary = run_headless_sim(&config, 3000);
    assert!(
        summary.alive > 0,
        "local-model population extinct before 3k ticks: {summary:?}"
    );
    assert!(
        summary.births > 0,
        "expected reproduction under local model over 3k ticks: {summary:?}"
    );
}

#[test]
fn population_stable_5000_ticks_gradient_default_config() {
    let config = SimConfig::load_default().unwrap();
    assert_eq!(config.environment.light_mode, crate::config::LightMode::GradientY);

    let summary = run_headless_sim(&config, 5000);
    let cap = config.performance.max_entities;
    let initial = config.world.initial_population;

    assert!(summary.alive > 0, "default gradient_y run went extinct: {summary:?}");
    assert!(
        summary.births > 0,
        "expected reproduction under default local tuning: {summary:?}"
    );
    assert!(
        summary.peak_alive <= cap.saturating_mul(2),
        "peak population {} should stay bounded (max_entities={}, initial={initial})",
        summary.peak_alive,
        cap
    );
    assert!(
        summary.alive <= cap.saturating_mul(2),
        "final population {} should not far exceed 2× max_entities {}",
        summary.alive,
        cap
    );
}

#[test]
fn population_stable_10000_ticks_gradient_default_config() {
    let config = SimConfig::load_default().unwrap();
    let summary = run_headless_sim(&config, 10_000);
    let cap = config.performance.max_entities;
    assert!(summary.alive > 0, "10k-tick run extinct: {summary:?}");
    assert!(
        summary.peak_alive <= cap.saturating_mul(2),
        "peak {} exceeded 2× max_entities at 10k ticks",
        summary.peak_alive
    );
    assert!(
        summary.alive <= cap.saturating_mul(2),
        "10k-tick population grew without bound (alive={})",
        summary.alive
    );
}

#[test]
fn repair_energy_spent_after_hazard_damage() {
    let config = SimConfig::load_default().unwrap();
    assert!(config.economy.repair_energy_per_hp > 0.0);
    let hazard_center = Vec2::from_array(config.environment.hazards[0].center);
    let phenotype = phenotype_with_armor(&config, 0x0000);

    let mut agent = hazard_harness_agent(&config, phenotype, hazard_center);
    let start_energy = agent.energy.internal_energy;
    let hazards = HazardField::from_environment(&config.environment);

    let mut total_repair = 0.0f32;
    for _ in 0..200 {
        let raw = hazards.damage_at(agent.position);
        let applied = mitigated_damage(raw, armor_mitigation(&agent.phenotype, &config));
        agent.health.apply_damage_to_core(applied);
        total_repair += apply_heal_tick_v2(
            &mut agent.health,
            &mut agent.energy,
            &agent.phenotype,
            config.economy.repair_policy,
            &config.economy,
            SIM_DT,
            0,
        );
        if agent.health.core_hp <= 0.0 {
            break;
        }
    }

    assert!(
        total_repair > 0.0,
        "repair should spend energy after hazard damage (spent={total_repair})"
    );
    assert!(
        agent.energy.internal_energy < start_energy,
        "energy should drop from repair (start={start_energy}, end={})",
        agent.energy.internal_energy
    );
}

#[test]
fn hazard_kills_faster_than_safe_zone_when_repair_disabled() {
    let mut config = SimConfig::load_default().unwrap();
    config.economy.repair_energy_per_hp = 0.0;
    assert!(!config.environment.hazards.is_empty());

    let hazard_center = Vec2::from_array(config.environment.hazards[0].center);
    let safe_pos = Vec2::new(-450.0, -700.0);

    let soft = phenotype_with_armor(&config, 0x0000);
    let mut in_hazard: Vec<_> = (0..12)
        .map(|_| hazard_harness_agent(&config, soft.clone(), hazard_center))
        .collect();
    let mut in_safe: Vec<_> = (0..12)
        .map(|_| hazard_harness_agent(&config, soft.clone(), safe_pos))
        .collect();

    simulate_hazard_damage_ticks(&config, &mut in_hazard, 15_000, false);
    simulate_hazard_damage_ticks(&config, &mut in_safe, 15_000, false);

    assert!(
        in_hazard.len() < in_safe.len(),
        "hazard cohort should suffer more deaths without repair (hazard_alive={}, safe_alive={})",
        in_hazard.len(),
        in_safe.len()
    );
    assert_eq!(in_safe.len(), 12, "safe zone should take no hazard damage");
}

#[test]
fn hardness_hazard_survival_tradeoff() {
    let mut config = SimConfig::load_default().unwrap();
    config.economy.spawn_energy = 200.0;
    assert!(config.economy.repair_energy_per_hp > 0.0);
    let hazard_center = Vec2::from_array(config.environment.hazards[0].center);

    let soft = phenotype_with_armor(&config, 0x0000);
    let hard = phenotype_with_armor(&config, 0xFFFF);
    assert!(armor_mitigation(&hard, &config) > armor_mitigation(&soft, &config));

    let soft_ticks = ticks_until_hazard_death(&config, soft, hazard_center, false);
    let hard_ticks = ticks_until_hazard_death(&config, hard, hazard_center, false);
    assert!(
        hard_ticks > soft_ticks,
        "high hardness should survive more hazard ticks (soft={soft_ticks}, hard={hard_ticks})"
    );
}

#[test]
fn hardness_safe_zone_metabolism_penalty() {
    let config = SimConfig::load_default().unwrap();
    let soft = phenotype_with_armor(&config, 0x0000);
    let hard = phenotype_with_armor(&config, 0xFFFF);
    assert!(
        hard.metabolism_demand > soft.metabolism_demand,
        "hard armor should increase per-tick metabolism demand"
    );
}

/// G8: default EVO2 config decodes and stays metabolically viable in bright band.
#[test]
fn v2_default_ancestor_positive_metabolism_in_bright_band() {
    use super::metabolism_tick_delta_v2;

    let config = SimConfig::load_default().unwrap();
    assert_eq!(config.genetics.genome_version, 2);
    let genome = GenomeV2::ancestor_from_config(&config).unwrap();
    let phenotype = decode_v2(&genome, &config).unwrap();
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
        1.0,
    );
    assert!(
        delta > 0.0,
        "v2 default sessile photo ancestor should net positive in bright band (delta={delta})"
    );
    assert!(phenotype.max_energy > 0.0 && phenotype.max_health > 0.0);
}

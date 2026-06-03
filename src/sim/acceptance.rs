//! Phase 1L / Phase 2E acceptance tests.

#![cfg(test)]

use std::collections::HashSet;

use crate::config::{EnergyModel, SimConfig};
use crate::genome::{decode, Genome};

use super::headless::run_headless_sim;

/// Config where `repro_death_probability` stays meaningful so `p_death_on_repro_base` A/B diverges.
fn repro_risk_ab_config(mut config: SimConfig) -> SimConfig {
    let genome = Genome::ancestor_from_config(&config).unwrap();
    let phenotype = decode(&genome, &config).unwrap();
    config.economy.repro_safety_margin = 2.0;
    config.economy.spawn_energy =
        phenotype.repro_threshold + phenotype.repro_cost + 3.0;
    config.environment.energy_model = EnergyModel::LegacyFlat;
    config.economy.flat_energy_grant_per_tick =
        (phenotype.metabolism_idle + 0.08).max(config.economy.metabolism_floor);
    config
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
    assert!(
        high_run.repro_fatalities > low_run.repro_fatalities,
        "higher p_death should cause more repro fatalities (low={}, high={})",
        low_run.repro_fatalities,
        high_run.repro_fatalities
    );
}

#[test]
fn diversity_proxy_color_variance_across_seeds() {
    let mut all_colors = HashSet::new();

    for seed in 1..=10_u64 {
        let mut config = SimConfig::load_default().unwrap();
        config.run.master_seed = Some(seed);
        config.world.initial_population = 15;

        let summary = run_headless_sim(&config, 5000);
        assert!(summary.alive > 0, "seed {seed} went extinct: {summary:?}");
        all_colors.extend(summary.survivor_color_keys);
    }

    assert!(
        all_colors.len() >= 2,
        "expected color diversity across 10 seeded 5k-tick runs, got {} distinct colors",
        all_colors.len()
    );
}

#[test]
fn single_run_accumulates_phenotype_color_diversity() {
    let mut config = SimConfig::load_default().unwrap();
    config.world.initial_population = 25;

    let summary = run_headless_sim(&config, 5000);
    let distinct: HashSet<_> = summary.survivor_color_keys.iter().copied().collect();
    assert!(
        distinct.len() >= 2,
        "mutations should yield multiple survivor colors in one run, got {distinct:?}"
    );
}

#[test]
fn zero_solar_local_model_extinguishes_population() {
    let mut config = SimConfig::load_default().unwrap();
    config.environment.solar_constant = 0.0;
    config.world.initial_population = 20;

    let summary = run_headless_sim(&config, 2000);
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

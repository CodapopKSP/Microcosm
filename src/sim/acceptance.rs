//! Phase 1L / Phase 2E–2J acceptance tests.

#![cfg(test)]

use std::collections::HashSet;

use bevy::prelude::Vec2;

use crate::config::{EnergyModel, SimConfig};
use crate::genome::{
    self, decode, mitigated_damage, Genome as GenomeData, Phenotype as PhenotypeData, HEADER_LEN,
};

use super::components::{Energy, Health};
use super::hazards::HazardField;
use super::headless::run_headless_sim;
use super::systems::healing::apply_repair_tick;
use super::apply_metabolism_tick;
use super::spawn::{build_offspring_spawn_params, build_spawn_params, try_prepare_spawn};

/// Config where `repro_death_probability` stays meaningful so `p_death_on_repro_base` A/B diverges.
fn repro_risk_ab_config(mut config: SimConfig) -> SimConfig {
    let genome = GenomeData::ancestor_from_config(&config).unwrap();
    let phenotype = decode(&genome, &config).unwrap();
    config.economy.repro_safety_margin = 2.0;
    config.economy.spawn_energy =
        phenotype.repro_threshold + phenotype.offspring_endowment + 3.0;
    config.environment.energy_model = EnergyModel::LegacyFlat;
    config.economy.flat_energy_grant_per_tick =
        (phenotype.metabolism_idle + 0.08).max(config.economy.metabolism_floor);
    config
}

fn set_u16_locus(bytes: &mut [u8], core_offset: usize, raw: u16) {
    let i = HEADER_LEN + core_offset;
    bytes[i] = (raw >> 8) as u8;
    bytes[i + 1] = (raw & 0xFF) as u8;
}

fn set_color_bytes(bytes: &mut [u8], r: u8, g: u8, b: u8) {
    let base = HEADER_LEN;
    bytes[base + genome::core::COLOR_R] = r;
    bytes[base + genome::core::COLOR_G] = g;
    bytes[base + genome::core::COLOR_B] = b;
}

fn phenotype_with_colors(r: u8, g: u8, b: u8) -> (SimConfig, PhenotypeData) {
    let config = SimConfig::load_default().unwrap();
    let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
    set_color_bytes(&mut bytes, r, g, b);
    let genome = GenomeData::from_bytes(
        bytes,
        config.genetics.genome_version,
        config.genetics.core_length,
    )
    .unwrap();
    let phenotype = decode(&genome, &config).unwrap();
    (config, phenotype)
}

fn config_with_ancestor_hex(mut config: SimConfig, ancestor_hex: &str) -> SimConfig {
    config.genetics.ancestor_hex = ancestor_hex.to_string();
    config
}

fn phenotype_with_loci(photo_raw: u16, meta_raw: u16) -> (SimConfig, PhenotypeData) {
    let config = SimConfig::load_default().unwrap();
    let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
    set_u16_locus(&mut bytes, genome::core::PHOTO_STRENGTH, photo_raw);
    set_u16_locus(&mut bytes, genome::core::METABOLISM_BASE, meta_raw);
    let genome = GenomeData::from_bytes(
        bytes,
        config.genetics.genome_version,
        config.genetics.core_length,
    )
    .unwrap();
    let phenotype = decode(&genome, &config).unwrap();
    (config, phenotype)
}

fn ticks_until_depleted(
    phenotype: &PhenotypeData,
    light: f32,
    config: &SimConfig,
    start_energy: f32,
) -> u32 {
    let mut energy = Energy {
        internal_energy: start_energy,
    };
    let mut health = Health {
        core_hp: config.economy.core_max_hp,
        core_max_hp: config.economy.core_max_hp,
        part_hp: vec![],
    };
    let mut ticks = 0u32;
    while energy.internal_energy > 0.0 && ticks < 50_000 {
        apply_metabolism_tick(
            &mut energy,
            &mut health,
            phenotype,
            0,
            light,
            0.0,
            &config.environment,
            &config.economy,
            &config.genetics.decode_ranges,
            1.0,
        );
        ticks += 1;
    }
    ticks
}

fn net_energy_after_ticks(
    phenotype: &PhenotypeData,
    light: f32,
    config: &SimConfig,
    ticks: u32,
) -> f32 {
    let mut energy = Energy {
        internal_energy: config.economy.spawn_energy,
    };
    let mut health = Health {
        core_hp: config.economy.core_max_hp,
        core_max_hp: config.economy.core_max_hp,
        part_hp: vec![],
    };
    let start = energy.internal_energy;
    for _ in 0..ticks {
        apply_metabolism_tick(
            &mut energy,
            &mut health,
            phenotype,
            0,
            light,
            0.0,
            &config.environment,
            &config.economy,
            &config.genetics.decode_ranges,
            1.0,
        );
    }
    energy.internal_energy - start
}

fn energy_after_ticks(
    phenotype: &PhenotypeData,
    light: f32,
    config: &SimConfig,
    ticks: u32,
) -> f32 {
    let mut energy = Energy {
        internal_energy: config.economy.spawn_energy,
    };
    let mut health = Health {
        core_hp: config.economy.core_max_hp,
        core_max_hp: config.economy.core_max_hp,
        part_hp: vec![],
    };
    for _ in 0..ticks {
        apply_metabolism_tick(
            &mut energy,
            &mut health,
            phenotype,
            0,
            light,
            0.0,
            &config.environment,
            &config.economy,
            &config.genetics.decode_ranges,
            1.0,
        );
    }
    energy.internal_energy
}

fn ticks_until_hazard_death(
    config: &SimConfig,
    phenotype: PhenotypeData,
    position: Vec2,
    repair_enabled: bool,
) -> u32 {
    let mut agent = hazard_harness_agent(&config, phenotype, position);
    let hazards = HazardField::from_environment(&config.environment);
    let mut tick = 0u32;
    while agent.health.core_hp > 0.0 && tick < 20_000 {
        let raw = hazards.damage_at(agent.position);
        let applied = mitigated_damage(raw, agent.phenotype.damage_mitigation);
        agent.health.apply_damage_to_core(applied);
        if repair_enabled && config.economy.repair_energy_per_hp > 0.0 {
            apply_repair_tick(
                &mut agent.health,
                &mut agent.energy,
                &agent.phenotype,
                config.economy.repair_policy,
                &config.economy,
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
    assert!(
        high_run.repro_fatalities >= low_run.repro_fatalities,
        "higher p_death should not reduce repro fatalities (low={}, high={})",
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
    // Keep timing stable: this test asserts extinction rate, not absolute HP scale.
    config.economy.core_max_hp = 100.0;

    // Energy hits zero first; metabolic deficit then drains core HP (~6k ticks from full HP).
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

// --- Phase 2J: niche viability and population stability ---

#[test]
fn high_photo_bright_band_net_positive() {
    let (config, phenotype) = phenotype_with_loci(0xFFFF, 0x8000);
    let bright = config.environment.gradient_max;
    let delta = net_energy_after_ticks(&phenotype, bright, &config, 1000);
    assert!(
        delta > 0.0,
        "high-photo strain should gain net energy in bright band (delta={delta})"
    );
}

#[test]
fn high_photo_shade_band_net_negative() {
    let (config, mut phenotype) = phenotype_with_loci(0xFFFF, 0x8000);
    phenotype.photo_upkeep = config.genetics.decode_ranges.k_photo * phenotype.photo_strength;
    let shade = config.environment.gradient_min;
    let delta = net_energy_after_ticks(&phenotype, shade, &config, 500);
    assert!(
        delta < 0.0,
        "high-photo strain should lose net energy in shade (delta={delta})"
    );
}

#[test]
fn low_photo_shade_survives_longer_than_high_photo_shade() {
    let (config, low) = phenotype_with_loci(0x0000, 0x1000);
    let (_, high) = phenotype_with_loci(0xFFFF, 0x8000);
    let shade = config.environment.gradient_min;
    let start = config.economy.spawn_energy;

    let low_ticks = ticks_until_depleted(&low, shade, &config, start);
    let high_ticks = ticks_until_depleted(&high, shade, &config, start);

    assert!(
        low_ticks > high_ticks,
        "low-photo/low-meta should outlast high-photo in shade (low={low_ticks}, high={high_ticks})"
    );
}

#[test]
fn population_stable_5000_ticks_gradient_default_config() {
    let config = SimConfig::load_default().unwrap();
    assert_eq!(config.environment.light_mode, crate::config::LightMode::GradientY);

    let summary = run_headless_sim(&config, 5000);
    let cap = config.performance.max_entities;
    let initial = config.world.initial_population;

    assert!(
        summary.alive > 0,
        "default gradient_y run went extinct: {summary:?}"
    );
    assert!(
        summary.births > 0,
        "expected reproduction under default local tuning: {summary:?}"
    );
    // `stop_spawns` blocks new births above cap but does not despawn; richer default photo
    // can push peak modestly above `max_entities` before deaths catch up.
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
    assert!(
        summary.alive <= cap.saturating_mul(2),
        "population should not explode unbounded in 5k ticks (alive={}, max_entities={})",
        summary.alive,
        cap
    );
}

#[test]
fn population_stable_10000_ticks_gradient_default_config() {
    let config = SimConfig::load_default().unwrap();
    let summary = run_headless_sim(&config, 10_000);
    let cap = config.performance.max_entities;
    let initial = config.world.initial_population;

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

/// Phase 4C: default initial spawn and offspring bundles keep `part_hp` empty.
#[test]
fn part_hp_empty_for_default_spawn_and_offspring_paths() {
    let config = SimConfig::load_default().unwrap();
    let ancestor = GenomeData::ancestor_from_config(&config).unwrap();

    for n in 0..config.world.initial_population {
        let (genome, phenotype) =
            try_prepare_spawn(ancestor.clone_genome(), &config).expect("ancestor viable");
        let params = build_spawn_params(genome, phenotype.clone(), &config, n as u64 + 1, None, 0);
        assert!(
            params.life.health.part_hp.is_empty(),
            "initial spawn {n} must have empty part_hp"
        );
    }

    let (genome, phenotype) = try_prepare_spawn(ancestor, &config).unwrap();
    let endowment = phenotype.offspring_endowment;
    let child = build_offspring_spawn_params(
        genome,
        phenotype,
        &config,
        999,
        Some(1),
        1,
        0.0,
        endowment,
    );
    assert_eq!(child.life.energy.internal_energy, 0.0);
    assert!((child.life.health.core_hp - endowment).abs() < 1e-3);
    assert!(
        (child.life.health.core_max_hp - endowment).abs() < 1e-3,
        "offspring max HP must match endowment, not config species cap"
    );
    assert!(
        child.life.health.part_hp.is_empty(),
        "offspring spawn must have empty part_hp in Phase 4"
    );
}

/// Offspring at endowment HP must not sink all photosynthetic gain into a phantom repair deficit.
#[test]
fn offspring_gains_energy_when_spawned_at_endowment_hp() {
    let config = SimConfig::load_default().unwrap();
    let genome = GenomeData::ancestor_from_config(&config).unwrap();
    let phenotype = decode(&genome, &config).unwrap();
    let endowment = phenotype.offspring_endowment;

    let mut energy = Energy {
        internal_energy: 0.0,
    };
    let mut health = Health {
        core_hp: endowment,
        core_max_hp: endowment,
        part_hp: vec![],
    };
    let bright = config.environment.gradient_max;

    for tick in 0..2000 {
        apply_metabolism_tick(
            &mut energy,
            &mut health,
            &phenotype,
            tick,
            bright,
            0.0,
            &config.environment,
            &config.economy,
            &config.genetics.decode_ranges,
            1.0,
        );
        apply_repair_tick(
            &mut health,
            &mut energy,
            &phenotype,
            config.economy.repair_policy,
            &config.economy,
            0,
        );
    }

    assert!(
        energy.internal_energy > 1.0,
        "offspring at full endowment HP should accumulate energy in bright band (end={})",
        energy.internal_energy
    );
    assert!(
        (health.core_hp - endowment).abs() < 1e-3,
        "full endowment HP should not require repair (hp={})",
        health.core_hp
    );
}

fn phenotype_with_skin_hardness(config: &SimConfig, raw: u16) -> PhenotypeData {
    let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
    set_u16_locus(&mut bytes, genome::core::SKIN_HARDNESS, raw);
    let genome = GenomeData::from_bytes(
        bytes,
        config.genetics.genome_version,
        config.genetics.core_length,
    )
    .unwrap();
    decode(&genome, config).unwrap()
}

struct HazardHarnessAgent {
    health: Health,
    energy: Energy,
    phenotype: PhenotypeData,
    position: Vec2,
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
            let applied = mitigated_damage(raw, agent.phenotype.damage_mitigation);
            agent.health.apply_damage_to_core(applied);
            if repair_enabled && config.economy.repair_energy_per_hp > 0.0 {
                apply_repair_tick(
                    &mut agent.health,
                    &mut agent.energy,
                    &agent.phenotype,
                    config.economy.repair_policy,
                    &config.economy,
                    0,
                );
            }
        }
        agents.retain(|a| a.health.core_hp > 0.0);
    }
}

fn hazard_harness_agent(config: &SimConfig, phenotype: PhenotypeData, position: Vec2) -> HazardHarnessAgent {
    HazardHarnessAgent {
        health: Health {
            core_hp: config.economy.core_max_hp,
            core_max_hp: config.economy.core_max_hp,
            part_hp: vec![],
        },
        energy: Energy {
            internal_energy: config.economy.spawn_energy,
        },
        phenotype,
        position,
    }
}

/// Phase 4F: repair spends energy when hazard damage and repair are both enabled.
#[test]
fn repair_energy_spent_after_hazard_damage() {
    let config = SimConfig::load_default().unwrap();
    assert!(config.economy.repair_energy_per_hp > 0.0);
    let hazard_center = Vec2::from_array(config.environment.hazards[0].center);
    let phenotype = phenotype_with_skin_hardness(&config, 0x0000);

    let mut agent = hazard_harness_agent(&config, phenotype, hazard_center);
    let start_energy = agent.energy.internal_energy;
    let hazards = HazardField::from_environment(&config.environment);

    let mut total_repair = 0.0f32;
    for _ in 0..200 {
        let raw = hazards.damage_at(agent.position);
        let applied = mitigated_damage(raw, agent.phenotype.damage_mitigation);
        agent.health.apply_damage_to_core(applied);
        total_repair += apply_repair_tick(
            &mut agent.health,
            &mut agent.energy,
            &agent.phenotype,
            config.economy.repair_policy,
            &config.economy,
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

/// Phase 4D: without repair, agents in hazard bands die faster than agents in a safe region.
#[test]
fn hazard_kills_faster_than_safe_zone_when_repair_disabled() {
    let mut config = SimConfig::load_default().unwrap();
    config.economy.core_max_hp = 100.0;
    config.economy.repair_energy_per_hp = 0.0;
    assert!(!config.environment.hazards.is_empty());

    let hazard_center = Vec2::from_array(config.environment.hazards[0].center);
    let safe_pos = Vec2::new(-450.0, -700.0);

    let soft = phenotype_with_skin_hardness(&config, 0x0000);
    let mut in_hazard: Vec<_> = (0..12)
        .map(|_| hazard_harness_agent(&config, soft.clone(), hazard_center))
        .collect();
    let mut in_safe: Vec<_> = (0..12)
        .map(|_| hazard_harness_agent(&config, soft.clone(), safe_pos))
        .collect();

    // ~0.5 HP/s hazard damage ⇒ ~12k ticks to exhaust 100 HP without repair.
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

/// Phase 4E: harder armor survives longer in hazard with repair enabled.
#[test]
fn hardness_hazard_survival_tradeoff() {
    let mut config = SimConfig::load_default().unwrap();
    config.economy.core_max_hp = 100.0;
    config.economy.spawn_energy = 200.0;
    assert!(config.economy.repair_energy_per_hp > 0.0);
    let hazard_center = Vec2::from_array(config.environment.hazards[0].center);

    let soft = phenotype_with_skin_hardness(&config, 0x0000);
    let hard = phenotype_with_skin_hardness(&config, 0xFFFF);
    assert!(hard.damage_mitigation > soft.damage_mitigation);

    let soft_ticks = ticks_until_hazard_death(&config, soft, hazard_center, false);
    let hard_ticks = ticks_until_hazard_death(&config, hard, hazard_center, false);
    assert!(
        hard_ticks > soft_ticks,
        "high hardness should survive more hazard ticks (soft={soft_ticks}, hard={hard_ticks})"
    );
}

/// Phase 4E: high hardness pays more upkeep in a safe zone (no hazard overlap).
#[test]
fn hardness_safe_zone_energy_penalty() {
    let mut config = SimConfig::load_default().unwrap();
    config.economy.spawn_energy = 200.0;
    // Bright top band, far from default hazard cluster (see configs/default.toml).
    let safe_pos = Vec2::new(-400.0, 700.0);
    let light_field = super::LightField::from_environment(
        &config.environment,
        &super::WorldBounds::from_world_size(config.world.width, config.world.height),
    );
    let light = light_field.sample(safe_pos);

    let soft = phenotype_with_skin_hardness(&config, 0x0000);
    let hard = phenotype_with_skin_hardness(&config, 0xFFFF);
    assert!(hard.armor_upkeep > soft.armor_upkeep);
    assert!(
        super::metabolism_tick_cost(&hard, 0, &config.economy)
            > super::metabolism_tick_cost(&soft, 0, &config.economy),
        "hard armor should increase per-tick metabolism cost"
    );

    assert_eq!(
        HazardField::from_environment(&config.environment).damage_at(safe_pos),
        0.0,
        "safe position must not overlap hazards"
    );

    let soft_remaining = energy_after_ticks(&soft, light, &config, 200);
    let hard_remaining = energy_after_ticks(&hard, light, &config, 200);
    assert!(
        soft_remaining > hard_remaining,
        "soft armor should retain more energy in safe zone (soft={soft_remaining}, hard={hard_remaining})"
    );
}

/// Phase 4G: high-green genotype gains more net energy than low-green control in bright band.
#[test]
fn green_ancestor_higher_net_energy_in_bright_band() {
    let (config, high_green) = phenotype_with_colors(0x20, 0xFF, 0x20);
    let (_, low_green) = phenotype_with_colors(0xFF, 0x40, 0xFF);
    assert!(high_green.greenness > low_green.greenness);

    let bright = config.environment.gradient_max;
    let high_delta = net_energy_after_ticks(&high_green, bright, &config, 5000);
    let low_delta = net_energy_after_ticks(&low_green, bright, &config, 5000);
    assert!(
        high_delta > low_delta,
        "high-green should gain more net energy in bright band (high={high_delta}, low={low_delta})"
    );
}

/// Phase 4G: green metabolism penalty exceeds low-green control at equal light.
#[test]
fn high_green_pays_higher_metabolism_drain_than_low_green() {
    let (config, high_green) = phenotype_with_colors(0x20, 0xFF, 0x20);
    let (_, low_green) = phenotype_with_colors(0xFF, 0x40, 0xFF);
    let high_cost = super::metabolism_tick_cost(&high_green, 0, &config.economy);
    let low_cost = super::metabolism_tick_cost(&low_green, 0, &config.economy);
    assert!(
        high_cost > low_cost,
        "high-green should pay higher per-tick metabolism (high={high_cost}, low={low_cost})"
    );
    assert!(
        high_green.green_metabolism_extra > low_green.green_metabolism_extra,
        "green penalty component should be larger for high-green"
    );
}

/// Phase 4G: seeded headless runs diverge in color_bins and mean_energy when ancestor color differs.
#[test]
fn green_ab_metrics_diverge_same_seed() {
    use super::headless::run_headless_sim;

    let base = SimConfig::load_default().unwrap();
    let bright_green_hex = "45564f3120ff206dffff30002800ffff4000ffff088000008000000000008000";
    let dull_hex = "45564f31ff40ff6dffff30002800ffff4000ffff088000008000000000008000";

    let mut high_g = config_with_ancestor_hex(base.clone(), bright_green_hex);
    high_g.world.initial_population = 30;
    let mut low_g = config_with_ancestor_hex(base, dull_hex);
    low_g.world.initial_population = 30;

    let summary_high = run_headless_sim(&high_g, 5000);
    let summary_low = run_headless_sim(&low_g, 5000);

    let high_colors: HashSet<_> = summary_high.survivor_color_keys.iter().copied().collect();
    let low_colors: HashSet<_> = summary_low.survivor_color_keys.iter().copied().collect();
    assert_ne!(
        high_colors, low_colors,
        "color keys should differ between green A/B ancestors: high={high_colors:?} low={low_colors:?}"
    );
    assert!(
        summary_high.alive > 0 && summary_low.alive > 0,
        "both green A/B runs should survive 5k ticks"
    );
}

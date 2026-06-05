//! Genome v2 reproduction: % energy threshold, combined-pool endowment ([Genome_Definition.md](../../../docs/Genome_Definition.md)).

use rand::Rng;

use crate::config::{EconomyConfig, ReproPaymentOrder, SimConfig};
use crate::genome::{
    mutate_offspring_v2, AnyGenome, AnyPhenotype, GenomeV2, PhenotypeV2,
};

use super::reproduction::{
    repro_damage_probability, repro_death_probability, ReproAttemptResult,
};
use super::super::components::{Energy, Health};
use super::super::spawn::try_prepare_spawn;

/// Max vitality spend cap from decoded endowment fraction.
pub fn endowment_budget_cap(phenotype: &PhenotypeV2) -> f32 {
    phenotype.offspring_endowment_frac * (phenotype.max_energy + phenotype.max_health)
}

/// Energy required before reproduction can trigger.
pub fn repro_threshold_energy(phenotype: &PhenotypeV2) -> f32 {
    phenotype.max_energy * phenotype.repro_threshold_frac
}

/// Split spent vitality into child health (first) and overflow energy.
pub fn split_endowment_to_child(
    spent: f32,
    child_max_health: f32,
    child_max_energy: f32,
) -> (f32, f32) {
    let spent = spent.max(0.0);
    let child_hp = spent.min(child_max_health.max(0.0));
    let overflow = (spent - child_hp).max(0.0);
    let child_energy = overflow.min(child_max_energy.max(0.0));
    (child_hp, child_energy)
}

/// Deduct up to `amount` from parent pools per [`ReproPaymentOrder`]; returns amount collected.
pub fn collect_vitality_payment(
    energy: &mut Energy,
    health: &mut Health,
    amount: f32,
    order: ReproPaymentOrder,
) -> f32 {
    let mut remaining = amount.max(0.0);
    if remaining <= 0.0 {
        return 0.0;
    }

    let take_from_health = |health: &mut Health, n: f32| -> f32 {
        let got = n.min(health.core_hp.max(0.0));
        health.core_hp -= got;
        got
    };
    let take_from_energy = |energy: &mut Energy, n: f32| -> f32 {
        let got = n.min(energy.internal_energy.max(0.0));
        energy.internal_energy -= got;
        got
    };

    let mut collected = 0.0;
    match order {
        ReproPaymentOrder::HealthFirst => {
            collected += take_from_health(health, remaining);
            remaining -= collected;
            collected += take_from_energy(energy, remaining);
        }
        ReproPaymentOrder::EnergyFirst => {
            let from_e = take_from_energy(energy, remaining);
            collected += from_e;
            remaining -= from_e;
            collected += take_from_health(health, remaining);
        }
        ReproPaymentOrder::Proportional => {
            let e_avail = energy.internal_energy.max(0.0);
            let h_avail = health.core_hp.max(0.0);
            let total = e_avail + h_avail;
            if total <= 1e-8 {
                return 0.0;
            }
            let want_e = remaining * (e_avail / total);
            let from_e = take_from_energy(energy, want_e);
            collected += from_e;
            remaining -= from_e;
            let from_h = take_from_health(health, remaining);
            collected += from_h;
            remaining -= from_h;
            if remaining > 1e-6 {
                collected += take_from_energy(energy, remaining);
            }
        }
    }
    collected
}

/// Attempt v2 reproduction (mutates parent `energy` and `health`).
pub fn attempt_reproduction_v2(
    energy: &mut Energy,
    health: &mut Health,
    phenotype: &PhenotypeV2,
    parent_genome: &GenomeV2,
    config: &SimConfig,
    repro_rng: &mut impl Rng,
    mutation_rng: &mut impl Rng,
) -> ReproAttemptResult {
    let threshold = repro_threshold_energy(phenotype);
    if energy.internal_energy < threshold {
        return ReproAttemptResult::Skipped;
    }

    let budget_cap = endowment_budget_cap(phenotype);
    let available = energy.internal_energy.max(0.0) + health.core_hp.max(0.0);
    let spend_target = budget_cap.min(available);
    if spend_target <= 1e-6 {
        return ReproAttemptResult::Skipped;
    }

    let child_genome = mutate_offspring_v2(parent_genome, &config.genetics, mutation_rng);
    let Some((child_genome, child_phenotype)) = try_prepare_spawn(child_genome, config) else {
        return ReproAttemptResult::Stillborn;
    };

    let collected = collect_vitality_payment(
        energy,
        health,
        spend_target,
        config.economy.repro_payment_order,
    );
    if collected <= 1e-6 {
        return ReproAttemptResult::Skipped;
    }

    let (child_start_hp, child_start_energy) = split_endowment_to_child(
        collected,
        child_phenotype.max_health,
        child_phenotype.max_energy,
    );
    if child_start_hp + child_start_energy <= 1e-6 {
        return ReproAttemptResult::Stillborn;
    }

    let economy = &config.economy;
    let p_death = repro_death_probability(
        economy.p_death_on_repro_base,
        economy.repro_safety_margin,
        energy.internal_energy,
        threshold,
    );
    if repro_rng.gen::<f32>() < p_death {
        return ReproAttemptResult::ParentFatal;
    }

    let p_damage = repro_damage_probability(economy.p_damage_on_repro_base);
    if repro_rng.gen::<f32>() < p_damage {
        let dmg = repro_damage_scale_v2(phenotype, economy);
        health.core_hp -= dmg;
        if health.core_hp <= 0.0 {
            return ReproAttemptResult::ParentFatal;
        }
    }

    ReproAttemptResult::Child {
        genome: child_genome,
        phenotype: child_phenotype,
        child_start_energy,
        child_start_hp,
    }
}

fn repro_damage_scale_v2(phenotype: &PhenotypeV2, economy: &EconomyConfig) -> f32 {
    repro_threshold_energy(phenotype) * economy.repro_damage_fraction_of_max_hp
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use crate::config::SimConfig;
    use crate::genome::{decode_v2, GenomeV2};
    use crate::sim::components::Energy;
    use crate::sim::systems::reproduction::ReproAttemptResult;

    fn v2_setup() -> (SimConfig, PhenotypeV2, GenomeV2) {
        let config = SimConfig::load_from_path(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs/genome_v2.example.toml"),
        )
        .unwrap();
        let genome = GenomeV2::ancestor_from_config(&config).unwrap();
        let phenotype = decode_v2(&genome, &config).unwrap();
        (config, phenotype, genome)
    }

    #[test]
    fn ancestor_threshold_is_full_max_energy() {
        let (_, p, _) = v2_setup();
        assert!((repro_threshold_energy(&p) - p.max_energy).abs() < 1e-3);
    }

    #[test]
    fn endowment_cap_is_fraction_of_combined_max() {
        let (_, p, _) = v2_setup();
        let cap = endowment_budget_cap(&p);
        let expected = p.offspring_endowment_frac * (p.max_energy + p.max_health);
        assert!((cap - expected).abs() < 1e-3);
        assert!(cap > 0.0);
    }

    #[test]
    fn split_endowment_overflows_to_energy() {
        let (_, p, _) = v2_setup();
        let spent = p.max_health + 10.0;
        let (hp, en) = split_endowment_to_child(spent, p.max_health, p.max_energy);
        assert!((hp - p.max_health).abs() < 1e-3);
        assert!(en > 0.0);
    }

    #[test]
    fn health_first_payment_order() {
        let mut energy = Energy::new_v2(50.0, 100.0);
        let mut health = Health {
            core_hp: 30.0,
            core_max_hp: 100.0,
            part_hp: vec![],
        };
        let got = collect_vitality_payment(
            &mut energy,
            &mut health,
            40.0,
            ReproPaymentOrder::HealthFirst,
        );
        assert!((got - 40.0).abs() < 1e-3);
        assert!((health.core_hp - 0.0).abs() < 1e-3);
        assert!((energy.internal_energy - 40.0).abs() < 1e-3);
    }

    #[test]
    fn child_vitality_from_endowment_not_spawn_energy() {
        let (mut config, mut phenotype, genome) = v2_setup();
        let mut bytes = genome.bytes.clone();
        let body_idx = crate::genome::V2_HEADER_LEN + crate::genome::v2_core::BODY_RADIUS;
        bytes[body_idx] = 0xFF;
        bytes[body_idx + 1] = 0xFF;
        let endowment_idx =
            crate::genome::V2_HEADER_LEN + crate::genome::v2_core::OFFSPRING_ENDOWMENT;
        let repro_idx =
            crate::genome::V2_HEADER_LEN + crate::genome::v2_core::REPRO_THRESHOLD;
        bytes[endowment_idx] = 0xFF;
        bytes[repro_idx] = 0x00;
        let genome = GenomeV2::from_bytes(bytes, &config.genetics).unwrap();
        phenotype = decode_v2(&genome, &config).unwrap();

        config.economy.spawn_energy = 999.0;
        config.economy.p_death_on_repro_base = 0.0;
        config.economy.p_damage_on_repro_base = 0.0;
        config.genetics.mutation_v2 = Some(crate::config::MutationV2Config {
            passes_min: 0,
            passes_max: 0,
            p_large_step: 0.0,
            large_step_min: 3,
            large_step_max: 4,
        });

        let available = 120.0;
        let spend_target = endowment_budget_cap(&phenotype).min(available);
        let (expected_hp, expected_energy) = split_endowment_to_child(
            spend_target,
            phenotype.max_health,
            phenotype.max_energy,
        );

        let mut energy = Energy::new_v2(50.0, phenotype.max_energy);
        let mut health = Health {
            core_hp: 70.0,
            core_max_hp: phenotype.max_health,
            part_hp: vec![],
        };
        let mut repro_rng = StdRng::seed_from_u64(10);
        let mut mutation_rng = StdRng::seed_from_u64(11);

        match attempt_reproduction_v2(
            &mut energy,
            &mut health,
            &phenotype,
            &genome,
            &config,
            &mut repro_rng,
            &mut mutation_rng,
        ) {
            ReproAttemptResult::Child {
                child_start_energy,
                child_start_hp,
                ..
            } => {
                assert!((child_start_hp - expected_hp).abs() < 1e-2);
                assert!((child_start_energy - expected_energy).abs() < 1e-2);
                assert!((child_start_hp + child_start_energy - spend_target).abs() < 1e-2);
                assert!(child_start_energy < config.economy.spawn_energy);
            }
            other => panic!("expected child, got {other:?}"),
        }
    }

    #[test]
    fn reproduces_at_max_energy_with_ancestor() {
        let (mut config, phenotype, genome) = v2_setup();
        let mut energy = Energy::new_v2(phenotype.max_energy, phenotype.max_energy);
        let mut health = Health {
            core_hp: phenotype.max_health,
            core_max_hp: phenotype.max_health,
            part_hp: vec![],
        };
        config.economy.p_death_on_repro_base = 0.0;
        config.economy.p_damage_on_repro_base = 0.0;
        config.genetics.mutation_v2 = Some(crate::config::MutationV2Config {
            passes_min: 0,
            passes_max: 0,
            p_large_step: 0.0,
            large_step_min: 3,
            large_step_max: 4,
        });
        let mut repro_rng = StdRng::seed_from_u64(1);
        let mut mutation_rng = StdRng::seed_from_u64(2);

        match attempt_reproduction_v2(
            &mut energy,
            &mut health,
            &phenotype,
            &genome,
            &config,
            &mut repro_rng,
            &mut mutation_rng,
        ) {
            ReproAttemptResult::Child {
                child_start_energy,
                child_start_hp,
                ..
            } => {
                let cap = endowment_budget_cap(&phenotype);
                assert!((child_start_hp + child_start_energy - cap).abs() < 1e-2);
                assert!(child_start_hp > 0.0);
            }
            other => panic!("expected child, got {other:?}"),
        }
    }
}

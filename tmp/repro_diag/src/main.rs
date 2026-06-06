//! One-off reproduction vitality / energy accounting diagnostic.

use evolution_sim::config::{ReproPaymentOrder, SimConfig};
use evolution_sim::genome::{decode_v2, GenomeV2};
use evolution_sim::sim::{attempt_reproduction_v2, endowment_budget_cap, Energy, Health, ReproAttemptResult};
use rand::SeedableRng;
use rand::rngs::StdRng;

fn main() {
    let mut config = SimConfig::load_default().unwrap();
    config.economy.p_death_on_repro_base = 0.0;
    config.economy.p_damage_on_repro_base = 0.0;
    config.genetics.mutation_v2 = Some(evolution_sim::config::MutationV2Config {
        passes_min: 0,
        passes_max: 0,
        p_large_step: 0.0,
        large_step_min: 3,
        large_step_max: 4,
    });

    let genome = GenomeV2::ancestor_from_config(&config).unwrap();
    let phenotype = decode_v2(&genome, &config).unwrap();

    println!("=== ancestor phenotype ===");
    println!("max_energy={:.2}", phenotype.max_energy);
    println!("max_health={:.2}", phenotype.max_health);
    println!(
        "repro_threshold={:.2} ({:.1}%)",
        phenotype.max_energy * phenotype.repro_threshold_frac,
        phenotype.repro_threshold_frac * 100.0
    );
    println!(
        "endowment_cap={:.2} ({:.1}%)",
        endowment_budget_cap(&phenotype),
        phenotype.offspring_endowment_frac * 100.0
    );
    println!("spawn_energy={:.2}", config.economy.spawn_energy);
    println!("repro_payment_order={:?}", config.economy.repro_payment_order);

    let parent_e = phenotype.max_energy;
    let parent_h = phenotype.max_health;
    let total_before = parent_e + parent_h;

    let mut energy = Energy::new_v2(parent_e, phenotype.max_energy);
    let mut health = Health {
        core_hp: parent_h,
        core_max_hp: phenotype.max_health,
        part_hp: vec![],
    };

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
            let parent_e_after = energy.internal_energy;
            let parent_h_after = health.core_hp;
            let child_e = child_start_energy;
            let child_h = child_start_hp;
            let total_after = parent_e_after + parent_h_after + child_e + child_h;
            let energy_sum_before = parent_e;
            let energy_sum_after = parent_e_after + child_e;

            println!("\n=== successful reproduction ===");
            println!("parent energy: {parent_e:.2} -> {parent_e_after:.2} (delta {:.2})", parent_e_after - parent_e);
            println!("parent health: {parent_h:.2} -> {parent_h_after:.2} (delta {:.2})", parent_h_after - parent_h);
            println!("child energy: {child_e:.2}");
            println!("child health: {child_h:.2}");
            println!("total vitality: {total_before:.2} -> {total_after:.2} (delta {:.2})", total_after - total_before);
            println!(
                "sum(internal_energy): {energy_sum_before:.2} -> {energy_sum_after:.2} (delta {:.2})",
                energy_sum_after - energy_sum_before
            );
            if child_e > (parent_e - parent_e_after).max(0.0) + 1e-3 {
                println!(
                    "\nNOTE: child energy ({child_e:.2}) exceeds parent energy spent ({:.2})",
                    (parent_e - parent_e_after).max(0.0)
                );
            }
            if (total_after - total_before).abs() > 1e-2 {
                println!("\nBUG: vitality not conserved!");
            }
        }
        other => println!("\nrepro result: {other:?}"),
    }

    // Compare payment orders
    println!("\n=== payment order comparison (same starting vitals) ===");
    for order in [
        ReproPaymentOrder::HealthFirst,
        ReproPaymentOrder::EnergyFirst,
        ReproPaymentOrder::Proportional,
    ] {
        config.economy.repro_payment_order = order;
        let mut energy = Energy::new_v2(parent_e, phenotype.max_energy);
        let mut health = Health {
            core_hp: parent_h,
            core_max_hp: phenotype.max_health,
            part_hp: vec![],
        };
        let mut repro_rng = StdRng::seed_from_u64(1);
        let mut mutation_rng = StdRng::seed_from_u64(2);
        if let ReproAttemptResult::Child {
            child_start_energy,
            child_start_hp,
            ..
        } = attempt_reproduction_v2(
            &mut energy,
            &mut health,
            &phenotype,
            &genome,
            &config,
            &mut repro_rng,
            &mut mutation_rng,
        ) {
            let e_delta = parent_e - energy.internal_energy;
            println!(
                "{order:?}: parent E -{e_delta:.2}, child E +{child_start_energy:.2}, child H +{child_start_hp:.2}, total E delta {:.2}",
                child_start_energy + energy.internal_energy - parent_e
            );
        }
    }

    // Mutation loop: when child max_health is small, health-first payment can inflate total energy.
    config.economy.repro_payment_order = ReproPaymentOrder::HealthFirst;
    config.genetics.mutation_v2 = Some(evolution_sim::config::MutationV2Config {
        passes_min: 2,
        passes_max: 8,
        p_large_step: 0.50,
        large_step_min: 3,
        large_step_max: 4,
    });
    println!("\n=== mutation loop (health_first, default mutation) ===");
    let mut energy_inflation_cases = 0u32;
    let trials = 5000u32;
    for seed in 0..trials {
        let mut energy = Energy::new_v2(parent_e, phenotype.max_energy);
        let mut health = Health {
            core_hp: parent_h,
            core_max_hp: phenotype.max_health,
            part_hp: vec![],
        };
        let mut repro_rng = StdRng::seed_from_u64(seed as u64 + 1000);
        let mut mutation_rng = StdRng::seed_from_u64(seed as u64 + 2000);
        if let ReproAttemptResult::Child {
            child_start_energy,
            child_start_hp,
            phenotype: child_phenotype,
            ..
        } = attempt_reproduction_v2(
            &mut energy,
            &mut health,
            &phenotype,
            &genome,
            &config,
            &mut repro_rng,
            &mut mutation_rng,
        ) {
            let parent_e_spent = parent_e - energy.internal_energy;
            if child_start_energy > parent_e_spent + 1e-3 {
                energy_inflation_cases += 1;
                if energy_inflation_cases <= 5 {
                    let child = &child_phenotype;
                    println!(
                        "  seed {seed}: child E={child_start_energy:.2} > parent E spent={parent_e_spent:.2} (child max_h={:.2}, endowment={:.2})",
                        child.max_health,
                        child_start_energy + child_start_hp
                    );
                }
            }
            let _ = child_phenotype;
            let _ = child_start_hp;
        }
    }
    println!(
        "trials={trials}: child_energy > parent_energy_spent in {energy_inflation_cases} cases"
    );

    // Forced ratio mutation: tiny child max_health makes endowment overflow to energy.
    println!("\n=== forced energy-heavy child (ratio byte = 0x00) ===");
    let mut bytes = genome.bytes.clone();
    let ratio_idx = evolution_sim::genome::V2_HEADER_LEN + evolution_sim::genome::v2_core::ENERGY_HEALTH_RATIO;
    bytes[ratio_idx] = 0x00;
    let child_genome = GenomeV2::from_bytes(bytes, &config.genetics).unwrap();
    let child_phenotype = decode_v2(&child_genome, &config).unwrap();
    println!(
        "child max_energy={:.2}, max_health={:.2}",
        child_phenotype.max_energy, child_phenotype.max_health
    );
    let spent = endowment_budget_cap(&phenotype).min(parent_e + parent_h);
    let (hp, en) = evolution_sim::sim::split_endowment_to_child(
        spent,
        child_phenotype.max_health,
        child_phenotype.max_energy,
    );
    println!("planned split: spent={spent:.2} -> child H={hp:.2}, E={en:.2}");
    let mut energy = Energy::new_v2(parent_e, phenotype.max_energy);
    let mut health = Health {
        core_hp: parent_h,
        core_max_hp: phenotype.max_health,
        part_hp: vec![],
    };
    config.genetics.mutation_v2 = Some(evolution_sim::config::MutationV2Config {
        passes_min: 0,
        passes_max: 0,
        p_large_step: 0.0,
        large_step_min: 3,
        large_step_max: 4,
    });
    let Some((paid_hp, paid_energy)) = evolution_sim::sim::pay_child_endowment(
        &mut energy,
        &mut health,
        spent,
        child_phenotype.max_health,
        child_phenotype.max_energy,
        ReproPaymentOrder::HealthFirst,
    ) else {
        println!("pay_child_endowment failed");
        return;
    };
    println!(
        "after payment: parent E={:.2} (spent {:.2}), parent H={:.2}, child H={paid_hp:.2}, child E={paid_energy:.2}",
        energy.internal_energy,
        parent_e - energy.internal_energy,
        health.core_hp,
    );
    let parent_energy_spent = parent_e - energy.internal_energy;
    assert!(paid_energy <= parent_energy_spent + 1e-3);
    println!(
        "sum(internal_energy) delta {:.2} (should be 0)",
        energy.internal_energy + paid_energy - parent_e
    );
}

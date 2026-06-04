//! Genome v2 healing: `healing_rate` u16 throughput and cost multiplier ([Genome_Definition.md](../../../docs/Genome_Definition.md)).

use bevy::prelude::*;

use crate::config::{EconomyConfig, RepairPolicy};
use crate::genome::PhenotypeV2;

use super::healing::{
    apply_hp_to_slot, collect_damaged_slots, pick_slot_index, repair_energy_cost,
};
use super::super::components::{Energy, Health, Phenotype};
use super::super::metrics::RepairStats;
use super::super::world::SimConfigResource;

/// Max HP restorable this tick from decoded heal rate (per-second → `dt`).
pub fn heal_hp_cap_per_tick(heal_hp_per_sec: f32, dt: f32) -> f32 {
    heal_hp_per_sec.max(0.0) * dt.max(0.0)
}

/// Energy cost to restore HP at v2 exchange rate × gene cost multiplier.
pub fn heal_energy_cost(hp_repaired: f32, repair_energy_per_hp: f32, heal_cost_multiplier: f32) -> f32 {
    repair_energy_cost(hp_repaired, repair_energy_per_hp) * heal_cost_multiplier.max(0.0)
}

/// Repair as much HP as energy and decoded heal rate allow; returns energy spent.
pub fn apply_heal_tick_v2(
    health: &mut Health,
    energy: &mut Energy,
    phenotype: &PhenotypeV2,
    policy: RepairPolicy,
    economy: &EconomyConfig,
    dt: f32,
    symmetric_cursor: usize,
) -> f32 {
    if economy.repair_energy_per_hp <= 0.0 {
        return 0.0;
    }

    let hp_cap = heal_hp_cap_per_tick(phenotype.heal_hp_per_sec, dt);
    if hp_cap <= 1e-6 {
        return 0.0;
    }

    let cost_per_hp = economy.repair_energy_per_hp * phenotype.heal_cost_multiplier;
    let mut spent = 0.0;
    let mut hp_repaired_this_tick = 0.0;
    let mut cursor = symmetric_cursor;

    loop {
        if hp_repaired_this_tick >= hp_cap - 1e-6 {
            break;
        }

        let slots = collect_damaged_slots(health);
        if slots.is_empty() || energy.internal_energy <= 1e-6 {
            break;
        }

        let slot_idx = pick_slot_index(&slots, policy, cursor);
        let slot = &slots[slot_idx];
        let deficit = slot.max - slot.current;
        if deficit <= 1e-6 {
            break;
        }

        let hp_budget = (hp_cap - hp_repaired_this_tick).max(0.0);
        let affordable_hp = energy.internal_energy / cost_per_hp;
        if affordable_hp <= 1e-6 {
            break;
        }

        let hp_repaired = affordable_hp.min(deficit).min(hp_budget);
        let cost = heal_energy_cost(hp_repaired, economy.repair_energy_per_hp, phenotype.heal_cost_multiplier);
        apply_hp_to_slot(health, slot.part_index, hp_repaired);
        energy.internal_energy = (energy.internal_energy - cost).max(0.0);
        spent += cost;
        hp_repaired_this_tick += hp_repaired;

        if policy == RepairPolicy::Symmetric {
            cursor = cursor.wrapping_add(1);
        }
    }

    spent
}

/// Bevy system: v2 agents use decoded healing rate and cost penalty.
pub fn healing_v2(
    time: Res<Time>,
    config: Res<SimConfigResource>,
    mut repair_stats: ResMut<RepairStats>,
    mut query: Query<(Entity, &Phenotype, &mut Health, &mut Energy)>,
) {
    let economy = &config.0.economy;
    let policy = economy.repair_policy;
    let dt = time.delta_secs();

    for (entity, phenotype, mut health, mut energy) in &mut query {
        let Some(phenotype_v2) = phenotype.v2() else {
            continue;
        };
        let cursor = entity.to_bits() as usize;
        let spent = apply_heal_tick_v2(
            &mut health,
            &mut energy,
            phenotype_v2,
            policy,
            economy,
            dt,
            cursor,
        );
        repair_stats.energy_spent_this_period += spent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::genome::{v2_core, GenomeV2, V2_HEADER_LEN};
    use crate::genome::decode_v2;
    use crate::sim::components::Energy;

    fn v2_config() -> SimConfig {
        SimConfig::load_from_path(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs/genome_v2.example.toml"),
        )
        .unwrap()
    }

    fn phenotype_with_healing_raw(raw: u16) -> (SimConfig, PhenotypeV2) {
        let config = v2_config();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        let i = V2_HEADER_LEN + v2_core::HEALING_RATE;
        bytes[i] = (raw >> 8) as u8;
        bytes[i + 1] = (raw & 0xFF) as u8;
        let genome = GenomeV2::from_bytes(bytes, &config.genetics).unwrap();
        let p = decode_v2(&genome, &config).unwrap();
        (config, p)
    }

    #[test]
    fn ancestor_uses_base_heal_rate() {
        let (config, p) = phenotype_with_healing_raw(0);
        let d2 = config.genetics.decode_v2.as_ref().unwrap();
        assert!((p.heal_hp_per_sec - d2.heal_base_hp_per_sec).abs() < 1e-4);
        assert!((p.heal_cost_multiplier - 1.0).abs() < 1e-4);
    }

    #[test]
    fn fast_healing_costs_more_energy_per_hp() {
        let (config, slow) = phenotype_with_healing_raw(0x1000);
        let (_, fast) = phenotype_with_healing_raw(0xF000);
        assert!(fast.heal_hp_per_sec > slow.heal_hp_per_sec);
        assert!(fast.heal_cost_multiplier > slow.heal_cost_multiplier);

        let dt = 1.0;
        let mut health_slow = Health {
            core_hp: 50.0,
            core_max_hp: 100.0,
            part_hp: vec![],
        };
        let mut health_fast = health_slow.clone();
        let mut energy_slow = Energy::new_v2(200.0, 200.0);
        let mut energy_fast = energy_slow.clone();

        let spent_slow = apply_heal_tick_v2(
            &mut health_slow,
            &mut energy_slow,
            &slow,
            RepairPolicy::CoreFirst,
            &config.economy,
            dt,
            0,
        );
        let spent_fast = apply_heal_tick_v2(
            &mut health_fast,
            &mut energy_fast,
            &fast,
            RepairPolicy::CoreFirst,
            &config.economy,
            dt,
            0,
        );

        assert!(health_fast.core_hp > health_slow.core_hp);
        let hp_slow = health_slow.core_hp - 50.0;
        let hp_fast = health_fast.core_hp - 50.0;
        assert!(hp_fast > hp_slow);
        assert!(spent_fast > spent_slow);
        let cost_per_hp_slow = spent_slow / hp_slow.max(1e-6);
        let cost_per_hp_fast = spent_fast / hp_fast.max(1e-6);
        assert!(cost_per_hp_fast > cost_per_hp_slow);
    }
}

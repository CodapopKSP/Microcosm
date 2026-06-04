//! Energy-funded HP repair after damage (Phase 4D).
//!
//! Cost per HP: `hp_repaired * repair_energy_per_hp`. Throughput per tick is capped by
//! `repair_rate * economy.repair_hp_per_tick_at_rate_1` ([GENOME_REVISIONS.md](../../../docs/GENOME_REVISIONS.md) #6).

use bevy::prelude::*;

use crate::config::{EconomyConfig, RepairPolicy};
use crate::genome::Phenotype as PhenotypeData;

use super::super::components::{Energy, Health, Phenotype};
use super::super::metrics::RepairStats;
use super::super::world::SimConfigResource;

/// One repairable HP slot (core or appendage index).
#[derive(Debug, Clone, Copy)]
pub(crate) struct RepairSlot {
    pub part_index: Option<usize>,
    pub current: f32,
    pub max: f32,
}

/// Energy cost to restore `hp_repaired` HP at the fixed config exchange rate.
pub fn repair_energy_cost(hp_repaired: f32, repair_energy_per_hp: f32) -> f32 {
    if hp_repaired <= 0.0 || repair_energy_per_hp <= 0.0 {
        return 0.0;
    }
    hp_repaired * repair_energy_per_hp
}

/// Max HP that can be restored this tick at the given repair rate.
pub fn repair_hp_cap_per_tick(repair_rate: f32, economy: &EconomyConfig) -> f32 {
    repair_rate.max(0.0) * economy.repair_hp_per_tick_at_rate_1
}

fn part_max_hp(core_max_hp: f32) -> f32 {
    core_max_hp
}

pub(crate) fn collect_damaged_slots(health: &Health) -> Vec<RepairSlot> {
    let mut slots = Vec::new();
    if health.core_hp < health.core_max_hp {
        slots.push(RepairSlot {
            part_index: None,
            current: health.core_hp,
            max: health.core_max_hp,
        });
    }
    let part_max = part_max_hp(health.core_max_hp);
    for (i, &hp) in health.part_hp.iter().enumerate() {
        if hp < part_max {
            slots.push(RepairSlot {
                part_index: Some(i),
                current: hp,
                max: part_max,
            });
        }
    }
    slots
}

pub(crate) fn pick_slot_index(
    slots: &[RepairSlot],
    policy: RepairPolicy,
    symmetric_cursor: usize,
) -> usize {
    match policy {
        RepairPolicy::CoreFirst => slots
            .iter()
            .position(|s| s.part_index.is_none())
            .unwrap_or(0),
        RepairPolicy::Cheapest => slots
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let da = a.max - a.current;
                let db = b.max - b.current;
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
            .unwrap_or(0),
        RepairPolicy::Symmetric => symmetric_cursor % slots.len(),
    }
}

pub(crate) fn apply_hp_to_slot(health: &mut Health, part_index: Option<usize>, hp: f32) {
    match part_index {
        None => health.core_hp = (health.core_hp + hp).min(health.core_max_hp),
        Some(i) => {
            if let Some(slot) = health.part_hp.get_mut(i) {
                let max = part_max_hp(health.core_max_hp);
                *slot = (*slot + hp).min(max);
            }
        }
    }
}

/// Repair as much HP as energy and `repair_rate` allow this tick; returns energy spent.
pub fn apply_repair_tick(
    health: &mut Health,
    energy: &mut Energy,
    phenotype: &PhenotypeData,
    policy: RepairPolicy,
    economy: &EconomyConfig,
    symmetric_cursor: usize,
) -> f32 {
    if economy.repair_energy_per_hp <= 0.0 {
        return 0.0;
    }

    let hp_cap = repair_hp_cap_per_tick(phenotype.repair_rate, economy);
    if hp_cap <= 1e-6 {
        return 0.0;
    }

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
        let cost_per_hp = economy.repair_energy_per_hp;
        let affordable_hp = energy.internal_energy / cost_per_hp;
        if affordable_hp <= 1e-6 {
            break;
        }

        let hp_repaired = affordable_hp.min(deficit).min(hp_budget);
        let cost = repair_energy_cost(hp_repaired, economy.repair_energy_per_hp);
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

/// Bevy system: spend energy to restore HP per [`RepairPolicy`].
pub fn healing(
    config: Res<SimConfigResource>,
    mut repair_stats: ResMut<RepairStats>,
    mut query: Query<(Entity, &Phenotype, &mut Health, &mut Energy)>,
) {
    let economy = &config.0.economy;
    let policy = economy.repair_policy;

    for (entity, phenotype, mut health, mut energy) in &mut query {
        let Some(phenotype_v1) = phenotype.v1() else {
            continue;
        };
        let cursor = entity.to_bits() as usize;
        let spent = apply_repair_tick(
            &mut health,
            &mut energy,
            phenotype_v1,
            policy,
            economy,
            cursor,
        );
        repair_stats.energy_spent_this_period += spent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::genome::{core, decode, Genome, HEADER_LEN};

    fn set_u16_locus(bytes: &mut [u8], core_offset: usize, raw: u16) {
        let i = HEADER_LEN + core_offset;
        bytes[i] = (raw >> 8) as u8;
        bytes[i + 1] = (raw & 0xFF) as u8;
    }

    fn phenotype_with_repair_rate(raw: u16) -> (SimConfig, PhenotypeData) {
        let config = SimConfig::load_v1_regression().unwrap();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        set_u16_locus(&mut bytes, core::REPAIR_RATE, raw);
        let genome = Genome::from_bytes(
            bytes,
            config.genetics.genome_version,
            config.genetics.core_length,
        )
        .unwrap();
        let phenotype = decode(&genome, &config).unwrap();
        (config, phenotype)
    }

    #[test]
    fn repair_energy_cost_is_linear_in_hp() {
        assert_eq!(repair_energy_cost(10.0, 1.0), 10.0);
        assert_eq!(repair_energy_cost(0.0, 1.0), 0.0);
    }

    #[test]
    fn damaged_core_repairs_and_spends_energy() {
        let (config, phenotype) = phenotype_with_repair_rate(0x5555);
        let mut health = Health {
            core_hp: 70.0,
            core_max_hp: 100.0,
            part_hp: vec![],
        };
        let mut energy = Energy::new_v1(50.0);
        let start_energy = energy.internal_energy;
        let spent = apply_repair_tick(
            &mut health,
            &mut energy,
            &phenotype,
            RepairPolicy::CoreFirst,
            &config.economy,
            0,
        );
        assert!(health.core_hp > 70.0);
        assert!(spent > 0.0);
        assert!(energy.internal_energy < start_energy);
        let hp_gained = health.core_hp - 70.0;
        assert!((spent - hp_gained * config.economy.repair_energy_per_hp).abs() < 1e-4);
    }

    #[test]
    fn fast_repair_rate_drains_more_energy_per_tick() {
        let (mut config, slow) = phenotype_with_repair_rate(0x1000);
        let (_, fast) = phenotype_with_repair_rate(0xF000);
        config.economy.repair_hp_per_tick_at_rate_1 = 5.0;
        assert!(fast.repair_rate > slow.repair_rate);

        let mut health_slow = Health {
            core_hp: 50.0,
            core_max_hp: 100.0,
            part_hp: vec![],
        };
        let mut health_fast = health_slow.clone();
        let mut energy_slow = Energy::new_v1(100.0);
        let mut energy_fast = energy_slow.clone();

        let spent_slow = apply_repair_tick(
            &mut health_slow,
            &mut energy_slow,
            &slow,
            RepairPolicy::CoreFirst,
            &config.economy,
            0,
        );
        let spent_fast = apply_repair_tick(
            &mut health_fast,
            &mut energy_fast,
            &fast,
            RepairPolicy::CoreFirst,
            &config.economy,
            0,
        );

        let repaired_slow = health_slow.core_hp - 50.0;
        let repaired_fast = health_fast.core_hp - 50.0;
        assert!(
            repaired_fast > repaired_slow,
            "fast repair_rate should restore more HP per tick (slow={repaired_slow}, fast={repaired_fast})"
        );
        assert!(
            spent_fast > spent_slow,
            "fast repair should spend more energy per tick (slow={spent_slow}, fast={spent_fast})"
        );
        let cost_per_hp_slow = spent_slow / repaired_slow.max(1e-6);
        let cost_per_hp_fast = spent_fast / repaired_fast.max(1e-6);
        assert!(
            (cost_per_hp_slow - config.economy.repair_energy_per_hp).abs() < 1e-4
                && (cost_per_hp_fast - config.economy.repair_energy_per_hp).abs() < 1e-4,
            "energy per HP should match config, not vary with repair_rate"
        );
    }

    #[test]
    fn core_first_repairs_core_before_parts() {
        let config = SimConfig::load_v1_regression().unwrap();
        let genome = Genome::ancestor_from_config(&config).unwrap();
        let phenotype = decode(&genome, &config).unwrap();
        let mut health = Health {
            core_hp: 80.0,
            core_max_hp: 100.0,
            part_hp: vec![40.0],
        };
        let mut energy = Energy::new_v1(5.0);
        apply_repair_tick(
            &mut health,
            &mut energy,
            &phenotype,
            RepairPolicy::CoreFirst,
            &config.economy,
            0,
        );
        assert!(health.core_hp > 80.0);
        assert_eq!(health.part_hp[0], 40.0, "core_first should not repair parts until core full");
    }

    #[test]
    fn zero_repair_energy_per_hp_skips_repair() {
        let (config, phenotype) = phenotype_with_repair_rate(0x5555);
        let mut economy = config.economy.clone();
        economy.repair_energy_per_hp = 0.0;
        let mut health = Health {
            core_hp: 50.0,
            core_max_hp: 100.0,
            part_hp: vec![],
        };
        let mut energy = Energy::new_v1(100.0);
        let spent = apply_repair_tick(
            &mut health,
            &mut energy,
            &phenotype,
            RepairPolicy::CoreFirst,
            &economy,
            0,
        );
        assert_eq!(spent, 0.0);
        assert_eq!(health.core_hp, 50.0);
    }
}

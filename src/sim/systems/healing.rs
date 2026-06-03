//! Energy-funded HP repair after damage (Phase 4D).
//!
//! Runs after [`super::interactions`](super::interactions); cost per HP:
//! `(hp_repaired / repair_efficiency) * repair_energy_per_hp`.

use bevy::prelude::*;

use crate::config::{EconomyConfig, RepairPolicy};
use crate::genome::Phenotype as PhenotypeData;

use super::super::components::{Energy, Health, Phenotype};
use super::super::metrics::RepairStats;
use super::super::world::SimConfigResource;

/// One repairable HP slot (core or appendage index).
#[derive(Debug, Clone, Copy)]
struct RepairSlot {
    part_index: Option<usize>,
    current: f32,
    max: f32,
}

/// Energy cost to restore `hp_repaired` HP at given efficiency.
pub fn repair_energy_cost(
    hp_repaired: f32,
    repair_efficiency: f32,
    repair_energy_per_hp: f32,
) -> f32 {
    if hp_repaired <= 0.0 || repair_energy_per_hp <= 0.0 {
        return 0.0;
    }
    let eff = repair_efficiency.max(1e-6);
    (hp_repaired / eff) * repair_energy_per_hp
}

fn part_max_hp(core_max_hp: f32) -> f32 {
    core_max_hp
}

fn collect_damaged_slots(health: &Health) -> Vec<RepairSlot> {
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

fn pick_slot_index(slots: &[RepairSlot], policy: RepairPolicy, symmetric_cursor: usize) -> usize {
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

fn apply_hp_to_slot(health: &mut Health, part_index: Option<usize>, hp: f32) {
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

/// Repair as much HP as energy allows this tick; returns energy spent.
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

    let mut spent = 0.0;
    let mut cursor = symmetric_cursor;

    loop {
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

        let cost_per_hp =
            economy.repair_energy_per_hp / phenotype.repair_efficiency.max(1e-6);
        let affordable_hp = energy.internal_energy / cost_per_hp;
        if affordable_hp <= 1e-6 {
            break;
        }

        let hp_repaired = affordable_hp.min(deficit);
        let cost = repair_energy_cost(
            hp_repaired,
            phenotype.repair_efficiency,
            economy.repair_energy_per_hp,
        );
        apply_hp_to_slot(health, slot.part_index, hp_repaired);
        energy.internal_energy = (energy.internal_energy - cost).max(0.0);
        spent += cost;

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
        let cursor = entity.to_bits() as usize;
        let spent = apply_repair_tick(
            &mut health,
            &mut energy,
            &phenotype.0,
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

    fn phenotype_with_repair_efficiency(raw: u16) -> (SimConfig, PhenotypeData) {
        let config = SimConfig::load_default().unwrap();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        set_u16_locus(&mut bytes, core::REPAIR_EFFICIENCY, raw);
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
    fn damaged_core_repairs_and_spends_energy() {
        let (config, phenotype) = phenotype_with_repair_efficiency(0x8000);
        let mut health = Health {
            core_hp: 70.0,
            core_max_hp: 100.0,
            part_hp: vec![],
        };
        let mut energy = Energy {
            internal_energy: 50.0,
        };
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
    }

    #[test]
    fn high_repair_efficiency_costs_less_energy_for_same_hp() {
        let (config, low_eff) = phenotype_with_repair_efficiency(0x2000);
        let (_, high_eff) = phenotype_with_repair_efficiency(0xF000);

        let mut health_low = Health {
            core_hp: 50.0,
            core_max_hp: 100.0,
            part_hp: vec![],
        };
        let mut health_high = health_low.clone();
        let mut energy_low = Energy {
            internal_energy: 100.0,
        };
        let mut energy_high = energy_low.clone();

        let spent_low = apply_repair_tick(
            &mut health_low,
            &mut energy_low,
            &low_eff,
            RepairPolicy::CoreFirst,
            &config.economy,
            0,
        );
        let spent_high = apply_repair_tick(
            &mut health_high,
            &mut energy_high,
            &high_eff,
            RepairPolicy::CoreFirst,
            &config.economy,
            0,
        );

        let repaired_low = health_low.core_hp - 50.0;
        let repaired_high = health_high.core_hp - 50.0;
        assert!(
            repaired_high >= repaired_low * 0.9,
            "high efficiency should repair at least as much HP"
        );
        assert!(
            spent_high < spent_low || repaired_high > repaired_low + 0.5,
            "high efficiency should cost less energy for comparable repair (low={spent_low}, high={spent_high})"
        );
    }

    #[test]
    fn core_first_repairs_core_before_parts() {
        let config = SimConfig::load_default().unwrap();
        let genome = Genome::ancestor_from_config(&config).unwrap();
        let phenotype = decode(&genome, &config).unwrap();
        let mut health = Health {
            core_hp: 80.0,
            core_max_hp: 100.0,
            part_hp: vec![40.0],
        };
        let mut energy = Energy {
            internal_energy: 5.0,
        };
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
        let (config, phenotype) = phenotype_with_repair_efficiency(0x8000);
        let mut economy = config.economy.clone();
        economy.repair_energy_per_hp = 0.0;
        let mut health = Health {
            core_hp: 50.0,
            core_max_hp: 100.0,
            part_hp: vec![],
        };
        let mut energy = Energy {
            internal_energy: 100.0,
        };
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

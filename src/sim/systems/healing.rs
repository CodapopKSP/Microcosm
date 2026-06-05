//! Shared HP repair helpers (used by [`super::healing_v2`]).

use crate::config::{EconomyConfig, RepairPolicy};

use super::super::components::{Energy, Health};

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

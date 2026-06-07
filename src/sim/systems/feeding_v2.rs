//! Biomass / minerals / soup ingestion for genome v2 ([Genome_Definition.md](../../../docs/Genome_Definition.md)).

use bevy::prelude::*;
use std::collections::HashMap;

use super::metabolism_v2::clamp_vitality_pools;
use super::metabolism::circles_overlap;
use super::super::components::{Agent, Corpse, Energy, Health, MineralExposure, Phenotype};
use super::super::soup::SoupField;
use super::metabolism_v2::TICKS_PER_SEC;
use super::super::interval::{interval_active_this_tick, logic_active_this_tick, logic_elapsed_ticks};
use super::super::world::{SimConfigResource, SimTick};

/// Health-energy units per second at full biomass digestion: `body_radius × biomass_effective × scale`.
pub fn biomass_uptake_per_sec(
    body_radius: f32,
    biomass_effective: f32,
    uptake_scale: f32,
) -> f32 {
    if body_radius <= 0.0 || biomass_effective <= 0.0 || uptake_scale <= 0.0 {
        return 0.0;
    }
    body_radius * biomass_effective * uptake_scale
}

/// Whether biomass feeding (corpses + living prey) runs this tick (default: once per second, phase 30).
pub fn biomass_feeding_active_this_tick(tick: u64, interval_ticks: u32, phase_ticks: u32) -> bool {
    interval_active_this_tick(tick, interval_ticks, phase_ticks)
}

/// Drain from a corpse or prey pool (1:1 health-energy → predator energy).
pub fn consume_from_pool(uptake_per_interval: f32, available: f32) -> f32 {
    if uptake_per_interval <= 0.0 || available <= 0.0 {
        return 0.0;
    }
    uptake_per_interval.min(available)
}

/// Living prey: consumable only when per-second uptake meets or exceeds current HP; then grant max HP as energy.
pub fn living_prey_gain(
    uptake_per_sec: f32,
    remaining_hp: f32,
    max_hp: f32,
) -> (f32, f32) {
    if remaining_hp <= 0.0 || uptake_per_sec < remaining_hp {
        return (0.0, 0.0);
    }
    (max_hp.max(0.0), remaining_hp)
}

/// Energy pool left on a corpse: fraction of the parent's max health as consumable units.
pub fn corpse_energy_from_max_health(core_max_hp: f32, fraction_of_max_health: f32) -> f32 {
    if core_max_hp <= 0.0 || fraction_of_max_health <= 0.0 {
        return 0.0;
    }
    (core_max_hp * fraction_of_max_health).max(0.0)
}

/// Energy gained from local mineral concentration in one tick.
pub fn minerals_gain_per_tick(
    minerals_effective: f32,
    concentration: f32,
    uptake_per_tick: f32,
) -> f32 {
    minerals_effective.max(0.0) * concentration.clamp(0.0, 1.0) * uptake_per_tick.max(0.0)
}

/// Config-only soup uptake (transporter loci are future).
pub fn soup_gain_per_tick(
    biomass_effective: f32,
    soup_concentration: f32,
    uptake_rate: f32,
) -> f32 {
    biomass_effective.max(0.0) * soup_concentration.clamp(0.0, 1.0) * uptake_rate.max(0.0)
}

/// Bevy system: digest minerals, corpses, living prey, and optional soup before metabolism costs.
pub fn feeding_v2(
    tick: Res<SimTick>,
    config: Res<SimConfigResource>,
    soup: Res<SoupField>,
    mut agents: Query<
        (
            Entity,
            &Transform,
            &Agent,
            &Phenotype,
            &MineralExposure,
            &mut Energy,
            &mut Health,
        ),
        With<Agent>,
    >,
    mut corpses: Query<(&Transform, &mut Corpse)>,
) {
    if config.0.genetics.genome_version != 2 {
        return;
    }

    let env = &config.0.environment;
    let feeding = &env.feeding;
    let uptake_scale = feeding.biomass_uptake_per_radius_per_sec;
    let biomass_interval = feeding.biomass_feeding_interval_ticks;
    let run_logic = logic_active_this_tick(tick.tick, &config.0.run);
    let logic_steps = logic_elapsed_ticks(&config.0.run);
    let run_biomass = biomass_feeding_active_this_tick(
        tick.tick,
        biomass_interval,
        feeding.biomass_feeding_phase_ticks,
    ) && uptake_scale > 0.0;

    if !run_logic && !run_biomass {
        return;
    }

    let mut energy_gain: HashMap<Entity, f32> = HashMap::new();
    let mut hp_drain: HashMap<Entity, f32> = HashMap::new();

    if run_logic {
        for (entity, _, _, phenotype, minerals_exp, _, _) in agents.iter() {
            let ph = &phenotype.0;
            let mut gain = 0.0;
            if minerals_exp.concentration > 0.0 {
                gain += minerals_gain_per_tick(
                    ph.minerals_effective,
                    minerals_exp.concentration,
                    feeding.minerals_uptake_per_tick,
                ) * logic_steps as f32;
            }
            if soup.is_enabled() && ph.biomass_effective > 0.0 {
                gain += soup_gain_per_tick(
                    ph.biomass_effective,
                    soup.concentration,
                    feeding.soup_uptake_rate,
                ) * logic_steps as f32;
            }
            if gain > 0.0 {
                energy_gain.insert(entity, gain);
            }
        }
    }

    if run_biomass {
        let predators: Vec<(Entity, Vec2, f32, f32)> = agents
            .iter()
            .filter_map(|(e, tf, a, p, _, _, _)| {
                let ph = &p.0;
                if ph.biomass_effective <= 0.0 {
                    return None;
                }
                let uptake_sec =
                    biomass_uptake_per_sec(ph.body_radius, ph.biomass_effective, uptake_scale);
                if uptake_sec <= 0.0 {
                    return None;
                }
                Some((e, tf.translation.truncate(), a.radius, uptake_sec))
            })
            .collect();

        if !predators.is_empty() {
            let prey_snapshots: Vec<(Entity, Vec2, f32, f32, f32)> = agents
                .iter()
                .map(|(e, tf, a, _, _, _, h)| {
                    (
                        e,
                        tf.translation.truncate(),
                        a.radius,
                        h.core_hp,
                        h.core_max_hp,
                    )
                })
                .collect();

            for (pred_e, pred_pos, pred_r, uptake_sec) in &predators {
                if env.corpses_enabled {
                    for (c_tf, mut corpse) in &mut corpses {
                        if corpse.remaining_energy <= 0.0 {
                            continue;
                        }
                        let cpos = c_tf.translation.truncate();
                        if !circles_overlap(*pred_pos, *pred_r, cpos, corpse.radius) {
                            continue;
                        }
                        let taken = consume_from_pool(*uptake_sec, corpse.remaining_energy);
                        if taken > 0.0 {
                            corpse.remaining_energy -= taken;
                            *energy_gain.entry(*pred_e).or_insert(0.0) += taken;
                        }
                    }
                }

                for (vic_e, vic_pos, vic_r, vic_hp, vic_max_hp) in &prey_snapshots {
                    if pred_e == vic_e {
                        continue;
                    }
                    let drained_so_far = hp_drain.get(vic_e).copied().unwrap_or(0.0);
                    let remaining_hp = (*vic_hp - drained_so_far).max(0.0);
                    if remaining_hp <= 0.0 {
                        continue;
                    }
                    if !circles_overlap(*pred_pos, *pred_r, *vic_pos, *vic_r) {
                        continue;
                    }
                    let (gain, drain) =
                        living_prey_gain(*uptake_sec, remaining_hp, *vic_max_hp);
                    if drain > 0.0 {
                        *hp_drain.entry(*vic_e).or_insert(0.0) += drain;
                        *energy_gain.entry(*pred_e).or_insert(0.0) += gain;
                    }
                }
            }
        }
    }

    for (entity, _, _, phenotype, _, mut energy, mut health) in &mut agents {
        if let Some(drain) = hp_drain.get(&entity).copied() {
            if drain > 0.0 {
                health.core_hp = (health.core_hp - drain).max(0.0);
            }
        }
        let Some(gain) = energy_gain.get(&entity).copied() else {
            continue;
        };
        if gain <= 0.0 {
            continue;
        }
        let p = &phenotype.0;
        energy.internal_energy += gain;
        clamp_vitality_pools(&mut energy, &mut health, p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uptake_scales_with_body_radius_and_efficiency() {
        assert_eq!(biomass_uptake_per_sec(10.0, 1.0, 10.0), 100.0);
        assert_eq!(biomass_uptake_per_sec(10.0, 0.1, 10.0), 10.0);
        assert_eq!(biomass_uptake_per_sec(20.0, 1.0, 10.0), 200.0);
        assert_eq!(biomass_uptake_per_sec(10.0, 0.0, 10.0), 0.0);
    }

    #[test]
    fn corpse_pool_scales_with_max_health_fraction() {
        assert_eq!(corpse_energy_from_max_health(100.0, 1.0), 100.0);
        assert_eq!(corpse_energy_from_max_health(100.0, 0.25), 25.0);
        assert_eq!(corpse_energy_from_max_health(-1.0, 0.25), 0.0);
        assert_eq!(corpse_energy_from_max_health(100.0, 0.0), 0.0);
    }

    #[test]
    fn consume_from_pool_takes_min_of_uptake_and_available() {
        let uptake = biomass_uptake_per_sec(10.0, 1.0, 10.0);
        assert_eq!(uptake, 100.0);
        assert_eq!(consume_from_pool(uptake, 20.0), 20.0);
    }

    #[test]
    fn overpowered_predator_gets_max_hp_energy() {
        let (gain, drain) = living_prey_gain(50.0, 20.0, 100.0);
        assert_eq!(gain, 100.0);
        assert_eq!(drain, 20.0);
    }

    #[test]
    fn living_prey_ignored_when_uptake_below_current_hp() {
        let (gain, drain) = living_prey_gain(10.0, 100.0, 100.0);
        assert_eq!(gain, 0.0);
        assert_eq!(drain, 0.0);
    }

    #[test]
    fn living_prey_consumed_when_uptake_meets_current_hp() {
        let (gain, drain) = living_prey_gain(100.0, 100.0, 100.0);
        assert_eq!(gain, 100.0);
        assert_eq!(drain, 100.0);
    }

    #[test]
    fn biomass_feeding_runs_once_per_second_offset_from_sensors() {
        let interval = TICKS_PER_SEC as u32;
        assert!(!biomass_feeding_active_this_tick(0, interval, 30));
        assert!(biomass_feeding_active_this_tick(30, interval, 30));
        assert!(!biomass_feeding_active_this_tick(60, interval, 30));
        assert!(biomass_feeding_active_this_tick(90, interval, 30));
    }

    #[test]
    fn minerals_gain_scales_with_efficiency_and_concentration() {
        let g = minerals_gain_per_tick(0.5, 1.0, 0.08);
        assert!((g - 0.04).abs() < 1e-6);
        assert_eq!(minerals_gain_per_tick(0.0, 1.0, 0.08), 0.0);
    }
}

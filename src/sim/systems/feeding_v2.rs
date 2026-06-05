//! Biomass / minerals / soup ingestion for genome v2 ([Genome_Definition.md](../../../docs/Genome_Definition.md)).

use bevy::prelude::*;
use std::collections::HashMap;

use crate::genome::AnyPhenotype;

use super::metabolism_v2::clamp_vitality_pools;
use super::metabolism::circles_overlap;
use super::super::components::{Agent, Corpse, Energy, Health, MineralExposure, Phenotype};
use super::super::minerals::MineralField;
use super::super::soup::SoupField;
use super::super::world::SimConfigResource;

/// Energy gained from local mineral concentration in one tick.
pub fn minerals_gain_per_tick(
    minerals_effective: f32,
    concentration: f32,
    uptake_per_tick: f32,
) -> f32 {
    minerals_effective.max(0.0) * concentration.clamp(0.0, 1.0) * uptake_per_tick.max(0.0)
}

/// Energy gained from overlapping corpse biomass in one tick.
pub fn biomass_gain_from_corpse(
    biomass_effective: f32,
    corpse_radius: f32,
    available: f32,
    uptake_per_radius: f32,
) -> f32 {
    if biomass_effective <= 0.0 || available <= 0.0 || corpse_radius <= 0.0 {
        return 0.0;
    }
    let demand = biomass_effective * uptake_per_radius.max(0.0) * corpse_radius;
    demand.min(available)
}

/// Config-only soup uptake (transporter loci are future).
pub fn soup_gain_per_tick(
    biomass_effective: f32,
    soup_concentration: f32,
    uptake_rate: f32,
) -> f32 {
    biomass_effective.max(0.0) * soup_concentration.clamp(0.0, 1.0) * uptake_rate.max(0.0)
}

/// Biomass pool left on a corpse from parent vitality at death.
pub fn corpse_biomass_from_vitals(energy: f32, health: f32, frac: f32) -> f32 {
    (energy.max(0.0) + health.max(0.0)) * frac.clamp(0.0, 1.0)
}

/// Bevy system: digest minerals, corpses, and optional soup before metabolism costs.
pub fn feeding_v2(
    config: Res<SimConfigResource>,
    minerals: Res<MineralField>,
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
    let mut energy_gain: HashMap<Entity, f32> = HashMap::new();

    let agent_snapshots: Vec<(Entity, Vec2, f32, f32, f32, f32)> = agents
        .iter()
        .map(|(e, tf, a, p, m, _, _)| {
            let ph = &p.0;
            (
                e,
                tf.translation.truncate(),
                a.radius,
                ph.biomass_effective,
                ph.minerals_effective,
                m.concentration,
            )
        })
        .collect();

    for (entity, _, _, bio_eff, minerals_eff, conc) in &agent_snapshots {
        let mut gain = 0.0;
        if minerals.is_enabled() {
            gain += minerals_gain_per_tick(
                *minerals_eff,
                *conc,
                feeding.minerals_uptake_per_tick,
            );
        }
        if soup.is_enabled() && *bio_eff > 0.0 {
            gain += soup_gain_per_tick(*bio_eff, soup.concentration, feeding.soup_uptake_rate);
        }
        if gain > 0.0 {
            energy_gain.insert(*entity, gain);
        }
    }

    if env.corpses_enabled {
        for (c_tf, mut corpse) in &mut corpses {
            let cpos = c_tf.translation.truncate();
            for (entity, apos, ar, bio_eff, _, _) in &agent_snapshots {
                if *bio_eff <= 0.0 || corpse.remaining_biomass <= 0.0 {
                    continue;
                }
                if !circles_overlap(*apos, *ar, cpos, corpse.radius) {
                    continue;
                }
                let taken = biomass_gain_from_corpse(
                    *bio_eff,
                    corpse.radius,
                    corpse.remaining_biomass,
                    feeding.biomass_uptake_per_radius,
                );
                if taken > 0.0 {
                    corpse.remaining_biomass -= taken;
                    *energy_gain.entry(*entity).or_insert(0.0) += taken;
                }
            }
        }
    }

    for (entity, _, _, phenotype, _, mut energy, mut health) in &mut agents {
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
    fn minerals_gain_scales_with_efficiency_and_concentration() {
        let g = minerals_gain_per_tick(0.5, 1.0, 0.08);
        assert!((g - 0.04).abs() < 1e-6);
        assert_eq!(minerals_gain_per_tick(0.0, 1.0, 0.08), 0.0);
    }

    #[test]
    fn biomass_takes_min_of_demand_and_available() {
        let taken = biomass_gain_from_corpse(1.0, 10.0, 0.5, 0.15);
        assert_eq!(taken, 0.5);
    }

    #[test]
    fn corpse_biomass_frac_of_pools() {
        assert_eq!(corpse_biomass_from_vitals(30.0, 20.0, 0.5), 25.0);
    }
}

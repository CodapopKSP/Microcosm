//! Metabolism and photosynthetic gain (Phase 2D Model A + legacy flat grant).

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::config::{DecodeRanges, EconomyConfig, EnergyModel, EnvironmentConfig};
use crate::genome::{chemo_gain_per_tick, AnyPhenotype, Phenotype as PhenotypeData};

use super::metabolism_v2::apply_metabolism_tick_v2;

use super::super::components::{Agent, Age, Energy, Health, LightExposure, Phenotype, VentExposure};
use super::super::world::SimConfigResource;

/// Logarithmic age metabolism per tick ([GENOME.md](../../../docs/GENOME.md)).
///
/// Rises slowly at first (~negligible in the first minute at 60 Hz), reaching
/// `age_metabolism_cap_per_sec` at `age_metabolism_cap_ticks`, scaled by the gene.
pub fn age_metabolism_per_tick(
    age_ticks: u32,
    age_metabolism_scale: f32,
    economy: &EconomyConfig,
) -> f32 {
    let cap_ticks = economy.age_metabolism_cap_ticks.max(1.0);
    let ref_ticks = economy.age_metabolism_ref_ticks.max(1.0);
    let cap_per_tick = economy.age_metabolism_cap_per_sec / 60.0;
    let age = age_ticks as f32;
    let num = (1.0 + age / ref_ticks).ln();
    let den = (1.0 + cap_ticks / ref_ticks).ln();
    if den <= 0.0 || cap_per_tick <= 0.0 {
        return 0.0;
    }
    cap_per_tick * (num / den) * age_metabolism_scale.max(0.0)
}

/// Per-tick energy drain from idle metabolism and trait upkeeps (see [PHASE_1.md](../../docs/PHASE_1.md)).
pub fn metabolism_tick_cost(phenotype: &PhenotypeData, age_ticks: u32, economy: &EconomyConfig) -> f32 {
    phenotype.metabolism_idle
        + phenotype.green_metabolism_extra
        + phenotype.photo_upkeep
        + phenotype.armor_upkeep
        + phenotype.chemo_upkeep
        + age_metabolism_per_tick(age_ticks, phenotype.age_metabolism_scale, economy)
}

/// Photosynthetic gain for one tick.
///
/// Model A (`energy_model = "local"`):
/// `photo_strength * photo_area_scale * light * solar_constant * photo_light_multiplier`.
/// Legacy (`legacy_flat`): `flat_energy_grant_per_tick` (Phase 1 stand-in).
pub fn photosynthetic_gain_per_tick(
    phenotype: &PhenotypeData,
    light_sample: f32,
    environment: &EnvironmentConfig,
    economy: &EconomyConfig,
    photo_light_multiplier: f32,
) -> f32 {
    let light_mult = photo_light_multiplier.clamp(0.0, 1.0);

    match environment.energy_model {
        EnergyModel::Local => {
            phenotype.photo_strength
                * phenotype.photo_area_scale
                * light_sample
                * environment.solar_constant
                * phenotype.green_photo_mult
                * light_mult
        }
        EnergyModel::LegacyFlat => {
            if light_mult <= 0.0 {
                0.0
            } else {
                economy.flat_energy_grant_per_tick
            }
        }
        EnergyModel::Pool | EnergyModel::Spatial => 0.0,
    }
}

/// Net energy change for one metabolism tick (photo + chemo gains minus costs).
pub fn metabolism_tick_delta(
    phenotype: &PhenotypeData,
    age_ticks: u32,
    light_sample: f32,
    vent_flux: f32,
    environment: &EnvironmentConfig,
    economy: &EconomyConfig,
    decode_ranges: &DecodeRanges,
    photo_light_multiplier: f32,
) -> f32 {
    photosynthetic_gain_per_tick(
        phenotype,
        light_sample,
        environment,
        economy,
        photo_light_multiplier,
    ) + chemo_gain_per_tick(
        phenotype,
        vent_flux,
        decode_ranges.chemo_cliff,
        decode_ranges.k_chemo,
    ) - metabolism_tick_cost(phenotype, age_ticks, economy)
}

/// Apply one metabolism tick.
///
/// Positive net gain adds to `internal_energy`. Negative net first drains energy; any
/// remainder while energy is depleted applies to `core_hp` (agents do not die at 0 energy).
pub fn apply_metabolism_tick(
    energy: &mut Energy,
    health: &mut Health,
    phenotype: &PhenotypeData,
    age_ticks: u32,
    light_sample: f32,
    vent_flux: f32,
    environment: &EnvironmentConfig,
    economy: &EconomyConfig,
    decode_ranges: &DecodeRanges,
    photo_light_multiplier: f32,
) {
    let delta = metabolism_tick_delta(
        phenotype,
        age_ticks,
        light_sample,
        vent_flux,
        environment,
        economy,
        decode_ranges,
        photo_light_multiplier,
    );
    energy.internal_energy += delta;
    if energy.internal_energy < 0.0 {
        let hp_deficit = -energy.internal_energy;
        energy.internal_energy = 0.0;
        health.apply_damage_to_core(hp_deficit);
    }
}

/// True when two agent circles have any area overlap (touching at exactly `r1 + r2` is not overlap).
pub fn circles_overlap(pos_a: Vec2, radius_a: f32, pos_b: Vec2, radius_b: f32) -> bool {
    let r_sum = radius_a + radius_b;
    if r_sum <= 0.0 {
        return false;
    }
    pos_a.distance_squared(pos_b) < r_sum * r_sum
}

/// Area of intersection between two circles (0 if separated).
pub fn circle_intersection_area(r1: f32, r2: f32, center_dist: f32) -> f32 {
    let r1 = r1.max(0.0);
    let r2 = r2.max(0.0);
    let d = center_dist.max(0.0);
    if r1 <= 0.0 || r2 <= 0.0 {
        return 0.0;
    }
    if d >= r1 + r2 {
        return 0.0;
    }
    if d <= (r1 - r2).abs() {
        let r_small = r1.min(r2);
        return std::f32::consts::PI * r_small * r_small;
    }

    let d_sq = d * d;
    let a1 = r1 * r1 * ((d_sq + r1 * r1 - r2 * r2) / (2.0 * d * r1)).acos();
    let a2 = r2 * r2 * ((d_sq + r2 * r2 - r1 * r1) / (2.0 * d * r2)).acos();
    let a3 = 0.5
        * ((-d + r1 + r2) * (d + r1 - r2) * (d - r1 + r2) * (d + r1 + r2))
            .max(0.0)
            .sqrt();
    (a1 + a2 - a3).max(0.0)
}

/// Connected overlap cluster (including `seed`) via circle intersection.
pub fn overlap_cluster_indices(agents: &[(Vec2, f32)], seed: usize) -> Vec<usize> {
    let mut cluster = vec![seed];
    let mut stack = vec![seed];
    while let Some(i) = stack.pop() {
        let (pos_i, r_i) = agents[i];
        for j in 0..agents.len() {
            if cluster.contains(&j) {
                continue;
            }
            let (pos_j, r_j) = agents[j];
            if circles_overlap(pos_i, r_i, pos_j, r_j) {
                cluster.push(j);
                stack.push(j);
            }
        }
    }
    cluster
}

/// Light retained on photosynthesis from overlap crowding ([ENERGY_AND_ENVIRONMENT.md]).
///
/// Uses the largest fraction of self-area overlapping a cluster mate. Shade =
/// `overlap_shade_factor * max_fraction`, multiplied by `(n-1)/n` when three or more
/// agents share the cluster (one sun per patch). Pairs (`n = 2`) use the full shade term.
pub fn overlap_photo_light_multiplier(
    agents: &[(Vec2, f32)],
    agent_index: usize,
    overlap_shade_factor: f32,
) -> f32 {
    let cluster = overlap_cluster_indices(agents, agent_index);
    let n = cluster.len();
    if n <= 1 {
        return 1.0;
    }

    let (pos_i, r_i) = agents[agent_index];
    let self_area = std::f32::consts::PI * r_i * r_i;
    if self_area <= 0.0 {
        return 1.0;
    }

    let mut max_frac = 0.0f32;
    for &j in &cluster {
        if j == agent_index {
            continue;
        }
        let (pos_j, r_j) = agents[j];
        let d = pos_i.distance(pos_j);
        let area_ij = circle_intersection_area(r_i, r_j, d);
        max_frac = max_frac.max((area_ij / self_area).clamp(0.0, 1.0));
    }

    let shade = if n == 2 {
        overlap_shade_factor * max_frac
    } else {
        overlap_shade_factor * max_frac * (n - 1) as f32 / n as f32
    };
    (1.0 - shade).clamp(0.0, 1.0)
}

/// Per-agent light multiplier from graded overlap (parallel to `agents` slice).
pub fn overlap_photo_light_multipliers(
    agents: &[(Vec2, f32)],
    overlap_shade_factor: f32,
) -> Vec<f32> {
    (0..agents.len())
        .map(|i| overlap_photo_light_multiplier(agents, i, overlap_shade_factor))
        .collect()
}

/// Per-entity light multiplier for Bevy agents `(Entity, pos, radius)`.
pub fn overlap_photo_light_multipliers_for_entities(
    agents: &[(Entity, Vec2, f32)],
    overlap_shade_factor: f32,
) -> HashMap<Entity, f32> {
    let geom: Vec<_> = agents.iter().map(|(_, p, r)| (*p, *r)).collect();
    let mults = overlap_photo_light_multipliers(&geom, overlap_shade_factor);
    agents
        .iter()
        .zip(mults)
        .map(|((entity, _, _), m)| (*entity, m))
        .collect()
}

/// Entities that overlap at least one other agent this tick (both parties included).
pub fn entities_blocked_by_overlap(agents: &[(Entity, Vec2, f32)]) -> HashSet<Entity> {
    let mut blocked = HashSet::new();
    let n = agents.len();
    for i in 0..n {
        let (e_i, pos_i, r_i) = agents[i];
        for j in (i + 1)..n {
            let (_, pos_j, r_j) = agents[j];
            if circles_overlap(pos_i, r_i, pos_j, r_j) {
                blocked.insert(e_i);
                blocked.insert(agents[j].0);
            }
        }
    }
    blocked
}

/// Bevy system: photosynthetic grant minus metabolism and upkeeps for every living agent.
pub fn metabolism_and_gains(
    config: Res<SimConfigResource>,
    mut query: Query<(
        Entity,
        &Transform,
        &Agent,
        &Phenotype,
        &Age,
        &LightExposure,
        &VentExposure,
        &mut Energy,
        &mut Health,
    )>,
) {
    let agents: Vec<_> = query
        .iter()
        .map(|(entity, transform, agent, _, _, _, _, _, _)| {
            (
                entity,
                transform.translation.truncate(),
                agent.radius,
            )
        })
        .collect();

    let environment = &config.0.environment;
    let economy = &config.0.economy;
    let light_mults = if environment.overlap_shades_photosynthesis {
        overlap_photo_light_multipliers_for_entities(&agents, environment.overlap_shade_factor)
    } else if environment.crowding_blocks_photosynthesis {
        let blocked = entities_blocked_by_overlap(&agents);
        agents
            .iter()
            .map(|(entity, _, _)| {
                (
                    *entity,
                    if blocked.contains(entity) { 0.0 } else { 1.0 },
                )
            })
            .collect()
    } else {
        HashMap::new()
    };

    for (entity, _, _, phenotype, age, exposure, vent_exp, mut energy, mut health) in &mut query {
        let photo_light_multiplier = light_mults.get(&entity).copied().unwrap_or(1.0);
        match &phenotype.0 {
            AnyPhenotype::V1(p) => apply_metabolism_tick(
                &mut energy,
                &mut health,
                p,
                age.ticks,
                exposure.0,
                vent_exp.vent_flux,
                environment,
                economy,
                config.0.genetics.decode_ranges_ref(),
                photo_light_multiplier,
            ),
            AnyPhenotype::V2(p) => apply_metabolism_tick_v2(
                &mut energy,
                &mut health,
                p,
                age.ticks,
                exposure.0,
                environment,
                economy,
                &config.0,
                photo_light_multiplier,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Entity;
    use crate::config::{EnergyModel, SimConfig};
    use crate::genome::{decode, Genome};
    use crate::sim::components::{Energy, Health};

    fn ancestor_phenotype(config: &SimConfig) -> PhenotypeData {
        let genome = Genome::ancestor_from_config(config).unwrap();
        decode(&genome, config).unwrap()
    }

    fn local_config() -> SimConfig {
        let mut config = SimConfig::load_v1_regression().unwrap();
        config.environment.energy_model = EnergyModel::Local;
        config
    }

    fn full_health(config: &SimConfig) -> Health {
        Health {
            core_hp: config.economy.core_max_hp,
            core_max_hp: config.economy.core_max_hp,
            part_hp: vec![],
        }
    }

    #[test]
    fn circles_overlap_when_interiors_intersect() {
        assert!(circles_overlap(Vec2::ZERO, 10.0, Vec2::new(15.0, 0.0), 10.0));
        assert!(!circles_overlap(Vec2::ZERO, 10.0, Vec2::new(20.0, 0.0), 10.0));
    }

    #[test]
    fn overlap_blocks_photosynthetic_gain_only_legacy_flat() {
        let mut config = SimConfig::load_v1_regression().unwrap();
        config.environment.energy_model = EnergyModel::LegacyFlat;
        config.economy.flat_energy_grant_per_tick = 0.8;
        let phenotype = ancestor_phenotype(&config);
        let environment = &config.environment;
        let economy = &config.economy;
        let light = 1.0;

        let dr = &config.genetics.decode_ranges_ref();
        let open = metabolism_tick_delta(&phenotype, 0, light, 0.0, environment, economy, dr, 1.0);
        let crowded = metabolism_tick_delta(&phenotype, 0, light, 0.0, environment, economy, dr, 0.0);
        assert!(
            (open - crowded - economy.flat_energy_grant_per_tick).abs() < 1e-5,
            "blocked tick should differ only by the photosynthetic grant"
        );
        assert_eq!(
            photosynthetic_gain_per_tick(&phenotype, light, environment, economy, 0.0),
            0.0
        );
    }

    #[test]
    fn overlap_blocks_local_gain_when_crowding_enabled() {
        let config = local_config();
        let phenotype = ancestor_phenotype(&config);
        let light = config.environment.gradient_max;
        let environment = &config.environment;
        let economy = &config.economy;

        let open_gain = photosynthetic_gain_per_tick(
            &phenotype, light, environment, economy, 1.0,
        );
        let blocked_gain = photosynthetic_gain_per_tick(
            &phenotype, light, environment, economy, 0.0,
        );

        assert!(open_gain > 0.0, "local gain should be positive in bright band");
        assert_eq!(blocked_gain, 0.0);

        let dr = &config.genetics.decode_ranges_ref();
        let open_delta = metabolism_tick_delta(&phenotype, 0, light, 0.0, environment, economy, dr, 1.0);
        let crowded_delta = metabolism_tick_delta(&phenotype, 0, light, 0.0, environment, economy, dr, 0.0);
        assert!(
            (open_delta - crowded_delta - open_gain).abs() < 1e-5,
            "blocked tick should differ only by photosynthetic gain under local model"
        );
    }

    #[test]
    fn overlap_photo_multiplier_pair_is_ninety_percent_at_twenty_percent_overlap() {
        let r = 10.0;
        // Centers ~14 apart → lens area ≈ 20% of each circle.
        let agents = [(Vec2::ZERO, r), (Vec2::new(14.0, 0.0), r)];
        let m0 = overlap_photo_light_multiplier(&agents, 0, 0.5);
        let m1 = overlap_photo_light_multiplier(&agents, 1, 0.5);
        assert!((m0 - 0.9).abs() < 0.05, "pair multiplier {m0}");
        assert!((m1 - 0.9).abs() < 0.05, "pair multiplier {m1}");
    }

    #[test]
    fn overlap_photo_multiplier_triple_uses_two_thirds_shade() {
        let r = 10.0;
        // Co-located cluster → each circle fully overlaps the others.
        let agents = [
            (Vec2::ZERO, r),
            (Vec2::ZERO, r),
            (Vec2::ZERO, r),
        ];
        let m = overlap_photo_light_multiplier(&agents, 0, 0.5);
        // max_frac ≈ 1, shade = 0.5 * 2/3.
        assert!((m - 2.0 / 3.0).abs() < 0.05, "triple multiplier {m}");
    }

    #[test]
    fn photo_area_scale_is_one_at_reference_radius() {
        let config = SimConfig::load_v1_regression().unwrap();
        let dr = &config.genetics.decode_ranges_ref();
        let mut phenotype = ancestor_phenotype(&config);
        phenotype.body_radius = dr.r_ref;
        phenotype.photo_area_scale = (dr.r_ref / dr.r_ref).powi(2);
        assert!((phenotype.photo_area_scale - 1.0).abs() < 1e-5);
    }

    #[test]
    fn entities_blocked_by_overlap_marks_both() {
        let a = Entity::from_raw(1);
        let b = Entity::from_raw(2);
        let c = Entity::from_raw(3);
        let agents = vec![
            (a, Vec2::ZERO, 10.0),
            (b, Vec2::new(15.0, 0.0), 10.0),
            (c, Vec2::new(200.0, 0.0), 10.0),
        ];
        let blocked = entities_blocked_by_overlap(&agents);
        assert!(blocked.contains(&a) && blocked.contains(&b));
        assert!(!blocked.contains(&c));
    }

    #[test]
    fn zero_photo_strength_has_zero_local_gain() {
        let config = local_config();
        let mut phenotype = ancestor_phenotype(&config);
        phenotype.photo_strength = 0.0;
        let gain = photosynthetic_gain_per_tick(
            &phenotype,
            1.0,
            &config.environment,
            &config.economy,
            1.0,
        );
        assert_eq!(gain, 0.0);
    }

    #[test]
    fn local_gain_scales_with_solar_constant() {
        let mut config = local_config();
        let phenotype = ancestor_phenotype(&config);
        let light = 0.8;
        let base = photosynthetic_gain_per_tick(
            &phenotype,
            light,
            &config.environment,
            &config.economy,
            1.0,
        );
        config.environment.solar_constant *= 2.0;
        let doubled = photosynthetic_gain_per_tick(
            &phenotype,
            light,
            &config.environment,
            &config.economy,
            1.0,
        );
        assert!((doubled - base * 2.0).abs() < 1e-5);
    }

    #[test]
    fn photosynthetic_gain_scales_with_strength_and_light() {
        let config = local_config();
        let phenotype = ancestor_phenotype(&config);
        let light = 0.5;
        let environment = &config.environment;
        let economy = &config.economy;

        let base = photosynthetic_gain_per_tick(
            &phenotype, light, environment, economy, 1.0,
        );
        assert!(base > 0.0);

        let mut strong = phenotype.clone();
        strong.photo_strength *= 2.0;
        let gain_2x_strength = photosynthetic_gain_per_tick(
            &strong, light, environment, economy, 1.0,
        );
        assert!((gain_2x_strength - base * 2.0).abs() < 1e-5);

        let gain_2x_light = photosynthetic_gain_per_tick(
            &phenotype, light * 2.0, environment, economy, 1.0,
        );
        assert!((gain_2x_light - base * 2.0).abs() < 1e-5);
    }

    #[test]
    fn local_gain_formula_matches_strength_light_solar() {
        let config = local_config();
        let phenotype = ancestor_phenotype(&config);
        let light = 0.75;
        let expected = phenotype.photo_strength
            * phenotype.photo_area_scale
            * light
            * config.environment.solar_constant
            * phenotype.green_photo_mult;
        let gain = photosynthetic_gain_per_tick(
            &phenotype,
            light,
            &config.environment,
            &config.economy,
            1.0,
        );
        assert!((gain - expected).abs() < 1e-5);
    }

    #[test]
    fn zero_local_solar_depletes_energy_over_ticks() {
        let mut config = local_config();
        config.environment.solar_constant = 0.0;
        config.economy.flat_energy_grant_per_tick = 0.0;
        let phenotype = ancestor_phenotype(&config);

        let mut energy = Energy::new_v1(config.economy.spawn_energy);
        let mut health = full_health(&config);
        let start = energy.internal_energy;

        for _ in 0..5_000 {
            apply_metabolism_tick(
                &mut energy,
                &mut health,
                &phenotype,
                0,
                config.environment.gradient_max,
                0.0,
                &config.environment,
                &config.economy,
                &config.genetics.decode_ranges_ref(),
                1.0,
            );
        }

        assert!(
            energy.internal_energy < 1.0,
            "zero solar should deplete energy (start={start}, end={})",
            energy.internal_energy
        );
        assert!(
            health.core_hp < config.economy.core_max_hp,
            "metabolic deficit should drain HP once energy is empty (hp={})",
            health.core_hp
        );
    }

    #[test]
    fn zero_legacy_grant_depletes_energy_over_ticks() {
        let mut config = SimConfig::load_v1_regression().unwrap();
        config.environment.energy_model = EnergyModel::LegacyFlat;
        let phenotype = ancestor_phenotype(&config);
        let mut economy = config.economy.clone();
        economy.flat_energy_grant_per_tick = 0.0;

        let mut energy = Energy::new_v1(config.economy.spawn_energy);
        let mut health = full_health(&config);
        let start = energy.internal_energy;

        for _ in 0..5_000 {
            apply_metabolism_tick(
                &mut energy,
                &mut health,
                &phenotype,
                0,
                1.0,
                0.0,
                &config.environment,
                &economy,
                &config.genetics.decode_ranges_ref(),
                1.0,
            );
        }

        assert!(
            energy.internal_energy < 1.0,
            "zero legacy grant should deplete energy (start={start}, end={})",
            energy.internal_energy
        );
        assert!(health.core_hp < config.economy.core_max_hp);
    }

    #[test]
    fn metabolic_deficit_drains_hp_when_energy_empty() {
        let config = local_config();
        let phenotype = ancestor_phenotype(&config);
        let mut energy = Energy::new_v1(0.0);
        let mut health = Health {
            core_hp: 20.0,
            core_max_hp: 100.0,
            part_hp: vec![],
        };
        let cost = metabolism_tick_cost(&phenotype, 0, &config.economy);
        assert!(cost > 0.0);

        apply_metabolism_tick(
            &mut energy,
            &mut health,
            &phenotype,
            0,
            0.0,
            0.0,
            &config.environment,
            &config.economy,
            &config.genetics.decode_ranges_ref(),
            1.0,
        );

        assert_eq!(energy.internal_energy, 0.0);
        assert!(health.core_hp < 20.0);
    }

    #[test]
    fn local_ancestor_bright_band_net_positive_over_ticks() {
        let config = local_config();
        let phenotype = ancestor_phenotype(&config);
        let bright = config.environment.gradient_max;

        let mut energy = Energy::new_v1(config.economy.spawn_energy);
        let mut health = full_health(&config);
        let start = energy.internal_energy;

        for _ in 0..1000 {
            apply_metabolism_tick(
                &mut energy,
                &mut health,
                &phenotype,
                0,
                bright,
                0.0,
                &config.environment,
                &config.economy,
                &config.genetics.decode_ranges_ref(),
                1.0,
            );
        }

        assert!(
            energy.internal_energy > start,
            "ancestor should gain net energy in bright band (start={start}, end={})",
            energy.internal_energy
        );
    }

    #[test]
    fn local_gain_independent_of_population_count() {
        use bevy::app::App;
        use crate::sim::components::{LifeBundle, LightExposure, VentExposure, Velocity};
        use crate::sim::world::SimConfigResource;

        let config = local_config();
        let bright = config.environment.gradient_max;
        let genome = Genome::ancestor_from_config(&config).unwrap();
        let phenotype = decode(&genome, &config).unwrap();
        let start_energy = 50.0;

        let mut solo = App::new();
        solo.insert_resource(SimConfigResource(config.clone()));
        let life = LifeBundle::from_parts_with_energy(
            genome.clone_genome(),
            phenotype.clone(),
            &config,
            1,
            None,
            0,
            start_energy,
        );
        solo.world_mut().spawn((
            life,
            Velocity(Vec2::ZERO),
            LightExposure(bright),
            VentExposure::default(),
            Transform::from_translation(Vec2::new(0.0, 600.0).extend(0.0)),
        ));
        solo.add_systems(Update, metabolism_and_gains);
        solo.update();
        let solo_end = solo
            .world_mut()
            .query::<&Energy>()
            .single(solo.world())
            .internal_energy;

        let config_crowd = config.clone();
        let mut crowd = App::new();
        crowd.insert_resource(SimConfigResource(config_crowd));
        for (i, x) in [-250.0, 0.0, 250.0].into_iter().enumerate() {
            let life = LifeBundle::from_parts_with_energy(
                genome.clone_genome(),
                phenotype.clone(),
                &config,
                i as u64 + 1,
                None,
                0,
                start_energy,
            );
            crowd.world_mut().spawn((
                life,
                Velocity(Vec2::ZERO),
                LightExposure(bright),
                VentExposure::default(),
                Transform::from_translation(Vec2::new(x, 600.0).extend(0.0)),
            ));
        }
        crowd.add_systems(Update, metabolism_and_gains);
        crowd.update();

        let mut crowd_ends: Vec<f32> = crowd
            .world_mut()
            .query::<&Energy>()
            .iter(crowd.world())
            .map(|e| e.internal_energy)
            .collect();
        crowd_ends.sort_by(|a, b| a.partial_cmp(b).unwrap());

        assert!(
            crowd_ends.windows(2).all(|w| (w[0] - w[1]).abs() < 1e-5),
            "agents at same light should share metabolism outcome: {crowd_ends:?}"
        );
        assert!(
            (solo_end - crowd_ends[1]).abs() < 1e-5,
            "solo agent delta should match crowded agent at same light (solo={solo_end}, crowd={crowd_ends:?})"
        );
    }

    #[test]
    fn shade_net_negative_for_high_photo() {
        let config = local_config();
        let mut phenotype = ancestor_phenotype(&config);
        phenotype.photo_strength = config.genetics.decode_ranges_ref().photo_max;
        phenotype.photo_upkeep =
            config.genetics.decode_ranges_ref().k_photo * phenotype.photo_strength;
        let shade = config.environment.gradient_min;

        let mut energy = Energy::new_v1(config.economy.spawn_energy);
        let mut health = full_health(&config);
        let start = energy.internal_energy;

        for _ in 0..500 {
            apply_metabolism_tick(
                &mut energy,
                &mut health,
                &phenotype,
                0,
                shade,
                0.0,
                &config.environment,
                &config.economy,
                &config.genetics.decode_ranges_ref(),
                1.0,
            );
        }

        assert!(
            energy.internal_energy < start,
            "high-photo genotype should lose energy in shade (start={start}, end={})",
            energy.internal_energy
        );
    }

    #[test]
    fn default_local_bright_band_stabilizes_above_repro_threshold() {
        let config = local_config();
        let phenotype = ancestor_phenotype(&config);

        let mut energy = Energy::new_v1(config.economy.spawn_energy);
        let mut health = full_health(&config);

        for _ in 0..2500 {
            apply_metabolism_tick(
                &mut energy,
                &mut health,
                &phenotype,
                0,
                config.environment.gradient_max,
                0.0,
                &config.environment,
                &config.economy,
                &config.genetics.decode_ranges_ref(),
                1.0,
            );
        }

        assert!(
            energy.internal_energy > phenotype.repro_threshold,
            "energy {} should exceed repro threshold {} after 2500 ticks in bright band",
            energy.internal_energy,
            phenotype.repro_threshold
        );
    }

    #[test]
    fn chemo_gain_requires_cliff_and_vent() {
        use crate::genome::core;
        use crate::genome::chemo_gain_per_tick;

        let config = local_config();
        let dr = &config.genetics.decode_ranges_ref();
        let genome = Genome::from_bytes(
            {
                let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
                let i = crate::genome::HEADER_LEN + core::CHEMO_POTENTIAL;
                let raw: u16 = 0x8CCC; // ~0.55, above cliff 0.35
                bytes[i] = (raw >> 8) as u8;
                bytes[i + 1] = (raw & 0xFF) as u8;
                bytes
            },
            config.genetics.genome_version,
            config.genetics.core_length,
        )
        .unwrap();
        let phenotype = decode(&genome, &config).unwrap();
        let vent_flux = 0.032;

        assert!(phenotype.is_chemo_above_cliff(dr.chemo_cliff));
        let gain = chemo_gain_per_tick(&phenotype, vent_flux, dr.chemo_cliff, dr.k_chemo);
        assert!(gain > 0.0);

        assert_eq!(chemo_gain_per_tick(&phenotype, 0.0, dr.chemo_cliff, dr.k_chemo), 0.0);
        assert_eq!(
            chemo_gain_per_tick(&phenotype, vent_flux, dr.chemo_cliff, 0.0),
            0.0
        );

        let below_genome = Genome::from_bytes(
            {
                let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
                let i = crate::genome::HEADER_LEN + core::CHEMO_POTENTIAL;
                let raw: u16 = 0x5709; // ~0.34, below cliff
                bytes[i] = (raw >> 8) as u8;
                bytes[i + 1] = (raw & 0xFF) as u8;
                bytes
            },
            config.genetics.genome_version,
            config.genetics.core_length,
        )
        .unwrap();
        let below = decode(&below_genome, &config).unwrap();
        assert!(!below.is_chemo_above_cliff(dr.chemo_cliff));
        assert_eq!(
            chemo_gain_per_tick(&below, vent_flux, dr.chemo_cliff, dr.k_chemo),
            0.0
        );
    }

    #[test]
    fn chemo_upkeep_without_gain_below_cliff() {
        use crate::genome::chemo_gain_per_tick;
        use crate::genome::core;

        let config = local_config();
        let dr = &config.genetics.decode_ranges_ref();
        let genome = Genome::from_bytes(
            {
                let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
                let i = crate::genome::HEADER_LEN + core::CHEMO_POTENTIAL;
                let raw: u16 = 0x5709;
                bytes[i] = (raw >> 8) as u8;
                bytes[i + 1] = (raw & 0xFF) as u8;
                bytes
            },
            config.genetics.genome_version,
            config.genetics.core_length,
        )
        .unwrap();
        let phenotype = decode(&genome, &config).unwrap();
        assert!(phenotype.chemo_upkeep > 0.0);
        assert_eq!(
            chemo_gain_per_tick(&phenotype, 0.05, dr.chemo_cliff, dr.k_chemo),
            0.0
        );
    }

    #[test]
    fn chemo_gain_zero_when_vent_flux_zero() {
        use crate::genome::chemo_gain_per_tick;

        let config = local_config();
        let dr = &config.genetics.decode_ranges_ref();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        let i = crate::genome::HEADER_LEN + crate::genome::core::CHEMO_POTENTIAL;
        bytes[i] = 0x8C;
        bytes[i + 1] = 0xCC;
        let genome = Genome::from_bytes(
            bytes,
            config.genetics.genome_version,
            config.genetics.core_length,
        )
        .unwrap();
        let phenotype = decode(&genome, &config).unwrap();
        assert!(phenotype.is_chemo_above_cliff(dr.chemo_cliff));
        assert_eq!(
            chemo_gain_per_tick(&phenotype, 0.0, dr.chemo_cliff, dr.k_chemo),
            0.0
        );
    }

    #[test]
    fn chemo_gain_increases_net_energy_at_vent() {
        use crate::genome::core;

        let config = local_config();
        let dr = &config.genetics.decode_ranges_ref();
        let vent_flux = 0.15;
        let genome = Genome::from_bytes(
            {
                let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
                let i = crate::genome::HEADER_LEN + core::PHOTO_STRENGTH;
                bytes[i] = 0x10;
                bytes[i + 1] = 0x00;
                let i = crate::genome::HEADER_LEN + core::CHEMO_POTENTIAL;
                bytes[i] = 0x8C;
                bytes[i + 1] = 0xCC;
                bytes
            },
            config.genetics.genome_version,
            config.genetics.core_length,
        )
        .unwrap();
        let phenotype = decode(&genome, &config).unwrap();
        let mut energy = Energy::new_v1(config.economy.spawn_energy);
        let mut health = full_health(&config);
        let start = energy.internal_energy;

        for _ in 0..500 {
            apply_metabolism_tick(
                &mut energy,
                &mut health,
                &phenotype,
                0,
                0.0,
                vent_flux,
                &config.environment,
                &config.economy,
                dr,
                1.0,
            );
        }

        let mut control = Energy::new_v1(config.economy.spawn_energy);
        let mut control_health = full_health(&config);
        for _ in 0..500 {
            apply_metabolism_tick(
                &mut control,
                &mut control_health,
                &phenotype,
                0,
                0.0,
                0.0,
                &config.environment,
                &config.economy,
                dr,
                1.0,
            );
        }

        assert!(
            energy.internal_energy > control.internal_energy,
            "vent flux should raise net energy for high-chemo genotype (vent end={}, off vent={})",
            energy.internal_energy,
            control.internal_energy
        );
    }

    #[test]
    fn tick_cost_includes_upkeeps() {
        let config = SimConfig::load_v1_regression().unwrap();
        let phenotype = ancestor_phenotype(&config);
        let cost = metabolism_tick_cost(&phenotype, 0, &config.economy);
        assert!(cost >= phenotype.metabolism_idle);
        assert!(cost > 0.0);
    }
}

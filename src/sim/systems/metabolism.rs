//! Metabolism and photosynthetic gain (v2).

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::config::{EconomyConfig, EnvironmentConfig};

use super::metabolism_v2::apply_metabolism_tick_v2;

use super::super::components::{Agent, Age, Energy, Health, LightExposure, Phenotype, VentExposure};
use super::super::world::SimConfigResource;

/// Logarithmic age metabolism per tick.
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

    for (entity, _, _, phenotype, age, exposure, _vent_exp, mut energy, mut health) in &mut query {
        let photo_light_multiplier = light_mults.get(&entity).copied().unwrap_or(1.0);
        apply_metabolism_tick_v2(
            &mut energy,
            &mut health,
            &phenotype.0,
            age.ticks,
            exposure.0,
            environment,
            economy,
            &config.0,
            photo_light_multiplier,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Entity;

    #[test]
    fn circles_overlap_when_interiors_intersect() {
        assert!(circles_overlap(Vec2::ZERO, 10.0, Vec2::new(15.0, 0.0), 10.0));
        assert!(!circles_overlap(Vec2::ZERO, 10.0, Vec2::new(20.0, 0.0), 10.0));
    }

    #[test]
    fn overlap_photo_multiplier_pair_is_ninety_percent_at_twenty_percent_overlap() {
        let r = 10.0;
        let agents = [(Vec2::ZERO, r), (Vec2::new(14.0, 0.0), r)];
        let m0 = overlap_photo_light_multiplier(&agents, 0, 0.5);
        let m1 = overlap_photo_light_multiplier(&agents, 1, 0.5);
        assert!((m0 - 0.9).abs() < 0.05, "pair multiplier {m0}");
        assert!((m1 - 0.9).abs() < 0.05, "pair multiplier {m1}");
    }

    #[test]
    fn overlap_photo_multiplier_triple_uses_two_thirds_shade() {
        let r = 10.0;
        let agents = [
            (Vec2::ZERO, r),
            (Vec2::ZERO, r),
            (Vec2::ZERO, r),
        ];
        let m = overlap_photo_light_multiplier(&agents, 0, 0.5);
        assert!((m - 2.0 / 3.0).abs() < 0.05, "triple multiplier {m}");
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
}

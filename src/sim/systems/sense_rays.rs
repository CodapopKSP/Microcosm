//! Sensor ray casting and pursue/avoid signature matching (v2).

use bevy::prelude::*;

use crate::genome::{parse_pursue_avoid_lists, slot_list_matches, SignatureSlot};
use crate::genome::{AnyGenome, AnyPhenotype};

use super::super::components::{Agent, Genome, Heading, LightExposure, Phenotype};
use super::super::perception::SensePerception;
use super::super::light::LightField;
use super::super::components::Corpse;
use super::super::minerals::MineralField;
use super::super::perception::{
    perceive_agent, stimulus_bearing, AgentSenseTarget, CorpseSenseTarget, SenseRaySample,
};
use super::super::interval::interval_active_this_tick;
use super::super::world::{SimConfigResource, SimTick};

/// Whether [`sense_rays`] should run on this tick (`interval_ticks = 1` ⇒ every tick).
pub fn sensor_rays_active_this_tick(tick: u64, interval_ticks: u32, phase_ticks: u32) -> bool {
    interval_active_this_tick(tick, interval_ticks, phase_ticks)
}

/// Result of matching pursue/avoid slot lists against ray hits.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SlotMatchResult {
    pub pursue_matched: bool,
    pub avoid_matched: bool,
    pub pursue_bearing: Option<f32>,
    pub avoid_bearing: Option<f32>,
}

/// Evaluate pursue/avoid lists; prefer closest matching ray for steering bearings.
pub fn evaluate_ray_slot_matches(
    rays: &[SenseRaySample],
    origin: Vec2,
    targets: &[AgentSenseTarget],
    pursue: &[SignatureSlot],
    avoid: &[SignatureSlot],
    fuzzy_delta: u8,
) -> SlotMatchResult {
    let mut pursue_matched = false;
    let mut avoid_matched = false;
    let mut best_pursue: Option<(f32, f32)> = None;
    let mut best_avoid: Option<(f32, f32)> = None;

    for ray in rays {
        let chemical = Some(ray.hit.chemical);
        let target_genome = ray
            .hit
            .agent_index
            .and_then(|i| targets.get(i))
            .map(|t| &t.genome);
        let dist = ray.hit.distance;
        let bearing = stimulus_bearing(ray, origin, targets);

        if slot_list_matches(pursue, target_genome, chemical, fuzzy_delta) {
            pursue_matched = true;
            if best_pursue.map(|(d, _)| dist < d).unwrap_or(true) {
                best_pursue = Some((dist, bearing));
            }
        }
        if slot_list_matches(avoid, target_genome, chemical, fuzzy_delta) {
            avoid_matched = true;
            if best_avoid.map(|(d, _)| dist < d).unwrap_or(true) {
                best_avoid = Some((dist, bearing));
            }
        }
    }

    SlotMatchResult {
        pursue_matched,
        avoid_matched,
        pursue_bearing: best_pursue.map(|(_, b)| b),
        avoid_bearing: best_avoid.map(|(_, b)| b),
    }
}

/// Evaluate pursue/avoid lists against ray hits for one observer.
pub fn evaluate_slot_matches(
    rays: &[SenseRaySample],
    origin: Vec2,
    targets: &[AgentSenseTarget],
    pursue: &[SignatureSlot],
    avoid: &[SignatureSlot],
    fuzzy_delta: u8,
) -> (bool, bool) {
    let r = evaluate_ray_slot_matches(rays, origin, targets, pursue, avoid, fuzzy_delta);
    (r.pursue_matched, r.avoid_matched)
}

/// Bevy system: cast sensor rays after [`super::sense_exposure`](super::sense_exposure).
///
/// Between scans, agents keep their previous [`SensePerception`] and [`super::behavior_v2`] reuses it.
pub fn sense_rays(
    config: Res<SimConfigResource>,
    tick: Res<SimTick>,
    light: Res<LightField>,
    minerals: Res<MineralField>,
    corpses: Query<(&Transform, &Corpse)>,
    mut query: Query<
        (
            Entity,
            &Transform,
            &Heading,
            &Genome,
            &Phenotype,
            &Agent,
            &LightExposure,
            &mut SensePerception,
        ),
        With<Agent>,
    >,
) {
    if config.0.genetics.genome_version != 2 {
        return;
    }
    let decode = match config.0.genetics.decode_v2.as_ref() {
        Some(d) => d,
        None => return,
    };
    let behavior = match config.0.genetics.behavior.as_ref() {
        Some(b) => b,
        None => return,
    };
    if !sensor_rays_active_this_tick(
        tick.tick,
        behavior.sensor_ray_interval_ticks,
        behavior.sensor_ray_phase_ticks,
    ) {
        return;
    }
    let environment = &config.0.environment;
    let fuzzy = behavior.fuzzy_nibble_delta;
    let corpse_targets: Vec<CorpseSenseTarget> = if environment.corpses_enabled {
        corpses
            .iter()
            .map(|(tf, c)| CorpseSenseTarget {
                position: tf.translation.truncate(),
                radius: c.radius,
            })
            .collect()
    } else {
        Vec::new()
    };

    let snapshots: Vec<(
        Entity,
        Vec2,
        Heading,
        AnyPhenotype,
        AnyGenome,
        f32,
        f32,
    )> = query
        .iter()
        .map(|(e, tf, h, g, p, a, l, _)| {
            (
                e,
                tf.translation.truncate(),
                *h,
                p.0.clone(),
                g.0.clone_genome(),
                a.radius,
                l.0,
            )
        })
        .collect();

    let targets: Vec<AgentSenseTarget> = snapshots
        .iter()
        .map(|(entity, pos, _, _, genome, radius, self_light)| AgentSenseTarget {
            entity: *entity,
            position: *pos,
            radius: *radius,
            genome: genome.clone(),
            self_light: *self_light,
        })
        .collect();

    let target_index: std::collections::HashMap<Entity, usize> = targets
        .iter()
        .enumerate()
        .map(|(i, t)| (t.entity, i))
        .collect();

    let mut updates: Vec<(Entity, SensePerception)> = Vec::with_capacity(snapshots.len());

    for (entity, pos, heading, phenotype, genome, _radius, self_light) in &snapshots {
        let perception = match phenotype {
            p if p.sensor_eye_count > 0 => {
                let self_idx = target_index.get(entity).copied().unwrap_or(0);
                let rays = perceive_agent(
                    self_idx,
                    *pos,
                    heading.0,
                    p,
                    decode,
                    &targets,
                    &corpse_targets,
                    environment.corpses_enabled,
                    &light,
                    &minerals,
                    *self_light,
                );
                let (pursue, avoid) =
                    parse_pursue_avoid_lists(genome).unwrap_or((Vec::new(), Vec::new()));
                let matches =
                    evaluate_ray_slot_matches(&rays, *pos, &targets, &pursue, &avoid, fuzzy);
                SensePerception {
                    rays,
                    pursue_matched: matches.pursue_matched,
                    avoid_matched: matches.avoid_matched,
                    pursue_bearing: matches.pursue_bearing,
                    avoid_bearing: matches.avoid_bearing,
                }
            }
            _ => SensePerception::default(),
        };
        updates.push((*entity, perception));
    }

    for (entity, perception) in updates {
        if let Ok((_, _, _, _, _, _, _, mut slot)) = query.get_mut(entity) {
            *slot = perception;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::genome::{SignatureAtom, SignatureSlot, SIG_BRIGHTER_SUNLIGHT};
    use crate::sim::perception::RayHit;
    use crate::sim::world::WorldBounds;
    use crate::sim::LightField;

    fn v2_config() -> SimConfig {
        SimConfig::load_from_path(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs/genome_v2.example.toml"),
        )
        .unwrap()
    }

    #[test]
    fn sensor_rays_active_every_tick_when_interval_is_one() {
        for tick in 1..=120 {
            assert!(sensor_rays_active_this_tick(tick, 1, 0));
        }
    }

    #[test]
    fn sensor_rays_active_every_sixty_ticks() {
        assert!(!sensor_rays_active_this_tick(1, 60, 0));
        assert!(!sensor_rays_active_this_tick(59, 60, 0));
        assert!(sensor_rays_active_this_tick(60, 60, 0));
        assert!(sensor_rays_active_this_tick(120, 60, 0));
        assert!(!sensor_rays_active_this_tick(61, 60, 0));
    }

    #[test]
    fn brighter_sunlight_signature_matches_upward_ray() {
        let config = v2_config();
        let bounds = WorldBounds::from_world_size(config.world.width, config.world.height);
        let light = LightField::from_environment(&config.environment, &bounds);
        let observer = Vec2::new(0.0, -bounds.half_y + 50.0);
        let top = Vec2::new(0.0, bounds.half_y - 20.0);
        let self_light = light.sample(observer);
        let hit_light = light.sample(top);
        let samples = vec![SenseRaySample {
            eye_index: 0,
            origin: observer,
            endpoint: top,
            half_width_deg: 30.0,
            hit: RayHit {
                distance: observer.distance(top),
                agent_index: None,
                light_at_hit: hit_light,
                chemical: crate::genome::ChemicalObservation {
                    dead_organism: false,
                    minerals_raw: 0,
                    light_delta: hit_light - self_light,
                },
            },
        }];

        let slots = vec![SignatureSlot::Simple(SignatureAtom {
            ty: SIG_BRIGHTER_SUNLIGHT,
            value: 1,
        })];

        let matches = evaluate_ray_slot_matches(&samples, observer, &[], &slots, &[], 1);
        let pursue = matches.pursue_matched;
        assert!(
            pursue,
            "ray toward brighter region should match brighter signature"
        );
    }
}

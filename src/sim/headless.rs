//! Fast headless tick loop for acceptance tests (no Bevy window).
//!
//! This is a **test-only shortcut** that mirrors metabolism → reproduction → age → death
//! without the full ECS pipeline. CLI and metrics determinism tests use [`run_headless_bevy`]
//! in [`app`](super::app) instead.

#![cfg(test)]

use crate::config::SimConfig;
use crate::genome::Genome;
use crate::sim::components::Energy;
use crate::sim::spawn::{
    build_offspring_spawn_params, build_spawn_params, place_near_parent, randomize_placement,
    try_prepare_spawn,
};
use crate::sim::{
    apply_failsafe, apply_metabolism_tick, attempt_reproduction,
    overlap_photo_light_multipliers, ReproAttemptResult,
};
use crate::sim::light::LightField;
use crate::sim::world::{NextEntityId, SimRng, WorldBounds};

/// Summary of a headless run (metabolism → reproduction → energy death).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessRunSummary {
    pub alive: usize,
    pub births: u32,
    pub repro_fatalities: u32,
    pub stillborns: u32,
    pub child_genome_fold: u64,
    /// Peak living count observed during the run.
    pub peak_alive: usize,
    /// Quantized RGB of survivors (for diversity checks).
    pub survivor_color_keys: Vec<(u8, u8, u8)>,
}

struct SimAgent {
    id: u64,
    generation: u32,
    genome: Genome,
    phenotype: crate::genome::Phenotype,
    energy: f32,
    core_hp: f32,
    position: bevy::prelude::Vec2,
    repro_cooldown: u32,
    pending_fatal: bool,
    age_ticks: u32,
}

fn fold_genome_bytes(g: &Genome) -> u64 {
    g.bytes
        .iter()
        .fold(0u64, |acc, &b| acc.wrapping_mul(31).wrapping_add(u64::from(b)))
}

fn color_key(rgb: [f32; 3]) -> (u8, u8, u8) {
    (
        (rgb[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (rgb[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (rgb[2].clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

/// Run Phase 1 metabolism + reproduction + death for `ticks` (requires `master_seed`).
pub fn run_headless_sim(config: &SimConfig, ticks: u64) -> HeadlessRunSummary {
    assert!(
        config.run.master_seed.is_some(),
        "headless acceptance runs require master_seed"
    );

    let ancestor = Genome::ancestor_from_config(config).unwrap();
    let bounds = WorldBounds::from_world_size(config.world.width, config.world.height);
    let light_field = LightField::from_environment(&config.environment, &bounds);
    let mut sim_rng = SimRng::from_config(config);
    let mut next_id = NextEntityId::new(1);
    let mut agents: Vec<SimAgent> = Vec::new();
    let spawn_rng = sim_rng.spawn_rng();

    for _ in 0..config.world.initial_population {
        let genome = ancestor.clone_genome();
        let Some((genome, phenotype)) = try_prepare_spawn(genome, config) else {
            continue;
        };
        let id = next_id.next();
        let mut params = build_spawn_params(genome.clone(), phenotype.clone(), config, id, None, 0);
        randomize_placement(&mut params, spawn_rng, &bounds, &config.world);

        agents.push(SimAgent {
            id,
            generation: 0,
            genome,
            phenotype,
            energy: config.economy.spawn_energy,
            core_hp: config.economy.core_max_hp,
            position: params.position,
            repro_cooldown: 0,
            pending_fatal: false,
            age_ticks: 0,
        });
    }

    let mut summary = HeadlessRunSummary {
        alive: 0,
        births: 0,
        repro_fatalities: 0,
        stillborns: 0,
        child_genome_fold: 0,
        peak_alive: agents.len(),
        survivor_color_keys: Vec::new(),
    };
    let mut spawn_blocked = false;

    for tick in 0..ticks {
        let overlap_geom: Vec<_> = agents
            .iter()
            .map(|a| (a.position, a.phenotype.body_radius))
            .collect();
        let photo_light_mults = if config.environment.overlap_shades_photosynthesis {
            overlap_photo_light_multipliers(&overlap_geom, config.environment.overlap_shade_factor)
        } else {
            vec![1.0; agents.len()]
        };

        for (idx, agent) in agents.iter_mut().enumerate() {
            let light = light_field.sample(agent.position);
            let mut energy = Energy {
                internal_energy: agent.energy,
            };
            let mut health = crate::sim::components::Health {
                core_hp: agent.core_hp,
                core_max_hp: config.economy.core_max_hp,
                part_hp: vec![],
            };
            let photo_light_multiplier = photo_light_mults.get(idx).copied().unwrap_or(1.0);
            apply_metabolism_tick(
                &mut energy,
                &mut health,
                &agent.phenotype,
                agent.age_ticks,
                light,
                &config.environment,
                &config.economy,
                photo_light_multiplier,
            );
            agent.energy = energy.internal_energy;
            agent.core_hp = health.core_hp;
        }

        agents.sort_by_key(|a| a.id);
        let mut new_children: Vec<SimAgent> = Vec::new();

        for agent in &mut agents {
            if agent.pending_fatal {
                continue;
            }
            if agent.repro_cooldown > 0 {
                agent.repro_cooldown -= 1;
                continue;
            }
            if spawn_blocked {
                continue;
            }
            if agent.energy < agent.phenotype.repro_threshold {
                continue;
            }

            let parent_pos = agent.position;
            let parent_radius = agent.phenotype.body_radius;

            let mut repro_rng = sim_rng.stream_repro_risk(agent.id, tick);
            let mut mutation_rng =
                sim_rng.stream_mutation(agent.id, agent.generation.saturating_add(1));
            let mut energy = Energy {
                internal_energy: agent.energy,
            };
            let mut health = crate::sim::components::Health {
                core_hp: agent.core_hp,
                core_max_hp: config.economy.core_max_hp,
                part_hp: vec![],
            };

            match attempt_reproduction(
                &mut energy,
                &mut health,
                &agent.phenotype,
                &agent.genome,
                config,
                &mut repro_rng,
                &mut mutation_rng,
            ) {
                ReproAttemptResult::Skipped => {}
                ReproAttemptResult::ParentFatal => {
                    agent.pending_fatal = true;
                    summary.repro_fatalities += 1;
                }
                ReproAttemptResult::Stillborn => {
                    agent.repro_cooldown = agent.phenotype.repro_cooldown_ticks;
                    summary.stillborns += 1;
                }
                ReproAttemptResult::Child {
                    genome: child_genome,
                    phenotype: child_phenotype,
                    child_start_hp,
                } => {
                    let child_id = next_id.next();
                    agent.repro_cooldown = agent.phenotype.repro_cooldown_ticks;
                    summary.births += 1;
                    summary.child_genome_fold ^= fold_genome_bytes(&child_genome);

                    let mut child_params = build_offspring_spawn_params(
                        child_genome.clone(),
                        child_phenotype.clone(),
                        config,
                        child_id,
                        Some(agent.id),
                        agent.generation.saturating_add(1),
                        0.0,
                        child_start_hp,
                    );
                    place_near_parent(
                        &mut child_params,
                        parent_pos,
                        parent_radius,
                        sim_rng.spawn_rng(),
                        &bounds,
                    );

                    new_children.push(SimAgent {
                        id: child_id,
                        generation: agent.generation.saturating_add(1),
                        genome: child_genome,
                        phenotype: child_phenotype,
                        energy: child_params.life.energy.internal_energy,
                        core_hp: child_params.life.health.core_hp,
                        position: child_params.position,
                        repro_cooldown: 0,
                        pending_fatal: false,
                        age_ticks: 0,
                    });
                }
            }
            agent.energy = energy.internal_energy;
            agent.core_hp = health.core_hp;
        }

        for agent in &mut agents {
            agent.age_ticks = agent.age_ticks.saturating_add(1);
        }

        agents.retain(|a| !a.pending_fatal && a.core_hp > 0.0);
        agents.extend(new_children);

        summary.peak_alive = summary.peak_alive.max(agents.len());
        let _ = apply_failsafe(
            agents.len(),
            config.performance.max_entities,
            config.performance.failsafe_policy,
            &mut spawn_blocked,
        );
    }

    summary.alive = agents.len();
    summary.survivor_color_keys = agents
        .iter()
        .map(|a| color_key(a.phenotype.color))
        .collect();
    summary
}

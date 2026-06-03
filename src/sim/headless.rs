//! Headless tick loop for Phase 1 acceptance tests (no Bevy window).

#![cfg(test)]

use crate::config::SimConfig;
use crate::genome::Genome;
use crate::sim::components::Energy;
use crate::sim::spawn::{
    build_offspring_spawn_params, build_spawn_params, place_near_parent, randomize_placement,
    try_prepare_spawn,
};
use crate::sim::{apply_metabolism_tick, attempt_reproduction, ReproAttemptResult};
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
    /// Quantized RGB of survivors (for diversity checks).
    pub survivor_color_keys: Vec<(u8, u8, u8)>,
}

struct SimAgent {
    id: u64,
    generation: u32,
    genome: Genome,
    phenotype: crate::genome::Phenotype,
    energy: f32,
    position: bevy::prelude::Vec2,
    repro_cooldown: u32,
    pending_fatal: bool,
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
            position: params.position,
            repro_cooldown: 0,
            pending_fatal: false,
        });
    }

    let mut summary = HeadlessRunSummary {
        alive: 0,
        births: 0,
        repro_fatalities: 0,
        stillborns: 0,
        child_genome_fold: 0,
        survivor_color_keys: Vec::new(),
    };

    for tick in 0..ticks {
        for agent in &mut agents {
            let light = light_field.sample(agent.position);
            let mut energy = Energy {
                internal_energy: agent.energy,
            };
            apply_metabolism_tick(
                &mut energy,
                &agent.phenotype,
                light,
                &config.environment,
                &config.economy,
                false,
            );
            agent.energy = energy.internal_energy;
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

            match attempt_reproduction(
                &mut energy,
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
                        agent.phenotype.offspring_endowment,
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
                        position: child_params.position,
                        repro_cooldown: 0,
                        pending_fatal: false,
                    });
                }
            }
            agent.energy = energy.internal_energy;
        }

        agents.retain(|a| !a.pending_fatal && a.energy > 0.0);
        agents.extend(new_children);
    }

    summary.alive = agents.len();
    summary.survivor_color_keys = agents
        .iter()
        .map(|a| color_key(a.phenotype.color))
        .collect();
    summary
}

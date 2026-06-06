//! Fast headless tick loop for acceptance tests (no Bevy window).
//!
//! Mirrors metabolism → reproduction → age → death without the full ECS pipeline.

#![cfg(test)]

use crate::config::SimConfig;
use crate::genome::{GenomeV2, PhenotypeV2};
use crate::sim::components::Energy;
use crate::sim::spawn::{
    build_initial_spawn_params, build_offspring_spawn_params, place_near_parent, randomize_placement,
    try_prepare_spawn,
};
use crate::sim::{
    apply_failsafe, apply_metabolism_tick_v2, attempt_reproduction_v2, ReproAttemptResult,
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
    genome: GenomeV2,
    phenotype: PhenotypeV2,
    energy: f32,
    core_hp: f32,
    core_max_hp: f32,
    max_energy: f32,
    position: bevy::prelude::Vec2,
    repro_cooldown: u32,
    pending_fatal: bool,
    age_ticks: u32,
}

fn fold_genome_bytes(g: &GenomeV2) -> u64 {
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

/// Run metabolism + reproduction + death for `ticks` (requires `master_seed`).
pub fn run_headless_sim(config: &SimConfig, ticks: u64) -> HeadlessRunSummary {
    assert!(
        config.run.master_seed.is_some(),
        "headless acceptance runs require master_seed"
    );

    let ancestor = GenomeV2::ancestor_from_config(config).unwrap();
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
        let mut params = build_initial_spawn_params(genome, phenotype, config, id, 0);
        randomize_placement(&mut params, spawn_rng, &bounds, &config.world);

        let p = &params.life.phenotype.0;
        agents.push(SimAgent {
            id,
            generation: 0,
            genome: params.life.genome.0.clone_genome(),
            phenotype: p.clone(),
            energy: params.life.energy.internal_energy,
            core_hp: params.life.health.core_hp,
            core_max_hp: params.life.health.core_max_hp,
            max_energy: p.max_energy,
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
        for agent in agents.iter_mut() {
            let light = light_field.sample(agent.position);
            let mut energy = Energy::new_v2(agent.energy, agent.max_energy);
            let mut health = crate::sim::components::Health {
                core_hp: agent.core_hp,
                core_max_hp: agent.core_max_hp,
                part_hp: vec![],
            };
            apply_metabolism_tick_v2(
                &mut energy,
                &mut health,
                &agent.phenotype,
                agent.age_ticks,
                light,
                &config.environment,
                &config.economy,
                config,
                1,
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
                if agent.repro_cooldown > 0 {
                    continue;
                }
            }
            if spawn_blocked {
                continue;
            }
            let threshold = agent.phenotype.max_energy * agent.phenotype.repro_threshold_frac;
            if agent.energy < threshold {
                continue;
            }

            let parent_pos = agent.position;
            let parent_radius = agent.phenotype.body_radius;

            let mut repro_rng = sim_rng.stream_repro_risk(agent.id, tick);
            let mut mutation_rng =
                sim_rng.stream_mutation(agent.id, agent.generation.saturating_add(1), tick);
            let mut energy = Energy::new_v2(agent.energy, agent.max_energy);
            let mut health = crate::sim::components::Health {
                core_hp: agent.core_hp,
                core_max_hp: agent.core_max_hp,
                part_hp: vec![],
            };

            match attempt_reproduction_v2(
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
                    agent.repro_cooldown = config.economy.repro_cooldown_ticks;
                    summary.stillborns += 1;
                }
                ReproAttemptResult::Child {
                    genome: child_genome,
                    phenotype: child_phenotype,
                    child_start_energy,
                    child_start_hp,
                } => {
                    let child_id = next_id.next();
                    agent.repro_cooldown = config.economy.repro_cooldown_ticks;
                    summary.births += 1;
                    summary.child_genome_fold ^= fold_genome_bytes(&child_genome);

                    let mut child_params = build_offspring_spawn_params(
                        child_genome.clone_genome(),
                        child_phenotype.clone(),
                        config,
                        child_id,
                        Some(agent.id),
                        agent.generation.saturating_add(1),
                        child_start_energy,
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
                        core_max_hp: child_params.life.health.core_max_hp,
                        max_energy: child_params.life.phenotype.0.max_energy,
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

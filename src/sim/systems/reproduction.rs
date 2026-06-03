//! Asexual reproduction, mutation, and repro-risk rolls (Phase 1H).
//!
//! `repro_ticks_remaining` decrements once per tick while cooldown is active (start of this
//! system's pass). Fatal repro-risk marks [`PendingDeath`](super::super::components::PendingDeath);
//! [`super::death::death_and_despawn`] despawns on the same tick.

use bevy::prelude::*;
use rand::Rng;

use crate::config::SimConfig;
use crate::genome::{mutate_offspring, Genome as GenomeData, Phenotype as PhenotypeData};
use crate::sim::spawn::{
    build_offspring_spawn_params, place_near_parent, spawn_agent_entity, try_prepare_spawn,
};

use super::super::components::{
    Cooldowns, DeathReason, Energy, Genome, Lineage, PendingDeath, Phenotype,
};
use super::super::spawn::SpawnParams;
use super::super::world::{NextEntityId, SimConfigResource, SimRng, SimTick, SpawnBlocked, WorldBounds};

/// Reproduction death probability after `repro_cost` is paid ([GENOME.md](../../docs/GENOME.md)).
pub fn repro_death_probability(
    energy_after_cost: f32,
    repro_threshold: f32,
    repro_risk_factor: f32,
    p_death_on_repro_base: f32,
    repro_safety_margin: f32,
) -> f32 {
    let threshold = repro_threshold.max(1e-6);
    let margin = energy_after_cost / threshold;
    let safety = repro_safety_margin.max(1e-6);
    let risk_term = (1.0 - margin / safety).max(0.0);
    (p_death_on_repro_base * repro_risk_factor * risk_term).clamp(0.0, 1.0)
}

/// Result of one reproduction attempt (pure logic for tests).
#[derive(Debug, Clone, PartialEq)]
pub enum ReproAttemptResult {
    /// Preconditions not met.
    Skipped,
    /// Parent died on repro-risk roll.
    ParentFatal,
    /// Risk passed but child invalid / stillborn; parent survives without child.
    Stillborn,
    /// Child genome produced (caller spawns entity).
    Child {
        genome: GenomeData,
        phenotype: PhenotypeData,
    },
}

/// Attempt reproduction for one parent (mutates `energy` in place when repro starts).
pub fn attempt_reproduction(
    energy: &mut Energy,
    phenotype: &PhenotypeData,
    parent_genome: &GenomeData,
    config: &SimConfig,
    repro_rng: &mut impl Rng,
    mutation_rng: &mut impl Rng,
) -> ReproAttemptResult {
    if energy.internal_energy < phenotype.repro_threshold {
        return ReproAttemptResult::Skipped;
    }

    energy.internal_energy -= phenotype.repro_cost;
    energy.internal_energy = energy.internal_energy.max(0.0);

    let p_death = repro_death_probability(
        energy.internal_energy,
        phenotype.repro_threshold,
        phenotype.repro_risk_factor,
        config.economy.p_death_on_repro_base,
        config.economy.repro_safety_margin,
    );

    if repro_rng.gen::<f32>() < p_death {
        return ReproAttemptResult::ParentFatal;
    }

    let child_genome = mutate_offspring(parent_genome, &config.genetics, mutation_rng);
    let Some((child_genome, child_phenotype)) = try_prepare_spawn(child_genome, config) else {
        return ReproAttemptResult::Stillborn;
    };

    if energy.internal_energy < phenotype.offspring_endowment {
        return ReproAttemptResult::Stillborn;
    }

    energy.internal_energy -= phenotype.offspring_endowment;
    energy.internal_energy = energy.internal_energy.max(0.0);

    ReproAttemptResult::Child {
        genome: child_genome,
        phenotype: child_phenotype,
    }
}

/// Bevy system: asexual reproduction after movement ([SIMULATION.md](../../docs/SIMULATION.md)).
pub fn reproduction(
    mut commands: Commands,
    config: Res<SimConfigResource>,
    tick: Res<SimTick>,
    bounds: Res<WorldBounds>,
    spawn_blocked: Res<SpawnBlocked>,
    mut next_id: ResMut<NextEntityId>,
    mut sim_rng: ResMut<SimRng>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<super::super::agent_material::AgentColorMaterial>>,
    mut query: Query<(
        Entity,
        &Transform,
        &Genome,
        &Phenotype,
        &mut Energy,
        &mut Cooldowns,
        &Lineage,
    )>,
) {
    let sim_config = &config.0;
    let mut children_to_spawn: Vec<(Vec2, f32, SpawnParams)> = Vec::new();

    // Stable order so `stream_spawn` child placement is reproducible under `master_seed`.
    let mut parents: Vec<(Entity, u64)> = query
        .iter()
        .map(|(entity, _, _, _, _, _, lineage)| (entity, lineage.id))
        .collect();
    parents.sort_by_key(|(_, id)| *id);

    for (entity, _) in parents {
        let Ok((_, transform, genome, phenotype, mut energy, mut cooldowns, lineage)) =
            query.get_mut(entity)
        else {
            continue;
        };
        if cooldowns.repro_ticks_remaining > 0 {
            cooldowns.repro_ticks_remaining -= 1;
            continue;
        }

        if spawn_blocked.0 {
            continue;
        }

        if energy.internal_energy < phenotype.0.repro_threshold {
            continue;
        }

        let mut repro_rng = sim_rng.stream_repro_risk(lineage.id, tick.tick);
        let mut mutation_rng =
            sim_rng.stream_mutation(lineage.id, lineage.generation.saturating_add(1));

        match attempt_reproduction(
            &mut energy,
            &phenotype.0,
            &genome.0,
            sim_config,
            &mut repro_rng,
            &mut mutation_rng,
        ) {
            ReproAttemptResult::Skipped => {}
            ReproAttemptResult::ParentFatal => {
                commands
                    .entity(entity)
                    .insert(PendingDeath(DeathReason::ReproFatality));
            }
            ReproAttemptResult::Stillborn => {
                cooldowns.repro_ticks_remaining = phenotype.0.repro_cooldown_ticks;
            }
            ReproAttemptResult::Child {
                genome: child_genome,
                phenotype: child_phenotype,
            } => {
                let child_id = next_id.next();
                let endowment = phenotype.0.offspring_endowment;
                let parent_radius = phenotype.0.body_radius;
                let params = build_offspring_spawn_params(
                    child_genome,
                    child_phenotype,
                    sim_config,
                    child_id,
                    Some(lineage.id),
                    lineage.generation.saturating_add(1),
                    endowment,
                );
                cooldowns.repro_ticks_remaining = phenotype.0.repro_cooldown_ticks;
                children_to_spawn.push((
                    transform.translation.truncate(),
                    parent_radius,
                    params,
                ));
            }
        }
    }

    let spawn_rng = sim_rng.spawn_rng();
    for (parent_pos, parent_radius, mut params) in children_to_spawn {
        place_near_parent(&mut params, parent_pos, parent_radius, spawn_rng, &bounds);
        spawn_agent_entity(&mut commands, &mut meshes, &mut materials, params);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use crate::sim::apply_metabolism_tick;
    use crate::config::SimConfig;
    use crate::genome::{decode, Genome};
    use crate::sim::components::Energy;

    fn setup() -> (SimConfig, PhenotypeData, GenomeData) {
        let config = SimConfig::load_default().unwrap();
        let genome = Genome::ancestor_from_config(&config).unwrap();
        let phenotype = decode(&genome, &config).unwrap();
        (config, phenotype, genome)
    }

    #[test]
    fn repro_risk_higher_when_low_energy() {
        let (config, phenotype, _genome) = setup();
        let high = repro_death_probability(
            phenotype.repro_threshold * 2.0,
            phenotype.repro_threshold,
            1.0,
            1.0,
            config.economy.repro_safety_margin,
        );
        let low = repro_death_probability(
            phenotype.repro_threshold * 0.1,
            phenotype.repro_threshold,
            1.0,
            1.0,
            config.economy.repro_safety_margin,
        );
        assert!(low > high);
    }

    #[test]
    fn mutation_produces_different_child_genome() {
        let (config, phenotype, genome) = setup();
        let mut energy = Energy {
            internal_energy: phenotype.repro_threshold + phenotype.repro_cost + phenotype.offspring_endowment + 100.0,
        };
        let mut repro_rng = StdRng::seed_from_u64(1);
        let mut mutation_rng = StdRng::seed_from_u64(2);

        let mut saw_child = false;
        for _ in 0..20 {
            energy.internal_energy =
                phenotype.repro_threshold + phenotype.repro_cost + phenotype.offspring_endowment + 100.0;
            if let ReproAttemptResult::Child { genome: child, .. } = attempt_reproduction(
                &mut energy,
                &phenotype,
                &genome,
                &config,
                &mut repro_rng,
                &mut mutation_rng,
            ) {
                assert_ne!(genome.to_hex(), child.to_hex());
                saw_child = true;
                break;
            }
        }
        assert!(saw_child);
    }

    fn genome_with_repro_loci(cost_raw: u16, threshold_raw: u16) -> (SimConfig, GenomeData, PhenotypeData) {
        let config = SimConfig::load_default().unwrap();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        let i = crate::genome::HEADER_LEN;
        bytes[i + crate::genome::core::REPRO_THRESHOLD] = (threshold_raw >> 8) as u8;
        bytes[i + crate::genome::core::REPRO_THRESHOLD + 1] = (threshold_raw & 0xFF) as u8;
        bytes[i + crate::genome::core::REPRO_COST] = (cost_raw >> 8) as u8;
        bytes[i + crate::genome::core::REPRO_COST + 1] = (cost_raw & 0xFF) as u8;
        let genome =
            Genome::from_bytes(bytes, config.genetics.genome_version, config.genetics.core_length)
                .unwrap();
        let phenotype = decode(&genome, &config).unwrap();
        (config, genome, phenotype)
    }

    #[test]
    fn low_energy_can_trigger_parent_fatal() {
        let (mut config, genome, phenotype) = genome_with_repro_loci(0xFFFF, 0x8000);
        config.economy.p_death_on_repro_base = 1.0;
        assert!(phenotype.repro_cost >= phenotype.repro_threshold);

        let energy_after_cost = 1.0;
        let attempt_energy = phenotype.repro_cost + energy_after_cost;
        let p_death = repro_death_probability(
            energy_after_cost,
            phenotype.repro_threshold,
            phenotype.repro_risk_factor,
            config.economy.p_death_on_repro_base,
            config.economy.repro_safety_margin,
        );
        assert!(p_death > 0.2, "p_death should be high, got {p_death}");

        let mut saw_fatal = false;
        for seed in 0..200_u64 {
            let mut repro_rng = StdRng::seed_from_u64(seed);
            let mut mutation_rng = StdRng::seed_from_u64(seed.wrapping_add(1));
            let mut energy = Energy {
                internal_energy: attempt_energy,
            };
            if matches!(
                attempt_reproduction(
                    &mut energy,
                    &phenotype,
                    &genome,
                    &config,
                    &mut repro_rng,
                    &mut mutation_rng,
                ),
                ReproAttemptResult::ParentFatal
            ) {
                saw_fatal = true;
                break;
            }
        }
        assert!(saw_fatal, "expected ParentFatal when margin is low after repro_cost");
    }

    struct SimAgent {
        genome: GenomeData,
        phenotype: PhenotypeData,
        energy: Energy,
        repro_cooldown: u32,
        generation: u32,
    }

    #[test]
    fn population_grows_under_default_economy() {
        let config = SimConfig::load_default().unwrap();
        let genome = Genome::ancestor_from_config(&config).unwrap();
        let phenotype = decode(&genome, &config).unwrap();

        let mut agents = vec![SimAgent {
            genome: genome.clone(),
            phenotype: phenotype.clone(),
            energy: Energy {
                internal_energy: config.economy.spawn_energy,
            },
            repro_cooldown: 0,
            generation: 0,
        }];

        let mut base_repro = StdRng::seed_from_u64(42);
        let mut base_mut = StdRng::seed_from_u64(43);

        for tick in 0..3000_u64 {
            for agent in &mut agents {
                if agent.repro_cooldown > 0 {
                    agent.repro_cooldown -= 1;
                }
            }

            let mut i = 0;
            while i < agents.len() {
                let agent = &mut agents[i];
                if agent.repro_cooldown > 0 || agent.energy.internal_energy < agent.phenotype.repro_threshold
                {
                    i += 1;
                    continue;
                }

                let mut repro_rng =
                    StdRng::seed_from_u64(base_repro.gen::<u64>() ^ tick ^ agent.generation as u64);
                let mut mutation_rng =
                    StdRng::seed_from_u64(base_mut.gen::<u64>() ^ tick ^ i as u64);

                match attempt_reproduction(
                    &mut agent.energy,
                    &agent.phenotype,
                    &agent.genome,
                    &config,
                    &mut repro_rng,
                    &mut mutation_rng,
                ) {
                    ReproAttemptResult::ParentFatal => {
                        agents.remove(i);
                    }
                    ReproAttemptResult::Stillborn => {
                        agent.repro_cooldown = agent.phenotype.repro_cooldown_ticks;
                        i += 1;
                    }
                    ReproAttemptResult::Child {
                        genome: child_g,
                        phenotype: child_p,
                    } => {
                        let endowment = agent.phenotype.offspring_endowment;
                        let child_generation = agent.generation.saturating_add(1);
                        agent.repro_cooldown = agent.phenotype.repro_cooldown_ticks;
                        agents.push(SimAgent {
                            genome: child_g,
                            phenotype: child_p,
                            energy: Energy {
                                internal_energy: endowment,
                            },
                            repro_cooldown: 0,
                            generation: child_generation,
                        });
                        i += 1;
                    }
                    ReproAttemptResult::Skipped => i += 1,
                }
            }

            for agent in &mut agents {
                apply_metabolism_tick(
                    &mut agent.energy,
                    &agent.phenotype,
                    1.0,
                    &config.environment,
                    &config.economy,
                    false,
                );
            }

            agents.retain(|a| a.energy.internal_energy > 0.0);
            if agents.is_empty() {
                break;
            }
        }

        assert!(
            agents.len() > 1,
            "population should grow with default grant over 3k ticks, got {}",
            agents.len()
        );
    }
}

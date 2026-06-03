//! Asexual reproduction, mutation, and repro-risk rolls (Phase 1H).
//!
//! Reproduction spends `repro_threshold` energy from the parent (parent HP unchanged).
//! That energy is converted into the child's starting `core_hp` (`offspring_endowment`, capped);
//! the child spawns with `internal_energy = 0`. `repro_ticks_remaining` decrements once per
//! tick while cooldown is active (start of this system's pass).

use bevy::prelude::*;
use rand::Rng;

use crate::config::SimConfig;
use crate::genome::{mutate_offspring, Genome as GenomeData, Phenotype as PhenotypeData};
use crate::sim::spawn::{
    build_offspring_spawn_params, place_near_parent, spawn_agent_entity, try_prepare_spawn,
};

use super::super::components::{
    Cooldowns, DeathReason, Energy, Genome, Health, Lineage, PendingDeath, Phenotype,
};
use super::super::spawn::SpawnParams;
use super::super::metrics::MetricsPeriodCounters;
use super::super::world::{NextEntityId, SimConfigResource, SimRng, SimTick, SpawnBlocked, WorldBounds};

/// Parent death probability on reproduce ([GENOME.md](../../docs/GENOME.md)).
pub fn repro_death_probability(
    repro_risk_factor: f32,
    p_death_on_repro_base: f32,
    repro_safety_margin: f32,
    energy_after_transfer: f32,
    repro_threshold: f32,
) -> f32 {
    let threshold = repro_threshold.max(1e-6);
    let margin = energy_after_transfer / threshold;
    let safety = repro_safety_margin.max(1e-6);
    let margin_term = (1.0 - margin / safety).max(0.0);
    (p_death_on_repro_base * repro_risk_factor * margin_term).clamp(0.0, 1.0)
}

/// Parent damage probability on reproduce (independent roll from death).
pub fn repro_damage_probability(repro_risk_factor: f32, p_damage_on_repro_base: f32) -> f32 {
    (p_damage_on_repro_base * repro_risk_factor).clamp(0.0, 1.0)
}

/// Result of one reproduction attempt (pure logic for tests).
#[derive(Debug, Clone, PartialEq)]
pub enum ReproAttemptResult {
    /// Preconditions not met.
    Skipped,
    /// Parent died on repro-risk roll or lethal HP cost.
    ParentFatal,
    /// Risk passed but child invalid / stillborn; parent survives without child.
    Stillborn,
    /// Child genome produced (caller spawns entity).
    Child {
        genome: GenomeData,
        phenotype: PhenotypeData,
        /// Starting core HP (parent energy cost converted; child energy is zero).
        child_start_hp: f32,
    },
}

/// Deduct reproduction energy cost from the parent. Returns `false` if energy is insufficient.
pub fn pay_reproduction_energy_cost(energy: &mut Energy, cost: f32) -> bool {
    let cost = cost.max(0.0);
    if energy.internal_energy < cost {
        return false;
    }
    energy.internal_energy -= cost;
    energy.internal_energy = energy.internal_energy.max(0.0);
    true
}

/// Attempt reproduction for one parent (mutates `energy` and `health` when repro starts).
pub fn attempt_reproduction(
    energy: &mut Energy,
    health: &mut Health,
    phenotype: &PhenotypeData,
    parent_genome: &GenomeData,
    config: &SimConfig,
    repro_rng: &mut impl Rng,
    mutation_rng: &mut impl Rng,
) -> ReproAttemptResult {
    if energy.internal_energy < phenotype.repro_threshold {
        return ReproAttemptResult::Skipped;
    }

    let energy_cost = phenotype.repro_threshold;
    // Paid energy converts 1:1 into child core HP (capped by gene and core_max_hp).
    let child_start_hp = phenotype
        .offspring_endowment
        .min(energy_cost)
        .min(phenotype.core_max_hp)
        .max(0.0);
    if energy_cost <= 0.0 || child_start_hp <= 0.0 {
        return ReproAttemptResult::Skipped;
    }

    if !pay_reproduction_energy_cost(energy, energy_cost) {
        return ReproAttemptResult::Skipped;
    }

    let economy = &config.economy;
    let p_death = repro_death_probability(
        phenotype.repro_risk_factor,
        economy.p_death_on_repro_base,
        economy.repro_safety_margin,
        energy.internal_energy,
        phenotype.repro_threshold,
    );

    if repro_rng.gen::<f32>() < p_death {
        return ReproAttemptResult::ParentFatal;
    }

    let p_damage = repro_damage_probability(
        phenotype.repro_risk_factor,
        economy.p_damage_on_repro_base,
    );
    if repro_rng.gen::<f32>() < p_damage {
        let dmg = health.core_max_hp * economy.repro_damage_fraction_of_max_hp;
        health.core_hp -= dmg;
        if health.core_hp <= 0.0 {
            return ReproAttemptResult::ParentFatal;
        }
    }

    let child_genome = mutate_offspring(parent_genome, &config.genetics, mutation_rng);
    let Some((child_genome, child_phenotype)) = try_prepare_spawn(child_genome, config) else {
        return ReproAttemptResult::Stillborn;
    };

    ReproAttemptResult::Child {
        genome: child_genome,
        phenotype: child_phenotype,
        child_start_hp,
    }
}

/// Bevy system: asexual reproduction after movement ([SIMULATION.md](../../docs/SIMULATION.md)).
pub fn reproduction(
    mut commands: Commands,
    config: Res<SimConfigResource>,
    tick: Res<SimTick>,
    bounds: Res<WorldBounds>,
    spawn_blocked: Res<SpawnBlocked>,
    mut counters: ResMut<MetricsPeriodCounters>,
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
        &mut Health,
        &mut Cooldowns,
        &Lineage,
    )>,
) {
    let sim_config = &config.0;
    let mut children_to_spawn: Vec<(Vec2, f32, SpawnParams)> = Vec::new();

    let mut parents: Vec<(Entity, u64)> = query
        .iter()
        .map(|(entity, _, _, _, _, _, _, lineage)| (entity, lineage.id))
        .collect();
    parents.sort_by_key(|(_, id)| *id);

    for (entity, _) in parents {
        let Ok((_, transform, genome, phenotype, mut energy, mut health, mut cooldowns, lineage)) =
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
            &mut health,
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
                child_start_hp,
            } => {
                let child_id = next_id.next();
                let parent_radius = phenotype.0.body_radius;
                let params = build_offspring_spawn_params(
                    child_genome,
                    child_phenotype,
                    sim_config,
                    child_id,
                    Some(lineage.id),
                    lineage.generation.saturating_add(1),
                    0.0,
                    child_start_hp,
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
        counters.record_birth();
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
    use crate::sim::components::{Energy, Health};

    fn setup() -> (SimConfig, PhenotypeData, GenomeData) {
        let config = SimConfig::load_default().unwrap();
        let genome = Genome::ancestor_from_config(&config).unwrap();
        let phenotype = decode(&genome, &config).unwrap();
        (config, phenotype, genome)
    }

    fn full_parent_state(config: &SimConfig, phenotype: &PhenotypeData) -> (Energy, Health) {
        (
            Energy {
                internal_energy: phenotype.repro_threshold + 50.0,
            },
            Health {
                core_hp: config.economy.core_max_hp,
                core_max_hp: config.economy.core_max_hp,
                part_hp: vec![],
            },
        )
    }

    #[test]
    fn repro_risk_higher_when_low_energy_after_transfer() {
        let (config, phenotype, _genome) = setup();
        let high = repro_death_probability(
            1.0,
            1.0,
            config.economy.repro_safety_margin,
            phenotype.repro_threshold * 2.0,
            phenotype.repro_threshold,
        );
        let low = repro_death_probability(
            1.0,
            1.0,
            config.economy.repro_safety_margin,
            phenotype.repro_threshold * 0.1,
            phenotype.repro_threshold,
        );
        assert!(low > high);
    }

    #[test]
    fn reproduction_cost_is_energy_only_parent_hp_unchanged() {
        let (config, phenotype, genome) = setup();
        let mut energy = Energy {
            internal_energy: phenotype.repro_threshold,
        };
        let mut health = Health {
            core_hp: config.economy.core_max_hp,
            core_max_hp: config.economy.core_max_hp,
            part_hp: vec![],
        };
        let mut repro_rng = StdRng::seed_from_u64(99);
        let mut mutation_rng = StdRng::seed_from_u64(100);

        if let ReproAttemptResult::Child { child_start_hp, .. } = attempt_reproduction(
            &mut energy,
            &mut health,
            &phenotype,
            &genome,
            &config,
            &mut repro_rng,
            &mut mutation_rng,
        ) {
            assert_eq!(energy.internal_energy, 0.0);
            assert_eq!(health.core_hp, config.economy.core_max_hp);
            assert!((child_start_hp - phenotype.offspring_endowment).abs() < 1e-3);
        } else {
            panic!("expected child");
        }
    }

    #[test]
    fn energy_cost_fails_when_below_threshold() {
        let mut energy = Energy {
            internal_energy: 50.0,
        };
        assert!(!pay_reproduction_energy_cost(&mut energy, 100.0));
        assert_eq!(energy.internal_energy, 50.0);
    }

    #[test]
    fn mutation_produces_different_child_genome() {
        let (config, phenotype, genome) = setup();
        let (mut energy, mut health) = full_parent_state(&config, &phenotype);
        let mut repro_rng = StdRng::seed_from_u64(1);
        let mut mutation_rng = StdRng::seed_from_u64(2);

        let mut saw_child = false;
        for _ in 0..20 {
            let (e, h) = full_parent_state(&config, &phenotype);
            energy = e;
            health = h;
            if let ReproAttemptResult::Child { genome: child, .. } = attempt_reproduction(
                &mut energy,
                &mut health,
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

    #[test]
    fn repro_death_roll_can_kill_parent() {
        let (mut config, mut phenotype, genome) = setup();
        config.economy.p_death_on_repro_base = 1.0;
        config.economy.p_damage_on_repro_base = 0.0;
        config.economy.repro_safety_margin = 0.25;
        phenotype.repro_risk_factor = 1.0;
        phenotype.offspring_endowment = config.economy.core_max_hp;

        let mut saw_fatal = false;
        for seed in 0..50_u64 {
            let mut repro_rng = StdRng::seed_from_u64(seed);
            let mut mutation_rng = StdRng::seed_from_u64(seed.wrapping_add(1));
            let mut energy = Energy {
                internal_energy: phenotype.repro_threshold + 1.0,
            };
            let mut health = Health {
                core_hp: config.economy.core_max_hp,
                core_max_hp: config.economy.core_max_hp,
                part_hp: vec![],
            };
            if matches!(
                attempt_reproduction(
                    &mut energy,
                    &mut health,
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
        assert!(saw_fatal, "expected ParentFatal with p_death_on_repro_base = 1");
    }

    struct SimAgent {
        genome: GenomeData,
        phenotype: PhenotypeData,
        energy: Energy,
        health: Health,
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
                internal_energy: phenotype.repro_threshold,
            },
            health: Health {
                core_hp: config.economy.core_max_hp,
                core_max_hp: config.economy.core_max_hp,
                part_hp: vec![],
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
                if agent.repro_cooldown > 0
                    || agent.energy.internal_energy < agent.phenotype.repro_threshold
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
                    &mut agent.health,
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
                        child_start_hp,
                    } => {
                        let child_generation = agent.generation.saturating_add(1);
                        agent.repro_cooldown = agent.phenotype.repro_cooldown_ticks;
                        agents.push(SimAgent {
                            genome: child_g,
                            phenotype: child_p,
                            energy: Energy {
                                internal_energy: 0.0,
                            },
                            health: Health {
                                core_hp: child_start_hp,
                                core_max_hp: config.economy.core_max_hp,
                                part_hp: vec![],
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
                    &mut agent.health,
                    &agent.phenotype,
                    tick as u32,
                    1.0,
                    &config.environment,
                    &config.economy,
                    1.0,
                );
            }

            agents.retain(|a| a.health.core_hp > 0.0);
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

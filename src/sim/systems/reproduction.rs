//! Asexual reproduction, mutation, and repro-risk rolls (v2).

use bevy::prelude::*;
use rand::Rng;

use crate::config::SimConfig;
use crate::genome::AnyGenome;

use super::reproduction_v2::{attempt_reproduction_v2, repro_threshold_energy};
use crate::sim::spawn::{
    build_offspring_spawn_params, place_near_parent, spawn_agent_entity, try_prepare_spawn,
};

use super::super::components::{
    Cooldowns, DeathReason, Energy, Genome, Health, Lineage, PendingDeath, Phenotype,
};
use super::super::spawn::SpawnParams;
use super::super::interval::{logic_active_this_tick, logic_elapsed_ticks};
use super::super::metrics::MetricsPeriodCounters;
use super::super::world::{NextEntityId, SimConfigResource, SimRng, SimTick, SpawnBlocked, WorldBounds};

/// Parent death probability on reproduce.
pub fn repro_death_probability(
    p_death_on_repro_base: f32,
    repro_safety_margin: f32,
    energy_after_transfer: f32,
    repro_threshold: f32,
) -> f32 {
    let threshold = repro_threshold.max(1e-6);
    let margin = energy_after_transfer / threshold;
    let safety = repro_safety_margin.max(1e-6);
    let margin_term = (1.0 - margin / safety).max(0.0);
    (p_death_on_repro_base * margin_term).clamp(0.0, 1.0)
}

/// Parent damage probability on reproduce (independent roll from death).
pub fn repro_damage_probability(p_damage_on_repro_base: f32) -> f32 {
    p_damage_on_repro_base.clamp(0.0, 1.0)
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
        genome: AnyGenome,
        phenotype: crate::genome::AnyPhenotype,
        child_start_energy: f32,
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
    if !logic_active_this_tick(tick.tick, &sim_config.run) {
        return;
    }
    let logic_steps = logic_elapsed_ticks(&sim_config.run);
    let mut children_to_spawn: Vec<(Vec2, f32, SpawnParams)> = Vec::new();

    // Cooldowns tick for everyone; only threshold-ready agents enter the repro pass.
    let mut candidates: Vec<(Entity, u64)> = Vec::new();
    for (entity, _, _, phenotype, energy, _, mut cooldowns, lineage) in query.iter_mut() {
        if cooldowns.repro_ticks_remaining > 0 {
            cooldowns.repro_ticks_remaining =
                cooldowns.repro_ticks_remaining.saturating_sub(logic_steps);
        }
        if cooldowns.repro_ticks_remaining > 0 || spawn_blocked.0 {
            continue;
        }
        let threshold = repro_threshold_energy(&phenotype.0);
        if energy.internal_energy >= threshold {
            candidates.push((entity, lineage.id));
        }
    }
    candidates.sort_by_key(|(_, id)| *id);

    for (entity, _) in candidates {
        let Ok((_, transform, genome, phenotype, mut energy, mut health, mut cooldowns, lineage)) =
            query.get_mut(entity)
        else {
            continue;
        };
        let phenotype_v2 = &phenotype.0;
        let genome_v2 = &genome.0;

        let mut repro_rng = sim_rng.stream_repro_risk(lineage.id, tick.tick);
        let mut mutation_rng =
            sim_rng.stream_mutation(lineage.id, lineage.generation.saturating_add(1), tick.tick);

        let result = attempt_reproduction_v2(
            &mut energy,
            &mut health,
            phenotype_v2,
            genome_v2,
            sim_config,
            &mut repro_rng,
            &mut mutation_rng,
        );

        match result {
            ReproAttemptResult::Skipped => {}
            ReproAttemptResult::ParentFatal => {
                commands
                    .entity(entity)
                    .insert(PendingDeath(DeathReason::ReproFatality));
            }
            ReproAttemptResult::Stillborn => {
                cooldowns.repro_ticks_remaining = sim_config.economy.repro_cooldown_ticks;
            }
            ReproAttemptResult::Child {
                genome: child_genome,
                phenotype: child_phenotype,
                child_start_energy,
                child_start_hp,
            } => {
                let child_id = next_id.next();
                let parent_radius = phenotype.body_radius();
                let params = build_offspring_spawn_params(
                    child_genome,
                    child_phenotype,
                    sim_config,
                    child_id,
                    Some(lineage.id),
                    lineage.generation.saturating_add(1),
                    child_start_energy,
                    child_start_hp,
                );
                cooldowns.repro_ticks_remaining = sim_config.economy.repro_cooldown_ticks;
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
    use crate::config::SimConfig;
    use crate::genome::{decode_v2, GenomeV2};
    use crate::sim::components::Energy;

    fn setup() -> (SimConfig, crate::genome::PhenotypeV2) {
        let config = SimConfig::load_default().unwrap();
        let genome = GenomeV2::ancestor_from_config(&config).unwrap();
        let phenotype = decode_v2(&genome, &config).unwrap();
        (config, phenotype)
    }

    #[test]
    fn repro_death_higher_when_low_energy_after_transfer() {
        let (config, phenotype) = setup();
        let threshold = phenotype.max_energy * phenotype.repro_threshold_frac;
        let high = repro_death_probability(
            1.0,
            config.economy.repro_safety_margin,
            threshold * 2.0,
            threshold,
        );
        let low = repro_death_probability(
            1.0,
            config.economy.repro_safety_margin,
            threshold * 0.1,
            threshold,
        );
        assert!(low > high);
    }

    #[test]
    fn energy_cost_fails_when_below_threshold() {
        let mut energy = Energy::new_v2(50.0, 100.0);
        assert!(!pay_reproduction_energy_cost(&mut energy, 100.0));
        assert_eq!(energy.internal_energy, 50.0);
    }
}

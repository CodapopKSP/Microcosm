//! Agent spawn pipeline ([ENTITIES.md](../../docs/ENTITIES.md#spawn), Phase 1E).

use bevy::prelude::*;
use rand::Rng;

use crate::config::{SimConfig, WorldConfig};
use super::agent_material::AgentColorMaterial;
use crate::genome::{decode, Genome as GenomeData, Phenotype as PhenotypeData};

use super::components::{LifeBundle, LightExposure, Velocity};
use super::world::{NextEntityId, SimRng, WorldBounds};

/// Agent mesh Z (draw order comes from [`AgentColorMaterial::depth_bias`](super::agent_material::AgentColorMaterial)).
const AGENT_MESH_Z: f32 = 0.0;

/// Outcome of one spawn attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnOutcome {
    Spawned,
    Stillborn,
}

/// Lifecycle + visual data ready for Bevy entity insertion.
#[derive(Clone)]
pub struct SpawnParams {
    pub life: LifeBundle,
    pub position: Vec2,
    pub velocity: Vec2,
    pub color: Color,
    pub body_radius: f32,
}

/// Validate genome and decode phenotype; returns `None` on stillborn.
pub fn try_prepare_spawn(
    genome: GenomeData,
    config: &SimConfig,
) -> Option<(GenomeData, PhenotypeData)> {
    let genetics = &config.genetics;
    if !genome.is_valid(genetics.genome_version, genetics.core_length) {
        return None;
    }
    let phenotype = decode(&genome, config).ok()?;
    Some((genome, phenotype))
}

/// Build spawn params after validation (caller assigns lineage id).
pub fn build_spawn_params(
    genome: GenomeData,
    phenotype: PhenotypeData,
    config: &SimConfig,
    id: u64,
    parent_id: Option<u64>,
    generation: u32,
) -> SpawnParams {
    build_offspring_spawn_params(
        genome,
        phenotype,
        config,
        id,
        parent_id,
        generation,
        config.economy.spawn_energy,
    )
}

/// Build spawn params for a child with explicit starting energy (offspring endowment).
pub fn build_offspring_spawn_params(
    genome: GenomeData,
    phenotype: PhenotypeData,
    config: &SimConfig,
    id: u64,
    parent_id: Option<u64>,
    generation: u32,
    initial_energy: f32,
) -> SpawnParams {
    let color = phenotype_to_color(&phenotype);
    let body_radius = phenotype.body_radius;
    let life = LifeBundle::from_parts_with_energy(
        genome,
        phenotype,
        config,
        id,
        parent_id,
        generation,
        initial_energy,
    );

    SpawnParams {
        life,
        position: Vec2::ZERO,
        velocity: Vec2::ZERO,
        color,
        body_radius,
    }
}

/// Validate, decode, and build a full spawn package; `None` if stillborn.
pub fn try_build_spawn(
    genome: GenomeData,
    config: &SimConfig,
    id: u64,
    parent_id: Option<u64>,
    generation: u32,
) -> Option<SpawnParams> {
    let (genome, phenotype) = try_prepare_spawn(genome, config)?;
    Some(build_spawn_params(
        genome,
        phenotype,
        config,
        id,
        parent_id,
        generation,
    ))
}

pub fn phenotype_to_color(phenotype: &PhenotypeData) -> Color {
    Color::srgb(phenotype.color[0], phenotype.color[1], phenotype.color[2])
}

/// Random position inside world bounds (`stream_spawn` RNG).
pub fn randomize_position(params: &mut SpawnParams, rng: &mut impl Rng, bounds: &WorldBounds) {
    let radius = params.body_radius;
    let (min_x, max_x) = axis_range(bounds.half_x, radius);
    let (min_y, max_y) = axis_range(bounds.half_y, radius);
    params.position = Vec2::new(rng.gen_range(min_x..max_x), rng.gen_range(min_y..max_y));
}

/// Random position and velocity inside world bounds (`stream_spawn` RNG).
pub fn randomize_placement(
    params: &mut SpawnParams,
    rng: &mut impl Rng,
    bounds: &WorldBounds,
    world: &WorldConfig,
) {
    randomize_position(params, rng, bounds);
    let angle = rng.gen_range(0.0..std::f32::consts::TAU);
    let speed = random_initial_speed(
        params.life.phenotype.0.max_speed,
        world.min_speed,
        rng,
    );
    params.velocity = Vec2::from_angle(angle) * speed;
}

/// Place child near parent (uses `stream_spawn` RNG semantics via caller-provided rng).
pub fn place_near_parent(
    params: &mut SpawnParams,
    parent_pos: Vec2,
    parent_radius: f32,
    rng: &mut impl Rng,
    bounds: &WorldBounds,
) {
    let child_radius = params.body_radius;
    let gap = rng.gen_range(2.0..12.0);
    let angle = rng.gen_range(0.0..std::f32::consts::TAU);
    let offset = Vec2::from_angle(angle) * (parent_radius + child_radius + gap);
    let mut pos = parent_pos + offset;

    let (min_x, max_x) = axis_range(bounds.half_x, child_radius);
    let (min_y, max_y) = axis_range(bounds.half_y, child_radius);
    pos.x = pos.x.clamp(min_x, max_x);
    pos.y = pos.y.clamp(min_y, max_y);
    params.position = pos;

    let speed = random_initial_speed(params.life.phenotype.0.max_speed, 5.0, rng);
    params.velocity = Vec2::from_angle(angle) * speed;
}

/// Spawn velocity magnitude from decoded [`Phenotype::max_speed`](crate::genome::Phenotype::max_speed).
fn random_initial_speed(max_speed: f32, min_speed: f32, rng: &mut impl Rng) -> f32 {
    let cap = max_speed.max(0.0);
    let min = min_speed.min(cap);
    if cap <= min + 1e-6 {
        return cap;
    }
    rng.gen_range(min..cap)
}

fn axis_range(half_extent: f32, radius: f32) -> (f32, f32) {
    let min = -half_extent + radius;
    let max = half_extent - radius;
    if min <= max {
        (min, max)
    } else {
        (0.0, 0.0)
    }
}

/// Insert one agent entity (mesh radius and color from decoded phenotype).
pub fn spawn_agent_entity(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<AgentColorMaterial>,
    params: SpawnParams,
) -> Entity {
    let lineage_id = params.life.lineage.id;
    commands
        .spawn((
            params.life,
            Velocity(params.velocity),
            LightExposure(0.0),
            Mesh2d(meshes.add(Circle::new(params.body_radius))),
            MeshMaterial2d(materials.add(AgentColorMaterial::new(params.color, lineage_id))),
            Transform::from_translation(params.position.extend(AGENT_MESH_Z)),
        ))
        .id()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitialPopulationStats {
    pub spawned: usize,
    pub stillborn: usize,
}

/// Spawn the initial population from a shared ancestor template (no worldgen mutation).
pub fn spawn_initial_population(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<AgentColorMaterial>,
    config: &SimConfig,
    ancestor: &GenomeData,
    bounds: &WorldBounds,
    next_id: &mut NextEntityId,
    sim_rng: &mut SimRng,
) -> InitialPopulationStats {
    let count = config.world.initial_population;
    let mut stats = InitialPopulationStats {
        spawned: 0,
        stillborn: 0,
    };
    let rng = sim_rng.spawn_rng();

    for _ in 0..count {
        let genome = ancestor.clone_genome();
        let Some((genome, phenotype)) = try_prepare_spawn(genome, config) else {
            stats.stillborn += 1;
            continue;
        };

        let id = next_id.next();
        let mut params = build_spawn_params(genome, phenotype, config, id, None, 0);
        randomize_placement(&mut params, rng, bounds, &config.world);
        debug_assert_eq!(params.life.agent.radius, params.body_radius);

        spawn_agent_entity(commands, meshes, materials, params);
        stats.spawned += 1;
    }

    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::Genome;

    #[test]
    fn invalid_genome_is_stillborn() {
        let config = SimConfig::load_default().unwrap();
        let mut genome = Genome::ancestor_from_config(&config).unwrap();
        genome.bytes[0] = b'X';
        assert!(try_prepare_spawn(genome, &config).is_none());
        assert!(try_build_spawn(
            Genome::ancestor_from_config(&config).unwrap(),
            &config,
            1,
            None,
            0,
        )
        .is_some());

        let mut bad = Genome::ancestor_from_config(&config).unwrap();
        bad.bytes[0] = b'X';
        assert!(try_build_spawn(bad, &config, 1, None, 0).is_none());
    }

    #[test]
    fn spawn_uses_phenotype_color_and_radius() {
        let config = SimConfig::load_default().unwrap();
        let genome = Genome::ancestor_from_config(&config).unwrap();
        let (_, phenotype) = try_prepare_spawn(genome, &config).unwrap();
        let params = build_spawn_params(
            Genome::ancestor_from_config(&config).unwrap(),
            phenotype.clone(),
            &config,
            1,
            None,
            0,
        );
        assert_eq!(params.body_radius, phenotype.body_radius);
        assert_eq!(params.life.agent.radius, phenotype.body_radius);
        let c = params.color.to_srgba();
        assert!((c.red - phenotype.color[0]).abs() < 1e-5);
    }

    #[test]
    fn stillborn_skips_id_assignment_in_prepare_path() {
        let config = SimConfig::load_default().unwrap();
        let mut bad = Genome::ancestor_from_config(&config).unwrap();
        bad.bytes[0] = b'X';
        assert!(try_prepare_spawn(bad, &config).is_none());
    }

    #[test]
    fn initial_population_prepare_succeeds_for_ancestor() {
        let config = SimConfig::load_default().unwrap();
        let ancestor = Genome::ancestor_from_config(&config).unwrap();
        for _ in 0..config.world.initial_population {
            assert!(try_prepare_spawn(ancestor.clone_genome(), &config).is_some());
        }
    }
}

//! Agent spawn pipeline ([ENTITIES.md](../../docs/ENTITIES.md#spawn), Phase 1E).

use bevy::prelude::*;
use rand::Rng;

use crate::config::{SimConfig, WorldConfig};
use super::agent_material::AgentColorMaterial;
use crate::genome::{
    decode, decode_phenotype, AnyGenome, AnyPhenotype,
    Genome as GenomeData, Phenotype as PhenotypeData,
};

use super::behavior::BehaviorState;
use super::components::{
    HazardExposure, Heading, LifeBundle, LightExposure, MineralExposure, VentExposure, Velocity,
};
use super::perception::SensePerception;
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
    pub heading: Heading,
    pub color: Color,
    pub body_radius: f32,
}

/// Validate genome and decode phenotype; returns `None` on stillborn.
pub fn try_prepare_spawn(
    genome: AnyGenome,
    config: &SimConfig,
) -> Option<(AnyGenome, AnyPhenotype)> {
    let phenotype = decode_phenotype(&genome, config).ok()?;
    Some((genome, phenotype))
}

/// Validate v1 genome and decode phenotype; returns `None` on stillborn.
pub fn try_prepare_spawn_v1(
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
    genome: AnyGenome,
    phenotype: AnyPhenotype,
    config: &SimConfig,
    id: u64,
    parent_id: Option<u64>,
    generation: u32,
) -> SpawnParams {
    let (initial_hp, _) = vitality_spawn_pools(&phenotype, config);
    build_offspring_spawn_params(
        genome,
        phenotype,
        config,
        id,
        parent_id,
        generation,
        config.economy.spawn_energy,
        initial_hp,
    )
}

/// Build spawn params for a child with explicit starting energy (offspring endowment).
pub fn build_offspring_spawn_params(
    genome: AnyGenome,
    phenotype: AnyPhenotype,
    config: &SimConfig,
    id: u64,
    parent_id: Option<u64>,
    generation: u32,
    initial_energy: f32,
    initial_core_hp: f32,
) -> SpawnParams {
    let color = phenotype_to_color(&phenotype);
    let body_radius = phenotype.body_radius();
    let (initial_hp, initial_max_hp) = match &phenotype {
        AnyPhenotype::V2(p) => (
            initial_core_hp.min(p.max_health.max(0.0)),
            p.max_health.max(0.0),
        ),
        AnyPhenotype::V1(_) => (initial_core_hp, initial_core_hp),
    };
    let life = LifeBundle::from_any_parts_with_vitals(
        genome,
        phenotype,
        config,
        id,
        parent_id,
        generation,
        initial_energy,
        initial_hp,
        initial_max_hp,
    );

    SpawnParams {
        life,
        position: Vec2::ZERO,
        velocity: Vec2::ZERO,
        heading: Heading(0.0),
        color,
        body_radius,
    }
}

/// Validate, decode, and build a full spawn package; `None` if stillborn.
pub fn try_build_spawn(
    genome: AnyGenome,
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

pub fn phenotype_to_color(phenotype: &AnyPhenotype) -> Color {
    let c = phenotype.color();
    Color::srgb(c[0], c[1], c[2])
}

fn vitality_spawn_pools(phenotype: &AnyPhenotype, config: &SimConfig) -> (f32, f32) {
    crate::sim::components::vitality_spawn_pools_for(phenotype, config)
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
        params.life.phenotype.max_speed(),
        world.min_speed,
        rng,
    );
    params.velocity = Vec2::from_angle(angle) * speed;
    params.heading = Heading::from_angle(angle);
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

    let speed = random_initial_speed(params.life.phenotype.max_speed(), 5.0, rng);
    params.velocity = Vec2::from_angle(angle) * speed;
    params.heading = Heading::from_angle(angle);
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
    let SpawnParams {
        life,
        position,
        velocity,
        heading,
        color,
        body_radius,
    } = params;
    let lineage_id = life.lineage.id;
    let armor_stroke = life.phenotype.v2().map(|p| p.armor_stroke);
    let mut entity_cmd = commands.spawn((
        life,
        heading,
        SensePerception::default(),
        BehaviorState::default(),
        Velocity(velocity),
        LightExposure(0.0),
        HazardExposure::default(),
        VentExposure::default(),
        MineralExposure::default(),
        Mesh2d(meshes.add(Circle::new(body_radius))),
        MeshMaterial2d(materials.add(AgentColorMaterial::new(color, lineage_id))),
        Transform::from_translation(position.extend(AGENT_MESH_Z)),
    ));

    if let Some(stroke) = armor_stroke {
        if stroke > 0.05 {
            let ring_radius = body_radius + stroke * 0.5;
            entity_cmd.with_children(|parent| {
                parent.spawn((
                    crate::visual::ArmorOutline,
                    Mesh2d(meshes.add(Circle::new(ring_radius))),
                    MeshMaterial2d(materials.add(AgentColorMaterial::new(
                        Color::srgba(0.15, 0.15, 0.18, 0.85),
                        lineage_id.wrapping_add(1),
                    ))),
                    Transform::from_xyz(0.0, 0.0, -0.05),
                ));
            });
        }
    }

    entity_cmd.id()
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
    ancestor: &AnyGenome,
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
    use crate::genome::{AnyGenome, Genome};

    fn v1(genome: Genome) -> AnyGenome {
        AnyGenome::V1(genome)
    }

    #[test]
    fn invalid_genome_is_stillborn() {
        let config = SimConfig::load_v1_regression().unwrap();
        let mut genome = Genome::ancestor_from_config(&config).unwrap();
        genome.bytes[0] = b'X';
        assert!(try_prepare_spawn(v1(genome), &config).is_none());
        assert!(try_build_spawn(
            v1(Genome::ancestor_from_config(&config).unwrap()),
            &config,
            1,
            None,
            0,
        )
        .is_some());

        let mut bad = Genome::ancestor_from_config(&config).unwrap();
        bad.bytes[0] = b'X';
        assert!(try_build_spawn(v1(bad), &config, 1, None, 0).is_none());
    }

    #[test]
    fn spawn_uses_phenotype_color_and_radius() {
        let config = SimConfig::load_v1_regression().unwrap();
        let genome = Genome::ancestor_from_config(&config).unwrap();
        let (_, phenotype) = try_prepare_spawn(v1(genome), &config).unwrap();
        let params = build_spawn_params(
            v1(Genome::ancestor_from_config(&config).unwrap()),
            phenotype.clone(),
            &config,
            1,
            None,
            0,
        );
        assert_eq!(params.body_radius, phenotype.body_radius());
        assert_eq!(params.life.agent.radius, phenotype.body_radius());
        let c = params.color.to_srgba();
        let color = phenotype.color();
        assert!((c.red - color[0]).abs() < 1e-5);
    }

    #[test]
    fn stillborn_skips_id_assignment_in_prepare_path() {
        let config = SimConfig::load_v1_regression().unwrap();
        let mut bad = Genome::ancestor_from_config(&config).unwrap();
        bad.bytes[0] = b'X';
        assert!(try_prepare_spawn(v1(bad), &config).is_none());
    }

    #[test]
    fn offspring_spawn_has_empty_part_hp() {
        let config = SimConfig::load_v1_regression().unwrap();
        let genome = Genome::ancestor_from_config(&config).unwrap();
        let (_, phenotype) = try_prepare_spawn(v1(genome.clone()), &config).unwrap();
        let params = build_offspring_spawn_params(
            v1(genome),
            phenotype,
            &config,
            2,
            Some(1),
            1,
            config.economy.spawn_energy,
            config.economy.spawn_energy,
        );
        assert!(params.life.health.part_hp.is_empty());
    }

    #[test]
    fn initial_population_prepare_succeeds_for_ancestor() {
        let config = SimConfig::load_v1_regression().unwrap();
        let ancestor = Genome::ancestor_from_config(&config).unwrap();
        for _ in 0..config.world.initial_population {
            assert!(try_prepare_spawn(v1(ancestor.clone_genome()), &config).is_some());
        }
    }
}

//! Agent spawn pipeline ([ENTITIES.md](../../docs/ENTITIES.md#spawn), Phase 1E).

use bevy::prelude::*;
use rand::Rng;

use crate::config::{SimConfig, WorldConfig};
use super::agent_material::AgentColorMaterial;
use crate::genome::{decode_phenotype, phenotype_color, AnyGenome, AnyPhenotype};

use super::behavior::BehaviorState;
use super::components::{
    Corpse, HazardExposure, Heading, LifeBundle, LightExposure, MineralExposure, VentExposure,
    Velocity,
};
use super::perception::SensePerception;
use super::structures::WorldStructures;
use super::world::{NextEntityId, SimRng, WorldBounds};

/// Agent mesh Z (draw order comes from [`AgentColorMaterial::depth_bias`](super::agent_material::AgentColorMaterial)).
const AGENT_MESH_Z: f32 = 0.0;

/// Minimum visible outline width in world units (always drawn, even at zero armor).
pub const MIN_AGENT_OUTLINE_WIDTH: f32 = 1.0;

/// Outline ring width: 1 world unit baseline; scales up to decoded [`PhenotypeV2::armor_stroke`].
pub fn agent_outline_width(armor_stroke: f32) -> f32 {
    armor_stroke.max(MIN_AGENT_OUTLINE_WIDTH)
}

/// Outer radius of the armor annulus (inner radius = body radius).
pub fn agent_outline_ring_radius(body_radius: f32, armor_stroke: f32) -> f32 {
    body_radius + agent_outline_width(armor_stroke)
}

/// Build a ring mesh for armor stroke (drawn on top of the body fill).
pub fn agent_armor_ring_mesh(meshes: &mut Assets<Mesh>, body_radius: f32, armor_stroke: f32) -> Handle<Mesh> {
    let outer = agent_outline_ring_radius(body_radius, armor_stroke);
    meshes.add(Annulus::new(body_radius, outer).mesh().resolution(32).build())
}

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
    mut genome: AnyGenome,
    config: &SimConfig,
) -> Option<(AnyGenome, AnyPhenotype)> {
    genome.sync_color_from_efficiency().ok()?;
    let phenotype = decode_phenotype(&genome, config).ok()?;
    Some((genome, phenotype))
}

/// Build spawn params for generation-0 population (uses `spawn_energy` + `spawn_health_frac`).
pub fn build_initial_spawn_params(
    genome: AnyGenome,
    phenotype: AnyPhenotype,
    config: &SimConfig,
    id: u64,
    generation: u32,
) -> SpawnParams {
    debug_assert!(generation == 0);
    let (initial_hp, _) = vitality_spawn_pools(&phenotype, config);
    build_spawn_params_with_vitals(
        genome,
        phenotype,
        config,
        id,
        None,
        generation,
        config.economy.spawn_energy,
        initial_hp,
    )
}

/// Build spawn params for initial population (alias for [`build_initial_spawn_params`]).
pub fn build_spawn_params(
    genome: AnyGenome,
    phenotype: AnyPhenotype,
    config: &SimConfig,
    id: u64,
    parent_id: Option<u64>,
    generation: u32,
) -> SpawnParams {
    let _ = parent_id;
    build_initial_spawn_params(genome, phenotype, config, id, generation)
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
    debug_assert!(
        parent_id.is_some(),
        "offspring spawn must have a parent; use build_initial_spawn_params for generation 0"
    );
    build_spawn_params_with_vitals(
        genome,
        phenotype,
        config,
        id,
        parent_id,
        generation,
        initial_energy,
        initial_core_hp,
    )
}

fn build_spawn_params_with_vitals(
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
    let body_radius = phenotype.body_radius;
    let initial_max_hp = phenotype.max_health.max(0.0);
    let initial_hp = initial_core_hp.min(initial_max_hp);
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
    let c = phenotype_color(phenotype);
    Color::srgb(c[0], c[1], c[2])
}

/// Desaturated fill for a dead agent (same hue, lower brightness and alpha).
pub fn corpse_color_from_phenotype(phenotype: &AnyPhenotype) -> Color {
    let c = phenotype_to_color(phenotype).to_srgba();
    Color::srgba(c.red * 0.45, c.green * 0.45, c.blue * 0.45, 0.65)
}

/// Insert one corpse marker (mesh + biomass pool for feeding).
pub fn spawn_corpse_entity(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<AgentColorMaterial>,
    position: Vec2,
    radius: f32,
    color: Color,
    depth_id: u64,
    corpse: Corpse,
) -> Entity {
    commands
        .spawn((
            corpse,
            Mesh2d(meshes.add(Circle::new(radius))),
            MeshMaterial2d(materials.add(AgentColorMaterial::new(color, depth_id))),
            Transform::from_translation(position.extend(AGENT_MESH_Z)),
        ))
        .id()
}

fn vitality_spawn_pools(phenotype: &AnyPhenotype, config: &SimConfig) -> (f32, f32) {
    crate::sim::components::vitality_spawn_pools_for(phenotype, config)
}

/// Random position inside world bounds, avoiding structures when possible (`stream_spawn` RNG).
pub fn randomize_position(
    params: &mut SpawnParams,
    rng: &mut impl Rng,
    bounds: &WorldBounds,
    structures: &WorldStructures,
) {
    let radius = params.body_radius;
    let (min_x, max_x) = axis_range(bounds.half_x, radius);
    let (min_y, max_y) = axis_range(bounds.half_y, radius);
    for _ in 0..48 {
        let pos = Vec2::new(rng.gen_range(min_x..max_x), rng.gen_range(min_y..max_y));
        if !structures.overlaps_circle(pos, radius) {
            params.position = pos;
            return;
        }
    }
    params.position = Vec2::new(rng.gen_range(min_x..max_x), rng.gen_range(min_y..max_y));
}

/// Random position and velocity inside world bounds (`stream_spawn` RNG).
pub fn randomize_placement(
    params: &mut SpawnParams,
    rng: &mut impl Rng,
    bounds: &WorldBounds,
    structures: &WorldStructures,
    world: &WorldConfig,
) {
    randomize_position(params, rng, bounds, structures);
    let angle = rng.gen_range(0.0..std::f32::consts::TAU);
    let speed = random_initial_speed(
        params.life.phenotype.max_speed(),
        world.min_speed,
        rng,
    );
    params.velocity = Vec2::from_angle(angle) * speed;
    params.heading = Heading::from_angle(angle);
}

/// Random placement for a child disc beside its parent.
pub fn placement_near_parent(
    parent_pos: Vec2,
    parent_radius: f32,
    child_radius: f32,
    child_max_speed: f32,
    min_speed: f32,
    rng: &mut impl Rng,
    bounds: &WorldBounds,
    structures: &WorldStructures,
) -> (Vec2, Vec2, Heading) {
    let angle = rng.gen_range(0.0..std::f32::consts::TAU);
    let speed = random_initial_speed(child_max_speed, min_speed, rng);
    let velocity = Vec2::from_angle(angle) * speed;
    let heading = Heading::from_angle(angle);

    if child_radius >= bounds.half_x || child_radius >= bounds.half_y {
        return (parent_pos, velocity, heading);
    }

    let gap = rng.gen_range(2.0..12.0);
    let standoff = parent_radius + child_radius + gap;
    let ideal = parent_pos + Vec2::from_angle(angle) * standoff;

    let (min_x, max_x) = axis_range(bounds.half_x, child_radius);
    let (min_y, max_y) = axis_range(bounds.half_y, child_radius);
    let mut pos = Vec2::new(ideal.x.clamp(min_x, max_x), ideal.y.clamp(min_y, max_y));
    if structures.overlaps_circle(pos, child_radius) {
        for _ in 0..24 {
            let try_angle = rng.gen_range(0.0..std::f32::consts::TAU);
            let try_ideal = parent_pos + Vec2::from_angle(try_angle) * standoff;
            let try_pos = Vec2::new(
                try_ideal.x.clamp(min_x, max_x),
                try_ideal.y.clamp(min_y, max_y),
            );
            if !structures.overlaps_circle(try_pos, child_radius) {
                pos = try_pos;
                break;
            }
        }
    }

    // When the child is larger than the world half-extent, axis clamping collapses to (0, 0)
    // and can teleport offspring to the map center — keep them beside the parent instead.
    let max_drift = standoff * 2.0;
    if pos.distance_squared(parent_pos) > max_drift * max_drift {
        pos = parent_pos;
        if child_radius < bounds.half_x {
            pos.x = pos.x.clamp(min_x, max_x);
        }
        if child_radius < bounds.half_y {
            pos.y = pos.y.clamp(min_y, max_y);
        }
    }

    (pos, velocity, heading)
}

/// Place child near parent (uses `stream_spawn` RNG semantics via caller-provided rng).
pub fn place_near_parent(
    params: &mut SpawnParams,
    parent_pos: Vec2,
    parent_radius: f32,
    rng: &mut impl Rng,
    bounds: &WorldBounds,
    structures: &WorldStructures,
) {
    let (pos, velocity, heading) = placement_near_parent(
        parent_pos,
        parent_radius,
        params.body_radius,
        params.life.phenotype.max_speed(),
        5.0,
        rng,
        bounds,
        structures,
    );
    params.position = pos;
    params.velocity = velocity;
    params.heading = heading;
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
    if half_extent <= 0.0 {
        return (0.0, 0.0);
    }
    let r = radius.min(half_extent);
    (-half_extent + r, half_extent - r)
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
    let armor_stroke = life.phenotype.0.armor_stroke;
    // Annulus ring sits on top of the fill; do not rely on depth_bias (broken in Bevy 0.15).
    let fill_sort = lineage_id.saturating_mul(2);
    let outline_sort = fill_sort.saturating_add(1);
    commands
        .spawn((
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
            MeshMaterial2d(materials.add(AgentColorMaterial::new(color, fill_sort))),
            Transform::from_translation(position.extend(AGENT_MESH_Z)),
        ))
        .with_children(|parent| {
            parent.spawn((
                crate::visual::AgentOutline,
                Mesh2d(agent_armor_ring_mesh(meshes, body_radius, armor_stroke)),
                MeshMaterial2d(materials.add(AgentColorMaterial::new(
                    Color::srgb(0.0, 0.0, 0.0),
                    outline_sort,
                ))),
                Transform::from_xyz(0.0, 0.0, 0.05),
            ));
        })
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
    ancestor: &AnyGenome,
    bounds: &WorldBounds,
    structures: &WorldStructures,
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
        let mut params = build_initial_spawn_params(genome, phenotype, config, id, 0);
        randomize_placement(&mut params, rng, bounds, structures, &config.world);
        debug_assert_eq!(params.life.agent.radius, params.body_radius);

        spawn_agent_entity(commands, meshes, materials, params);
        stats.spawned += 1;
    }

    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{ancestor_any_from_config, phenotype_color};

    #[test]
    fn invalid_genome_is_stillborn() {
        let config = SimConfig::load_default().unwrap();
        let mut bad = ancestor_any_from_config(&config).unwrap();
        bad.bytes[0] = b'X';
        assert!(try_prepare_spawn(bad, &config).is_none());
        assert!(try_build_spawn(
            ancestor_any_from_config(&config).unwrap(),
            &config,
            1,
            None,
            0,
        )
        .is_some());
    }

    #[test]
    fn outline_width_is_one_without_armor() {
        assert_eq!(agent_outline_width(0.0), MIN_AGENT_OUTLINE_WIDTH);
        assert_eq!(agent_outline_ring_radius(10.0, 0.0), 11.0);
    }

    #[test]
    fn outline_width_scales_with_armor_stroke() {
        assert_eq!(agent_outline_width(25.5), 25.5);
        assert_eq!(agent_outline_ring_radius(10.0, 25.5), 35.5);
    }

    #[test]
    fn armor_ring_mesh_is_created_for_nonzero_stroke() {
        let mut meshes = Assets::<Mesh>::default();
        let handle = agent_armor_ring_mesh(&mut meshes, 10.0, 25.5);
        assert!(meshes.get(&handle).is_some());
    }

    #[test]
    fn spawn_uses_phenotype_color_and_radius() {
        let config = SimConfig::load_default().unwrap();
        let genome = ancestor_any_from_config(&config).unwrap();
        let (_, phenotype) = try_prepare_spawn(genome.clone(), &config).unwrap();
        let params = build_spawn_params(genome, phenotype.clone(), &config, 1, None, 0);
        assert_eq!(params.body_radius, phenotype.body_radius);
        assert_eq!(params.life.agent.radius, phenotype.body_radius);
        let c = params.color.to_srgba();
        let color = phenotype_color(&phenotype);
        assert!((c.red - color[0]).abs() < 1e-5);
    }

    #[test]
    fn offspring_spawn_has_empty_part_hp() {
        let config = SimConfig::load_default().unwrap();
        let genome = ancestor_any_from_config(&config).unwrap();
        let (_, phenotype) = try_prepare_spawn(genome.clone(), &config).unwrap();
        let params = build_offspring_spawn_params(
            genome,
            phenotype,
            &config,
            2,
            Some(1),
            1,
            25.0,
            80.0,
        );
        assert!(params.life.health.part_hp.is_empty());
    }

    #[test]
    fn initial_population_prepare_succeeds_for_ancestor() {
        let config = SimConfig::load_default().unwrap();
        let ancestor = ancestor_any_from_config(&config).unwrap();
        for _ in 0..config.world.initial_population {
            assert!(try_prepare_spawn(ancestor.clone_genome(), &config).is_some());
        }
    }

    #[test]
    fn oversized_offspring_stays_near_parent_not_world_center() {
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        let config = SimConfig::load_default().unwrap();
        let ancestor = ancestor_any_from_config(&config).unwrap();
        let (_, phenotype) = try_prepare_spawn(ancestor.clone(), &config).unwrap();
        let bounds = WorldBounds::from_world_size(config.world.width, config.world.height);
        let parent_pos = Vec2::new(200.0, 300.0);
        let parent_radius = phenotype.body_radius;

        let mut params = build_offspring_spawn_params(
            ancestor,
            phenotype,
            &config,
            99,
            Some(1),
            1,
            10.0,
            10.0,
        );
        params.body_radius = bounds.half_y + 50.0;
        params.life.agent.radius = params.body_radius;

        let mut rng = StdRng::seed_from_u64(7);
        let structures = WorldStructures::from_config(&config.world.structures);
        place_near_parent(
            &mut params,
            parent_pos,
            parent_radius,
            &mut rng,
            &bounds,
            &structures,
        );

        assert!(
            params.position.distance(parent_pos) < 100.0,
            "offspring at {:?} should stay near parent {:?}, not teleport to center",
            params.position,
            parent_pos,
        );
        assert!(
            params.position.distance_squared(Vec2::ZERO) > 10_000.0,
            "offspring should not spawn at world origin",
        );
    }
}

use bevy::prelude::*;
use evolution_sim::config::{self, SimConfig};
use evolution_sim::genome::{decode, Genome};
use evolution_sim::sim::{
    advance_sim_tick, behavior_movement, death_and_despawn, metabolism_and_gains,
    performance_failsafe, reproduction, sense_exposure, spawn_initial_population, AgentColorMaterialPlugin,
    FailsafeWarnLatch, NextEntityId, LightField, SimConfigResource, SimRng, SimTick, SpawnBlocked,
    WorldBounds,
};
use evolution_sim::visual::VisualDebugPlugin;

/// Validated ancestor template for spawning (Phase 1E).
#[derive(Resource, Clone)]
struct AncestorGenome(Genome);

fn main() {
    let config_path = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(config::default_config_path);

    let sim_config = SimConfig::load_from_path(&config_path).unwrap_or_else(|err| {
        eprintln!("{err}");
        std::process::exit(1);
    });

    let ancestor = Genome::ancestor_from_config(&sim_config).unwrap_or_else(|err| {
        eprintln!("invalid ancestor genome in config: {err}");
        std::process::exit(1);
    });

    let ancestor_phenotype = decode(&ancestor, &sim_config).unwrap_or_else(|err| {
        eprintln!("failed to decode ancestor phenotype: {err}");
        std::process::exit(1);
    });

    if sim_config.is_seeded() {
        eprintln!(
            "loaded {} (seeded, master_seed={})",
            config_path.display(),
            sim_config.run.master_seed.unwrap()
        );
    } else {
        eprintln!("loaded {} (nondeterministic)", config_path.display());
    }
    eprintln!(
        "ancestor ok ({} bytes, v{}, radius {:.1}, photo {:.2})",
        ancestor.bytes.len(),
        ancestor.version,
        ancestor_phenotype.body_radius,
        ancestor_phenotype.photo_strength,
    );

    let (world_w, world_h) = (sim_config.world.width, sim_config.world.height);
    let world_bounds = WorldBounds::from_world_size(world_w, world_h);
    let light_field = LightField::from_environment(&sim_config.environment, &world_bounds);

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Evolution Sim".into(),
                resolution: (world_w, world_h).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(SimConfigResource(sim_config.clone()))
        .insert_resource(AncestorGenome(ancestor))
        .insert_resource(world_bounds)
        .insert_resource(light_field)
        .insert_resource(SimTick::default())
        .insert_resource(NextEntityId::new(1))
        .insert_resource(SimRng::from_config(&sim_config))
        .insert_resource(SpawnBlocked(false))
        .init_resource::<FailsafeWarnLatch>()
        .add_plugins((AgentColorMaterialPlugin, VisualDebugPlugin))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                advance_sim_tick,
                sense_exposure,
                metabolism_and_gains,
                behavior_movement,
                reproduction,
                death_and_despawn,
                performance_failsafe,
            )
                .chain(),
        )
        .run();
}

fn setup(
    mut commands: Commands,
    config: Res<SimConfigResource>,
    ancestor: Res<AncestorGenome>,
    mut next_id: ResMut<NextEntityId>,
    mut sim_rng: ResMut<SimRng>,
    bounds: Res<WorldBounds>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<evolution_sim::sim::AgentColorMaterial>>,
) {
    commands.spawn(Camera2d);

    let stats = spawn_initial_population(
        &mut commands,
        &mut meshes,
        &mut materials,
        &config.0,
        &ancestor.0,
        &bounds,
        &mut next_id,
        &mut sim_rng,
    );

    eprintln!(
        "spawned {} agents (stillborn {}); debug: L=light gradient, click=select agent, Esc=clear",
        stats.spawned, stats.stillborn
    );
}

#[cfg(test)]
mod tests {
    use bevy::app::App;
    use bevy::prelude::Transform;
        use evolution_sim::sim::{
            try_build_spawn, Agent, Cooldowns, Energy, Genome, Health, LightExposure, Lineage,
            Phenotype, Velocity,
        };

    #[test]
    fn spawned_agents_have_full_lifecycle_components() {
        let config = evolution_sim::config::SimConfig::load_default().unwrap();
        let genome = evolution_sim::genome::Genome::ancestor_from_config(&config).unwrap();
        let params = try_build_spawn(genome, &config, 1, None, 0).unwrap();

        let mut app = App::new();
        app.world_mut().spawn((
            params.life,
            Velocity(params.velocity),
            LightExposure(0.0),
            Transform::from_translation(params.position.extend(0.0)),
        ));

        let mut query = app.world_mut().query::<(
            &Genome,
            &Phenotype,
            &Energy,
            &Health,
            &Lineage,
            &Cooldowns,
            &Agent,
            &LightExposure,
        )>();
        let count = query.iter(app.world()).count();
        assert_eq!(count, 1);

        let (_, pheno, energy, health, lineage, cooldowns, agent, exposure) =
            query.single(app.world());
        assert_eq!(exposure.0, 0.0);
        assert_eq!(agent.radius, pheno.0.body_radius);
        assert_eq!(energy.internal_energy, config.economy.spawn_energy);
        assert_eq!(health.core_hp, pheno.0.core_max_hp);
        assert!(health.part_hp.is_empty());
        assert_eq!(lineage.generation, 0);
        assert_eq!(cooldowns.repro_ticks_remaining, 0);
    }
}

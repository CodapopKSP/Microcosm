//! Shared Bevy app bootstrap for visual and headless runs (Phase 3B).

use std::sync::{mpsc, Arc, Mutex};

use bevy::prelude::*;
use bevy::render::view::screenshot::CapturedScreenshots;
use bevy::window::ExitCondition;

use crate::config::SimConfig;
use crate::genome::Genome;
use crate::visual::VisualDebugPlugin;

use super::agent_material::AgentColorMaterialPlugin;
use super::hazards::HazardField;
use super::light::LightField;
use super::spawn::{spawn_initial_population, InitialPopulationStats};
use super::metrics::{MetricsPeriodCounters, MetricsWriter, RepairStats};
use super::systems::{
    advance_age, behavior_movement, death_and_despawn, exit_headless_when_done, healing,
    interactions, metabolism_and_gains, metrics_sample, performance_failsafe, reproduction,
    sense_exposure,
    FailsafeWarnLatch,
};
use super::world::{
    advance_sim_tick, NextEntityId, SimConfigResource, SimRng, SimTick, SpawnBlocked, WorldBounds,
};

/// Visual window or headless batch run ([PHASE_3.md](../../docs/PHASE_3.md) sub-phase 3B).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimRunMode {
    Visual,
    Headless,
}

/// Validated ancestor template for spawning (Phase 1E).
#[derive(Resource, Clone)]
pub struct AncestorGenome(pub Genome);

/// Build a Bevy app with shared sim resources, startup spawn, and tick systems.
pub fn build_sim_app(sim_config: SimConfig, ancestor: Genome, mode: SimRunMode) -> App {
    let mut app = App::new();

    match mode {
        SimRunMode::Visual => {
            let (world_w, world_h) = (sim_config.world.width, sim_config.world.height);
            app.add_plugins(DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Evolution Sim".into(),
                    resolution: (world_w, world_h).into(),
                    ..default()
                }),
                ..default()
            }))
            .add_plugins((AgentColorMaterialPlugin, VisualDebugPlugin))
            .add_systems(Startup, startup_visual);
        }
        SimRunMode::Headless => {
            app.add_plugins(DefaultPlugins.set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            }))
            .add_plugins(AgentColorMaterialPlugin)
            .add_systems(Startup, startup_headless)
            .add_systems(Update, exit_headless_when_done.after(performance_failsafe));
        }
    }

    let metrics_enabled = mode == SimRunMode::Headless;
    insert_sim_resources(&mut app, sim_config, ancestor, metrics_enabled);
    add_sim_systems(&mut app);

    app
}

/// Insert simulation resources (shared by visual and headless apps).
pub fn insert_sim_resources(app: &mut App, sim_config: SimConfig, ancestor: Genome, metrics_enabled: bool) {
    let metrics_path = sim_config.run.metrics_path.clone();
    let world_bounds = WorldBounds::from_world_size(sim_config.world.width, sim_config.world.height);
    let light_field = LightField::from_environment(&sim_config.environment, &world_bounds);
    let hazard_field = HazardField::from_environment(&sim_config.environment);
    let sim_rng = SimRng::from_config(&sim_config);

    app.insert_resource(SimConfigResource(sim_config))
        .insert_resource(AncestorGenome(ancestor))
        .insert_resource(world_bounds)
        .insert_resource(light_field)
        .insert_resource(hazard_field)
        .insert_resource(SimTick::default())
        .insert_resource(NextEntityId::new(1))
        .insert_resource(sim_rng)
        .insert_resource(SpawnBlocked(false))
        .init_resource::<FailsafeWarnLatch>()
        .init_resource::<MetricsPeriodCounters>()
        .init_resource::<RepairStats>()
        .insert_resource(MetricsWriter::new(metrics_path, metrics_enabled));

    if metrics_enabled {
        // Headless runs have no primary window; Bevy's screenshot plugin may not insert this resource.
        let (_, rx) = mpsc::channel();
        app.insert_resource(CapturedScreenshots(Arc::new(Mutex::new(rx))));
    }
}

/// Single registration site for the simulation tick chain.
pub fn add_sim_systems(app: &mut App) {
    app.add_systems(
        Update,
        (
            advance_sim_tick,
            sense_exposure,
            metabolism_and_gains,
            behavior_movement,
            interactions,
            healing,
            reproduction,
            advance_age,
            death_and_despawn,
            performance_failsafe,
            metrics_sample,
        )
            .chain(),
    );
}

fn startup_visual(
    mut commands: Commands,
    config: Res<SimConfigResource>,
    ancestor: Res<AncestorGenome>,
    mut next_id: ResMut<NextEntityId>,
    mut sim_rng: ResMut<SimRng>,
    bounds: Res<WorldBounds>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<super::AgentColorMaterial>>,
) {
    commands.spawn(Camera2d);
    log_spawn(startup_population(
        &mut commands,
        &config,
        &ancestor,
        &mut next_id,
        &mut sim_rng,
        &bounds,
        &mut meshes,
        &mut materials,
    ));
    eprintln!("debug: L=light gradient, H=hazard zones, click=select agent, Esc=clear");
}

fn startup_headless(
    mut commands: Commands,
    config: Res<SimConfigResource>,
    ancestor: Res<AncestorGenome>,
    mut next_id: ResMut<NextEntityId>,
    mut sim_rng: ResMut<SimRng>,
    bounds: Res<WorldBounds>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<super::AgentColorMaterial>>,
    mut metrics: ResMut<MetricsWriter>,
) {
    if metrics.enabled {
        if let Err(err) = metrics.open_truncate() {
            eprintln!(
                "metrics: failed to open {} for write: {err}",
                metrics.path
            );
            metrics.enabled = false;
        } else {
            eprintln!("metrics: writing to {}", metrics.path);
        }
    }

    log_spawn(startup_population(
        &mut commands,
        &config,
        &ancestor,
        &mut next_id,
        &mut sim_rng,
        &bounds,
        &mut meshes,
        &mut materials,
    ));
    eprintln!("headless sim");
}

fn startup_population(
    commands: &mut Commands,
    config: &SimConfigResource,
    ancestor: &AncestorGenome,
    next_id: &mut NextEntityId,
    sim_rng: &mut SimRng,
    bounds: &WorldBounds,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<super::AgentColorMaterial>,
) -> InitialPopulationStats {
    spawn_initial_population(
        commands,
        meshes,
        materials,
        &config.0,
        &ancestor.0,
        bounds,
        next_id,
        sim_rng,
    )
}

fn log_spawn(stats: InitialPopulationStats) {
    eprintln!(
        "spawned {} agents (stillborn {})",
        stats.spawned, stats.stillborn
    );
}

/// Run a headless Bevy sim until `exit_headless_when_done` fires (Phase 3E acceptance).
pub fn run_headless_bevy(config: SimConfig, ancestor: Genome) {
    build_sim_app(config, ancestor, SimRunMode::Headless).run();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_sim_resources_registers_core_state() {
        let config = SimConfig::load_default().unwrap();
        let ancestor = Genome::ancestor_from_config(&config).unwrap();
        let mut app = App::new();
        insert_sim_resources(&mut app, config, ancestor, false);

        assert!(app.world().contains_resource::<SimConfigResource>());
        assert!(app.world().contains_resource::<AncestorGenome>());
        assert!(app.world().contains_resource::<SimTick>());
        assert!(app.world().contains_resource::<WorldBounds>());
        assert!(app.world().contains_resource::<LightField>());
        assert!(app.world().contains_resource::<HazardField>());
    }
}

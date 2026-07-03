//! Environment field tick: soup regen, corpse decay, and ephemeral hazards (G7 / Phase 6 minimal).

use bevy::prelude::*;

use super::failsafe::{corpse_cull_count, cull_lowest_energy_entities};
use super::super::components::Corpse;
use super::super::hazards::HazardField;
use super::super::interval::{logic_active_this_tick, logic_elapsed_ticks};
use super::super::minerals::MineralField;
use super::super::soup::SoupField;
use super::super::world::{SimConfigResource, SimRng, SimTick, WorldBounds};

/// Advance global soup, expire corpse markers, and tick ephemeral hazard patches.
///
/// Corpses despawn when depleted (`remaining_energy == 0`) or when TTL elapses.
pub fn environment_tick(
    tick: Res<SimTick>,
    config: Res<SimConfigResource>,
    bounds: Res<WorldBounds>,
    sim_rng: Res<SimRng>,
    mut soup: ResMut<SoupField>,
    mut hazards: ResMut<HazardField>,
    mut minerals: ResMut<MineralField>,
    mut commands: Commands,
    mut corpses: Query<(Entity, &mut Corpse)>,
) {
    let dynamic = &config.0.environment.dynamic_hazards;
    if dynamic.enabled {
        let mut rng = sim_rng.stream_hazards(tick.tick);
        hazards.tick_dynamic(dynamic, *bounds, tick.tick, &mut rng);
        minerals.sync_hazard_zones(&hazards);
    }

    let run = &config.0.run;
    if !logic_active_this_tick(tick.tick, run) {
        return;
    }
    let steps = logic_elapsed_ticks(run);

    for _ in 0..steps {
        soup.tick_regen();
    }

    let ttl_default = config.0.environment.feeding.corpse_ttl_ticks;
    for (entity, mut corpse) in &mut corpses {
        if corpse.remaining_energy <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        if ttl_default != 0 {
            if corpse.ticks_remaining > 0 {
                corpse.ticks_remaining = corpse.ticks_remaining.saturating_sub(steps);
            }
            if corpse.ticks_remaining == 0 {
                commands.entity(entity).despawn();
            }
        }
    }

    let performance = &config.0.performance;
    let population = corpses.iter().count();
    let cull = corpse_cull_count(
        population,
        performance.corpse_max,
        performance.corpse_cull_count,
    );
    if cull > 0 {
        let mut ranked: Vec<(Entity, f32)> = corpses
            .iter()
            .map(|(entity, corpse)| (entity, corpse.remaining_energy))
            .collect();
        cull_lowest_energy_entities(&mut ranked, cull);
        for (entity, _) in ranked {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::sim::components::Corpse;
    use crate::sim::hazards::HazardField;
    use crate::sim::minerals::MineralField;
    use crate::sim::soup::SoupField;
    use crate::sim::vents::VentField;
    use crate::sim::world::{SimConfigResource, SimRng, SimTick, WorldBounds};

    fn env_tick_app(config: SimConfig) -> App {
        let feeding = config.environment.feeding.clone();
        let env = config.environment.clone();
        let bounds = WorldBounds::from_world_size(config.world.width, config.world.height);
        let hazard_field = HazardField::from_environment(&env);
        let vent_field = VentField::from_environment(&env);
        let mineral_field = MineralField::from_vent_field(
            &vent_field,
            &hazard_field,
            &feeding,
            env.minerals_enabled,
        );

        let mut app = App::new();
        app.insert_resource(SimConfigResource(config));
        app.insert_resource(SimTick::default());
        app.insert_resource(SoupField::from_feeding(&feeding));
        app.insert_resource(hazard_field);
        app.insert_resource(mineral_field);
        app.insert_resource(bounds);
        app.insert_resource(SimRng::from_config(
            &app.world().resource::<SimConfigResource>().0,
        ));
        app.add_systems(Update, environment_tick);
        app
    }

    #[test]
    fn depleted_corpse_despawns_immediately() {
        let mut config = SimConfig::load_default().unwrap();
        config.run.logic_interval_ticks = 1;
        config.environment.dynamic_hazards.enabled = false;
        let mut app = env_tick_app(config);

        let corpse = app
            .world_mut()
            .spawn(Corpse {
                remaining_energy: 0.0,
                radius: 10.0,
                ticks_remaining: 3600,
            })
            .id();

        app.update();
        assert!(app.world().get_entity(corpse).is_err());
    }

    #[test]
    fn corpse_cap_culls_lowest_biomass_first() {
        let mut config = SimConfig::load_default().unwrap();
        config.run.logic_interval_ticks = 1;
        config.environment.dynamic_hazards.enabled = false;
        config.environment.feeding.corpse_ttl_ticks = u32::MAX;
        config.performance.corpse_max = 10;
        config.performance.corpse_cull_count = 5;

        let mut app = env_tick_app(config);
        for i in 0..12 {
            app.world_mut().spawn(Corpse {
                remaining_energy: i as f32,
                radius: 10.0,
                ticks_remaining: u32::MAX,
            });
        }

        app.update();

        let mut query = app.world_mut().query::<&Corpse>();
        let remaining: Vec<f32> = query
            .iter(app.world())
            .map(|c| c.remaining_energy)
            .collect();
        assert_eq!(remaining.len(), 7);
        assert!(!remaining.contains(&0.0));
        assert!(!remaining.contains(&4.0));
        assert!(remaining.contains(&5.0));
        assert!(remaining.contains(&11.0));
    }
}

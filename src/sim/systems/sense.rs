//! Sense / exposure: sample environmental fields at entity positions (Phase 2C, 4A, 5A).

use bevy::prelude::*;

use super::super::components::{Agent, HazardExposure, LightExposure, MineralExposure, VentExposure};
use super::super::hazards::HazardField;
use super::super::light::LightField;
use super::super::minerals::MineralField;
use super::super::vents::VentField;
use super::super::world::SimConfigResource;

/// Sample light, hazard, vent, and mineral fields at each agent's position for this tick.
pub fn sense_exposure(
    config: Res<SimConfigResource>,
    light: Res<LightField>,
    hazards: Res<HazardField>,
    vents: Res<VentField>,
    minerals: Res<MineralField>,
    mut query: Query<(
        &Transform,
        &mut LightExposure,
        &mut HazardExposure,
        &mut VentExposure,
        &mut MineralExposure,
    ), With<Agent>>,
) {
    let _ = (&config, &vents);
    for (transform, mut light_exp, mut hazard_exp, mut vent_exp, mut mineral_exp) in &mut query {
        let pos = transform.translation.truncate();
        light_exp.0 = light.sample(pos);
        hazard_exp.damage_raw = hazards.damage_at(pos);
        vent_exp.vent_flux = 0.0;
        mineral_exp.concentration = minerals.concentration_at(pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HazardZone, SimConfig};
    use crate::config::VentZone;
    use crate::sim::components::{HazardExposure, LifeBundle, MineralExposure, VentExposure, Velocity};
    use crate::sim::hazards::HazardField;
    use crate::sim::minerals::MineralField;
    use crate::sim::vents::VentField;
    use crate::sim::world::{SimConfigResource, WorldBounds};

    fn insert_exposure_resources(app: &mut App, config: &SimConfig) {
        let bounds = WorldBounds::from_world_size(config.world.width, config.world.height);
        let vents = VentField::from_environment(&config.environment);
        app.insert_resource(SimConfigResource(config.clone()));
        app.insert_resource(LightField::from_environment(&config.environment, &bounds));
        app.insert_resource(HazardField::from_environment(&config.environment));
        app.insert_resource(vents.clone());
        app.insert_resource(MineralField::from_vent_field(
            &vents,
            &config.environment.feeding,
            config.environment.minerals_enabled,
        ));
    }

    fn test_app_with_agent_at(position: Vec2) -> App {
        let config = SimConfig::load_default().unwrap();
        let genome = crate::genome::ancestor_any_from_config(&config).unwrap();
        let phenotype = crate::genome::decode_phenotype(&genome, &config).unwrap();
        let life = LifeBundle::from_any_parts(genome, phenotype, &config, 1, None, 0);

        let mut app = App::new();
        insert_exposure_resources(&mut app, &config);
        app.world_mut().spawn((
            life,
            Velocity(Vec2::ZERO),
            LightExposure(0.0),
            HazardExposure::default(),
            VentExposure::default(),
            MineralExposure::default(),
            Transform::from_translation(position.extend(0.0)),
        ));
        app.add_systems(Update, sense_exposure);
        app
    }

    #[test]
    fn samples_light_at_agent_position() {
        let config = SimConfig::load_default().unwrap();
        let bounds = WorldBounds::from_world_size(config.world.width, config.world.height);
        let light = LightField::from_environment(&config.environment, &bounds);
        let top = Vec2::new(0.0, bounds.half_y);

        let mut app = test_app_with_agent_at(top);
        app.update();

        let world = app.world_mut();
        let exposure = world.query::<&LightExposure>().single(world).0;
        assert_eq!(exposure, light.sample(top));
    }

    #[test]
    fn stationary_agent_has_stable_light_across_ticks() {
        let position = Vec2::new(100.0, -200.0);
        let mut app = test_app_with_agent_at(position);

        app.update();
        let first = {
            let world = app.world_mut();
            world.query::<&LightExposure>().single(world).0
        };
        app.update();
        let second = {
            let world = app.world_mut();
            world.query::<&LightExposure>().single(world).0
        };

        assert_eq!(first, second);
        assert!(first > 0.0);
    }

    #[test]
    fn hazard_damage_inside_zone() {
        let config = SimConfig::load_default().unwrap();
        let bounds = WorldBounds::from_world_size(config.world.width, config.world.height);
        let mut env = config.environment.clone();
        env.hazards = vec![HazardZone {
            center: [0.0, -400.0],
            radius: 180.0,
            damage_per_tick: 2.5,
        }];
        let mut cfg = config.clone();
        cfg.environment = env;

        let mut app = App::new();
        insert_exposure_resources(&mut app, &cfg);
        let genome = crate::genome::ancestor_any_from_config(&config).unwrap();
        let phenotype = crate::genome::decode_phenotype(&genome, &config).unwrap();
        let life = LifeBundle::from_any_parts(genome, phenotype, &config, 1, None, 0);
        app.world_mut().spawn((
            life,
            Velocity(Vec2::ZERO),
            LightExposure(0.0),
            HazardExposure::default(),
            VentExposure::default(),
            MineralExposure::default(),
            Transform::from_translation(Vec3::new(0.0, -400.0, 0.0)),
        ));
        app.add_systems(Update, sense_exposure);
        app.update();

        let world = app.world_mut();
        let exposure = world.query::<&HazardExposure>().single(world);
        assert!(exposure.damage_raw > 0.0);
        assert_eq!(exposure.damage_raw, 2.5);
    }

    #[test]
    fn hazard_damage_outside_zone_is_zero() {
        let config = SimConfig::load_default().unwrap();
        let mut env = config.environment.clone();
        env.hazards = vec![HazardZone {
            center: [0.0, -400.0],
            radius: 180.0,
            damage_per_tick: 2.5,
        }];
        let mut cfg = config.clone();
        cfg.environment = env;

        let mut app = App::new();
        insert_exposure_resources(&mut app, &cfg);
        let genome = crate::genome::ancestor_any_from_config(&config).unwrap();
        let phenotype = crate::genome::decode_phenotype(&genome, &config).unwrap();
        let life = LifeBundle::from_any_parts(genome, phenotype, &config, 1, None, 0);
        app.world_mut().spawn((
            life,
            Velocity(Vec2::ZERO),
            LightExposure(0.0),
            HazardExposure::default(),
            VentExposure::default(),
            MineralExposure::default(),
            Transform::from_translation(Vec3::new(400.0, 400.0, 0.0)),
        ));
        app.add_systems(Update, sense_exposure);
        app.update();

        let world = app.world_mut();
        let exposure = world.query::<&HazardExposure>().single(world);
        assert_eq!(exposure.damage_raw, 0.0);
    }

    #[test]
    fn vent_flux_not_used_in_v2_exposure() {
        let config = SimConfig::load_default().unwrap();
        let mut app = App::new();
        insert_exposure_resources(&mut app, &config);
        let genome = crate::genome::ancestor_any_from_config(&config).unwrap();
        let phenotype = crate::genome::decode_phenotype(&genome, &config).unwrap();
        let life = LifeBundle::from_any_parts(genome, phenotype, &config, 1, None, 0);
        app.world_mut().spawn((
            life,
            Velocity(Vec2::ZERO),
            LightExposure(0.0),
            HazardExposure::default(),
            VentExposure::default(),
            MineralExposure::default(),
            Transform::from_translation(Vec3::new(0.0, -450.0, 0.0)),
        ));
        app.add_systems(Update, sense_exposure);
        app.update();

        let world = app.world_mut();
        let exposure = world.query::<&VentExposure>().single(world);
        assert_eq!(exposure.vent_flux, 0.0);
    }

    #[test]
    fn vent_flux_outside_zone_is_zero() {
        let config = SimConfig::load_default().unwrap();
        let mut env = config.environment.clone();
        env.vents = vec![VentZone {
            center: [0.0, -450.0],
            radius: 150.0,
            vent_flux: 0.032,
        }];
        let mut cfg = config.clone();
        cfg.environment = env;

        let mut app = App::new();
        insert_exposure_resources(&mut app, &cfg);
        let genome = crate::genome::ancestor_any_from_config(&config).unwrap();
        let phenotype = crate::genome::decode_phenotype(&genome, &config).unwrap();
        let life = LifeBundle::from_any_parts(genome, phenotype, &config, 1, None, 0);
        app.world_mut().spawn((
            life,
            Velocity(Vec2::ZERO),
            LightExposure(0.0),
            HazardExposure::default(),
            VentExposure::default(),
            MineralExposure::default(),
            Transform::from_translation(Vec3::new(400.0, 400.0, 0.0)),
        ));
        app.add_systems(Update, sense_exposure);
        app.update();

        let world = app.world_mut();
        let exposure = world.query::<&VentExposure>().single(world);
        assert_eq!(exposure.vent_flux, 0.0);
    }

    #[test]
    fn every_agent_receives_exposure_before_metabolism_order() {
        let config = SimConfig::load_default().unwrap();
        let bounds = WorldBounds::from_world_size(config.world.width, config.world.height);
        let light = LightField::from_environment(&config.environment, &bounds);
        let genome = crate::genome::ancestor_any_from_config(&config).unwrap();
        let phenotype = crate::genome::decode_phenotype(&genome, &config).unwrap();
        let expected_top = light.sample(Vec2::new(0.0, 100.0));
        let expected_bottom = light.sample(Vec2::new(0.0, -100.0));

        let mut app = App::new();
        insert_exposure_resources(&mut app, &config);

        for (i, y) in [0.0, 100.0, -100.0].into_iter().enumerate() {
            let life = LifeBundle::from_any_parts(
                genome.clone_genome(),
                phenotype.clone(),
                &config,
                i as u64 + 1,
                None,
                0,
            );
            app.world_mut().spawn((
                life,
                Velocity(Vec2::ZERO),
                LightExposure(0.0),
                HazardExposure::default(),
                VentExposure::default(),
                MineralExposure::default(),
                Transform::from_translation(Vec2::new(0.0, y).extend(0.0)),
            ));
        }

        app.add_systems(Update, (sense_exposure,).chain());
        app.update();

        let world = app.world_mut();
        let samples: Vec<f32> = world
            .query::<(&Transform, &LightExposure)>()
            .iter(world)
            .map(|(_, e)| e.0)
            .collect();
        assert_eq!(samples.len(), 3);
        assert!(samples[1] > samples[2], "higher y should be brighter by default");
        assert_eq!(samples[1], expected_top);
        assert_eq!(samples[2], expected_bottom);
    }
}

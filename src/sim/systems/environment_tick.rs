//! Environment field tick: soup regen and corpse decay (G7 / Phase 6 minimal).

use bevy::prelude::*;

use super::super::components::Corpse;
use super::super::interval::{logic_active_this_tick, logic_elapsed_ticks};
use super::super::soup::SoupField;
use super::super::world::{SimConfigResource, SimTick};

/// Advance global soup and expire corpse markers.
///
/// Corpses despawn only when TTL elapses. Depleted energy (`remaining_energy == 0`)
/// stops feeding but leaves the visual marker until expiry.
pub fn environment_tick(
    tick: Res<SimTick>,
    config: Res<SimConfigResource>,
    mut soup: ResMut<SoupField>,
    mut commands: Commands,
    mut corpses: Query<(Entity, &mut Corpse)>,
) {
    let run = &config.0.run;
    if !logic_active_this_tick(tick.tick, run) {
        return;
    }
    let steps = logic_elapsed_ticks(run);

    for _ in 0..steps {
        soup.tick_regen();
    }

    let ttl_default = config.0.environment.feeding.corpse_ttl_ticks;
    if ttl_default == 0 {
        return;
    }

    for (entity, mut corpse) in &mut corpses {
        if corpse.ticks_remaining > 0 {
            corpse.ticks_remaining = corpse.ticks_remaining.saturating_sub(steps);
        }
        if corpse.ticks_remaining == 0 {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::sim::components::Corpse;
    use crate::sim::soup::SoupField;
    use crate::sim::world::{SimConfigResource, SimTick};

    #[test]
    fn zero_biomass_corpse_survives_until_ttl() {
        let mut config = SimConfig::load_default().unwrap();
        config.run.logic_interval_ticks = 1;
        let feeding = config.environment.feeding.clone();
        let mut app = App::new();
        app.insert_resource(SimConfigResource(config));
        app.insert_resource(SimTick::default());
        app.insert_resource(SoupField::from_feeding(&feeding));
        app.add_systems(Update, environment_tick);

        let corpse = app
            .world_mut()
            .spawn(Corpse {
                remaining_energy: 0.0,
                radius: 10.0,
                ticks_remaining: 2,
            })
            .id();

        app.update();
        assert!(app.world().get_entity(corpse).is_ok());
        assert_eq!(
            app.world()
                .entity(corpse)
                .get::<Corpse>()
                .unwrap()
                .ticks_remaining,
            1
        );

        app.update();
        assert!(app.world().get_entity(corpse).is_err());
    }
}

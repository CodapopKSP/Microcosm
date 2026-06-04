//! Environment field tick: soup regen and corpse decay (G7 / Phase 6 minimal).

use bevy::prelude::*;

use super::super::components::Corpse;
use super::super::soup::SoupField;
use super::super::world::SimConfigResource;

/// Advance global soup and expire corpse markers.
pub fn environment_tick(
    config: Res<SimConfigResource>,
    mut soup: ResMut<SoupField>,
    mut commands: Commands,
    mut corpses: Query<(Entity, &mut Corpse)>,
) {
    soup.tick_regen();

    let ttl_default = config.0.environment.feeding.corpse_ttl_ticks;
    for (entity, mut corpse) in &mut corpses {
        if ttl_default > 0 {
            if corpse.ticks_remaining > 0 {
                corpse.ticks_remaining -= 1;
            }
            if corpse.ticks_remaining == 0 {
                commands.entity(entity).despawn();
                continue;
            }
        }
        if corpse.remaining_biomass <= 1e-6 {
            commands.entity(entity).despawn();
        }
    }
}

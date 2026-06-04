//! Pursue / avoid / wander behavior FSM for genome v2 (G5).

use bevy::prelude::*;

use super::super::behavior::{update_behavior_intent, BehaviorState};
use super::super::components::{Energy, Phenotype};
use super::super::perception::SensePerception;
use super::super::world::SimConfigResource;

/// Bevy system: set locomotion intent after [`super::sense_rays`](super::sense_rays).
pub fn behavior_v2(
    config: Res<SimConfigResource>,
    mut query: Query<(
        &Phenotype,
        &SensePerception,
        &Energy,
        &mut BehaviorState,
    )>,
) {
    if config.0.genetics.genome_version != 2 {
        return;
    }
    let behavior_cfg = match config.0.genetics.behavior.as_ref() {
        Some(b) => b,
        None => return,
    };
    let flee_offset = behavior_cfg.avoid_flee_heading_offset_deg;

    for (phenotype, perception, energy, mut state) in &mut query {
        let Some(phenotype_v2) = phenotype.v2() else {
            continue;
        };
        update_behavior_intent(
            &mut state,
            perception,
            phenotype_v2,
            energy,
            flee_offset,
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::genome::{decode_v2, v2_core, GenomeV2, PhenotypeV2, V2_HEADER_LEN};
    use crate::sim::behavior::{BehaviorMode, BehaviorState, flee_heading, update_behavior_intent};
    use crate::sim::components::Energy;
    use crate::sim::perception::SensePerception;

    fn mobile_phenotype() -> (crate::config::SimConfig, PhenotypeV2) {
        let config = crate::config::SimConfig::load_from_path(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs/genome_v2.example.toml"),
        )
        .unwrap();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        bytes[V2_HEADER_LEN + v2_core::MAX_SPEED] = 0xFF;
        bytes[V2_HEADER_LEN + v2_core::MOVEMENT_DESIRE] = 0xFF;
        let genome = GenomeV2::from_bytes(bytes, &config.genetics).unwrap();
        let p = decode_v2(&genome, &config).unwrap();
        (config, p)
    }

    #[test]
    fn pursue_sets_steer_toward_bearing() {
        let (_, phenotype) = mobile_phenotype();
        let mut state = BehaviorState::default();
        let bearing = std::f32::consts::FRAC_PI_2;
        let perception = SensePerception {
            pursue_matched: true,
            pursue_bearing: Some(bearing),
            ..Default::default()
        };
        let energy = Energy::new_v2(100.0, phenotype.max_energy);
        update_behavior_intent(&mut state, &perception, &phenotype, &energy, 180.0);
        assert_eq!(state.mode, BehaviorMode::Pursue);
        assert_eq!(state.steer_heading, Some(bearing));
        assert!(!state.force_max_speed);
    }

    #[test]
    fn avoid_flees_opposite_stimulus() {
        let (_, phenotype) = mobile_phenotype();
        let mut state = BehaviorState::default();
        let stimulus = 0.0;
        let perception = SensePerception {
            avoid_matched: true,
            avoid_bearing: Some(stimulus),
            ..Default::default()
        };
        let energy = Energy::new_v2(100.0, phenotype.max_energy);
        update_behavior_intent(&mut state, &perception, &phenotype, &energy, 180.0);
        assert_eq!(state.mode, BehaviorMode::Avoid);
        assert_eq!(state.steer_heading, Some(flee_heading(stimulus, 180.0)));
        assert!(state.force_max_speed);
        assert_eq!(state.avoid_distance_remaining, phenotype.sensor_reach_per_eye);
    }
}

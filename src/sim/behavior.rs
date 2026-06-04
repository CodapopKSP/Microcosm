//! Pursue / avoid / wander intent for genome v2 ([Genome_Definition.md](../../docs/Genome_Definition.md)).

use bevy::prelude::*;

use crate::genome::PhenotypeV2;
use crate::sim::components::Energy;
use crate::sim::perception::SensePerception;

use crate::sim::systems::MOVE_SPEED_EPS;

/// High-level locomotion intent (avoid > pursue > wander).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BehaviorMode {
    #[default]
    Wander,
    Pursue,
    Avoid,
}

/// Written by [`super::systems::behavior_v2`](crate::sim::systems::behavior_v2); consumed by movement.
#[derive(Component, Clone, Default, PartialEq)]
pub struct BehaviorState {
    pub mode: BehaviorMode,
    /// World bearing (radians) to steer toward (pursue target or flee direction).
    pub steer_heading: Option<f32>,
    /// Remaining flee distance before returning to wander (avoid budget).
    pub avoid_distance_remaining: f32,
    /// When true, movement always attempts max-speed travel (avoid flee).
    pub force_max_speed: bool,
}

impl BehaviorState {
    pub fn reset_wander(&mut self) {
        self.mode = BehaviorMode::Wander;
        self.steer_heading = None;
        self.force_max_speed = false;
    }
}

/// Shortest signed angle from `from` to `to` (radians).
pub fn shortest_angle_delta(from: f32, to: f32) -> f32 {
    let diff = (to - from).rem_euclid(std::f32::consts::TAU);
    if diff > std::f32::consts::PI {
        diff - std::f32::consts::TAU
    } else {
        diff
    }
}

/// Rotate `current` toward `target` by at most `max_step` radians.
pub fn turn_toward(current: f32, target: f32, max_step: f32) -> f32 {
    let step = max_step.max(0.0);
    if step <= 0.0 {
        return current;
    }
    let delta = shortest_angle_delta(current, target);
    (current + delta.clamp(-step, step)).rem_euclid(std::f32::consts::TAU)
}

/// Flee heading: stimulus bearing plus configured offset (default 180°).
pub fn flee_heading(stimulus_bearing: f32, offset_deg: f32) -> f32 {
    (stimulus_bearing + offset_deg.to_radians()).rem_euclid(std::f32::consts::TAU)
}

/// Update behavior intent from perception (avoid > pursue > wander).
pub fn update_behavior_intent(
    state: &mut BehaviorState,
    perception: &SensePerception,
    phenotype: &PhenotypeV2,
    energy: &Energy,
    flee_offset_deg: f32,
) {
    let can_move = phenotype.max_speed_effective > MOVE_SPEED_EPS && energy.internal_energy > 0.0;

    if state.mode == BehaviorMode::Avoid
        && state.avoid_distance_remaining > 0.0
        && can_move
    {
        if perception.avoid_matched {
            if let Some(bearing) = perception.avoid_bearing {
                state.steer_heading = Some(flee_heading(bearing, flee_offset_deg));
                state.avoid_distance_remaining = phenotype.sensor_reach_per_eye;
            }
        }
        state.force_max_speed = true;
        return;
    }

    if perception.avoid_matched && can_move {
        if let Some(bearing) = perception.avoid_bearing {
            state.mode = BehaviorMode::Avoid;
            state.steer_heading = Some(flee_heading(bearing, flee_offset_deg));
            state.avoid_distance_remaining = phenotype.sensor_reach_per_eye;
            state.force_max_speed = true;
            return;
        }
    }

    if perception.pursue_matched && can_move {
        if let Some(bearing) = perception.pursue_bearing {
            state.mode = BehaviorMode::Pursue;
            state.steer_heading = Some(bearing);
            state.force_max_speed = false;
            state.avoid_distance_remaining = 0.0;
            return;
        }
    }

    state.reset_wander();
    state.avoid_distance_remaining = 0.0;
}

/// Decrement avoid flee budget after movement.
pub fn apply_avoid_distance_traveled(state: &mut BehaviorState, distance: f32) {
    if state.mode != BehaviorMode::Avoid || distance <= 0.0 {
        return;
    }
    state.avoid_distance_remaining = (state.avoid_distance_remaining - distance).max(0.0);
    if state.avoid_distance_remaining <= 0.0 {
        state.reset_wander();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flee_heading_opposes_stimulus_by_180_deg() {
        let stimulus = 0.0;
        let flee = flee_heading(stimulus, 180.0);
        assert!((flee - std::f32::consts::PI).abs() < 1e-4);
    }

    #[test]
    fn turn_toward_clamps_step() {
        let current = 0.0;
        let target = std::f32::consts::FRAC_PI_2;
        let turned = turn_toward(current, target, 0.1);
        assert!((turned - 0.1).abs() < 1e-5);
        assert!(turn_toward(current, target, 10.0).abs() - target.abs() < 1e-4);
    }

    #[test]
    fn avoid_priority_over_pursue() {
        let mut state = BehaviorState::default();
        let perception = SensePerception {
            pursue_matched: true,
            avoid_matched: true,
            pursue_bearing: Some(0.0),
            avoid_bearing: Some(0.0),
            ..Default::default()
        };
        let config = crate::config::SimConfig::load_from_path(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs/genome_v2.example.toml"),
        )
        .unwrap();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        bytes[crate::genome::V2_HEADER_LEN + crate::genome::v2_core::MAX_SPEED] = 0xFF;
        let genome = crate::genome::GenomeV2::from_bytes(bytes, &config.genetics).unwrap();
        let phenotype = crate::genome::decode_v2(&genome, &config).unwrap();
        let energy = Energy::new_v2(100.0, phenotype.max_energy);
        update_behavior_intent(&mut state, &perception, &phenotype, &energy, 180.0);
        assert_eq!(state.mode, BehaviorMode::Avoid);
        assert!(state.force_max_speed);
    }
}

//! Genome v2 movement: heading, per-second desire/turn, armor speed cap, efficiency cost ([Genome_Definition.md](../../../docs/Genome_Definition.md)).

use bevy::prelude::*;
use rand::Rng;

use crate::config::{BoundaryMode, EconomyConfig};
use crate::genome::{wants_to_move_this_tick, PhenotypeV2};

use super::movement::{apply_world_bounds, clamp_velocity_to_max_speed, MOVE_SPEED_EPS};
use super::super::behavior::{
    apply_avoid_distance_traveled, turn_toward, BehaviorMode, BehaviorState,
};
use super::super::components::{Agent, Energy, Heading, Lineage, Phenotype, Velocity};
use super::super::world::{SimConfigResource, SimRng, SimTick, WorldBounds};

/// Per-second probability → per-`dt` attempt chance.
pub fn per_tick_chance(per_sec: f32, dt: f32) -> f32 {
    let p = per_sec.clamp(0.0, 1.0);
    if p <= 0.0 || dt <= 0.0 {
        return 0.0;
    }
    if p >= 1.0 {
        return 1.0;
    }
    1.0 - (1.0 - p).powf(dt)
}

/// Distance-indexed movement energy for v2 ([CONFIG.md](../../../docs/CONFIG.md) `move_energy_units_per_radius`).
pub fn movement_energy_for_distance_v2(
    distance: f32,
    body_radius: f32,
    units_per_radius: f32,
    cost_multiplier: f32,
) -> f32 {
    let r = body_radius.max(1e-6);
    let units = units_per_radius.max(1e-6);
    distance.max(0.0) / (units * r) * cost_multiplier.max(0.0)
}

/// Apply a random turn to heading from decoded turning desire (per-second rates scaled by `dt`).
pub fn apply_random_turn_heading(
    heading: &mut Heading,
    phenotype: &PhenotypeV2,
    dt: f32,
    rng: &mut impl Rng,
) {
    let turn_prob = per_tick_chance(phenotype.turn_chance_per_sec, dt);
    if turn_prob <= 0.0 || phenotype.turn_degrees_max <= 0.0 {
        return;
    }
    if rng.gen::<f32>() >= turn_prob {
        return;
    }
    let max_rad = phenotype.turn_degrees_max.to_radians();
    let delta = rng.gen_range(-max_rad..max_rad);
    heading.0 = (heading.0 + delta).rem_euclid(std::f32::consts::TAU);
}

/// Integrate v2 locomotion for one agent. Returns `false` if movement was skipped (insufficient energy).
pub fn apply_movement_tick_v2(
    pos: &mut Vec2,
    heading: &mut Heading,
    vel: &mut Vec2,
    energy: &mut Energy,
    phenotype: &PhenotypeV2,
    economy: &EconomyConfig,
    radius: f32,
    bounds: &WorldBounds,
    boundary: BoundaryMode,
    dt: f32,
    attempt_move: bool,
) -> bool {
    if phenotype.max_speed_effective <= MOVE_SPEED_EPS {
        *vel = Vec2::ZERO;
        return true;
    }

    if !attempt_move {
        *vel = Vec2::ZERO;
        return true;
    }

    *vel = heading.forward() * phenotype.max_speed_effective;
    *vel = clamp_velocity_to_max_speed(*vel, phenotype.max_speed_effective);
    let delta = *vel * dt;
    let distance = delta.length();

    if distance > MOVE_SPEED_EPS {
        let cost = movement_energy_for_distance_v2(
            distance,
            phenotype.body_radius,
            economy.move_energy_units_per_radius,
            phenotype.movement_cost_multiplier,
        );
        if energy.internal_energy < cost {
            return false;
        }
        energy.internal_energy = (energy.internal_energy - cost).max(0.0);
    }

    *pos += delta;
    apply_world_bounds(pos, vel, radius, bounds, boundary);
    heading.set_from_velocity(*vel);
    true
}

/// Bevy system: v2 agents use heading-based movement (wander / pursue / avoid).
pub fn behavior_movement_v2(
    time: Res<Time>,
    tick: Res<SimTick>,
    config: Res<SimConfigResource>,
    bounds: Res<WorldBounds>,
    sim_rng: Res<SimRng>,
    mut query: Query<(
        &mut Transform,
        &mut Heading,
        &mut Velocity,
        &Phenotype,
        &mut Energy,
        &mut BehaviorState,
        &Agent,
        &Lineage,
    )>,
) {
    let dt = time.delta_secs();
    let boundary = config.0.world.boundary;
    let economy = &config.0.economy;
    let sim_tick = tick.tick;

    for (mut transform, mut heading, mut velocity, phenotype, mut energy, mut behavior, agent, lineage) in
        &mut query
    {
        let Some(phenotype_v2) = phenotype.v2() else {
            continue;
        };
        let mut rng = sim_rng.stream_movement(lineage.id, sim_tick);
        let max_turn_rad = phenotype_v2.turn_degrees_max.to_radians() * dt;

        match behavior.mode {
            BehaviorMode::Avoid => {
                if let Some(target) = behavior.steer_heading {
                    heading.0 = target;
                }
            }
            BehaviorMode::Pursue => {
                if let Some(target) = behavior.steer_heading {
                    heading.0 = turn_toward(heading.0, target, max_turn_rad);
                }
            }
            BehaviorMode::Wander => {
                apply_random_turn_heading(&mut heading, phenotype_v2, dt, &mut rng);
            }
        }

        let attempt_move = match behavior.mode {
            BehaviorMode::Avoid if behavior.force_max_speed => true,
            BehaviorMode::Pursue | BehaviorMode::Wander => {
                let move_prob = per_tick_chance(phenotype_v2.movement_desire, dt);
                wants_to_move_this_tick(move_prob, rng.gen::<f32>())
            }
            _ => false,
        };

        let pos_before = transform.translation.truncate();
        let mut pos = pos_before;
        apply_movement_tick_v2(
            &mut pos,
            &mut heading,
            &mut **velocity,
            &mut energy,
            phenotype_v2,
            economy,
            agent.radius,
            &bounds,
            boundary,
            dt,
            attempt_move,
        );
        let distance = pos.distance(pos_before);
        apply_avoid_distance_traveled(&mut behavior, distance);
        transform.translation = pos.extend(transform.translation.z);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::genome::{v2_core, GenomeV2, V2_HEADER_LEN};
    use crate::genome::decode_v2;
    use crate::sim::components::Energy;

    fn v2_config() -> SimConfig {
        SimConfig::load_from_path(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs/genome_v2.example.toml"),
        )
        .unwrap()
    }

    fn set_core_byte(bytes: &mut [u8], offset: usize, value: u8) {
        bytes[V2_HEADER_LEN + offset] = value;
    }

    fn phenotype_from_core_patch<F>(patch: F) -> (SimConfig, crate::genome::PhenotypeV2)
    where
        F: FnOnce(&mut [u8]),
    {
        let config = v2_config();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        patch(&mut bytes);
        let genome = GenomeV2::from_bytes(bytes, &config.genetics).unwrap();
        let p = decode_v2(&genome, &config).unwrap();
        (config, p)
    }

    #[test]
    fn ancestor_is_sessile() {
        let (_, p) = phenotype_from_core_patch(|_| {});
        assert_eq!(p.max_speed_effective, 0.0);
        let mut pos = Vec2::ZERO;
        let mut heading = Heading(0.0);
        let mut vel = Vec2::new(100.0, 0.0);
        let mut energy = Energy::new_v2(100.0, 100.0);
        let bounds = WorldBounds::from_world_size(200.0, 200.0);
        let config = v2_config();
        apply_movement_tick_v2(
            &mut pos,
            &mut heading,
            &mut vel,
            &mut energy,
            &p,
            &config.economy,
            p.body_radius,
            &bounds,
            BoundaryMode::Bounce,
            1.0 / 60.0,
            true,
        );
        assert_eq!(pos, Vec2::ZERO);
        assert_eq!(vel, Vec2::ZERO);
    }

    #[test]
    fn mobile_mutant_moves_and_pays_efficiency_scaled_cost() {
        let (config, mut p) = phenotype_from_core_patch(|bytes| {
            set_core_byte(bytes, v2_core::MAX_SPEED, 0xFF);
            set_core_byte(bytes, v2_core::MOVEMENT_DESIRE, 0xFF);
            set_core_byte(bytes, v2_core::MOVEMENT_EFFICIENCY, 0xFF);
        });
        assert!(p.max_speed_effective > 0.0);
        p.movement_cost_multiplier = 1.0 - config.genetics.decode_v2.as_ref().unwrap().movement_cost_reduction_at_max;

        let dt = 1.0 / 60.0;
        let mut heading = Heading(0.0);
        let mut vel = Vec2::ZERO;
        let mut pos = Vec2::ZERO;
        let mut energy = Energy::new_v2(10_000.0, 10_000.0);
        let bounds = WorldBounds::from_world_size(500.0, 500.0);
        let start = energy.internal_energy;

        apply_movement_tick_v2(
            &mut pos,
            &mut heading,
            &mut vel,
            &mut energy,
            &p,
            &config.economy,
            p.body_radius,
            &bounds,
            BoundaryMode::Bounce,
            dt,
            true,
        );

        assert!(pos.x > 0.0);
        let distance = pos.length();
        let expected_cost = movement_energy_for_distance_v2(
            distance,
            p.body_radius,
            config.economy.move_energy_units_per_radius,
            p.movement_cost_multiplier,
        );
        let drain = start - energy.internal_energy;
        assert!(
            (drain - expected_cost).abs() < expected_cost.max(1e-6) * 0.15 + 1e-6,
            "drain={drain} expected={expected_cost}"
        );
        assert!(drain > 0.0);
        assert!(p.movement_cost_multiplier < 1.0, "FF efficiency should reduce move cost");
    }

    #[test]
    fn armor_reduces_effective_max_speed() {
        let (config, unarmored) = phenotype_from_core_patch(|_| {});
        let (_, armored) = phenotype_from_core_patch(|bytes| {
            let i = V2_HEADER_LEN + v2_core::ARMOR;
            bytes[i] = 0xFF;
            bytes[i + 1] = 0xFF;
            set_core_byte(bytes, v2_core::MAX_SPEED, 0xFF);
        });
        assert_eq!(unarmored.max_speed_effective, 0.0);
        assert!(armored.armor_speed_penalty > 0.0);
        assert!(
            armored.max_speed_effective < armored.max_speed_decoded,
            "armor should cap speed"
        );
        assert!(armored.armor_stroke > 0.0);
        let _ = config;
    }

    #[test]
    fn pursue_mode_turns_heading_toward_target() {
        use crate::sim::behavior::{BehaviorMode, BehaviorState};

        let (_, p) = phenotype_from_core_patch(|bytes| {
            set_core_byte(bytes, v2_core::MAX_SPEED, 0xFF);
            set_core_byte(bytes, v2_core::MOVEMENT_DESIRE, 0xFF);
            set_core_byte(bytes, v2_core::TURNING_DESIRE, 0xFF);
        });
        let dt = 1.0 / 60.0;
        let target = std::f32::consts::FRAC_PI_2;
        let mut heading = Heading(0.0);
        let behavior = BehaviorState {
            mode: BehaviorMode::Pursue,
            steer_heading: Some(target),
            ..Default::default()
        };
        let max_turn = p.turn_degrees_max.to_radians() * dt;
        heading.0 = crate::sim::behavior::turn_toward(heading.0, behavior.steer_heading.unwrap(), max_turn);
        assert!(heading.0 > 0.0, "pursue should rotate heading toward target");
        assert!(max_turn > 0.0);
    }

    #[test]
    fn avoid_budget_decrements_with_travel() {
        use crate::sim::behavior::{apply_avoid_distance_traveled, BehaviorMode, BehaviorState};

        let mut behavior = BehaviorState {
            mode: BehaviorMode::Avoid,
            avoid_distance_remaining: 100.0,
            force_max_speed: true,
            ..Default::default()
        };
        apply_avoid_distance_traveled(&mut behavior, 40.0);
        assert_eq!(behavior.avoid_distance_remaining, 60.0);
        apply_avoid_distance_traveled(&mut behavior, 60.0);
        assert_eq!(behavior.mode, BehaviorMode::Wander);
        assert!(!behavior.force_max_speed);
    }

    #[test]
    fn per_tick_chance_matches_one_second_at_60hz() {
        let dt = 1.0 / 60.0;
        let p = per_tick_chance(1.0, dt);
        assert!((p - 1.0).abs() < 1e-5);
        assert!(per_tick_chance(0.0, dt) < 1e-6);
    }
}

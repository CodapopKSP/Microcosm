//! Anti-bunching: periodic separation and reproduction placement checks.

use bevy::prelude::*;

use crate::config::{BoundaryMode, WorldPhysicsConfig};

use super::metabolism::circles_overlap;
use super::movement::apply_world_bounds;
use super::super::components::{Agent, Lineage};
use super::super::interval::interval_active_this_tick;
use super::super::world::{SimConfigResource, SimTick, WorldBounds};

/// One agent disc for overlap queries and separation passes.
#[derive(Debug, Clone, Copy)]
pub struct AgentCircle {
    pub id: u64,
    pub pos: Vec2,
    pub radius: f32,
}

/// True when `pos`/`radius` overlaps any circle in `others`, optionally skipping one id.
pub fn position_overlaps_any(
    pos: Vec2,
    radius: f32,
    others: &[AgentCircle],
    exclude_id: Option<u64>,
) -> bool {
    others.iter().any(|other| {
        if exclude_id == Some(other.id) {
            return false;
        }
        circles_overlap(pos, radius, other.pos, other.radius)
    })
}

/// Push apart overlapping discs over several relaxation passes; clamps to world bounds each pass.
pub fn separate_overlapping_circles(
    agents: &mut [AgentCircle],
    bounds: &WorldBounds,
    boundary: BoundaryMode,
    passes: usize,
) {
    let passes = passes.max(1);
    for _ in 0..passes {
        let n = agents.len();
        let mut deltas = vec![Vec2::ZERO; n];
        for i in 0..n {
            for j in (i + 1)..n {
                let (push_a, push_b) = pair_separation_push(&agents[i], &agents[j]);
                deltas[i] += push_a;
                deltas[j] += push_b;
            }
        }
        for (agent, delta) in agents.iter_mut().zip(deltas) {
            agent.pos += delta;
            let mut vel = Vec2::ZERO;
            apply_world_bounds(&mut agent.pos, &mut vel, agent.radius, bounds, boundary);
        }
    }
}

fn pair_separation_push(a: &AgentCircle, b: &AgentCircle) -> (Vec2, Vec2) {
    let diff = b.pos - a.pos;
    let dist_sq = diff.length_squared();
    let min_dist = a.radius + b.radius;
    if min_dist <= 0.0 {
        return (Vec2::ZERO, Vec2::ZERO);
    }
    let min_dist_sq = min_dist * min_dist;
    if dist_sq >= min_dist_sq {
        return (Vec2::ZERO, Vec2::ZERO);
    }

    let (dir, overlap) = if dist_sq > 1e-8 {
        let dist = dist_sq.sqrt();
        (diff / dist, min_dist - dist)
    } else {
        (coincident_direction(a.id, b.id), min_dist)
    };

    let half = overlap * 0.5;
    (-dir * half, dir * half)
}

fn coincident_direction(id_a: u64, id_b: u64) -> Vec2 {
    let mixed = id_a.wrapping_mul(0x9E37_79B9).wrapping_add(id_b);
    let angle = (mixed % 6283) as f32 / 1000.0;
    Vec2::from_angle(angle)
}

/// Bevy system: every [`WorldPhysicsConfig::crowding_interval_ticks`] (default 120 = 2 s at 60 Hz).
pub fn crowding_separation(
    tick: Res<SimTick>,
    config: Res<SimConfigResource>,
    bounds: Res<WorldBounds>,
    mut query: Query<(&mut Transform, &Agent, &Lineage)>,
) {
    let physics = &config.0.world.physics;
    if !physics.crowding_enabled {
        return;
    }
    if !interval_active_this_tick(
        tick.tick,
        physics.crowding_interval_ticks,
        physics.crowding_interval_phase_ticks,
    ) {
        return;
    }

    let boundary = config.0.world.boundary;
    let mut circles: Vec<AgentCircle> = query
        .iter()
        .map(|(transform, agent, lineage)| AgentCircle {
            id: lineage.id,
            pos: transform.translation.truncate(),
            radius: agent.radius,
        })
        .collect();
    circles.sort_by_key(|c| c.id);

    separate_overlapping_circles(&mut circles, &bounds, boundary, 4);

    for (mut transform, agent, lineage) in &mut query {
        if let Some(circle) = circles.iter().find(|c| c.id == lineage.id) {
            transform.translation.x = circle.pos.x;
            transform.translation.y = circle.pos.y;
            let _ = agent;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BoundaryMode;

    #[test]
    fn overlapping_pair_separates() {
        let mut agents = [
            AgentCircle {
                id: 1,
                pos: Vec2::ZERO,
                radius: 10.0,
            },
            AgentCircle {
                id: 2,
                pos: Vec2::new(5.0, 0.0),
                radius: 10.0,
            },
        ];
        let bounds = WorldBounds::from_world_size(1000.0, 1000.0);
        separate_overlapping_circles(&mut agents, &bounds, BoundaryMode::Bounce, 4);
        assert!(!circles_overlap(
            agents[0].pos,
            agents[0].radius,
            agents[1].pos,
            agents[1].radius
        ));
    }

    #[test]
    fn position_overlaps_any_detects_neighbor() {
        let others = vec![AgentCircle {
            id: 1,
            pos: Vec2::ZERO,
            radius: 10.0,
        }];
        assert!(position_overlaps_any(Vec2::new(15.0, 0.0), 10.0, &others, None));
        assert!(!position_overlaps_any(Vec2::new(25.0, 0.0), 10.0, &others, None));
        assert!(!position_overlaps_any(Vec2::new(15.0, 0.0), 10.0, &others, Some(1)));
    }

    #[test]
    fn separation_runs_on_two_second_interval() {
        let physics = WorldPhysicsConfig {
            crowding_enabled: true,
            crowding_interval_ticks: 120,
            crowding_interval_phase_ticks: 0,
            ..WorldPhysicsConfig::default()
        };
        assert!(interval_active_this_tick(0, physics.crowding_interval_ticks, 0));
        assert!(!interval_active_this_tick(1, physics.crowding_interval_ticks, 0));
        assert!(interval_active_this_tick(120, physics.crowding_interval_ticks, 0));
    }
}

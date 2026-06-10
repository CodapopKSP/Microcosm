//! Static oriented rectangles (terrain obstacles) configured in `[world.structures]`.

use bevy::prelude::*;

use crate::config::{StructureColor, StructureConfig};

/// One axis-aligned rectangle in local space, placed and rotated in world space.
#[derive(Debug, Clone)]
pub struct Structure {
    pub center: Vec2,
    pub half_width: f32,
    pub half_height: f32,
    /// Rotation from local +X toward world +X, radians.
    pub angle: f32,
    pub color: StructureColor,
}

impl Structure {
    pub fn from_config(cfg: &StructureConfig) -> Self {
        Self {
            center: Vec2::from_array(cfg.center),
            half_width: cfg.width.max(0.0) * 0.5,
            half_height: cfg.height.max(0.0) * 0.5,
            angle: cfg.angle_deg.to_radians(),
            color: cfg.color,
        }
    }

    fn rotate_vec2(v: Vec2, angle: f32) -> Vec2 {
        let (sin, cos) = angle.sin_cos();
        Vec2::new(cos * v.x - sin * v.y, sin * v.x + cos * v.y)
    }

    fn to_local(&self, world: Vec2) -> Vec2 {
        Self::rotate_vec2(world - self.center, -self.angle)
    }

    fn local_normal_to_world(&self, local_normal: Vec2) -> Vec2 {
        Self::rotate_vec2(local_normal, self.angle).normalize_or_zero()
    }

    /// World-space corners (CCW).
    pub fn corners(&self) -> [Vec2; 4] {
        let hw = self.half_width;
        let hh = self.half_height;
        [
            Vec2::new(-hw, -hh),
            Vec2::new(hw, -hh),
            Vec2::new(hw, hh),
            Vec2::new(-hw, hh),
        ]
        .map(|local| self.center + Self::rotate_vec2(local, self.angle))
    }

    pub fn contains_point(&self, pos: Vec2) -> bool {
        let local = self.to_local(pos);
        local.x.abs() <= self.half_width && local.y.abs() <= self.half_height
    }

    /// True when a ray from `pos` toward `sun_dir` intersects this structure.
    pub fn occludes_sun_at(&self, pos: Vec2, sun_dir: Vec2) -> bool {
        if self.contains_point(pos) {
            return true;
        }
        ray_hits_obb(pos, sun_dir, self)
    }

    /// Shadow footprint polygon extruded from this structure along `shadow_dir`.
    pub fn shadow_polygon(&self, shadow_dir: Vec2, far_y: f32) -> Vec<Vec2> {
        let corners = self.corners();
        let mut points = Vec::with_capacity(8);
        for corner in corners {
            points.push(corner);
            points.push(extrude_to_y(corner, shadow_dir, far_y));
        }
        convex_hull(points)
    }

    /// Circle center overlaps this rectangle (including interior).
    pub fn overlaps_circle(&self, pos: Vec2, radius: f32) -> bool {
        self.circle_penetration(pos, radius).is_some()
    }

    /// Push vector and outward normal if `pos` with `radius` overlaps this structure.
    pub fn circle_penetration(&self, pos: Vec2, radius: f32) -> Option<(Vec2, Vec2)> {
        if self.half_width <= 0.0 || self.half_height <= 0.0 || radius <= 0.0 {
            return None;
        }

        let local = self.to_local(pos);
        let closest = Vec2::new(
            local.x.clamp(-self.half_width, self.half_width),
            local.y.clamp(-self.half_height, self.half_height),
        );
        let diff = local - closest;
        let dist_sq = diff.length_squared();
        let r = radius;
        if dist_sq >= r * r {
            return None;
        }

        let (normal_local, penetration) = if dist_sq > 1e-8 {
            let dist = dist_sq.sqrt();
            (diff / dist, r - dist)
        } else {
            let to_left = local.x + self.half_width;
            let to_right = self.half_width - local.x;
            let to_bottom = local.y + self.half_height;
            let to_top = self.half_height - local.y;
            let (normal, penetration) = if to_left <= to_right && to_left <= to_bottom && to_left <= to_top {
                (Vec2::new(-1.0, 0.0), r + to_left)
            } else if to_right <= to_bottom && to_right <= to_top {
                (Vec2::new(1.0, 0.0), r + to_right)
            } else if to_bottom <= to_top {
                (Vec2::new(0.0, -1.0), r + to_bottom)
            } else {
                (Vec2::new(0.0, 1.0), r + to_top)
            };
            (normal, penetration)
        };

        let push = self.local_normal_to_world(normal_local) * penetration;
        let normal = self.local_normal_to_world(normal_local);
        if normal.length_squared() <= 1e-8 {
            return None;
        }
        Some((push, normal))
    }
}

/// Static obstacles loaded from config at startup.
#[derive(Debug, Clone, Resource, Default)]
pub struct WorldStructures {
    structures: Vec<Structure>,
}

impl WorldStructures {
    pub fn from_config(configs: &[StructureConfig]) -> Self {
        Self {
            structures: configs.iter().map(Structure::from_config).collect(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Structure> {
        self.structures.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.structures.is_empty()
    }

    pub fn casts_sun_shadows(&self) -> bool {
        !self.structures.is_empty()
    }

    pub fn occludes_sun(&self, pos: Vec2, sun_dir: Vec2) -> bool {
        self.structures
            .iter()
            .any(|s| s.occludes_sun_at(pos, sun_dir))
    }

    pub fn shadow_polygons(&self, shadow_dir: Vec2, far_y: f32) -> Vec<Vec<Vec2>> {
        self.structures
            .iter()
            .map(|s| s.shadow_polygon(shadow_dir, far_y))
            .filter(|poly| poly.len() >= 3)
            .collect()
    }

    pub fn overlaps_circle(&self, pos: Vec2, radius: f32) -> bool {
        self.structures
            .iter()
            .any(|s| s.overlaps_circle(pos, radius))
    }

    /// Bounce `pos`/`vel` off any overlapping structures (iterates until resolved).
    pub fn resolve_circle_collisions(&self, pos: &mut Vec2, vel: &mut Vec2, radius: f32) {
        if self.structures.is_empty() || radius <= 0.0 {
            return;
        }

        let max_passes = self.structures.len().saturating_mul(2).max(1);
        for _ in 0..max_passes {
            let mut resolved = false;
            for structure in &self.structures {
                if let Some((push, normal)) = structure.circle_penetration(*pos, radius) {
                    *pos += push;
                    let vn = vel.dot(normal);
                    if vn < 0.0 {
                        *vel -= 2.0 * vn * normal;
                    }
                    resolved = true;
                }
            }
            if !resolved {
                break;
            }
        }
    }
}

fn extrude_to_y(corner: Vec2, shadow_dir: Vec2, far_y: f32) -> Vec2 {
    if shadow_dir.y.abs() <= 1e-6 {
        return corner;
    }
    let t = (far_y - corner.y) / shadow_dir.y;
    corner + shadow_dir * t
}

fn ray_hits_obb(origin: Vec2, dir: Vec2, structure: &Structure) -> bool {
    if dir.length_squared() <= 1e-8 {
        return false;
    }

    let local_origin = structure.to_local(origin);
    let local_dir = Structure::rotate_vec2(dir, -structure.angle);
    let hw = structure.half_width;
    let hh = structure.half_height;

    let mut t_min = f32::NEG_INFINITY;
    let mut t_max = f32::INFINITY;

    for (o, d, half) in [
        (local_origin.x, local_dir.x, hw),
        (local_origin.y, local_dir.y, hh),
    ] {
        if d.abs() <= 1e-8 {
            if o < -half || o > half {
                return false;
            }
        } else {
            let mut t1 = (-half - o) / d;
            let mut t2 = (half - o) / d;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            t_min = t_min.max(t1);
            t_max = t_max.min(t2);
        }
    }

    t_max >= 0.0 && t_min <= t_max
}

fn convex_hull(mut points: Vec<Vec2>) -> Vec<Vec2> {
    if points.len() <= 1 {
        return points;
    }
    points.sort_by(|a, b| {
        a.x
            .partial_cmp(&b.x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
    });
    points.dedup_by(|a, b| (*a - *b).length_squared() < 1e-6);
    let n = points.len();
    if n <= 2 {
        return points;
    }

    let mut lower = Vec::new();
    for p in &points {
        while lower.len() >= 2
            && cross(lower[lower.len() - 2], lower[lower.len() - 1], *p) <= 0.0
        {
            lower.pop();
        }
        lower.push(*p);
    }

    let mut upper = Vec::new();
    for p in points.iter().rev() {
        while upper.len() >= 2
            && cross(upper[upper.len() - 2], upper[upper.len() - 1], *p) <= 0.0
        {
            upper.pop();
        }
        upper.push(*p);
    }

    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

fn cross(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    (b - a).perp_dot(c - a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StructureConfig;

    fn horizontal_bar() -> Structure {
        Structure::from_config(&StructureConfig {
            center: [0.0, 0.0],
            width: 200.0,
            height: 40.0,
            angle_deg: 0.0,
            color: StructureColor::default(),
        })
    }

    #[test]
    fn circle_outside_does_not_overlap() {
        let s = horizontal_bar();
        assert!(!s.overlaps_circle(Vec2::new(0.0, 100.0), 10.0));
    }

    #[test]
    fn circle_overlapping_edge_is_pushed_out() {
        let s = horizontal_bar();
        let mut pos = Vec2::new(0.0, 28.0);
        let mut vel = Vec2::new(0.0, -50.0);
        let field = WorldStructures {
            structures: vec![s],
        };
        field.resolve_circle_collisions(&mut pos, &mut vel, 10.0);
        assert!(pos.y > 28.0);
        assert!(vel.y > 0.0);
    }

    #[test]
    fn point_below_structure_is_in_sun_shadow() {
        let s = horizontal_bar();
        assert!(s.occludes_sun_at(Vec2::new(0.0, -100.0), Vec2::new(0.0, 1.0)));
        assert!(!s.occludes_sun_at(Vec2::new(300.0, -100.0), Vec2::new(0.0, 1.0)));
        assert!(!s.occludes_sun_at(Vec2::new(0.0, 100.0), Vec2::new(0.0, 1.0)));
    }

    #[test]
    fn shadow_polygon_extends_toward_world_bottom() {
        let s = horizontal_bar();
        let poly = s.shadow_polygon(Vec2::new(0.0, -1.0), -500.0);
        assert!(poly.len() >= 4);
        assert!(poly.iter().any(|p| (p.y + 500.0).abs() < 1e-3));
    }

    #[test]
    fn rotated_structure_blocks_diagonal_approach() {
        let s = Structure::from_config(&StructureConfig {
            center: [0.0, 0.0],
            width: 100.0,
            height: 20.0,
            angle_deg: 45.0,
            color: StructureColor::default(),
        });
        assert!(s.overlaps_circle(Vec2::new(20.0, 20.0), 5.0));
    }
}

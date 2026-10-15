//! Ocean currents: a divergence-free flow field from curl noise.
//!
//! Large slow gyres everywhere, plus faster eddies in the surface mixed layer that
//! stir plankton through the sunlit water. Deterministic in position and time.

use glam::Vec2;

use crate::config::{CurrentsConfig, EventsConfig};
use crate::rng::mix64;

#[inline]
fn hash3(seed: u64, x: i32, y: i32, z: i32) -> f32 {
    let h = mix64(
        seed ^ (x as u32 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (y as u32 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ (z as u32 as u64).wrapping_mul(0x1656_67B1_9E37_79F9),
    );
    (h >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0
}

/// 3D value noise with its analytic gradient in x and y: `(value, d/dx, d/dy)`.
fn noise3_grad(seed: u64, x: f32, y: f32, z: f32) -> (f32, f32, f32) {
    let (xi, yi, zi) = (x.floor(), y.floor(), z.floor());
    let (fx, fy, fz) = (x - xi, y - yi, z - zi);
    let (ix, iy, iz) = (xi as i32, yi as i32, zi as i32);
    // Quintic fade and derivative.
    let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let dfade = |t: f32| 30.0 * t * t * (t * (t - 2.0) + 1.0);
    let (u, v, w) = (fade(fx), fade(fy), fade(fz));
    let (du, dv) = (dfade(fx), dfade(fy));

    let c = |dx: i32, dy: i32, dz: i32| hash3(seed, ix + dx, iy + dy, iz + dz);
    // Interpolate along z first.
    let lz = |dx: i32, dy: i32| {
        let a = c(dx, dy, 0);
        a + (c(dx, dy, 1) - a) * w
    };
    let (a, b, cc, d) = (lz(0, 0), lz(1, 0), lz(0, 1), lz(1, 1));
    let k1 = b - a;
    let k2 = cc - a;
    let k3 = a - b - cc + d;
    let value = a + k1 * u + k2 * v + k3 * u * v;
    let dx = du * (k1 + k3 * v);
    let dy = dv * (k2 + k3 * u);
    (value, dx, dy)
}

/// Water velocity at `pos` (units/s). `height` is the world height (surface).
pub fn flow(pos: Vec2, time: f32, height: f32, cfg: &CurrentsConfig, seed: u64) -> Vec2 {
    if cfg.gyre_speed <= 0.0 && cfg.eddy_speed <= 0.0 {
        return Vec2::ZERO;
    }
    let depth = (height - pos.y).max(0.0);
    let mut v = Vec2::ZERO;

    if cfg.gyre_speed > 0.0 {
        let s = cfg.gyre_scale.max(1.0);
        let (_, gx, gy) = noise3_grad(seed, pos.x / s, pos.y / s, time / cfg.gyre_period.max(1.0));
        v += Vec2::new(gy, -gx) * cfg.gyre_speed;
    }

    let mixing = 1.0 - depth / cfg.mixing_depth.max(1.0);
    if cfg.eddy_speed > 0.0 && mixing > 0.0 {
        let s = cfg.eddy_scale.max(1.0);
        let (_, gx, gy) = noise3_grad(
            seed ^ 0xEDD1,
            pos.x / s,
            pos.y / s,
            time / cfg.eddy_period.max(1.0),
        );
        v += Vec2::new(gy, -gx) * cfg.eddy_speed * mixing * mixing;
    }

    // Water can't flow out through the surface.
    let near_surface = (depth / 60.0).clamp(0.0, 1.0);
    v.y *= near_surface;
    v
}

/// Extra water velocity at `pos` from storms, given as (centre, width, strength 0..1): fast
/// small eddies in the upper water, strongest at the surface and in the storm's middle.
pub fn storm_flow(
    pos: Vec2,
    time: f32,
    height: f32,
    storms: &[(f32, f32, f32)],
    cfg: &EventsConfig,
    seed: u64,
) -> Vec2 {
    let depth = (height - pos.y).max(0.0);
    let reach = (1.0 - depth / cfg.storm_depth.max(1.0)).max(0.0);
    if storms.is_empty() || reach <= 0.0 {
        return Vec2::ZERO;
    }
    let mut v = Vec2::ZERO;
    for &(x, width, strength) in storms {
        let k = strength * crate::events::across(x, width, pos.x) * reach * reach;
        if k > 0.0 {
            let s = cfg.storm_eddy_scale.max(1.0);
            let (_, gx, gy) = noise3_grad(seed ^ 0x5707, pos.x / s, pos.y / s, time / 20.0);
            v += Vec2::new(gy, -gx) * cfg.storm_speed * k;
        }
    }
    // Water can't flow out through the surface.
    v.y *= (depth / 60.0).clamp(0.0, 1.0);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_gradient_matches_finite_difference() {
        let (x, y, z) = (3.37, -1.21, 0.64);
        let (_, dx, dy) = noise3_grad(7, x, y, z);
        let e = 1e-3;
        let fdx = (noise3_grad(7, x + e, y, z).0 - noise3_grad(7, x - e, y, z).0) / (2.0 * e);
        let fdy = (noise3_grad(7, x, y + e, z).0 - noise3_grad(7, x, y - e, z).0) / (2.0 * e);
        assert!((dx - fdx).abs() < 1e-2, "{dx} vs {fdx}");
        assert!((dy - fdy).abs() < 1e-2, "{dy} vs {fdy}");
    }

    #[test]
    fn flow_is_stronger_near_the_surface_and_has_no_vertical_escape() {
        let cfg = CurrentsConfig::default();
        let h = 6000.0;
        let mut shallow = 0.0;
        let mut deep = 0.0;
        for i in 0..200 {
            let x = i as f32 * 71.0;
            shallow += flow(Vec2::new(x, h - 80.0), 10.0, h, &cfg, 1).length();
            deep += flow(Vec2::new(x, h - 3000.0), 10.0, h, &cfg, 1).length();
            assert!(flow(Vec2::new(x, h), 10.0, h, &cfg, 1).y.abs() < 1e-6);
        }
        assert!(shallow > deep * 1.3, "shallow {shallow} deep {deep}");
    }
}

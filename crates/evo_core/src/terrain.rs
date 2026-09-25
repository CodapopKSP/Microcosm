//! Procedural seafloor and rocks, stored as a solid mask plus a signed distance field.
//!
//! The SDF (positive in water, negative in rock) drives collisions; the mask blocks
//! light and field flow.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::config::{Config, TerrainConfig};
use crate::field::Field;
use crate::rng::{Rng, mix64};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Terrain {
    /// Signed distance to rock at cell centers; `sdf.solid` is the (smoothed) rock mask.
    pub sdf: Field,
    /// Raw rock mask as generated (seafloor + rocks).
    base_mask: Vec<bool>,
    /// Raw rock mask right now (base + event walls).
    mask: Vec<bool>,
    /// Height of the bare seafloor (no rocks) per terrain column.
    pub floor: Vec<f32>,
    pub width: f32,
    pub height: f32,
    /// Incremented whenever the terrain changes (so the renderer can refresh).
    pub version: u64,
}

/// A wall rising from the seafloor (from an event).
#[derive(Clone, Copy, Debug)]
pub struct Wall {
    pub x: f32,
    pub width: f32,
    pub top: f32,
    pub seed: u64,
}

/// Smooth 1D value noise in [-1, 1].
fn value_noise(seed: u64, x: f32) -> f32 {
    let x0 = x.floor();
    let t = x - x0;
    let t = t * t * (3.0 - 2.0 * t);
    let h = |i: i64| {
        (mix64(seed ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)) >> 40) as f32
            / (1u64 << 24) as f32
            * 2.0
            - 1.0
    };
    let a = h(x0 as i64);
    let b = h(x0 as i64 + 1);
    a + (b - a) * t
}

fn fbm(seed: u64, x: f32, octaves: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut freq = 1.0;
    let mut norm = 0.0;
    for o in 0..octaves {
        sum += amp * value_noise(seed.wrapping_add(o as u64 * 7919), x * freq);
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Bare seafloor height at `x`.
fn floor_height(cfg: &TerrainConfig, seed: u64, width: f32, height: f32, x: f32) -> f32 {
    let mut h = cfg.floor_height
        + cfg.floor_roughness * fbm(seed, x / 1800.0, 4)
        + cfg.floor_roughness * 0.2 * fbm(seed ^ 0xABCD, x / 240.0, 2);

    // Shallow shelf toward the right edge.
    let shelf_x = cfg.shelf_start * width;
    let s = smoothstep(shelf_x - 900.0, shelf_x + 300.0, x);
    let shelf_top = height - cfg.shelf_depth + 120.0 * fbm(seed ^ 0x5151, x / 500.0, 3);
    h = h + (shelf_top - h) * s;

    // Trench.
    let tc = cfg.trench_center * width;
    let tw = (cfg.trench_width * width).max(1.0);
    let t = ((x - tc) / tw).abs();
    if t < 1.0 {
        let bump = (1.0 - t * t).powi(2);
        h -= cfg.trench_depth * bump;
    }
    h.clamp(64.0, height - 200.0)
}

/// Distance transform helper (Felzenszwalb & Huttenlocher), squared distances in cells.
fn edt_1d(f: &[f32], out: &mut [f32], v: &mut [usize], z: &mut [f32]) {
    let n = f.len();
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f32::NEG_INFINITY;
    z[1] = f32::INFINITY;
    for q in 1..n {
        loop {
            let p = v[k];
            let s = ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32))
                / (2.0 * q as f32 - 2.0 * p as f32);
            if s <= z[k] && k > 0 {
                k -= 1;
                continue;
            }
            if s <= z[k] {
                // k == 0: replace the first parabola.
                v[0] = q;
                z[0] = f32::NEG_INFINITY;
                z[1] = f32::INFINITY;
                break;
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = f32::INFINITY;
            break;
        }
    }
    k = 0;
    for (q, o) in out.iter_mut().enumerate() {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let d = q as f32 - v[k] as f32;
        *o = d * d + f[v[k]];
    }
}

/// Squared distance (in cells) from every cell to the nearest `target` cell.
fn distance_transform(nx: usize, ny: usize, target: impl Fn(usize) -> bool) -> Vec<f32> {
    const INF: f32 = 1e20;
    let mut grid: Vec<f32> = (0..nx * ny)
        .map(|i| if target(i) { 0.0 } else { INF })
        .collect();
    let n = nx.max(ny);
    let mut f = vec![0.0; n];
    let mut out = vec![0.0; n];
    let mut v = vec![0usize; n];
    let mut z = vec![0.0f32; n + 1];
    // Columns.
    for x in 0..nx {
        for y in 0..ny {
            f[y] = grid[y * nx + x];
        }
        edt_1d(&f[..ny], &mut out[..ny], &mut v, &mut z);
        for y in 0..ny {
            grid[y * nx + x] = out[y];
        }
    }
    // Rows.
    for y in 0..ny {
        f[..nx].copy_from_slice(&grid[y * nx..(y + 1) * nx]);
        edt_1d(&f[..nx], &mut out[..nx], &mut v, &mut z);
        grid[y * nx..(y + 1) * nx].copy_from_slice(&out[..nx]);
    }
    grid
}

impl Terrain {
    pub fn generate(config: &Config) -> Terrain {
        let cfg = &config.terrain;
        let (width, height) = (config.world.width, config.world.height);
        let seed = mix64(config.world.seed ^ 0x7E44_A1B0);
        let field = Field::new(width, height, cfg.cell, 0.0);
        let (nx, ny, cell) = (field.nx, field.ny, field.cell);

        let floor: Vec<f32> = (0..nx)
            .map(|x| floor_height(cfg, seed, width, height, (x as f32 + 0.5) * cell))
            .collect();

        let mut solid = vec![false; nx * ny];
        let wall = 3usize;
        for y in 0..ny {
            let cy = (y as f32 + 0.5) * cell;
            for x in 0..nx {
                if x < wall || x >= nx - wall || cy < floor[x] {
                    solid[y * nx + x] = true;
                }
            }
        }

        // Rock outcrops.
        let mut rng = Rng::new(seed ^ 0x0C0C);
        let floor_at = |px: f32| floor[((px / cell) as usize).min(nx - 1)];
        let stamp = |solid: &mut Vec<bool>, shape: &dyn Fn(Vec2) -> bool, lo: Vec2, hi: Vec2| {
            let x0 = ((lo.x / cell).floor().max(0.0)) as usize;
            let y0 = ((lo.y / cell).floor().max(0.0)) as usize;
            let x1 = ((hi.x / cell).ceil() as usize).min(nx);
            let y1 = ((hi.y / cell).ceil() as usize).min(ny);
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = Vec2::new((x as f32 + 0.5) * cell, (y as f32 + 0.5) * cell);
                    if shape(p) {
                        solid[y * nx + x] = true;
                    }
                }
            }
        };

        for k in 0..cfg.rocks {
            let size = cfg.rock_size * rng.range(0.5, 1.6);
            let cx = rng.range(0.05, 0.95) * width;
            let base = floor_at(cx);
            let nseed = seed.wrapping_add(k as u64 * 104_729);
            match k % 4 {
                // Pillar, sometimes with a ledge.
                0 | 3 => {
                    let h = size * rng.range(1.5, 4.0);
                    let w = size * rng.range(0.25, 0.5);
                    let ledge = k % 4 == 3;
                    let ledge_y = base + h * rng.range(0.6, 0.95);
                    let ledge_len =
                        size * rng.range(1.0, 2.2) * if rng.chance(0.5) { 1.0 } else { -1.0 };
                    let shape = move |p: Vec2| {
                        let wobble = 1.0 + 0.35 * value_noise(nseed, p.y / 60.0);
                        let in_pillar = (p.x - cx).abs() < w * 0.5 * wobble && p.y < base + h;
                        let in_ledge = ledge && {
                            let along = (p.x - cx) / ledge_len;
                            (0.0..1.0).contains(&along)
                                && (p.y - ledge_y).abs() < size * 0.18 * (1.0 - along * 0.6)
                        };
                        in_pillar || in_ledge
                    };
                    let reach = size * 2.5;
                    stamp(
                        &mut solid,
                        &shape,
                        Vec2::new(cx - reach, base - 50.0),
                        Vec2::new(cx + reach, base + h + size),
                    );
                }
                // Arch (two legs and a span) — makes a sheltered cave.
                1 => {
                    let span = size * rng.range(2.0, 4.0);
                    let h = size * rng.range(1.2, 2.5);
                    let leg = size * 0.35;
                    let thick = size * 0.4;
                    let shape = move |p: Vec2| {
                        let dx = (p.x - cx).abs();
                        let wobble = 1.0 + 0.3 * value_noise(nseed, p.x / 70.0 + p.y / 90.0);
                        let legs = (dx - span * 0.5).abs() < leg * wobble && p.y < base + h;
                        let top = dx < span * 0.5 + leg
                            && (p.y - (base + h)).abs() < thick * 0.5 * wobble;
                        legs || top
                    };
                    let reach = span + size;
                    stamp(
                        &mut solid,
                        &shape,
                        Vec2::new(cx - reach, base - 50.0),
                        Vec2::new(cx + reach, base + h + size),
                    );
                }
                // Boulder field with narrow crevices between the stones.
                _ => {
                    let stones = 3 + rng.below(4);
                    let mut x = cx;
                    for _ in 0..stones {
                        let r = size * rng.range(0.35, 0.7);
                        let gap = rng.range(12.0, 40.0);
                        let c = Vec2::new(x + r, floor_at(x + r) + r * 0.4);
                        let shape = move |p: Vec2| {
                            let a = (p - c).to_angle();
                            let wobble = 1.0 + 0.2 * value_noise(nseed, a * 3.0);
                            p.distance(c) < r * wobble
                        };
                        stamp(
                            &mut solid,
                            &shape,
                            c - Vec2::splat(r * 1.3),
                            c + Vec2::splat(r * 1.3),
                        );
                        x += 2.0 * r + gap;
                    }
                }
            }
        }

        let mut terrain = Terrain {
            sdf: field,
            base_mask: solid.clone(),
            mask: solid,
            floor,
            width,
            height,
            version: 0,
        };
        terrain.recompute_sdf(0, nx);
        terrain
    }

    /// Recompute the signed distance field for terrain columns `x0..x1` (with padding so the
    /// result matches a full recompute near surfaces).
    fn recompute_sdf(&mut self, x0: usize, x1: usize) {
        const PAD: usize = 48;
        let (nx, ny, cell) = (self.sdf.nx, self.sdf.ny, self.sdf.cell);
        let wx0 = x0.saturating_sub(PAD);
        let wx1 = (x1 + PAD).min(nx);
        let wnx = wx1 - wx0;
        let mask = |lx: usize, y: usize| self.mask[y * nx + wx0 + lx];
        let dist_to_rock = distance_transform(wnx, ny, |i| mask(i % wnx, i / wnx));
        let dist_to_water = distance_transform(wnx, ny, |i| !mask(i % wnx, i / wnx));
        let mut data: Vec<f32> = (0..wnx * ny)
            .map(|i| {
                if mask(i % wnx, i / wnx) {
                    -(dist_to_water[i].sqrt() - 0.5) * cell
                } else {
                    (dist_to_rock[i].sqrt() - 0.5) * cell
                }
            })
            .collect();
        // Soften the cell staircase: a few box-blur passes round corners without moving edges much.
        let mut tmp = data.clone();
        for _ in 0..3 {
            for y in 0..ny {
                for x in 0..wnx {
                    let mut sum = 0.0;
                    let mut n = 0.0;
                    for (dx, dy) in [(0i32, 0i32), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let (xx, yy) = (x as i32 + dx, y as i32 + dy);
                        if xx >= 0 && yy >= 0 && (xx as usize) < wnx && (yy as usize) < ny {
                            sum += data[yy as usize * wnx + xx as usize];
                            n += 1.0;
                        }
                    }
                    tmp[y * wnx + x] = sum / n;
                }
            }
            std::mem::swap(&mut data, &mut tmp);
        }
        // Write back the interior (the padding near a window edge is less accurate).
        let keep0 = if wx0 == 0 {
            0
        } else {
            x0.saturating_sub(PAD / 2)
        };
        let keep1 = if wx1 == nx {
            nx
        } else {
            (x1 + PAD / 2).min(nx)
        };
        for y in 0..ny {
            for x in keep0..keep1 {
                let v = data[y * wnx + (x - wx0)];
                let i = y * nx + x;
                self.sdf.data[i] = v;
                self.sdf.solid[i] = v < 0.0;
            }
        }
        self.version += 1;
    }

    /// Rebuild the region `x_min..x_max` (world units) from the base terrain plus `walls`.
    /// Returns the column range of terrain cells that changed.
    pub fn apply_walls(&mut self, walls: &[Wall], x_min: f32, x_max: f32) -> (f32, f32) {
        let (nx, ny, cell) = (self.sdf.nx, self.sdf.ny, self.sdf.cell);
        let x0 = ((x_min / cell).floor().max(0.0) as usize).min(nx);
        let x1 = ((x_max / cell).ceil().max(0.0) as usize).min(nx);
        for y in 0..ny {
            let cy = (y as f32 + 0.5) * cell;
            for x in x0..x1 {
                let cx = (x as f32 + 0.5) * cell;
                let i = y * nx + x;
                let in_wall = walls.iter().any(|w| {
                    let wobble = 1.0 + 0.3 * value_noise(w.seed, cy / 90.0);
                    (cx - w.x).abs() < w.width * 0.5 * wobble && cy < w.top
                });
                self.mask[i] = self.base_mask[i] || in_wall;
            }
        }
        self.recompute_sdf(x0, x1);
        (x0 as f32 * cell, x1 as f32 * cell)
    }

    /// Signed distance to rock (positive in water).
    #[inline]
    pub fn distance(&self, pos: Vec2) -> f32 {
        self.sdf.sample(pos)
    }

    /// Outward surface normal near `pos`.
    pub fn normal(&self, pos: Vec2) -> Vec2 {
        let g = self.sdf.gradient(pos, self.sdf.cell * 0.5);
        g.try_normalize().unwrap_or(Vec2::Y)
    }

    /// Whether a circle fits in open water.
    pub fn is_free(&self, pos: Vec2, radius: f32) -> bool {
        pos.x > radius
            && pos.x < self.width - radius
            && pos.y > radius
            && pos.y < self.height - radius
            && self.distance(pos) >= radius
    }

    /// Push a circle out of rock and inside the world. Returns true on contact.
    pub fn collide(&self, pos: &mut Vec2, vel: &mut Vec2, radius: f32) -> bool {
        let mut hit = false;
        let d = self.distance(*pos);
        if d < radius {
            let n = self.normal(*pos);
            *pos += n * (radius - d);
            let vn = vel.dot(n);
            if vn < 0.0 {
                *vel -= n * vn;
            }
            hit = true;
        }
        let r = radius.min(self.width * 0.25);
        if pos.x < r {
            pos.x = r;
            vel.x = vel.x.max(0.0);
            hit = true;
        } else if pos.x > self.width - r {
            pos.x = self.width - r;
            vel.x = vel.x.min(0.0);
            hit = true;
        }
        if pos.y > self.height - r {
            pos.y = self.height - r;
            vel.y = vel.y.min(0.0);
        } else if pos.y < r {
            pos.y = r;
            vel.y = vel.y.max(0.0);
            hit = true;
        }
        hit
    }

    /// Bare seafloor height at `x` (ignores rocks).
    pub fn floor_at(&self, x: f32) -> f32 {
        let i = ((x / self.sdf.cell) as usize).min(self.floor.len() - 1);
        self.floor[i]
    }

    /// Whether the terrain cell containing `pos` is rock.
    pub fn is_solid(&self, pos: Vec2) -> bool {
        self.sdf.solid[self.sdf.index_at(pos)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_has_water_above_and_rock_below() {
        let config = Config::default();
        let t = Terrain::generate(&config);
        let (w, h) = (config.world.width, config.world.height);
        assert!(
            t.distance(Vec2::new(w * 0.5, h - 50.0)) > 0.0,
            "surface is water"
        );
        assert!(t.distance(Vec2::new(w * 0.5, 10.0)) < 0.0, "bottom is rock");
        assert!(t.distance(Vec2::new(5.0, h * 0.5)) < 0.0, "left wall");
        // Shelf is shallower than the open ocean floor; trench is deeper.
        let shelf = t.floor_at(w * 0.95);
        let trench = t.floor_at(w * config.terrain.trench_center);
        let open = t.floor_at(w * 0.55);
        assert!(
            shelf > open && open > trench,
            "shelf {shelf} open {open} trench {trench}"
        );
    }

    #[test]
    fn collision_pushes_bodies_out_of_rock() {
        let config = Config::default();
        let t = Terrain::generate(&config);
        let x = config.world.width * 0.5;
        let mut pos = Vec2::new(x, t.floor_at(x) - 30.0);
        let mut vel = Vec2::new(0.0, -10.0);
        for _ in 0..20 {
            t.collide(&mut pos, &mut vel, 5.0);
        }
        assert!(t.distance(pos) >= 4.5, "distance {}", t.distance(pos));
        assert!(
            vel.length() < 10.0,
            "velocity into the rock should be removed"
        );
    }

    #[test]
    fn distance_transform_is_euclidean() {
        let d = distance_transform(5, 5, |i| i == 12);
        assert_eq!(d[12], 0.0);
        assert_eq!(d[0], 8.0);
        assert_eq!(d[13], 1.0);
    }
}

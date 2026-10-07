//! Procedural seafloor and rocks, stored as a solid mask plus a signed distance field.
//!
//! The SDF (positive in water, negative in rock) drives collisions; the mask blocks
//! light and field flow.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::config::{Config, TerrainConfig, TerrainGenerator};
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
    /// Where permanent vents sit (in the shelf cave, and on the cliff ledge).
    #[serde(default)]
    pub vent_sites: Vec<Vec2>,
    /// Where temporary vents may open: anywhere on the floor (`None`), or only at these
    /// points (painted maps).
    #[serde(default)]
    pub vent_spawns: Option<Vec<Vec2>>,
    /// How far rock stands above the water in each terrain column (painted maps; empty
    /// otherwise, where the generator's mountain is the only land).
    #[serde(default)]
    pub land: Vec<f32>,
}

/// Where a generated cave is: its x, the top of its mound, and its chamber (center and radii).
#[derive(Clone, Copy, Debug)]
struct Cave {
    x: f32,
    top: f32,
    chamber: Vec2,
    ch_w: f32,
    ch_h: f32,
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

/// The mountain past the ocean's right edge: its peak's x and half-width.
pub fn mountain(cfg: &TerrainConfig, ocean_width: f32) -> (f32, f32) {
    let half = cfg.mountain_width * 0.5;
    (ocean_width + half, half)
}

/// Height of the mountain's surface at `x` (above `height`, the sea surface, near the peak),
/// falling to the shelf on the left and the basin floor on the right, with ridges, shoulders
/// and crags so it doesn't look like a smooth curve.
fn mountain_height_at(
    cfg: &TerrainConfig,
    seed: u64,
    ocean_width: f32,
    height: f32,
    x: f32,
) -> f32 {
    let (peak_x, half) = mountain(cfg, ocean_width);
    let top = height + cfg.mountain_height;
    let foot = height
        - if x < peak_x {
            cfg.shelf_depth
        } else {
            cfg.basin_depth
        };
    // A slightly off-center, uneven peak: the shape bends with a slow noise.
    let skew = 0.18 * value_noise(seed ^ 0x3B11, 1.7);
    let t = ((x - peak_x) / half.max(1.0) - skew).abs().clamp(0.0, 1.0);
    let smooth = top - (top - foot) * t * t * (3.0 - 2.0 * t);
    // Shoulders and ridges (big, slow), then crags (small, fast), strongest mid-slope.
    let shoulders = 0.22 * fbm(seed ^ 0x3B12, x / 520.0, 3);
    let crags = 0.06 * fbm(seed ^ 0x3B13, x / 70.0, 3);
    let slope = (4.0 * t * (1.0 - t)).max(0.25);
    smooth + (top - foot) * (shoulders + crags) * slope
}

/// Bare seafloor height at `x` (the ocean's layout is relative to `width`, the ocean's width;
/// the mountain and basin lie beyond it). Can rise above `height` (dry land).
fn floor_height(cfg: &TerrainConfig, seed: u64, width: f32, height: f32, x: f32) -> f32 {
    let (peak_x, half) = mountain(cfg, width);
    if x > peak_x {
        // The basin beyond the mountain: a gently uneven floor, cut off from the ocean.
        let basin = height - cfg.basin_depth + 80.0 * fbm(seed ^ 0xBA51, x / 400.0, 3);
        return basin.max(mountain_height_at(cfg, seed, width, height, x));
    }
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
    let h = h.clamp(64.0, height - 200.0);
    if x > peak_x - half {
        h.max(mountain_height_at(cfg, seed, width, height, x))
    } else {
        h
    }
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
    /// Build the map's terrain. It depends only on the map (`[world]` size and `[terrain]`),
    /// never on the world seed, so every run on a map shares it.
    pub fn generate(config: &Config) -> Terrain {
        match (config.terrain.generator, &config.terrain.painted) {
            (TerrainGenerator::Painted, Some(map)) => Self::painted(config, map),
            _ => Self::ocean(config),
        }
    }

    /// A map painted in the editor: its rock mask (up to the water level), land above the
    /// water, permanent vents, and where temporary vents may open.
    fn painted(config: &Config, map: &crate::map::PaintedMap) -> Terrain {
        let (width, height) = (config.world.width, config.world.height);
        let field = Field::new(width, height, config.terrain.cell, 0.0);
        let (nx, ny, cell) = (field.nx, field.ny, field.cell);
        let wall = 3usize;
        let mut solid = vec![false; nx * ny];
        for y in 0..ny {
            for x in 0..nx {
                let p = Vec2::new((x as f32 + 0.5) * cell, (y as f32 + 0.5) * cell);
                solid[y * nx + x] = x < wall || x >= nx - wall || map.rock_at(p);
            }
        }
        // The bare floor: the top of the rock that rises from the bottom in each column.
        let floor = (0..nx)
            .map(|x| (0..ny).take_while(|&y| solid[y * nx + x]).count() as f32 * cell)
            .collect();
        let land_cells = map.land_heights();
        let land = (0..nx)
            .map(|x| {
                let px = (x as f32 + 0.5) * cell;
                land_cells[((px / map.cell) as usize).min(land_cells.len() - 1)]
            })
            .collect();
        let point = |v: &[f32; 2]| Vec2::new(v[0], v[1]);
        let mut terrain = Terrain {
            sdf: field,
            base_mask: solid.clone(),
            mask: solid,
            floor,
            width,
            height,
            version: 0,
            vent_sites: map.permanent_vents.iter().map(point).collect(),
            vent_spawns: Some(map.vent_spawns.iter().map(point).collect()),
            land,
        };
        terrain.recompute_sdf(0, nx);
        terrain
    }

    /// Open ocean: seafloor with a trench, rocks and caves, a shelf, and optionally a mountain
    /// and basin beyond the ocean and the permanent shelf and cliff vents.
    fn ocean(config: &Config) -> Terrain {
        let cfg = &config.terrain;
        let (width, height) = (config.world.width, config.world.height);
        // The ocean's layout is relative to its own width; the mountain and basin lie beyond.
        let ocean = config.ocean_width();
        let seed = mix64(cfg.seed ^ 0x7E44_A1B0);
        let field = Field::new(width, height, cfg.cell, 0.0);
        let (nx, ny, cell) = (field.nx, field.ny, field.cell);

        let mut floor: Vec<f32> = (0..nx)
            .map(|x| floor_height(cfg, seed, ocean, height, (x as f32 + 0.5) * cell))
            .collect();

        // The cliff vent's ledge: a small flat step partway down the cliff, with the cliff below
        // it bulging out before it curves down to the deep floor (not one even slope).
        let mut ledge = None;
        if cfg.shelf_vents {
            let ledge_y = height - cfg.cliff_vent_depth;
            // Where the cliff passes that depth, walking left from the shelf.
            let mut i = ((cfg.shelf_start * ocean / cell) as usize).min(nx - 1);
            while i > 4 && floor[i] > ledge_y {
                i -= 1;
            }
            let face = (i as f32 + 0.5) * cell;
            let (ledge_w, bulge_w) = (300.0, 650.0);
            let lseed = seed ^ 0x1ED6;
            for (x, f) in floor.iter_mut().enumerate() {
                let px = (x as f32 + 0.5) * cell;
                let from_edge = face - ledge_w - px; // > 0 below the ledge's outer edge
                if px > face || from_edge > bulge_w {
                    continue;
                }
                let bump = 12.0 * value_noise(lseed, px / 60.0);
                let shape = if from_edge <= 0.0 {
                    ledge_y + bump
                } else {
                    // Rounded: it falls away slowly at first, then steeply.
                    let t = from_edge / bulge_w;
                    ledge_y - (ledge_y - *f).max(0.0) * t * t * (1.6 - 0.6 * t) + bump
                };
                *f = f.max(shape);
            }
            ledge = Some(Vec2::new(face - ledge_w * 0.5, ledge_y));
        }

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
            let cx = rng.range(0.05, 0.95) * ocean;
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

        // Caves: a mound of rock with a hollow, dark chamber inside and a narrow entrance on
        // one side (a refuge for small bodies, a lair for ambushers). Half sit on the shelf.
        let carve = |solid: &mut Vec<bool>, shape: &dyn Fn(Vec2) -> bool, lo: Vec2, hi: Vec2| {
            let x0 = ((lo.x / cell).floor().max(0.0)) as usize;
            let y0 = ((lo.y / cell).floor().max(0.0)) as usize;
            let x1 = ((hi.x / cell).ceil() as usize).min(nx - wall);
            let y1 = ((hi.y / cell).ceil() as usize).min(ny);
            for y in y0..y1 {
                for x in x0.max(wall)..x1 {
                    let p = Vec2::new((x as f32 + 0.5) * cell, (y as f32 + 0.5) * cell);
                    if shape(p) {
                        solid[y * nx + x] = false;
                    }
                }
            }
        };
        // Build one cave at `cx` and return where its mound and chamber are.
        let build_cave = |solid: &mut Vec<bool>, cx: f32, size: f32, side: f32, nseed: u64| {
            let base = floor_at(cx);
            let (mound_w, mound_h) = (size * 1.6, size * 1.1);
            let mound = move |p: Vec2| {
                let a = (p - Vec2::new(cx, base)).to_angle();
                let wobble = 1.0 + 0.12 * value_noise(nseed, a * 4.0);
                let q = Vec2::new((p.x - cx) / mound_w, (p.y - base) / mound_h);
                q.length() < wobble && p.y > base - 60.0
            };
            stamp(
                solid,
                &mound,
                Vec2::new(cx - mound_w * 1.2, base - 60.0),
                Vec2::new(cx + mound_w * 1.2, base + mound_h * 1.2),
            );
            let chamber = Vec2::new(cx, base + mound_h * 0.4);
            let (ch_w, ch_h) = (size * 0.9, size * 0.35);
            let tunnel_y = chamber.y - ch_h * 0.3;
            let tunnel_half = (size * 0.08).max(cell * 1.5);
            let hollow = move |p: Vec2| {
                let wobble = 1.0 + 0.15 * value_noise(nseed ^ 0x55, p.x / 60.0);
                let q = (p - chamber) / Vec2::new(ch_w, ch_h * wobble);
                let tunnel = (p.x - cx) * side > 0.0
                    && (p.y - tunnel_y).abs()
                        < tunnel_half * (1.0 + 0.3 * value_noise(nseed ^ 0x77, p.x / 40.0));
                q.length() < 1.0 || tunnel
            };
            carve(
                solid,
                &hollow,
                Vec2::new(cx - mound_w * 1.3, chamber.y - ch_h * 1.3),
                Vec2::new(cx + mound_w * 1.3, chamber.y + ch_h * 1.3),
            );
            Cave {
                x: cx,
                top: base + mound_h * 1.15,
                chamber,
                ch_w,
                ch_h,
            }
        };
        let shelf_x = cfg.shelf_start * ocean;
        let mut caves = Vec::new();
        for k in 0..cfg.caves {
            let size = cfg.cave_size * rng.range(0.7, 1.3);
            let cx = if k % 2 == 0 {
                rng.range(shelf_x + 400.0, ocean * 0.97 - size)
            } else {
                rng.range(0.06, 0.9) * ocean
            };
            let nseed = seed.wrapping_add(0xCA7E + k as u64 * 7_919);
            let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
            caves.push(build_cave(&mut solid, cx, size, side, nseed));
        }

        // Permanent vents: one inside a cave on the shelf near the cliff's edge, with the cave's
        // roof opened so the vent sees open water, and one partway down the cliff. Winding
        // tunnels branch up from the cliff vent into that cave and the little cave nearest the
        // cliff.
        let mut vent_sites = Vec::new();
        if cfg.shelf_vents {
            let want = cfg.vent_cave_x * ocean;
            let nearest = caves
                .iter()
                .filter(|c| c.x > shelf_x)
                .min_by(|a, b| (a.x - want).abs().total_cmp(&(b.x - want).abs()))
                .copied();
            let vent_cave = match nearest {
                Some(c) if (c.x - want).abs() < 1000.0 => c,
                // No cave there in this world: make one.
                _ => build_cave(&mut solid, want, cfg.cave_size, 1.0, seed ^ 0xCAFE),
            };
            // The vent on the cave's floor, under a wide opening in the roof.
            let vent = Vec2::new(
                vent_cave.chamber.x,
                vent_cave.chamber.y - vent_cave.ch_h * 0.55,
            );
            let rseed = seed ^ 0x2007;
            // A pocket of water around the vent (a later cave's mound may have filled part of
            // the chamber), and a funnel widening up through the roof to the water above.
            // It goes on up well past the cave's top, so no slivers of rock are left hanging
            // over it, but stops widening so it doesn't cut into neighboring caves.
            let funnel_top = vent_cave.top + 400.0;
            let max_half = 260.0;
            let roof = move |p: Vec2| {
                let wobble = 1.0 + 0.25 * value_noise(rseed, p.y / 50.0 + p.x / 200.0);
                let rise = (p.y - vent.y).max(0.0);
                let half = (55.0 + 0.75 * rise).min(max_half);
                p.distance(vent) < 60.0 || (p.x - vent.x).abs() < half * wobble && p.y > vent.y
            };
            let spread = max_half * 1.25;
            carve(
                &mut solid,
                &roof,
                Vec2::new(vent.x - spread, vent.y - 70.0),
                Vec2::new(vent.x + spread, funnel_top),
            );
            // A warm ledge beside the vent: a flat platform out of the plume, a little above the
            // vent, where the water is hot (about 43 °C at its near end, 26 °C at its far end)
            // and rich in minerals seeping out of the vent, joined to the vent by a short
            // passage. Level, so nothing on it slides into the vent.
            let (near, far) = (vent.x + 130.0, vent.x + 270.0);
            let ledge_top = vent.y + 40.0;
            let lseed = seed ^ 0x1ED7;
            let platform = move |p: Vec2| {
                let lip = 6.0 * value_noise(lseed, p.x / 40.0);
                p.x > near && p.x < far && p.y < ledge_top + lip && p.y > ledge_top - 220.0
            };
            stamp(
                &mut solid,
                &platform,
                Vec2::new(near, ledge_top - 230.0),
                Vec2::new(far, ledge_top + 10.0),
            );
            let alcove = move |p: Vec2| {
                let wobble = 1.0 + 0.2 * value_noise(lseed ^ 1, p.x / 50.0);
                let above = p.x > near - 10.0
                    && p.x < far + 10.0
                    && p.y > ledge_top + 8.0
                    && p.y < ledge_top + 95.0 * wobble;
                let passage = p.x > vent.x + 30.0
                    && p.x < near
                    && p.y > vent.y - 10.0
                    && p.y < ledge_top + 70.0;
                above || passage
            };
            carve(
                &mut solid,
                &alcove,
                Vec2::new(vent.x + 20.0, vent.y - 20.0),
                Vec2::new(far + 20.0, ledge_top + 120.0),
            );
            // The little cave nearest the cliff, if there's another one on the shelf.
            let little = caves
                .iter()
                .filter(|c| c.x > shelf_x - 300.0 && c.x < vent_cave.x - 200.0)
                .min_by(|a, b| a.x.total_cmp(&b.x))
                .copied();
            // A short passage joining the vent cave to the little cave on its left.
            if let Some(l) = little {
                let (a, b) = (
                    l.chamber + Vec2::new(l.ch_w * 0.7, 0.0),
                    vent_cave.chamber - Vec2::new(vent_cave.ch_w * 0.7, 0.0),
                );
                let cseed = seed ^ 0xC0EC;
                let link = move |p: Vec2| {
                    let ab = b - a;
                    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
                    let along = a + ab * t;
                    let on = along + Vec2::Y * 25.0 * value_noise(cseed, along.x / 90.0);
                    p.distance(on) < 20.0
                };
                carve(
                    &mut solid,
                    &link,
                    a.min(b) - Vec2::splat(80.0),
                    a.max(b) + Vec2::splat(80.0),
                );
            }
            // The cliff vent sits on its ledge.
            let ledge = ledge.unwrap_or(Vec2::new(shelf_x, height - cfg.cliff_vent_depth));
            let cliff_vent = Vec2::new(ledge.x, floor_at(ledge.x) + 12.0);
            let niche = cliff_vent + Vec2::new(0.0, 30.0);
            // Tunnels stay in the rock: up from the cliff to below the shelf floor, along under
            // a cave, then up into its chamber. Two into each cave.
            let mut ends = Vec::new();
            for cave in little.iter().chain(std::iter::once(&vent_cave)) {
                for f in [-0.45, 0.35] {
                    ends.push((
                        cave.chamber + Vec2::new(f * cave.ch_w, -0.6 * cave.ch_h),
                        cave.x,
                    ));
                }
            }
            let tunnels: Vec<[Vec2; 4]> = ends
                .iter()
                .enumerate()
                .map(|(k, &(end, cave_x))| {
                    let k = k as f32;
                    let below = floor_at(cave_x).min(floor_at(end.x)) - 220.0 - 25.0 * k;
                    let start = niche + Vec2::new(30.0, 25.0 + 45.0 * k);
                    let a = Vec2::new(end.x - 160.0 + 30.0 * k, below);
                    let b = Vec2::new(end.x + 40.0 * (k % 2.0 - 0.5), below + 40.0);
                    [start, a, b, end]
                })
                .collect();
            let tseed = seed ^ 0x7077;
            let passages = move |p: Vec2| {
                if p.distance(niche) < 60.0 {
                    return true;
                }
                tunnels.iter().enumerate().any(|(k, path)| {
                    path.windows(2).any(|w| {
                        let (a, b) = (w[0], w[1]);
                        let ab = b - a;
                        let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
                        let along = a + ab * t;
                        let wiggle =
                            30.0 * value_noise(tseed ^ k as u64, (along.x + along.y) / 120.0);
                        let on = along + ab.perp().normalize_or_zero() * wiggle;
                        p.distance(on) < 16.0 + 4.0 * value_noise(tseed ^ 0x99, along.x / 50.0)
                    })
                })
            };
            let right = vent_cave.chamber.x + vent_cave.ch_w + 100.0;
            let lo = Vec2::new(cliff_vent.x - 100.0, cliff_vent.y - 100.0);
            let hi = Vec2::new(right, vent_cave.chamber.y + 100.0);
            carve(&mut solid, &passages, lo, hi);
            vent_sites.push(vent);
            vent_sites.push(cliff_vent);
        }

        let mut terrain = Terrain {
            sdf: field,
            base_mask: solid.clone(),
            mask: solid,
            floor,
            width,
            height,
            version: 0,
            vent_sites,
            vent_spawns: None,
            land: Vec::new(),
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
    fn the_terrain_is_the_same_for_every_world_seed() {
        let mut config = Config::default();
        let a = Terrain::generate(&config);
        config.world.seed = 2;
        let b = Terrain::generate(&config);
        assert_eq!(a.sdf.data, b.sdf.data);
        assert_eq!(a.vent_sites, b.vent_sites);
        config.terrain.seed = 2;
        assert_ne!(
            Terrain::generate(&config).sdf.data,
            a.sdf.data,
            "the map seed still matters"
        );
    }

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
        let ocean = config.ocean_width();
        let shelf = t.floor_at(ocean * 0.95);
        let trench = t.floor_at(ocean * config.terrain.trench_center);
        let open = t.floor_at(ocean * 0.55);
        assert!(
            shelf > open && open > trench,
            "shelf {shelf} open {open} trench {trench}"
        );
    }

    #[test]
    fn a_mountain_cuts_off_a_basin_and_vents_sit_in_open_water() {
        for seed in [1, 2, 3, 7, 42] {
            let mut config = Config::default();
            config.world.seed = seed;
            let t = Terrain::generate(&config);
            let (w, h) = (config.world.width, config.world.height);
            let (peak_x, _) = mountain(&config.terrain, config.ocean_width());
            // The mountain reaches the surface; water lies on both sides of it.
            assert!(
                t.is_solid(Vec2::new(peak_x, h - 4.0)),
                "peak is above water"
            );
            assert!(
                !t.is_solid(Vec2::new(peak_x - 1500.0, h - 100.0)),
                "ocean side"
            );
            assert!(!t.is_solid(Vec2::new(w - 600.0, h - 100.0)), "basin side");
            assert!(t.floor_at(w - 600.0) < h - 800.0, "the basin is deep");
            // Two permanent vents in water: one near the surface, one down the cliff.
            assert_eq!(t.vent_sites.len(), 2);
            for &v in &t.vent_sites {
                assert!(
                    t.distance(v) > 2.0,
                    "vent at {v:?} is in water (seed {seed})"
                );
            }
            let (shelf, cliff) = (t.vent_sites[0], t.vent_sites[1]);
            let depth = h - shelf.y;
            assert!(
                (300.0..1300.0).contains(&depth),
                "shelf vent at depth {depth}"
            );
            assert!(h - cliff.y > 1400.0, "cliff vent at depth {}", h - cliff.y);
            let ocean = config.ocean_width();
            assert!(
                (shelf.x - config.terrain.vent_cave_x * ocean).abs() < 1000.0,
                "the shelf vent is in the cave near the cliff's edge"
            );
            // Open water straight up from the vent, out through the cave's roof.
            let mut p = shelf + Vec2::new(0.0, 30.0);
            while p.y < h - 400.0 && p.y < shelf.y + 600.0 {
                assert!(!t.is_solid(p), "covered at {p:?} (seed {seed})");
                p.y += 8.0;
            }
        }
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

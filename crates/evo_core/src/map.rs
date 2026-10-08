//! Painted maps, made in the map editor: a rock mask painted cell by cell, a water level, and
//! vent nodes. Saved as `configs/maps/<name>.map` and used like any other map.
//!
//! The canvas can be taller than the water: rock painted above the water level stands out of
//! the sea as land (drawn against the sky), while the world itself is only as tall as the water.

use glam::Vec2;
use serde::{Deserialize, Serialize};
use std::path::Path;

const MAGIC: &[u8] = b"EVOMAP1\n";

/// The editor's grid, in world units per cell (the same as the terrain's).
pub const CELL: f32 = 8.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaintedMap {
    #[serde(default)]
    pub description: String,
    /// Canvas size in world units (the height includes any land above the water).
    pub width: f32,
    pub height: f32,
    /// The water surface's height above the bottom: the world's height.
    pub water_level: f32,
    pub cell: f32,
    pub nx: usize,
    pub ny: usize,
    /// One bit per cell, row by row from the bottom: set = rock.
    bits: Vec<u64>,
    /// One bit per cell, as `bits`: set = cave (open water, dark, rich in detritus).
    #[serde(default)]
    cave: Vec<u64>,
    /// Vents that never die out.
    #[serde(default)]
    pub permanent_vents: Vec<[f32; 2]>,
    /// Where temporary vents may open (and only there).
    #[serde(default)]
    pub vent_spawns: Vec<[f32; 2]>,
    /// Named areas where a new world's first organisms can start (offered on the New world
    /// screen, the first one picked by default).
    #[serde(default)]
    pub spawn_areas: Vec<SpawnArea>,
}

/// A named box (world units, `min` = bottom left) where life can start.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpawnArea {
    pub name: String,
    pub min: [f32; 2],
    pub max: [f32; 2],
}

impl SpawnArea {
    /// Its span across the map (fractions of `width`) and in depth below the surface
    /// `water_level` (shallow first), clipped to the water.
    pub fn in_water(&self, width: f32, water_level: f32) -> ([f32; 2], [f32; 2]) {
        let x = [
            (self.min[0] / width).clamp(0.0, 1.0),
            (self.max[0] / width).clamp(0.0, 1.0),
        ];
        let depth = [
            (water_level - self.max[1]).clamp(0.0, water_level),
            (water_level - self.min[1]).clamp(0.0, water_level),
        ];
        (x, depth)
    }
}

impl PaintedMap {
    /// An empty canvas: all water (and air above the water level).
    pub fn new(width: f32, height: f32, water_level: f32) -> PaintedMap {
        let cell = CELL;
        let nx = (width / cell).ceil().max(1.0) as usize;
        let ny = (height / cell).ceil().max(1.0) as usize;
        PaintedMap {
            description: String::new(),
            width: nx as f32 * cell,
            height: ny as f32 * cell,
            water_level: water_level.clamp(cell, ny as f32 * cell),
            cell,
            nx,
            ny,
            bits: vec![0; (nx * ny).div_ceil(64)],
            cave: vec![0; (nx * ny).div_ceil(64)],
            permanent_vents: Vec::new(),
            vent_spawns: Vec::new(),
            spawn_areas: Vec::new(),
        }
    }

    /// A canvas with a flat seafloor `floor` high.
    pub fn flat(width: f32, height: f32, water_level: f32, floor: f32) -> PaintedMap {
        let mut map = PaintedMap::new(width, height, water_level);
        let rows = ((floor / map.cell).round() as usize).min(map.ny);
        for y in 0..rows {
            for x in 0..map.nx {
                map.set(x, y, true);
            }
        }
        map
    }

    /// A copy of a generated map's terrain (and its permanent vents), to start from.
    pub fn from_config(config: &crate::Config) -> PaintedMap {
        let terrain = crate::terrain::Terrain::generate(config);
        let (w, h) = (config.world.width, config.world.height);
        let mut map = PaintedMap::new(w, h, h);
        for y in 0..map.ny {
            for x in 0..map.nx {
                let p = map.cell_center(x, y);
                if terrain.is_solid(p) {
                    map.set(x, y, true);
                }
            }
        }
        map.permanent_vents = terrain.vent_sites.iter().map(|v| [v.x, v.y]).collect();
        map
    }

    #[inline]
    fn index(&self, x: usize, y: usize) -> usize {
        y * self.nx + x
    }

    pub fn get(&self, x: usize, y: usize) -> bool {
        if x >= self.nx || y >= self.ny {
            return false;
        }
        let i = self.index(x, y);
        self.bits[i / 64] >> (i % 64) & 1 == 1
    }

    /// Make a cell rock (`true`) or open water (`false`, which also clears any cave).
    pub fn set(&mut self, x: usize, y: usize, rock: bool) {
        if x >= self.nx || y >= self.ny {
            return;
        }
        let i = self.index(x, y);
        if rock {
            self.bits[i / 64] |= 1 << (i % 64);
        } else {
            self.bits[i / 64] &= !(1 << (i % 64));
        }
        self.cave[i / 64] &= !(1 << (i % 64));
    }

    pub fn is_cave(&self, x: usize, y: usize) -> bool {
        if x >= self.nx || y >= self.ny {
            return false;
        }
        let i = self.index(x, y);
        self.cave[i / 64] >> (i % 64) & 1 == 1
    }

    /// Make a cell cave: open water (no rock), dark and rich in detritus.
    pub fn set_cave(&mut self, x: usize, y: usize) {
        if x >= self.nx || y >= self.ny {
            return;
        }
        let i = self.index(x, y);
        self.bits[i / 64] &= !(1 << (i % 64));
        self.cave[i / 64] |= 1 << (i % 64);
    }

    /// Cave at world position `p`.
    pub fn cave_at(&self, p: Vec2) -> bool {
        if p.x < 0.0 || p.y < 0.0 {
            return false;
        }
        self.is_cave((p.x / self.cell) as usize, (p.y / self.cell) as usize)
    }

    /// Rock at world position `p` (outside the canvas: none).
    pub fn rock_at(&self, p: Vec2) -> bool {
        if p.x < 0.0 || p.y < 0.0 {
            return false;
        }
        self.get((p.x / self.cell) as usize, (p.y / self.cell) as usize)
    }

    pub fn cell_center(&self, x: usize, y: usize) -> Vec2 {
        Vec2::new((x as f32 + 0.5) * self.cell, (y as f32 + 0.5) * self.cell)
    }

    /// Change the canvas size, keeping what's painted where it was (from the bottom left).
    /// New space is water; vents outside the new canvas are dropped.
    pub fn resize(&mut self, width: f32, height: f32) {
        let mut next = PaintedMap::new(width, height, self.water_level);
        for y in 0..self.ny.min(next.ny) {
            for x in 0..self.nx.min(next.nx) {
                if self.get(x, y) {
                    next.set(x, y, true);
                } else if self.is_cave(x, y) {
                    next.set_cave(x, y);
                }
            }
        }
        let inside = |v: &[f32; 2]| v[0] < next.width && v[1] < next.height;
        next.description = std::mem::take(&mut self.description);
        next.permanent_vents = self.permanent_vents.iter().copied().filter(inside).collect();
        next.vent_spawns = self.vent_spawns.iter().copied().filter(inside).collect();
        // Start areas are clipped to the new canvas (dropped if nothing's left).
        next.spawn_areas = self
            .spawn_areas
            .iter()
            .filter_map(|a| {
                let max = [a.max[0].min(next.width), a.max[1].min(next.height)];
                (max[0] > a.min[0] + 1.0 && max[1] > a.min[1] + 1.0)
                    .then(|| SpawnArea { name: a.name.clone(), min: a.min, max })
            })
            .collect();
        *self = next;
    }

    /// For each column, how far rock rises above the water (0 where none does).
    pub fn land_heights(&self) -> Vec<f32> {
        let first_air = (self.water_level / self.cell).round() as usize;
        (0..self.nx)
            .map(|x| {
                (first_air..self.ny)
                    .rev()
                    .find(|&y| self.get(x, y))
                    .map_or(0.0, |y| (y + 1) as f32 * self.cell - self.water_level)
            })
            .collect()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let packed = rmp_serde::to_vec_named(self).map_err(|e| e.to_string())?;
        let mut bytes = MAGIC.to_vec();
        bytes.extend(lz4_flex::compress_prepend_size(&packed));
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    pub fn load(path: &Path) -> Result<PaintedMap, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_bytes(&bytes).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// A map from the contents of a `.map` file.
    pub fn from_bytes(bytes: &[u8]) -> Result<PaintedMap, String> {
        let body = bytes.strip_prefix(MAGIC).ok_or("not a map file")?;
        let packed = lz4_flex::decompress_size_prepended(body).map_err(|e| e.to_string())?;
        let mut map: PaintedMap = rmp_serde::from_slice(&packed).map_err(|e| e.to_string())?;
        let words = (map.nx * map.ny).div_ceil(64);
        // Maps saved before caves existed have none.
        if map.cave.is_empty() {
            map.cave = vec![0; words];
        }
        if map.bits.len() != words || map.cave.len() != words {
            return Err("damaged map".into());
        }
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_resize_and_round_trip() {
        let mut m = PaintedMap::flat(4000.0, 3000.0, 2400.0, 400.0);
        assert!(m.get(10, 0) && m.get(10, 49) && !m.get(10, 50));
        // A peak standing 80 units out of the water.
        let x = 100;
        for y in 0..((2480.0 / CELL) as usize) {
            m.set(x, y, true);
        }
        let land = m.land_heights();
        assert_eq!(land[x], 80.0);
        assert_eq!(land[x + 1], 0.0);
        m.vent_spawns.push([3900.0, 500.0]);
        m.set_cave(20, 100);
        assert!(m.is_cave(20, 100) && !m.get(20, 100));
        m.set(20, 100, true);
        assert!(!m.is_cave(20, 100));
        m.set_cave(20, 100);
        // A start area half off the new canvas is clipped; one wholly off it is dropped.
        m.spawn_areas.push(SpawnArea { name: "a".into(), min: [2000.0, 1000.0], max: [3600.0, 2600.0] });
        m.spawn_areas.push(SpawnArea { name: "b".into(), min: [3200.0, 100.0], max: [3800.0, 300.0] });
        // In the water: x as fractions, depths below the surface (shallow first), clipped.
        let (x, d) = m.spawn_areas[0].in_water(4000.0, 2400.0);
        assert_eq!((x, d), ([0.5, 0.9], [0.0, 1400.0]));
        m.resize(3000.0, 3000.0);
        assert_eq!(m.spawn_areas.len(), 1);
        assert_eq!(m.spawn_areas[0].max, [3000.0, 2600.0]);
        assert!(m.get(10, 0) && m.vent_spawns.is_empty() && m.is_cave(20, 100));
        let dir = std::env::temp_dir().join(format!("evo_map_{}", std::process::id()));
        let path = dir.join("t.map");
        m.save(&path).unwrap();
        let back = PaintedMap::load(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(back, m);
    }
}

#[cfg(test)]
mod world_tests {
    use super::*;
    use crate::{Config, World};

    /// A painted map runs as a world: its size, rock, land above water and vent nodes.
    #[test]
    fn a_painted_map_makes_a_world() {
        let mut m = PaintedMap::flat(6000.0, 3400.0, 3000.0, 600.0);
        // An island poking out of the water.
        for y in 0..((3200.0 / CELL) as usize) {
            for x in 300..340 {
                m.set(x, y, true);
            }
        }
        m.permanent_vents.push([1000.0, 620.0]);
        m.vent_spawns.push([4000.0, 620.0]);
        m.vent_spawns.push([5000.0, 620.0]);
        let dir = std::env::temp_dir().join(format!("evo_painted_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        m.save(&dir.join("maps/isle.map")).unwrap();
        std::fs::write(dir.join("c.toml"), "[world]\nmap = \"isle\"\n").unwrap();
        let mut config = Config::load(&dir.join("c.toml")).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(config.world.width, 6000.0);
        assert_eq!(config.world.height, 3000.0);
        assert_eq!(config.ocean_width(), 6000.0);
        config.population.initial = 10;
        config.vents.count = 2;
        let mut world = World::new(config);
        assert!(world.terrain.is_solid(glam::Vec2::new(2600.0, 2900.0)));
        assert!(!world.terrain.is_solid(glam::Vec2::new(2000.0, 2900.0)));
        assert!(world.terrain.land[(2600.0 / 8.0) as usize] > 150.0);
        // The permanent vent, and temporary ones only at the spawn nodes.
        let vents = &world.env.vents;
        assert!(vents.iter().any(|v| v.fixed && v.pos == glam::Vec2::new(1000.0, 620.0)));
        for v in vents.iter().filter(|v| !v.fixed) {
            assert!(v.pos.x == 4000.0 || v.pos.x == 5000.0, "{:?}", v.pos);
        }
        for _ in 0..30 {
            world.step();
        }
    }

    /// Caves are open water, dark, and fill with detritus and its nutrients.
    #[test]
    fn caves_are_dark_and_rich() {
        let mut m = PaintedMap::flat(6000.0, 3000.0, 3000.0, 400.0);
        // A cave chamber 1000 units across, in the open water above the floor.
        for y in 70..170 {
            for x in 300..425 {
                m.set_cave(x, y);
            }
        }
        let dir = std::env::temp_dir().join(format!("evo_cave_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        m.save(&dir.join("maps/cave.map")).unwrap();
        std::fs::write(dir.join("c.toml"), "[world]\nmap = \"cave\"\n").unwrap();
        let mut config = Config::load(&dir.join("c.toml")).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        config.population.initial = 0;
        config.vents.count = 0;
        let mut world = World::new(config);
        let inside = glam::Vec2::new(2900.0, 950.0);
        let outside = glam::Vec2::new(1000.0, 950.0);
        assert!(world.terrain.is_cave(inside) && !world.terrain.is_cave(outside));
        assert!(!world.terrain.is_solid(inside));
        for _ in 0..900 {
            world.step();
        }
        assert_eq!(world.env.light.level_at(inside), 0.0);
        assert!(world.env.light.level_at(outside) > 0.0);
        let d_in = world.env.detritus.sample(inside);
        let d_out = world.env.detritus.sample(outside);
        assert!(d_in > d_out * 10.0 && d_in > 1e-5, "{d_in} vs {d_out}");
        assert!(world.env.snow_nutrients.sample(inside) > world.env.snow_nutrients.sample(outside));
    }
}

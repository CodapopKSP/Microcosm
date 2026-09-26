//! Multi-level uniform-grid spatial index, rebuilt every tick with a counting sort.
//!
//! Organism sizes span two orders of magnitude, so a single cell size would be
//! either too coarse for microbes or too fine for giants. Each body is stored in
//! the finest level whose cells are at least its diameter; queries visit every
//! level with the search box expanded by that level's largest possible radius.
//! Entries are stored sorted by (level, row, column), so each row of a query box
//! is one contiguous slice.

use glam::Vec2;

/// Per-body data copied into the grid for cache-friendly queries.
#[derive(Clone, Copy, Debug, Default)]
pub struct Entry {
    /// Index into the source array (organisms or carcasses).
    pub index: u32,
    pub pos: Vec2,
    pub radius: f32,
    pub hue: u8,
    pub saturation: u8,
    /// Light absorptance (organisms only; used for the light deposit).
    pub absorptance: f32,
    /// Half-width that catches light (body plus fronds).
    pub light_radius: f32,
    /// Lure glow (0 = none).
    pub lure: f32,
}

#[derive(Clone, Debug)]
struct Level {
    cell: f32,
    inv_cell: f32,
    nx: usize,
    ny: usize,
    /// `starts[c]..starts[c + 1]` is the range of entries in cell `c` (relative to `base`).
    starts: Vec<u32>,
    /// Offset of this level's entries in `SpatialGrid::entries`.
    base: usize,
    count: usize,
    /// Largest radius actually present at this level (tightens query boxes).
    observed_max: f32,
}

impl Level {
    #[inline]
    fn cell_coords(&self, pos: Vec2) -> (usize, usize) {
        let x = ((pos.x * self.inv_cell) as isize).clamp(0, self.nx as isize - 1) as usize;
        let y = ((pos.y * self.inv_cell) as isize).clamp(0, self.ny as isize - 1) as usize;
        (x, y)
    }

    /// Largest radius stored at this level.
    #[inline]
    fn max_radius(&self) -> f32 {
        self.cell * 0.5
    }
}

#[inline]
fn level_for_radius(levels: &[Level], radius: f32) -> usize {
    for (i, l) in levels.iter().enumerate() {
        if radius <= l.max_radius() {
            return i;
        }
    }
    levels.len() - 1
}

#[derive(Clone, Debug)]
pub struct SpatialGrid {
    levels: Vec<Level>,
    entries: Vec<Entry>,
    /// Scratch: (level, cell) of each input item.
    slots: Vec<(u8, u32)>,
}

impl SpatialGrid {
    /// `base_cell` is the finest cell size; each level is `factor` times coarser.
    pub fn new(width: f32, height: f32, base_cell: f32, factor: f32, levels: usize) -> Self {
        let levels = (0..levels)
            .map(|i| {
                let cell = base_cell * factor.powi(i as i32);
                let nx = (width / cell).ceil().max(1.0) as usize;
                let ny = (height / cell).ceil().max(1.0) as usize;
                Level {
                    cell,
                    inv_cell: 1.0 / cell,
                    nx,
                    ny,
                    starts: vec![0; nx * ny + 1],
                    base: 0,
                    count: 0,
                    observed_max: 0.0,
                }
            })
            .collect();
        Self {
            levels,
            entries: Vec::new(),
            slots: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Rebuild from scratch from entries in index order.
    pub fn rebuild(&mut self, items: &[Entry]) {
        let n = items.len();
        self.slots.clear();
        self.slots.reserve(n);
        for level in &mut self.levels {
            level.starts.fill(0);
            level.count = 0;
            level.observed_max = 0.0;
        }

        // Count per cell (stored shifted by one for the prefix sum).
        for e in items {
            let li = level_for_radius(&self.levels, e.radius);
            let level = &mut self.levels[li];
            let (x, y) = level.cell_coords(e.pos);
            let c = y * level.nx + x;
            level.starts[c + 1] += 1;
            level.count += 1;
            level.observed_max = level.observed_max.max(e.radius);
            self.slots.push((li as u8, c as u32));
        }

        let mut base = 0;
        for level in &mut self.levels {
            level.base = base;
            let mut acc = 0u32;
            for v in level.starts.iter_mut() {
                acc += *v;
                *v = acc;
            }
            base += level.count;
        }

        // Scatter using `starts` as cursors, then shift back (memmove) to restore offsets.
        self.entries.clear();
        self.entries.resize(n, Entry::default());
        for (e, &(li, c)) in items.iter().zip(self.slots.iter()) {
            let level = &mut self.levels[li as usize];
            let slot = level.starts[c as usize];
            level.starts[c as usize] += 1;
            self.entries[level.base + slot as usize] = *e;
        }
        for level in &mut self.levels {
            let len = level.starts.len();
            level.starts.copy_within(0..len - 1, 1);
            level.starts[0] = 0;
        }
    }

    /// Visit every entry that might overlap the circle (`pos`, `radius`), skipping levels whose
    /// bodies are all smaller than `min_body_radius`. The callback must do its own exact check.
    /// Return `false` from the callback to stop early.
    #[inline]
    pub fn query(
        &self,
        pos: Vec2,
        radius: f32,
        min_body_radius: f32,
        mut f: impl FnMut(&Entry) -> bool,
    ) {
        for level in &self.levels {
            if level.count == 0 || level.observed_max < min_body_radius {
                continue;
            }
            let reach = radius + level.observed_max;
            let (x0, y0) = level.cell_coords(pos - Vec2::splat(reach));
            let (x1, y1) = level.cell_coords(pos + Vec2::splat(reach));
            for y in y0..=y1 {
                let row = y * level.nx;
                let start = level.starts[row + x0] as usize;
                let end = level.starts[row + x1 + 1] as usize;
                for e in &self.entries[level.base + start..level.base + end] {
                    if !f(e) {
                        return;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(index: u32, x: f32, y: f32, r: f32) -> Entry {
        Entry {
            index,
            pos: Vec2::new(x, y),
            radius: r,
            ..Default::default()
        }
    }

    fn overlapping(grid: &SpatialGrid, pos: Vec2, r: f32) -> Vec<u32> {
        let mut out = Vec::new();
        grid.query(pos, r, 0.0, |e| {
            if e.pos.distance(pos) < e.radius + r {
                out.push(e.index);
            }
            true
        });
        out.sort();
        out
    }

    #[test]
    fn finds_bodies_of_all_sizes() {
        let items = vec![
            entry(0, 100.0, 100.0, 2.0),
            entry(1, 110.0, 100.0, 3.0),
            entry(2, 500.0, 500.0, 200.0),
            entry(3, 900.0, 100.0, 40.0),
        ];
        let mut grid = SpatialGrid::new(2000.0, 1000.0, 16.0, 4.0, 4);
        grid.rebuild(&items);
        assert_eq!(grid.len(), 4);
        assert_eq!(overlapping(&grid, Vec2::new(105.0, 100.0), 4.0), vec![0, 1]);
        assert!(overlapping(&grid, Vec2::new(105.0, 100.0), 1.0).is_empty());
        // Deep inside the big body, far from its center cell.
        assert_eq!(overlapping(&grid, Vec2::new(650.0, 600.0), 1.0), vec![2]);
        assert_eq!(overlapping(&grid, Vec2::new(870.0, 100.0), 5.0), vec![3]);
        assert!(overlapping(&grid, Vec2::new(1500.0, 900.0), 5.0).is_empty());
    }

    #[test]
    fn matches_brute_force() {
        let mut rng = crate::rng::Rng::new(3);
        let items: Vec<Entry> = (0..2000)
            .map(|i| {
                let r = if i % 50 == 0 {
                    rng.range(20.0, 300.0)
                } else {
                    rng.range(1.0, 8.0)
                };
                entry(i, rng.range(0.0, 4000.0), rng.range(0.0, 2000.0), r)
            })
            .collect();
        let mut grid = SpatialGrid::new(4000.0, 2000.0, 16.0, 4.0, 4);
        grid.rebuild(&items);
        for _ in 0..200 {
            let p = Vec2::new(rng.range(0.0, 4000.0), rng.range(0.0, 2000.0));
            let r = rng.range(1.0, 150.0);
            let mut expected: Vec<u32> = items
                .iter()
                .filter(|e| e.pos.distance(p) < e.radius + r)
                .map(|e| e.index)
                .collect();
            expected.sort();
            assert_eq!(overlapping(&grid, p, r), expected);
        }
    }

    #[test]
    fn min_body_radius_skips_fine_levels() {
        let items = vec![entry(0, 100.0, 100.0, 2.0), entry(1, 120.0, 100.0, 60.0)];
        let mut grid = SpatialGrid::new(1000.0, 1000.0, 16.0, 4.0, 4);
        grid.rebuild(&items);
        let mut seen = Vec::new();
        grid.query(Vec2::new(100.0, 100.0), 50.0, 20.0, |e| {
            seen.push(e.index);
            true
        });
        assert_eq!(seen, vec![1]);
    }
}

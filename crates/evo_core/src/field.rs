//! Scalar fields on regular grids (minerals, detritus, temperature).

use glam::Vec2;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// Row-major grid of `f32`, cell `(x, y)` covers `[x·cell, (x+1)·cell) × [y·cell, (y+1)·cell)`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Field {
    pub nx: usize,
    pub ny: usize,
    pub cell: f32,
    pub data: Vec<f32>,
    /// Cells that are solid rock (no content, no flux). Same layout as `data`.
    pub solid: Vec<bool>,
    #[serde(skip)]
    scratch: Vec<f32>,
}

impl Field {
    pub fn new(width: f32, height: f32, cell: f32, value: f32) -> Self {
        let nx = (width / cell).ceil().max(1.0) as usize;
        let ny = (height / cell).ceil().max(1.0) as usize;
        Self {
            nx,
            ny,
            cell,
            data: vec![value; nx * ny],
            solid: vec![false; nx * ny],
            scratch: Vec::new(),
        }
    }

    #[inline]
    pub fn index(&self, x: usize, y: usize) -> usize {
        y * self.nx + x
    }

    #[inline]
    pub fn coords(&self, pos: Vec2) -> (usize, usize) {
        let x = ((pos.x / self.cell) as isize).clamp(0, self.nx as isize - 1) as usize;
        let y = ((pos.y / self.cell) as isize).clamp(0, self.ny as isize - 1) as usize;
        (x, y)
    }

    #[inline]
    pub fn index_at(&self, pos: Vec2) -> usize {
        let (x, y) = self.coords(pos);
        self.index(x, y)
    }

    /// Index of the cell at `pos`, or of a neighboring water cell if that one is rock
    /// (bodies resting on rock sit partly inside rock cells). Prefers the cell above.
    pub fn water_index_at(&self, pos: Vec2) -> usize {
        let (x, y) = self.coords(pos);
        let i = self.index(x, y);
        if !self.solid[i] {
            return i;
        }
        for (dx, dy) in [
            (0, 1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (1, 1),
            (0, -1),
            (-1, -1),
            (1, -1),
        ] {
            let (nx, ny) = (x as isize + dx, y as isize + dy);
            if nx >= 0 && ny >= 0 && (nx as usize) < self.nx && (ny as usize) < self.ny {
                let j = self.index(nx as usize, ny as usize);
                if !self.solid[j] {
                    return j;
                }
            }
        }
        i
    }

    /// Nearest-cell value.
    #[inline]
    pub fn get(&self, pos: Vec2) -> f32 {
        self.data[self.index_at(pos)]
    }

    /// Bilinear sample between cell centers.
    pub fn sample(&self, pos: Vec2) -> f32 {
        let fx = (pos.x / self.cell - 0.5).clamp(0.0, (self.nx - 1) as f32);
        let fy = (pos.y / self.cell - 0.5).clamp(0.0, (self.ny - 1) as f32);
        let x0 = fx.floor() as usize;
        let y0 = fy.floor() as usize;
        let x1 = (x0 + 1).min(self.nx - 1);
        let y1 = (y0 + 1).min(self.ny - 1);
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;
        let a = self.data[self.index(x0, y0)];
        let b = self.data[self.index(x1, y0)];
        let c = self.data[self.index(x0, y1)];
        let d = self.data[self.index(x1, y1)];
        let top = a + (b - a) * tx;
        let bottom = c + (d - c) * tx;
        top + (bottom - top) * ty
    }

    /// Central-difference gradient over `distance` (per unit length).
    pub fn gradient(&self, pos: Vec2, distance: f32) -> Vec2 {
        let d = distance.max(self.cell * 0.5);
        let gx = self.sample(pos + Vec2::new(d, 0.0)) - self.sample(pos - Vec2::new(d, 0.0));
        let gy = self.sample(pos + Vec2::new(0.0, d)) - self.sample(pos - Vec2::new(0.0, d));
        Vec2::new(gx, gy) / (2.0 * d)
    }

    /// Add to the water cell at (or next to) `pos`; content put in rock would be lost.
    #[inline]
    pub fn add(&mut self, pos: Vec2, amount: f32) {
        let i = self.water_index_at(pos);
        self.data[i] += amount;
    }

    pub fn total(&self) -> f64 {
        self.data.iter().map(|&v| v as f64).sum()
    }

    pub fn cell_area(&self) -> f32 {
        self.cell * self.cell
    }

    /// Explicit diffusion with no flux into solid cells. `rate = D·dt/cell²`, must be ≤ 0.25.
    pub fn diffuse(&mut self, rate: f32) {
        let rate = rate.clamp(0.0, 0.24);
        if rate <= 0.0 {
            return;
        }
        let (nx, ny) = (self.nx, self.ny);
        let data = &self.data;
        let solid = &self.solid;
        self.scratch.resize(data.len(), 0.0);
        self.scratch
            .par_chunks_mut(nx)
            .enumerate()
            .for_each(|(y, row)| {
                for x in 0..nx {
                    let i = y * nx + x;
                    if solid[i] {
                        row[x] = 0.0;
                        continue;
                    }
                    let v = data[i];
                    let mut flux = 0.0;
                    let mut visit = |j: usize| {
                        if !solid[j] {
                            flux += data[j] - v;
                        }
                    };
                    if x > 0 {
                        visit(i - 1);
                    }
                    if x + 1 < nx {
                        visit(i + 1);
                    }
                    if y > 0 {
                        visit(i - nx);
                    }
                    if y + 1 < ny {
                        visit(i + nx);
                    }
                    row[x] = v + rate * flux;
                }
            });
        std::mem::swap(&mut self.data, &mut self.scratch);
    }

    /// Move a fraction of each cell's content one cell vertically (`up` or down),
    /// stopping at solid cells and the grid edge. Mass-conserving.
    pub fn advect_vertical(&mut self, fraction: f32, up: bool, column_mask: Option<&[bool]>) {
        let fraction = fraction.clamp(0.0, 1.0);
        if fraction <= 0.0 {
            return;
        }
        let (nx, ny) = (self.nx, self.ny);
        for x in 0..nx {
            if let Some(mask) = column_mask
                && !mask[x]
            {
                continue;
            }
            // Process in the direction of motion so content moves at most one cell per call.
            if up {
                for y in (0..ny - 1).rev() {
                    self.shift(x + y * nx, x + (y + 1) * nx, fraction);
                }
            } else {
                for y in 1..ny {
                    self.shift(x + y * nx, x + (y - 1) * nx, fraction);
                }
            }
        }
    }

    #[inline]
    fn shift(&mut self, from: usize, to: usize, fraction: f32) {
        if self.solid[from] || self.solid[to] {
            return;
        }
        let moved = self.data[from] * fraction;
        self.data[from] -= moved;
        self.data[to] += moved;
    }

    /// Multiply every cell by `factor`, returning the total amount removed.
    pub fn decay(&mut self, factor: f32) -> f64 {
        let keep = factor.clamp(0.0, 1.0);
        let mut removed = 0.0f64;
        for v in &mut self.data {
            let loss = *v * (1.0 - keep);
            removed += loss as f64;
            *v -= loss;
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diffusion_conserves_mass_and_spreads() {
        let mut f = Field::new(320.0, 320.0, 32.0, 0.0);
        f.add(Vec2::new(160.0, 160.0), 100.0);
        f.solid[0] = true;
        let before = f.total();
        for _ in 0..50 {
            f.diffuse(0.2);
        }
        assert!((f.total() - before).abs() < 1e-3);
        assert!(f.get(Vec2::new(100.0, 160.0)) > 0.0);
        assert_eq!(f.data[0], 0.0);
    }

    #[test]
    fn advection_moves_content_down_and_stops_at_rock() {
        let mut f = Field::new(32.0, 320.0, 32.0, 0.0);
        f.add(Vec2::new(16.0, 300.0), 10.0);
        f.solid[0] = true;
        for _ in 0..200 {
            f.advect_vertical(0.5, false, None);
        }
        assert!((f.total() - 10.0).abs() < 1e-4);
        assert!(
            f.data[f.index(0, 1)] > 9.9,
            "should settle on top of the rock"
        );
    }

    #[test]
    fn bilinear_sampling_is_smooth() {
        let mut f = Field::new(64.0, 32.0, 32.0, 0.0);
        f.data[1] = 1.0;
        let mid = f.sample(Vec2::new(32.0, 16.0));
        assert!((mid - 0.5).abs() < 1e-5);
        let g = f.gradient(Vec2::new(32.0, 16.0), 16.0);
        assert!(g.x > 0.0);
    }
}

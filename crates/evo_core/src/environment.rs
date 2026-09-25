//! Environment: clock (day, season, climate), light, temperature, minerals, detritus, vents.

use glam::Vec2;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::field::Field;
use crate::rng::Rng;
use crate::terrain::Terrain;

/// Light computed column by column from the surface down (column-major storage).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LightField {
    pub nx: usize,
    pub ny: usize,
    pub cell: f32,
    /// Irradiance entering each cell (energy/s per unit width).
    pub level: Vec<f32>,
    /// Energy/s delivered per unit of absorbing weight in each cell.
    pub share: Vec<f32>,
    /// Σ (covered width fraction × absorptance) of bodies in each cell, this tick.
    pub weight: Vec<f32>,
    pub opaque: Vec<bool>,
}

impl LightField {
    pub fn new(config: &Config, terrain: &Terrain) -> Self {
        let cell = config.light.cell;
        let nx = (config.world.width / cell).ceil() as usize;
        let ny = (config.world.height / cell).ceil() as usize;
        let mut opaque = vec![false; nx * ny];
        for x in 0..nx {
            for y in 0..ny {
                let p = Vec2::new((x as f32 + 0.5) * cell, (y as f32 + 0.5) * cell);
                opaque[x * ny + y] = terrain.distance(p) < 0.0;
            }
        }
        Self {
            nx,
            ny,
            cell,
            level: vec![0.0; nx * ny],
            share: vec![0.0; nx * ny],
            weight: vec![0.0; nx * ny],
            opaque,
        }
    }

    #[inline]
    pub fn geometry(&self) -> LightGeometry {
        LightGeometry {
            nx: self.nx,
            ny: self.ny,
            cell: self.cell,
        }
    }

    #[inline]
    pub fn index(&self, col: usize, row: usize) -> usize {
        col * self.ny + row
    }

    /// Irradiance at `pos` (nearest cell).
    #[inline]
    pub fn level_at(&self, pos: Vec2) -> f32 {
        self.level[self.geometry().index_at(pos)]
    }

    /// Vertical-and-horizontal gradient of irradiance, normalized to the local level.
    pub fn relative_gradient(&self, pos: Vec2, distance: f32) -> Vec2 {
        let d = distance.max(self.cell);
        let here = self.level_at(pos).max(1e-4);
        let gx = self.level_at(pos + Vec2::new(d, 0.0)) - self.level_at(pos - Vec2::new(d, 0.0));
        let gy = self.level_at(pos + Vec2::new(0.0, d)) - self.level_at(pos - Vec2::new(0.0, d));
        Vec2::new(gx, gy) / (2.0 * here)
    }

    /// See [`LightGeometry::for_each_covered`].
    #[inline]
    pub fn for_each_covered(&self, pos: Vec2, radius: f32, f: impl FnMut(usize, f32)) {
        self.geometry().for_each_covered(pos, radius, f);
    }

    /// Propagate sunlight from the surface down every column.
    pub fn propagate(&mut self, surface: f32, half_depth: f32) {
        let ny = self.ny;
        let cell = self.cell;
        let water = (-(std::f32::consts::LN_2) * cell / half_depth.max(1.0)).exp();
        self.level
            .par_chunks_mut(ny)
            .zip(self.share.par_chunks_mut(ny))
            .zip(self.weight.par_chunks(ny))
            .zip(self.opaque.par_chunks(ny))
            .for_each(|(((level, share), weight), opaque)| {
                let mut light = surface;
                for y in (0..ny).rev() {
                    level[y] = light;
                    if opaque[y] {
                        share[y] = 0.0;
                        light = 0.0;
                        continue;
                    }
                    let w = weight[y];
                    if w > 1e-6 {
                        let transmitted = (-w).exp();
                        // Energy/s absorbed across the cell's width, per unit of weight.
                        share[y] = light * cell * (1.0 - transmitted) / w;
                        light *= transmitted;
                    } else {
                        share[y] = light * cell;
                    }
                    light *= water;
                }
            });
    }
}

/// Grid layout of a [`LightField`], separable from its data for split borrows.
#[derive(Clone, Copy, Debug)]
pub struct LightGeometry {
    pub nx: usize,
    pub ny: usize,
    pub cell: f32,
}

impl LightGeometry {
    #[inline]
    pub fn column(&self, x: f32) -> usize {
        ((x / self.cell) as isize).clamp(0, self.nx as isize - 1) as usize
    }

    #[inline]
    pub fn row(&self, y: f32) -> usize {
        ((y / self.cell) as isize).clamp(0, self.ny as isize - 1) as usize
    }

    #[inline]
    pub fn index(&self, col: usize, row: usize) -> usize {
        col * self.ny + row
    }

    #[inline]
    pub fn index_at(&self, pos: Vec2) -> usize {
        self.index(self.column(pos.x), self.row(pos.y))
    }

    /// Visit the columns covered by a body of `radius` at `pos`: `(cell index, covered width fraction)`.
    #[inline]
    pub fn for_each_covered(&self, pos: Vec2, radius: f32, mut f: impl FnMut(usize, f32)) {
        let row = self.row(pos.y);
        let x0 = pos.x - radius;
        let x1 = pos.x + radius;
        let c0 = self.column(x0);
        let c1 = self.column(x1);
        for c in c0..=c1 {
            let left = (c as f32 * self.cell).max(x0);
            let right = ((c + 1) as f32 * self.cell).min(x1);
            let w = (right - left) / self.cell;
            if w > 0.0 {
                f(self.index(c, row), w);
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vent {
    pub id: u32,
    pub pos: Vec2,
    /// Current output as a fraction of full strength.
    pub strength: f32,
    pub peak: f32,
    /// Sim seconds.
    pub age: f32,
    pub lifetime: f32,
}

impl Vent {
    fn envelope(&self) -> f32 {
        let l = self.lifetime.max(1.0);
        let rise = (self.age / (0.1 * l)).clamp(0.0, 1.0);
        let fall = ((l - self.age) / (0.3 * l)).clamp(0.0, 1.0);
        rise.min(fall)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Clock {
    /// Sim seconds since the start.
    pub time: f64,
    /// 0 = midnight, 0.5 = noon.
    pub day_phase: f32,
    /// 0..1 through the year; 0.25 = midsummer.
    pub year_phase: f32,
    pub day: u64,
    pub year: u64,
    /// Current global temperature offset (°C).
    pub climate: f32,
    /// Irradiance at the surface right now.
    pub sun: f32,
    /// Surface temperature right now (°C).
    pub surface_temp: f32,
}

impl Clock {
    pub fn season_name(&self) -> &'static str {
        match (self.year_phase * 4.0) as u32 {
            0 => "Spring",
            1 => "Summer",
            2 => "Autumn",
            _ => "Winter",
        }
    }

    /// `(hours, minutes)` on a 24 h clock.
    pub fn time_of_day(&self) -> (u32, u32) {
        let minutes = (self.day_phase * 24.0 * 60.0) as u32;
        (minutes / 60, minutes % 60)
    }
}

/// Things that happened in the environment this update (for the event log).
pub enum EnvEvent {
    VentOpened(Vec2),
    VentDied(Vec2),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Environment {
    pub clock: Clock,
    pub light: LightField,
    pub temperature: Field,
    pub minerals: Field,
    pub detritus: Field,
    pub vents: Vec<Vent>,
    next_vent_id: u32,
    rng: Rng,
    since_temperature_update: f32,
}

/// Copy a terrain rock mask onto another field's grid (cell is solid if its center is rock).
fn mask_from_terrain(field: &mut Field, terrain: &Terrain) {
    for y in 0..field.ny {
        for x in 0..field.nx {
            let p = Vec2::new((x as f32 + 0.5) * field.cell, (y as f32 + 0.5) * field.cell);
            let i = field.index(x, y);
            field.solid[i] = terrain.distance(p) < 0.0;
        }
    }
}

impl Environment {
    pub fn new(config: &Config, terrain: &Terrain) -> Self {
        let (w, h) = (config.world.width, config.world.height);
        let mut minerals = Field::new(w, h, config.minerals.cell, 0.0);
        let mut detritus = Field::new(w, h, config.detritus.cell, 0.0);
        let mut temperature = Field::new(w, h, config.temperature.cell, config.temperature.deep);
        mask_from_terrain(&mut minerals, terrain);
        mask_from_terrain(&mut detritus, terrain);
        mask_from_terrain(&mut temperature, terrain);
        let mut env = Self {
            clock: Clock::default(),
            light: LightField::new(config, terrain),
            temperature,
            minerals,
            detritus,
            vents: Vec::new(),
            next_vent_id: 0,
            rng: Rng::new(config.world.seed ^ 0xE4E4_0001),
            since_temperature_update: f32::INFINITY,
        };
        // Start mid-morning, with vents already at various stages of life.
        env.update_clock(0, config);
        for _ in 0..config.vents.count {
            if let Some(mut vent) = env.new_vent(config, terrain) {
                vent.age = env.rng.range(0.1, 0.6) * vent.lifetime;
                vent.strength = vent.peak * vent.envelope();
                env.vents.push(vent);
            }
        }
        env.update_temperature(config);
        env
    }

    /// Advance the clock to `tick`.
    pub fn update_clock(&mut self, tick: u64, config: &Config) {
        let t = &config.time;
        let start = 0.3 * t.day_length as f64; // start at ~07:00
        let time = tick as f64 * t.dt as f64 + start;
        let days = time / t.day_length as f64;
        let years = days / t.year_length as f64;
        let c = &mut self.clock;
        c.time = tick as f64 * t.dt as f64;
        c.day = days as u64;
        c.year = years as u64;
        c.day_phase = days.fract() as f32;
        c.year_phase = years.fract() as f32;

        let elevation = -(std::f32::consts::TAU * c.day_phase).cos();
        let t_sun = ((elevation + 0.15) / 0.55).clamp(0.0, 1.0);
        let daylight = t_sun * t_sun * (3.0 - 2.0 * t_sun);
        let season = (std::f32::consts::TAU * c.year_phase).sin();
        let l = &config.light;
        c.sun = l.surface_intensity
            * (1.0 + l.season_amplitude * season)
            * (l.night_level + (1.0 - l.night_level) * daylight);
        let temp = &config.temperature;
        c.surface_temp = temp.surface + temp.season_amplitude * season + c.climate;
    }

    fn new_vent(&mut self, config: &Config, terrain: &Terrain) -> Option<Vent> {
        let v = &config.vents;
        for _ in 0..200 {
            let x = self.rng.range(0.03, 0.97) * config.world.width;
            let floor = terrain.floor_at(x);
            if config.world.height - floor < v.min_depth {
                continue;
            }
            let pos = Vec2::new(x, floor + 12.0);
            if terrain.distance(pos + Vec2::new(0.0, 24.0)) < 20.0 {
                continue;
            }
            if self.vents.iter().any(|o| o.pos.distance(pos) < 800.0) {
                continue;
            }
            let hours = self.rng.range(v.lifetime_min, v.lifetime_max);
            let id = self.next_vent_id;
            self.next_vent_id += 1;
            return Some(Vent {
                id,
                pos,
                strength: 0.0,
                peak: self.rng.range(0.6, 1.4),
                age: 0.0,
                lifetime: hours * 3600.0,
            });
        }
        None
    }

    /// Update fields and vents; `dt` is the time since the last update.
    pub fn update_fields(
        &mut self,
        dt: f32,
        config: &Config,
        terrain: &Terrain,
        events: &mut Vec<EnvEvent>,
    ) {
        // Climate: Ornstein–Uhlenbeck random walk.
        let tc = &config.temperature;
        let tau = (tc.climate_timescale * config.time.day_length).max(1.0);
        let sigma = tc.climate_amplitude * (2.0 / tau).sqrt() * 0.5;
        let noise = (self.rng.f32() + self.rng.f32() + self.rng.f32() - 1.5) * 2.0; // ~N(0,1)
        let c = &mut self.clock.climate;
        *c += -*c / tau * dt + sigma * dt.sqrt() * noise;
        *c = c.clamp(-tc.climate_amplitude, tc.climate_amplitude);

        // Vents: age, emit, die, and open.
        let vc = &config.vents;
        let mut plume_columns = vec![false; self.minerals.nx];
        let mut i = 0;
        while i < self.vents.len() {
            let vent = &mut self.vents[i];
            vent.age += dt;
            vent.strength = vent.peak * vent.envelope();
            if vent.age >= vent.lifetime {
                events.push(EnvEvent::VentDied(vent.pos));
                self.vents.swap_remove(i);
                continue;
            }
            let amount = vc.output * vent.strength * dt;
            let cells = emit_disc(&mut self.minerals, vent.pos, vc.radius, amount);
            let _ = cells;
            let half = config.minerals.plume_width / self.minerals.cell;
            let cx = vent.pos.x / self.minerals.cell;
            let x0 = (cx - half).floor().max(0.0) as usize;
            let x1 = ((cx + half).ceil() as usize).min(self.minerals.nx - 1);
            for col in plume_columns.iter_mut().take(x1 + 1).skip(x0) {
                *col = true;
            }
            i += 1;
        }
        let missing = vc.count.saturating_sub(self.vents.len() as u32);
        if missing > 0
            && self.rng.chance(missing as f32 * dt / 600.0)
            && let Some(vent) = self.new_vent(config, terrain)
        {
            events.push(EnvEvent::VentOpened(vent.pos));
            self.vents.push(vent);
        }

        // Minerals: rise in plumes, diffuse, precipitate.
        let mc = &config.minerals;
        self.minerals.advect_vertical(
            mc.plume_rise * dt / self.minerals.cell,
            true,
            Some(&plume_columns),
        );
        self.minerals
            .diffuse(mc.diffusion * dt / (self.minerals.cell * self.minerals.cell));
        self.minerals.decay((-mc.decay * dt).exp());

        // Detritus: sink, diffuse, decompose into minerals.
        let dc = &config.detritus;
        self.detritus
            .advect_vertical(dc.sink_speed * dt / self.detritus.cell, false, None);
        self.detritus
            .diffuse(dc.diffusion * dt / (self.detritus.cell * self.detritus.cell));
        let keep = (-dc.decay * dt).exp();
        if (self.detritus.cell - self.minerals.cell).abs() < 1e-3 {
            for (d, m) in self
                .detritus
                .data
                .iter_mut()
                .zip(self.minerals.data.iter_mut())
            {
                let lost = *d * (1.0 - keep);
                *d -= lost;
                *m += lost * dc.to_minerals;
            }
        } else {
            self.detritus.decay(keep);
        }

        self.since_temperature_update += dt;
        if self.since_temperature_update >= tc.update_interval {
            self.update_temperature(config);
        }
    }

    fn update_temperature(&mut self, config: &Config) {
        self.since_temperature_update = 0.0;
        let tc = &config.temperature;
        let height = config.world.height;
        let surface = self.clock.surface_temp;
        let cell = self.temperature.cell;
        let nx = self.temperature.nx;
        let vents: Vec<(Vec2, f32)> = self.vents.iter().map(|v| (v.pos, v.strength)).collect();
        let inv_r2 = 1.0 / (tc.vent_heat_radius * tc.vent_heat_radius);
        self.temperature
            .data
            .par_chunks_mut(nx)
            .enumerate()
            .for_each(|(y, row)| {
                let py = (y as f32 + 0.5) * cell;
                let depth = (height - py).max(0.0);
                let base = tc.deep + (surface - tc.deep) * (-depth / tc.thermocline_depth).exp();
                for (x, t) in row.iter_mut().enumerate() {
                    let p = Vec2::new((x as f32 + 0.5) * cell, py);
                    let mut heat = 0.0;
                    for &(vp, s) in &vents {
                        let d2 = p.distance_squared(vp);
                        heat += tc.vent_heat * s * (-d2 * inv_r2).exp();
                    }
                    *t = base + heat;
                }
            });
    }

    /// Spread `amount` of mineral energy over the water cells in the rectangle `min..max`.
    pub fn inject_minerals(&mut self, min: Vec2, max: Vec2, amount: f32) {
        let f = &mut self.minerals;
        let (x0, y0) = f.coords(min);
        let (x1, y1) = f.coords(max);
        let cells: Vec<usize> = (y0..=y1)
            .flat_map(|y| (x0..=x1).map(move |x| (x, y)))
            .map(|(x, y)| f.index(x, y))
            .filter(|&i| !f.solid[i])
            .collect();
        if cells.is_empty() {
            return;
        }
        let per = amount / cells.len() as f32 / f.cell_area();
        for i in cells {
            f.data[i] += per;
        }
    }

    /// Re-derive rock masks for the fields after the terrain changed between `x_min` and `x_max`.
    /// Content of cells that became rock is lost (it's a small amount).
    pub fn refresh_masks(&mut self, terrain: &Terrain, x_min: f32, x_max: f32) {
        for (field, holds_content) in [
            (&mut self.minerals, true),
            (&mut self.detritus, true),
            (&mut self.temperature, false),
        ] {
            let (x0, _) = field.coords(Vec2::new(x_min, 0.0));
            let (x1, _) = field.coords(Vec2::new(x_max, 0.0));
            for y in 0..field.ny {
                for x in x0..=x1 {
                    let p = Vec2::new((x as f32 + 0.5) * field.cell, (y as f32 + 0.5) * field.cell);
                    let i = field.index(x, y);
                    let solid = terrain.distance(p) < 0.0;
                    if solid && !field.solid[i] && holds_content {
                        field.data[i] = 0.0;
                    }
                    field.solid[i] = solid;
                }
            }
        }
        let light = &mut self.light;
        let g = light.geometry();
        for x in g.column(x_min)..=g.column(x_max) {
            for y in 0..g.ny {
                let p = Vec2::new((x as f32 + 0.5) * g.cell, (y as f32 + 0.5) * g.cell);
                light.opaque[x * g.ny + y] = terrain.distance(p) < 0.0;
            }
        }
    }

    /// Mineral field total + detritus total (for diagnostics).
    pub fn field_totals(&self) -> (f64, f64) {
        let area_m = self.minerals.cell_area() as f64;
        let area_d = self.detritus.cell_area() as f64;
        (
            self.minerals.total() * area_m,
            self.detritus.total() * area_d,
        )
    }
}

/// Spread `amount` (energy) evenly over the field cells within `radius` of `pos`, as concentration.
fn emit_disc(field: &mut Field, pos: Vec2, radius: f32, amount: f32) -> usize {
    let r = radius.max(field.cell * 0.5);
    let (x0, y0) = field.coords(pos - Vec2::splat(r));
    let (x1, y1) = field.coords(pos + Vec2::splat(r));
    let mut cells = Vec::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            let i = field.index(x, y);
            let c = Vec2::new((x as f32 + 0.5) * field.cell, (y as f32 + 0.5) * field.cell);
            if !field.solid[i] && c.distance(pos) <= r + field.cell * 0.5 {
                cells.push(i);
            }
        }
    }
    if cells.is_empty() {
        let i = field.index_at(pos);
        cells.push(i);
    }
    let per = amount / cells.len() as f32 / field.cell_area();
    for &i in &cells {
        field.data[i] += per;
    }
    cells.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_is_conserved_and_shading_works() {
        let config = Config::default();
        let terrain = Terrain::generate(&config);
        let mut light = LightField::new(&config, &terrain);
        let col = light.nx / 2;
        let top = light.ny - 1;
        // A dense layer at the top of one column.
        let i = light.index(col, top);
        light.weight[i] = 3.0;
        light.propagate(1.0, 400.0);
        let below = light.level[light.index(col, top - 1)];
        let open = light.level[light.index(col + 5, top - 1)];
        assert!(below < open * 0.1, "shaded {below} vs open {open}");
        // Absorbed + transmitted == incoming.
        let absorbed = light.share[i] * 3.0;
        let transmitted = (-3.0f32).exp() * light.cell;
        assert!((absorbed + transmitted - light.cell).abs() < 1e-4);
    }

    #[test]
    fn light_fades_with_depth() {
        let config = Config::default();
        let terrain = Terrain::generate(&config);
        let mut light = LightField::new(&config, &terrain);
        light.propagate(1.0, 400.0);
        let x = config.world.width * 0.5;
        let h = config.world.height;
        let top = light.level_at(Vec2::new(x, h - 1.0));
        let deeper = light.level_at(Vec2::new(x, h - 401.0));
        assert!((deeper / top - 0.5).abs() < 0.05, "ratio {}", deeper / top);
    }

    #[test]
    fn day_night_cycle() {
        let config = Config::default();
        let terrain = Terrain::generate(&config);
        let mut env = Environment::new(&config, &terrain);
        let ticks_per_day = (config.time.day_length / config.time.dt) as u64;
        let mut min = f32::MAX;
        let mut max = 0.0f32;
        for k in 0..48 {
            env.update_clock(k * ticks_per_day / 48, &config);
            min = min.min(env.clock.sun);
            max = max.max(env.clock.sun);
        }
        let night = config.light.night_level;
        assert!(min < max * (night + 0.1) && max > 0.5 * config.light.surface_intensity);
    }

    #[test]
    fn vents_exist_and_warm_the_water() {
        let config = Config::default();
        let terrain = Terrain::generate(&config);
        let env = Environment::new(&config, &terrain);
        assert!(!env.vents.is_empty());
        let v = &env.vents[0];
        let near = env.temperature.sample(v.pos + Vec2::new(0.0, 30.0));
        let far = env
            .temperature
            .sample(Vec2::new(v.pos.x + 1500.0, v.pos.y + 30.0));
        assert!(near > far + 5.0, "near {near} far {far}");
    }
}

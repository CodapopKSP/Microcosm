//! Environment: clock (day, season, climate), light, temperature, minerals, detritus, vents,
//! and the water chemistry (oxygen, CO₂, nutrients, toxins) with the air above it.

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
                // Caves are dark: no sunlight reaches in.
                opaque[x * ny + y] = terrain.distance(p) < 0.0 || terrain.is_cave(p);
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
    /// Part of the terrain: never fades or dies.
    #[serde(default)]
    pub fixed: bool,
}

impl Vent {
    fn envelope(&self) -> f32 {
        if self.fixed {
            return 1.0;
        }
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
    /// Current global temperature offset (°C) from the random climate walk.
    pub climate: f32,
    /// Warming (or cooling) from CO₂ in the air (°C).
    pub greenhouse: f32,
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

/// The air above the ocean: global levels relative to today's (1.0).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Atmosphere {
    pub oxygen: f32,
    pub co2: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Environment {
    pub clock: Clock,
    pub light: LightField,
    pub temperature: Field,
    pub minerals: Field,
    pub detritus: Field,
    /// Nutrients bound up in marine snow (dead bodies' nutrients): it sinks with the detritus
    /// and releases them into the water where it rots, so the sunlit water empties and the
    /// deep fills (the biological pump). Detritus grid.
    #[serde(default)]
    pub snow_nutrients: Field,
    /// Dissolved gases (1.0 = in equilibrium with today's air), nutrients and toxins.
    /// These share the minerals grid, so one index addresses all of them.
    pub oxygen: Field,
    pub co2: Field,
    pub nutrients: Field,
    pub toxin: Field,
    /// Toxin weighted by the producers' hue (as a unit vector), so kin recognize their own.
    pub toxin_hue_x: Field,
    pub toxin_hue_y: Field,
    pub atmosphere: Atmosphere,
    /// Light from lures, rebuilt every tick (minerals grid).
    #[serde(skip)]
    pub glow: Field,
    #[serde(skip)]
    pub any_glow: bool,
    /// Set when toxin is released; cleared once it has all broken down (skips the work).
    #[serde(default)]
    pub toxin_present: bool,
    /// Time since the last chemistry update, and the number of water cells (cached).
    #[serde(skip)]
    chem_clock: f32,
    #[serde(skip)]
    water_cells: usize,
    pub vents: Vec<Vent>,
    next_vent_id: u32,
    rng: Rng,
    since_temperature_update: f32,
    /// Detritus cells in caves, with the share of each that's cave (built on first use).
    #[serde(skip)]
    cave_cells: Option<Vec<(u32, f32)>>,
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

/// The detritus cells that are (partly) cave, and how much of each: sampled at 4×4 points.
fn cave_cells(field: &Field, terrain: &Terrain) -> Vec<(u32, f32)> {
    if terrain.cave.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for y in 0..field.ny {
        for x in 0..field.nx {
            let i = field.index(x, y);
            if field.solid[i] {
                continue;
            }
            let mut hits = 0;
            for sy in 0..4 {
                for sx in 0..4 {
                    let p = Vec2::new(
                        (x as f32 + (sx as f32 + 0.5) / 4.0) * field.cell,
                        (y as f32 + (sy as f32 + 0.5) / 4.0) * field.cell,
                    );
                    hits += terrain.is_cave(p) as u32;
                }
            }
            if hits > 0 {
                out.push((i as u32, hits as f32 / 16.0));
            }
        }
    }
    out
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
        let atmosphere = Atmosphere {
            oxygen: config.atmosphere.initial_oxygen,
            co2: config.atmosphere.initial_co2,
        };
        // Chemistry fields start at their usual depth profiles.
        let chem = |kind: Chemical| {
            let mut f = minerals.clone();
            let refs = reference_profile(kind, &f, config, atmosphere);
            for (y, &value) in refs.iter().enumerate() {
                for x in 0..f.nx {
                    let i = f.index(x, y);
                    f.data[i] = if f.solid[i] { 0.0 } else { value };
                }
            }
            f
        };
        let oxygen = chem(Chemical::Oxygen);
        let co2 = chem(Chemical::Co2);
        let nutrients = chem(Chemical::Nutrients);
        let mut toxin = minerals.clone();
        toxin.data.fill(0.0);
        let mut glow = toxin.clone();
        glow.solid.fill(false);
        let mut env = Self {
            clock: Clock::default(),
            light: LightField::new(config, terrain),
            temperature,
            oxygen,
            co2,
            nutrients,
            toxin_hue_x: toxin.clone(),
            toxin_hue_y: toxin.clone(),
            toxin,
            atmosphere,
            glow,
            any_glow: false,
            toxin_present: false,
            chem_clock: 0.0,
            water_cells: 0,
            snow_nutrients: {
                let mut f = detritus.clone();
                f.data.fill(0.0);
                f
            },
            minerals,
            detritus,
            vents: Vec::new(),
            next_vent_id: 0,
            rng: Rng::new(config.world.seed ^ 0xE4E4_0001),
            since_temperature_update: f32::INFINITY,
            cave_cells: None,
        };
        // Start mid-morning, with vents already at various stages of life.
        env.update_clock(0, config);
        // Permanent vents where the terrain has them.
        for &pos in &terrain.vent_sites {
            let id = env.next_vent_id;
            env.next_vent_id += 1;
            env.vents.push(Vent {
                id,
                pos,
                strength: 1.0,
                peak: 1.0,
                age: 0.0,
                lifetime: 0.0,
                fixed: true,
            });
        }
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
        c.surface_temp = temp.surface + temp.season_amplitude * season + c.climate + c.greenhouse;
    }

    fn new_vent(&mut self, config: &Config, terrain: &Terrain) -> Option<Vent> {
        let v = &config.vents;
        // Painted maps: only at their spawn points, one vent per point.
        if let Some(spawns) = &terrain.vent_spawns {
            let free: Vec<Vec2> = spawns
                .iter()
                .copied()
                .filter(|p| self.vents.iter().all(|o| o.pos.distance(*p) > 40.0))
                .collect();
            if free.is_empty() {
                return None;
            }
            let pos = free[self.rng.below(free.len() as u32) as usize];
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
                fixed: false,
            });
        }
        // Keep a few vents shallow (on the shelf and upper slope): stepping stones between the
        // sunlit water and the deep. The rest open on the deep floor.
        let height = config.world.height;
        let shallow_now = self
            .vents
            .iter()
            .filter(|o| height - o.pos.y < v.shallow_max_depth)
            .count() as u32;
        let deep = (v.deep_min_depth, f32::MAX);
        let (lo, hi) = if shallow_now < v.shallow {
            (v.min_depth, v.shallow_max_depth)
        } else {
            deep
        };
        for attempt in 0..600 {
            // If no shallow spot turns up, settle for a deep one.
            let (lo, hi) = if attempt >= 400 { deep } else { (lo, hi) };
            // In the ocean only: the basin beyond the mountain stays as it is.
            let x = self.rng.range(0.03, 0.97) * config.ocean_width();
            let floor = terrain.floor_at(x);
            let depth = height - floor;
            if depth < lo || depth > hi {
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
                fixed: false,
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
        // Upward drift of minerals per column (units/s): the plumes over vents.
        let mc = &config.minerals;
        let mut plume_columns = vec![0.0f32; self.minerals.nx];
        let mut i = 0;
        while i < self.vents.len() {
            let vent = &mut self.vents[i];
            vent.age += dt;
            vent.strength = vent.peak * vent.envelope();
            if !vent.fixed && vent.age >= vent.lifetime {
                events.push(EnvEvent::VentDied(vent.pos));
                self.vents.swap_remove(i);
                continue;
            }
            // Most of the output pours out at the vent; some seeps out over a wide area around
            // its base.
            let amount = vc.output * vent.strength * dt;
            emit_disc(
                &mut self.minerals,
                vent.pos,
                vc.radius,
                amount * (1.0 - vc.halo),
            );
            emit_disc(
                &mut self.minerals,
                vent.pos,
                vc.halo_radius,
                amount * vc.halo,
            );
            let cc = &config.chemistry;
            emit_disc(
                &mut self.co2,
                vent.pos,
                vc.radius,
                cc.vent_co2 * vent.strength * dt,
            );
            emit_disc(
                &mut self.nutrients,
                vent.pos,
                vc.radius,
                cc.vent_nutrients * vent.strength * dt,
            );
            // Vent water carries no oxygen.
            let keep = (-cc.vent_oxygen_sink * vent.strength * dt).exp();
            for i in disc_cells(&self.oxygen, vent.pos, vc.radius * 2.0) {
                self.oxygen.data[i] *= keep;
            }
            let cell = self.minerals.cell;
            let nx = self.minerals.nx;
            let (half, cx) = (mc.plume_width / cell, vent.pos.x / cell);
            let x0 = (cx - half).floor().max(0.0) as usize;
            let x1 = ((cx + half).ceil() as usize).min(nx - 1);
            for col in plume_columns.iter_mut().take(x1 + 1).skip(x0) {
                *col = col.max(mc.plume_rise);
            }
            i += 1;
        }
        let random = self.vents.iter().filter(|v| !v.fixed).count() as u32;
        let missing = vc.count.saturating_sub(random);
        if missing > 0
            && self.rng.chance(missing as f32 * dt / 600.0)
            && let Some(vent) = self.new_vent(config, terrain)
        {
            events.push(EnvEvent::VentOpened(vent.pos));
            self.vents.push(vent);
        }

        // Minerals: rise in plumes, diffuse, precipitate.
        let cell = self.minerals.cell;
        for speed in &mut plume_columns {
            *speed *= dt / cell;
        }
        self.minerals.rise_columns(&plume_columns);
        self.minerals
            .diffuse(mc.diffusion * dt / (self.minerals.cell * self.minerals.cell));
        self.minerals.decay((-mc.decay * dt).exp());

        // Detritus: sink, diffuse, decompose. Rotting uses oxygen and releases CO₂ and minerals;
        // without oxygen it is slower and releases more minerals (sulfides).
        let dc = &config.detritus;
        let cc = &config.chemistry;
        self.detritus
            .advect_vertical(dc.sink_speed * dt / self.detritus.cell, false, None);
        self.detritus
            .diffuse(dc.diffusion * dt / (self.detritus.cell * self.detritus.cell));
        let rate = 1.0 - (-dc.decay * dt).exp();
        if (self.detritus.cell - self.minerals.cell).abs() < 1e-3 {
            let oxygen_use = cc.oxygen_per_energy * cc.decomposition_oxygen;
            let nx = self.detritus.nx;
            self.detritus
                .data
                .par_chunks_mut(nx)
                .zip(self.oxygen.data.par_chunks_mut(nx))
                .zip(self.co2.data.par_chunks_mut(nx))
                .zip(self.minerals.data.par_chunks_mut(nx))
                .for_each(|(((detritus, oxygen), co2), minerals)| {
                    for i in 0..detritus.len() {
                        let d = detritus[i];
                        if d <= 0.0 {
                            continue;
                        }
                        let o2 = oxygen[i];
                        let aerobic = (o2 / cc.oxygen_half).clamp(0.0, 1.0);
                        let lost = d * rate * (aerobic + (1.0 - aerobic) * cc.anoxic_decay);
                        let breathed = (lost * aerobic).min(o2 / oxygen_use.max(1e-6));
                        let anoxic = lost - breathed;
                        detritus[i] = d - lost;
                        oxygen[i] = o2 - breathed * oxygen_use;
                        co2[i] += lost * cc.co2_per_energy;
                        minerals[i] += breathed * dc.to_minerals + anoxic * cc.anoxic_minerals;
                    }
                });
        } else {
            self.detritus.decay(1.0 - rate);
        }

        // Nutrients in the marine snow sink and spread with it, and are released into the water
        // as it rots (at the same pace: slower without oxygen).
        if self.snow_nutrients.data.len() != self.detritus.data.len() {
            self.snow_nutrients = self.detritus.clone();
            self.snow_nutrients.data.fill(0.0);
        }

        // Caves keep shedding detritus (and its nutrients) into their water.
        if self.cave_cells.is_none() {
            self.cave_cells = Some(cave_cells(&self.detritus, terrain));
        }
        if let Some(cells) = &self.cave_cells {
            let (d, n) = (dc.cave_rate * dt, dc.cave_nutrients * dt);
            for &(i, share) in cells {
                self.detritus.data[i as usize] += d * share;
                self.snow_nutrients.data[i as usize] += n * share;
            }
        }
        let cell = self.detritus.cell;
        self.snow_nutrients
            .advect_vertical(dc.sink_speed * dt / cell, false, None);
        self.snow_nutrients
            .diffuse(dc.diffusion * dt / (cell * cell));
        let release = |snow: f32, o2: f32| {
            let aerobic = (o2 / cc.oxygen_half).clamp(0.0, 1.0);
            snow * rate * (aerobic + (1.0 - aerobic) * cc.anoxic_decay)
        };
        if (cell - self.nutrients.cell).abs() < 1e-3 {
            let nx = self.nutrients.nx;
            self.snow_nutrients
                .data
                .par_chunks_mut(nx)
                .zip(self.nutrients.data.par_chunks_mut(nx))
                .zip(self.oxygen.data.par_chunks(nx))
                .for_each(|((snow, nutrients), oxygen)| {
                    for i in 0..snow.len() {
                        if snow[i] > 0.0 {
                            let released = release(snow[i], oxygen[i]);
                            snow[i] -= released;
                            nutrients[i] += released;
                        }
                    }
                });
        } else {
            // Different grids: release each cell's share where it is.
            let ratio = self.snow_nutrients.cell_area() / self.nutrients.cell_area();
            for y in 0..self.snow_nutrients.ny {
                for x in 0..self.snow_nutrients.nx {
                    let i = self.snow_nutrients.index(x, y);
                    let snow = self.snow_nutrients.data[i];
                    if snow > 0.0 {
                        let p = Vec2::new((x as f32 + 0.5) * cell, (y as f32 + 0.5) * cell);
                        let released = release(snow, self.oxygen.sample(p));
                        self.snow_nutrients.data[i] -= released;
                        self.nutrients.add(p, released * ratio);
                    }
                }
            }
        }

        // Dissolved gases change slowly; update them a few times a second.
        self.chem_clock += dt;
        if self.chem_clock >= 0.2 {
            let elapsed = std::mem::take(&mut self.chem_clock);
            self.update_chemistry(elapsed, config);
        }

        self.since_temperature_update += dt;
        if self.since_temperature_update >= tc.update_interval {
            self.update_temperature(config);
        }
    }

    /// Diffusion, exchange with the air, ventilation of deep water, and toxin breakdown.
    fn update_chemistry(&mut self, dt: f32, config: &Config) {
        let cc = &config.chemistry;
        let ac = &config.atmosphere;
        let rate = cc.diffusion * dt / (self.oxygen.cell * self.oxygen.cell);
        let mut fields = vec![&mut self.oxygen, &mut self.co2, &mut self.nutrients];
        if self.toxin_present {
            fields.extend([
                &mut self.toxin,
                &mut self.toxin_hue_x,
                &mut self.toxin_hue_y,
            ]);
        }
        fields.into_par_iter().for_each(|f| f.diffuse(rate));

        if self.toxin_present {
            let keep = (-cc.toxin_decay * dt).exp();
            for f in [
                &mut self.toxin,
                &mut self.toxin_hue_x,
                &mut self.toxin_hue_y,
            ] {
                f.decay(keep);
            }
            // Once it has all but broken down, clear it and stop spending time on it.
            if self.toxin.total() * (self.toxin.cell_area() as f64) < 1e-3 {
                for f in [
                    &mut self.toxin,
                    &mut self.toxin_hue_x,
                    &mut self.toxin_hue_y,
                ] {
                    f.data.fill(0.0);
                }
                self.toxin_present = false;
            }
        }

        // Ventilation: every cell relaxes slowly toward its usual make-up at that depth.
        let vent = 1.0 - (-cc.ventilation * dt).exp();
        let atmosphere = self.atmosphere;
        for (field, kind) in [
            (&mut self.oxygen, Chemical::Oxygen),
            (&mut self.co2, Chemical::Co2),
            (&mut self.nutrients, Chemical::Nutrients),
        ] {
            let refs = reference_profile(kind, field, config, atmosphere);
            let nx = field.nx;
            let solid = &field.solid;
            field
                .data
                .par_chunks_mut(nx)
                .zip(solid.par_chunks(nx))
                .zip(refs.par_iter())
                .for_each(|((row, solid), &target)| {
                    for (v, &s) in row.iter_mut().zip(solid) {
                        if !s {
                            *v = (*v + (target - *v) * vent).max(0.0);
                        }
                    }
                });
        }

        // The surface trades gases with the air; the air changes by what it gains or loses.
        let exchange = 1.0 - (-ac.surface_exchange * dt).exp();
        let top = self.oxygen.ny - 1;
        let area = self.oxygen.cell_area();
        let mut flux = [0.0f64; 2];
        for (k, (field, level)) in [
            (&mut self.oxygen, atmosphere.oxygen),
            (&mut self.co2, atmosphere.co2),
        ]
        .into_iter()
        .enumerate()
        {
            for x in 0..field.nx {
                let i = field.index(x, top);
                if field.solid[i] {
                    continue;
                }
                let delta = (level - field.data[i]) * exchange;
                field.data[i] += delta;
                flux[k] += (delta * area) as f64;
            }
        }
        // Capacity: `size` times the gas the whole ocean holds when saturated.
        if self.water_cells == 0 {
            self.water_cells = self.oxygen.solid.iter().filter(|s| !**s).count();
        }
        let ocean = self.water_cells as f64 * area as f64;
        let capacity = (ac.size as f64 * ocean).max(1.0);
        self.atmosphere.oxygen =
            (self.atmosphere.oxygen as f64 - flux[0] / capacity).max(0.0) as f32;
        self.atmosphere.co2 = (self.atmosphere.co2 as f64 - flux[1] / capacity).max(0.0) as f32;
        self.clock.greenhouse = ac.greenhouse * self.atmosphere.co2.max(0.05).log2();
    }

    /// Rebuild the lure glow field from `(position, lure strength)` pairs.
    pub fn rebuild_glow(&mut self, lures: impl Iterator<Item = (Vec2, f32)>, config: &Config) {
        if self.glow.data.len() != self.minerals.data.len() {
            self.glow = self.minerals.clone();
            self.glow.solid.fill(false);
        }
        if self.any_glow {
            self.glow.data.fill(0.0);
        }
        self.any_glow = false;
        let pc = &config.parts;
        let brightness = pc.lure_brightness * config.light.surface_intensity;
        for (pos, lure) in lures {
            self.any_glow = true;
            let radius = (pc.lure_radius * lure).max(self.glow.cell);
            let inv = 1.0 / radius;
            let glow = &mut self.glow;
            let (x0, y0) = glow.coords(pos - Vec2::splat(radius));
            let (x1, y1) = glow.coords(pos + Vec2::splat(radius));
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let c = Vec2::new((x as f32 + 0.5) * glow.cell, (y as f32 + 0.5) * glow.cell);
                    let t = (1.0 - c.distance(pos) * inv).max(0.0);
                    let i = glow.index(x, y);
                    glow.data[i] += brightness * lure * t * t;
                }
            }
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
        inject(&mut self.minerals, min, max, amount);
    }

    /// Spread `amount` of nutrients over the water cells in the rectangle `min..max`.
    pub fn inject_nutrients(&mut self, min: Vec2, max: Vec2, amount: f32) {
        inject(&mut self.nutrients, min, max, amount);
    }

    /// Re-derive rock masks for the fields after the terrain changed between `x_min` and `x_max`.
    /// Content of cells that became rock is lost (it's a small amount).
    pub fn refresh_masks(&mut self, terrain: &Terrain, x_min: f32, x_max: f32) {
        for (field, holds_content) in [
            (&mut self.minerals, true),
            (&mut self.detritus, true),
            (&mut self.snow_nutrients, true),
            (&mut self.oxygen, true),
            (&mut self.co2, true),
            (&mut self.nutrients, true),
            (&mut self.toxin, true),
            (&mut self.toxin_hue_x, true),
            (&mut self.toxin_hue_y, true),
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
        self.water_cells = 0;
        self.cave_cells = None;
        let light = &mut self.light;
        let g = light.geometry();
        for x in g.column(x_min)..=g.column(x_max) {
            for y in 0..g.ny {
                let p = Vec2::new((x as f32 + 0.5) * g.cell, (y as f32 + 0.5) * g.cell);
                light.opaque[x * g.ny + y] = terrain.distance(p) < 0.0 || terrain.is_cave(p);
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

fn inject(f: &mut Field, min: Vec2, max: Vec2, amount: f32) {
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

#[derive(Clone, Copy)]
enum Chemical {
    Oxygen,
    Co2,
    Nutrients,
}

/// Usual concentration of a chemical in each row of `field` (surface water in equilibrium with
/// the air; deep water older, with less oxygen and more CO₂ and nutrients).
fn reference_profile(kind: Chemical, field: &Field, config: &Config, air: Atmosphere) -> Vec<f32> {
    let cc = &config.chemistry;
    let height = config.world.height;
    (0..field.ny)
        .map(|y| {
            let depth = (height - (y as f32 + 0.5) * field.cell).max(0.0);
            let surface = (-depth / cc.nutricline_depth.max(1.0)).exp();
            match kind {
                Chemical::Oxygen => {
                    air.oxygen * (cc.deep_oxygen + (1.0 - cc.deep_oxygen) * surface)
                }
                Chemical::Co2 => air.co2 * (1.3 - 0.3 * surface),
                Chemical::Nutrients => {
                    cc.nutrient_deep + (cc.nutrient_surface - cc.nutrient_deep) * surface
                }
            }
        })
        .collect()
}

/// The water cells within `radius` of `pos`.
fn disc_cells(field: &Field, pos: Vec2, radius: f32) -> Vec<usize> {
    let mut cells = Vec::new();
    let r = radius.max(field.cell * 0.5);
    let (x0, y0) = field.coords(pos - Vec2::splat(r));
    let (x1, y1) = field.coords(pos + Vec2::splat(r));
    for y in y0..=y1 {
        for x in x0..=x1 {
            let i = field.index(x, y);
            let c = Vec2::new((x as f32 + 0.5) * field.cell, (y as f32 + 0.5) * field.cell);
            if !field.solid[i] && c.distance(pos) <= r + field.cell * 0.5 {
                cells.push(i);
            }
        }
    }
    cells
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

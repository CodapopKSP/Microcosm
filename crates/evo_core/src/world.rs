//! The world and its tick pipeline.
//!
//! Each tick:
//! 1. Environment clock (and fields every few ticks).
//! 2. Rebuild spatial grids; deposit bodies into the light field; propagate light.
//! 3. **Think** (parallel, read-only): contacts, collision push, steering → one `Intent` per organism.
//! 4. **Act** (parallel, each organism mutates only itself): movement, metabolism, photosynthesis,
//!    temperature, healing, growth, aging.
//! 5. **Feed** (sequential, touches shared state): minerals, detritus, carcasses, hunting.
//! 6. Carcasses sink and decay; reproduction; deaths; seed bank; statistics.
//!
//! Everything is deterministic for a given seed and config: parallel phases only write
//! per-organism outputs, and every organism has its own RNG.

use glam::Vec2;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::environment::{EnvEvent, Environment};
use crate::events::{Events, WorldEvent, barrier_height};
use crate::genome::{Genome, color_distance, drive};
use crate::grid::{Entry, SpatialGrid};
use crate::organism::{Activity, Carcass, DeathCause, Organism};
use crate::rng::{Rng, mix64};
use crate::species::{SpeciesEvent, SpeciesRegistry};
use crate::stats::{Event, EventKind, EventLog, Milestones, StatSample, Stats};
use crate::terrain::Terrain;

/// Field (minerals, detritus, vents) update interval in ticks.
const FIELD_INTERVAL: u64 = 3;
/// Max organisms considered per perception query (bounds cost in dense crowds).
const PERCEPTION_LIMIT: u32 = 96;
const NONE: u32 = u32::MAX;
/// Ticks between steering decisions of motile organisms.
const STEER_INTERVAL: u64 = 2;
/// Drives weaker than this are ignored (skips sensing work for genes that drifted near neutral).
const MIN_DRIVE: f32 = 0.02;
const SAVE_MAGIC: &[u8] = b"EVOSAVE1";

#[derive(Clone, Copy, Debug, Default)]
struct Intent {
    heading: f32,
    throttle: f32,
    push: Vec2,
    attack: u32,
    carcass: u32,
    activity: Activity,
}

/// Accumulated wall-clock time per phase (seconds), for profiling and the debug overlay.
#[derive(Clone, Debug, Default)]
pub struct Timings {
    pub ticks: u64,
    pub environment: f64,
    pub staging: f64,
    pub grids: f64,
    pub light: f64,
    pub think: f64,
    pub act: f64,
    pub feed: f64,
    pub life: f64,
}

impl Timings {
    pub fn phases(&self) -> [(&'static str, f64); 8] {
        [
            ("environment", self.environment),
            ("staging", self.staging),
            ("grids", self.grids),
            ("light", self.light),
            ("think", self.think),
            ("act", self.act),
            ("feed", self.feed),
            ("life", self.life),
        ]
    }

    /// Mean milliseconds per tick for each phase.
    pub fn per_tick_ms(&self) -> [(&'static str, f64); 8] {
        let n = self.ticks.max(1) as f64;
        self.phases().map(|(name, t)| (name, t * 1000.0 / n))
    }
}

struct Scratch {
    grid: SpatialGrid,
    carcass_grid: SpatialGrid,
    intents: Vec<Intent>,
    entries: Vec<Entry>,
    carcass_entries: Vec<Entry>,
}

impl Scratch {
    fn new(config: &Config) -> Self {
        let (w, h) = (config.world.width, config.world.height);
        Self {
            grid: SpatialGrid::new(w, h, 16.0, 4.0, 4),
            carcass_grid: SpatialGrid::new(w, h, 64.0, 4.0, 3),
            intents: Vec::new(),
            entries: Vec::new(),
            carcass_entries: Vec::new(),
        }
    }
}

/// The whole simulation state.
///
/// Save compatibility: saves use named fields, so a field added later must carry
/// `#[serde(default)]` (or `default = "..."`) for older saves to keep loading.
#[derive(Serialize, Deserialize)]
pub struct World {
    pub config: Config,
    pub tick: u64,
    pub organisms: Vec<Organism>,
    pub carcasses: Vec<Carcass>,
    pub env: Environment,
    pub terrain: Terrain,
    pub species: SpeciesRegistry,
    #[serde(default)]
    pub stats: Stats,
    #[serde(default)]
    pub events: EventLog,
    #[serde(default)]
    pub milestones: Milestones,
    /// Active random events (plagues, upwellings, impact winters, barriers).
    pub happenings: Events,
    pub ancestor: Genome,
    pub next_id: u64,
    rng: Rng,
    #[serde(skip)]
    scratch: Option<Scratch>,
    #[serde(skip)]
    pub timings: Timings,
}

#[inline]
fn wrap_angle(a: f32) -> f32 {
    let mut a = a % std::f32::consts::TAU;
    if a > std::f32::consts::PI {
        a -= std::f32::consts::TAU;
    } else if a < -std::f32::consts::PI {
        a += std::f32::consts::TAU;
    }
    a
}

/// Unit vector scaled so its length saturates at 1.
#[inline]
fn saturate(v: Vec2) -> Vec2 {
    let l = v.length();
    if l > 1.0 { v / l } else { v }
}

/// Direction of a gradient, with a magnitude that saturates for strong gradients.
#[inline]
fn gradient_drive(g: Vec2, gain: f32) -> Vec2 {
    let l = g.length();
    if l < 1e-9 {
        Vec2::ZERO
    } else {
        g / l * (l * gain).min(1.0)
    }
}

/// Mutable references to two distinct elements.
fn pair_mut<T>(v: &mut [T], i: usize, j: usize) -> (&mut T, &mut T) {
    assert_ne!(i, j);
    if i < j {
        let (a, b) = v.split_at_mut(j);
        (&mut a[i], &mut b[0])
    } else {
        let (a, b) = v.split_at_mut(i);
        (&mut b[0], &mut a[j])
    }
}

struct ThinkCtx<'a> {
    config: &'a Config,
    grid: &'a SpatialGrid,
    carcass_grid: &'a SpatialGrid,
    env: &'a Environment,
    tick: u64,
}

fn think(i: usize, o: &Organism, ctx: &ThinkCtx) -> Intent {
    let cfg = ctx.config;
    let t = &o.traits;
    let r = o.radius;
    let pos = o.pos;
    let mut intent = Intent {
        heading: o.heading,
        throttle: 0.0,
        push: Vec2::ZERO,
        attack: NONE,
        carcass: NONE,
        activity: Activity::Drifting,
    };
    let kin_threshold = cfg.species.kin_threshold;
    let is_kin =
        |e: &Entry| color_distance(t.hue, t.saturation, e.hue, e.saturation) < kin_threshold;

    // Contacts: soft collision push and a prey candidate.
    let mass = r * r;
    let hunts = t.hunt > 0.0;
    let mut prey: Option<(u32, f32)> = None;
    let mut push = Vec2::ZERO;
    ctx.grid.query(pos, r, 0.0, |e| {
        if e.index as usize == i {
            return true;
        }
        let d = e.pos - pos;
        let min = r + e.radius;
        let dist2 = d.length_squared();
        if dist2 >= min * min {
            return true;
        }
        let dist = dist2.sqrt();
        let dir = if dist > 1e-4 {
            d / dist
        } else {
            let h = mix64((i as u64) << 32 | e.index as u64);
            Vec2::from_angle((h >> 40) as f32 / (1u64 << 24) as f32 * std::f32::consts::TAU)
        };
        let other_mass = e.radius * e.radius;
        push -= dir * (min - dist) * (other_mass / (mass + other_mass));
        if hunts && !is_kin(e) && prey.is_none_or(|(_, pr)| e.radius < pr) {
            prey = Some((e.index, e.radius));
        }
        true
    });
    intent.push = saturate(push * cfg.physics.push_stiffness / r.max(1e-3)) * r;
    if let Some((j, _)) = prey {
        intent.attack = j;
    }

    // Carcass contact.
    let eats_carcasses = t.scavenge > 0.0 || t.hunt > 0.0;
    if eats_carcasses && !ctx.carcass_grid.is_empty() {
        ctx.carcass_grid.query(pos, r, 0.0, |e| {
            if e.pos.distance_squared(pos) < (r + e.radius) * (r + e.radius) {
                intent.carcass = e.index;
                return false;
            }
            true
        });
    }

    if !t.is_motile() {
        return intent;
    }
    // Steering decisions are made every STEER_INTERVAL ticks (staggered by id); in between,
    // keep swimming on the last decision.
    if (o.id + ctx.tick) % STEER_INTERVAL != 0 {
        intent.heading = o.steer_heading;
        intent.throttle = o.steer_throttle;
        intent.activity = o.activity;
        return intent;
    }

    // Steering by drives.
    let d = &t.drives;
    let hunger = 1.0 - (o.energy / o.max_energy().max(1e-6)).clamp(0.0, 1.0);
    let fc = &cfg.feeding;
    let food = fc.hunger_min + (fc.hunger_max - fc.hunger_min) * hunger;
    let probe = (2.0 * r).max(24.0);
    let env = ctx.env;

    let mut foraging = Vec2::ZERO;
    if d[drive::LIGHT].abs() > MIN_DRIVE {
        let g = env.light.relative_gradient(pos, probe);
        foraging += d[drive::LIGHT] * food * gradient_drive(g, 8.0);
    }
    if d[drive::MINERALS].abs() > MIN_DRIVE {
        let here = env.minerals.sample(pos);
        let g = env.minerals.gradient(pos, probe.max(env.minerals.cell)) / (here + 1e-3);
        foraging += d[drive::MINERALS] * food * gradient_drive(g, 30.0);
    }
    let mut carrion = Vec2::ZERO;
    if d[drive::CARRION].abs() > MIN_DRIVE {
        let here = env.detritus.sample(pos);
        let g = env.detritus.gradient(pos, probe.max(env.detritus.cell)) / (here + 1e-3);
        carrion += gradient_drive(g, 30.0);
    }
    let mut steer = foraging;
    if d[drive::WARMTH].abs() > MIN_DRIVE {
        let g = env
            .temperature
            .gradient(pos, probe.max(env.temperature.cell));
        steer += d[drive::WARMTH] * gradient_drive(g, 20.0);
    }
    steer.y += d[drive::RISE];

    let mut hunting = Vec2::ZERO;
    let mut fleeing = Vec2::ZERO;
    let mut schooling = Vec2::ZERO;
    let range = (t.sense_radii * r).min(cfg.genes.sense_cap);
    if range > r {
        let forward = Vec2::from_angle(o.heading);
        let mut nearest_smaller = f32::MAX;
        let mut seen = 0u32;
        ctx.grid.query(pos, range, r / 8.0, |e| {
            if e.index as usize == i {
                return true;
            }
            let delta = e.pos - pos;
            let dist = delta.length();
            let reach = range + e.radius;
            if dist >= reach || dist < 1e-4 {
                return true;
            }
            let dir = delta / dist;
            if dir.dot(forward) < t.fov_cos {
                return true;
            }
            seen += 1;
            let closeness = 1.0 - dist / reach;
            if is_kin(e) {
                schooling += dir * closeness;
            } else if e.radius < 0.8 * r {
                if dist < nearest_smaller {
                    nearest_smaller = dist;
                    hunting = dir;
                }
            } else if e.radius > 1.25 * r {
                fleeing += dir * closeness;
            }
            seen < PERCEPTION_LIMIT
        });
        if d[drive::CARRION].abs() > MIN_DRIVE && !ctx.carcass_grid.is_empty() {
            let mut best = f32::MAX;
            ctx.carcass_grid.query(pos, range, 0.0, |e| {
                let dist = e.pos.distance(pos);
                if dist < best && dist < range + e.radius {
                    best = dist;
                    carrion = (e.pos - pos).normalize_or_zero();
                }
                true
            });
        }
    }
    let hunting = d[drive::SMALLER] * food * hunting;
    let fleeing = d[drive::LARGER] * saturate(fleeing);
    let schooling = d[drive::KIN] * saturate(schooling);
    let carrion = d[drive::CARRION] * food * saturate(carrion);
    steer += hunting + fleeing + schooling + carrion;

    let magnitude = steer.length();
    let mut rng = Rng::new(o.rng_seed_for(ctx.tick));
    if magnitude > 0.05 {
        intent.heading = steer.to_angle();
        intent.throttle = magnitude.min(1.0).max(t.restlessness * 0.5);
        // Label by the strongest component.
        let parts = [
            (foraging.length() + carrion.length(), Activity::Foraging),
            (hunting.length(), Activity::Hunting),
            (
                if d[drive::LARGER] < 0.0 {
                    fleeing.length()
                } else {
                    0.0
                },
                Activity::Fleeing,
            ),
            (schooling.length(), Activity::Schooling),
        ];
        intent.activity = parts
            .iter()
            .filter(|p| p.0 > 0.05)
            .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
            .map(|p| p.1)
            .unwrap_or(Activity::Wandering);
    } else if t.restlessness > 0.0 {
        intent.throttle = t.restlessness * 0.6;
        // Occasional random turns.
        if rng.chance(t.restlessness * 1.5 * cfg.time.dt) {
            intent.heading = o.heading + rng.signed() * std::f32::consts::FRAC_PI_2;
        }
        intent.activity = Activity::Wandering;
    }
    intent
}

struct ActCtx<'a> {
    config: &'a Config,
    env: &'a Environment,
    terrain: &'a Terrain,
    time: f32,
}

fn act(o: &mut Organism, intent: &Intent, ctx: &ActCtx) {
    let cfg = ctx.config;
    let dt = cfg.time.dt;
    let met = &cfg.metabolism;
    let phys = &cfg.physics;
    let t = o.traits;

    o.age += dt;
    o.repro_cooldown -= dt;
    if intent.activity != Activity::Feeding {
        o.activity = intent.activity;
    }
    o.steer_heading = intent.heading;
    o.steer_throttle = intent.throttle;

    // Movement.
    let r = o.radius;
    if t.is_motile() {
        let diff = wrap_angle(intent.heading - o.heading);
        let max_turn = t.turn_rate * dt;
        o.heading = wrap_angle(o.heading + diff.clamp(-max_turn, max_turn));
    }
    let target = Vec2::from_angle(o.heading) * intent.throttle * t.max_speed(r);
    let response = (phys.response / r.sqrt() * dt).min(1.0);
    o.vel += (target - o.vel) * response;
    let drift = Vec2::new(0.0, t.buoyancy * phys.buoyancy_speed * r.sqrt());
    let brownian = if phys.brownian > 0.0 {
        Vec2::from_angle(o.rng.angle()) * (phys.brownian / r * o.rng.f32())
    } else {
        Vec2::ZERO
    };
    let water = crate::currents::flow(
        o.pos,
        ctx.time,
        cfg.world.height,
        &cfg.currents,
        cfg.world.seed,
    );
    o.pos += (o.vel + drift + brownian + water) * dt + intent.push;
    ctx.terrain.collide(&mut o.pos, &mut o.vel, r);

    // Energy.
    let m75 = o.mass.powf(0.75);
    let temp = ctx.env.temperature.sample(o.pos);
    o.temperature = temp;
    let stress = ((temp - t.temp_pref) / t.temp_tol).powi(2);
    let basal = met.basal * m75 * t.upkeep * (1.0 + met.temp_stress_cost * stress.min(4.0));
    let movement = met.basal * m75 * met.move_cost * t.speed * intent.throttle * intent.throttle;
    let mut photo = 0.0;
    if t.pigment > 0.0 {
        let light = &ctx.env.light;
        light.for_each_covered(o.prev_pos, r, |idx, w| photo += light.share[idx] * w);
        photo *= t.pigment * cfg.light.photo_yield;
    }
    o.energy += (photo - basal - movement) * dt;
    o.gain_rate = photo;
    o.cost_rate = basal + movement;

    let max_hp = o.max_hp();
    let mut temperature_damage = 0.0;
    if stress > 1.0 {
        temperature_damage = met.temp_damage * (stress - 1.0).min(10.0) * max_hp * dt;
        o.hp -= temperature_damage;
    }
    if o.energy < 0.0 {
        o.hp -= -o.energy * met.starvation_damage;
        o.energy = 0.0;
    }

    // Regeneration.
    if o.hp < max_hp && o.energy > 0.0 {
        let heal = (t.regen * max_hp * dt)
            .min(max_hp - o.hp)
            .min(o.energy / met.regen_cost);
        o.hp += heal;
        o.energy -= heal * met.regen_cost;
        o.cost_rate += heal * met.regen_cost / dt;
    }

    // Growth toward adult size.
    if o.mass < t.adult_mass {
        let buffer = met.growth_buffer * o.max_energy();
        if o.energy > buffer {
            let invest = (o.energy - buffer)
                .min(met.growth_rate * o.mass * dt * met.build_cost)
                .min((t.adult_mass - o.mass) * met.build_cost);
            o.energy -= invest;
            let old_max_hp = max_hp;
            o.set_mass(o.mass + invest / met.build_cost);
            // Growing adds HP proportionally so a healthy juvenile stays healthy.
            o.hp += o.max_hp() - old_max_hp;
        }
    }
    o.energy = o.energy.min(o.max_energy());
    o.hp = o.hp.min(o.max_hp());

    if o.age > t.lifespan && o.rng.chance(met.old_age_death_rate * dt) {
        o.dead = Some(DeathCause::OldAge);
    } else if o.hp <= 0.0 {
        o.dead = Some(if temperature_damage > 0.0 && o.energy > 0.0 {
            DeathCause::Temperature
        } else {
            DeathCause::Starvation
        });
    }
}

impl Organism {
    /// Deterministic per-tick seed for read-only phases (think) that can't advance `rng`.
    #[inline]
    fn rng_seed_for(&self, tick: u64) -> u64 {
        self.id.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ tick.wrapping_mul(0xD1B5_4A32_D192_ED03)
    }
}

impl World {
    pub fn new(config: Config) -> World {
        let terrain = Terrain::generate(&config);
        let env = Environment::new(&config, &terrain);
        let ancestor = Genome::parse(&config.population.ancestor).expect("config validated");
        let species = SpeciesRegistry::new(&ancestor, &config);
        let rng = Rng::new(config.world.seed);
        let happenings = Events::new(config.world.seed);
        let mut world = World {
            config,
            tick: 0,
            organisms: Vec::new(),
            carcasses: Vec::new(),
            env,
            terrain,
            species,
            stats: Stats::default(),
            events: EventLog::default(),
            milestones: Milestones::default(),
            happenings,
            ancestor,
            next_id: 1,
            rng,
            scratch: None,
            timings: Timings::default(),
        };
        let initial = world.config.population.initial;
        world.spawn_ancestors(initial);
        world.log(
            EventKind::Milestone,
            "Life begins: the first microbes drift in the sunlit shallows.".into(),
            None,
            None,
        );
        world.sample_stats();
        world
    }

    /// Serialize the whole world (for autosave / resume).
    ///
    /// Format: magic, then lz4-compressed MessagePack with field names. Named fields mean
    /// saves survive code changes: new fields take their defaults, removed ones are ignored.
    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        let packed = rmp_serde::to_vec_named(self).map_err(|e| e.to_string())?;
        let mut bytes = SAVE_MAGIC.to_vec();
        bytes.extend(lz4_flex::compress_prepend_size(&packed));
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    pub fn load(path: &std::path::Path) -> Result<World, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let body = bytes
            .strip_prefix(SAVE_MAGIC)
            .ok_or_else(|| format!("{}: not an evolution save file", path.display()))?;
        let packed = lz4_flex::decompress_size_prepended(body).map_err(|e| e.to_string())?;
        let mut world: World =
            rmp_serde::from_slice(&packed).map_err(|e| format!("{}: {e}", path.display()))?;
        world.after_load();
        Ok(world)
    }

    /// Rebuild derived state that isn't saved (decoded traits).
    fn after_load(&mut self) {
        for o in &mut self.organisms {
            o.traits = crate::traits::Traits::decode(&o.genome, &self.config);
        }
    }

    pub fn time(&self) -> f64 {
        self.tick as f64 * self.config.time.dt as f64
    }

    fn log(&mut self, kind: EventKind, text: String, organism: Option<u64>, species: Option<u32>) {
        let event = Event {
            tick: self.tick,
            time: self.time(),
            kind,
            text,
            organism,
            species,
        };
        self.events.push(event, self.config.stats.max_events);
    }

    /// Spawn `count` adult ancestors in the sunlit zone.
    fn spawn_ancestors(&mut self, count: u32) {
        let cfg = &self.config;
        let (w, h) = (cfg.world.width, cfg.world.height);
        for _ in 0..count {
            let mut rng = self.rng.fork();
            let traits = crate::traits::Traits::decode(&self.ancestor, cfg);
            let r = traits.adult_radius;
            let mut pos = None;
            for _ in 0..50 {
                let p = Vec2::new(
                    rng.range(0.0, w),
                    h - rng.range(r, cfg.population.spawn_depth),
                );
                if self.terrain.is_free(p, r) {
                    pos = Some(p);
                    break;
                }
            }
            let Some(pos) = pos else { continue };
            let mass = traits.adult_mass;
            let energy = 0.5 * mass * traits.reserve_factor;
            let id = self.next_id;
            self.next_id += 1;
            let o = Organism::new(
                id,
                0,
                0,
                0,
                self.ancestor,
                0,
                cfg,
                pos,
                mass,
                energy,
                rng,
                self.tick,
            );
            self.species.on_birth(0, cfg, self.tick);
            self.organisms.push(o);
        }
        self.species.events.clear();
    }

    /// Advance one tick.
    pub fn step(&mut self) {
        let scratch = self
            .scratch
            .get_or_insert_with(|| Scratch::new(&self.config));
        self.tick += 1;
        let tick = self.tick;
        let config = &self.config;
        let dt = config.time.dt;

        let mut clock = std::time::Instant::now();
        let mut lap = |slot: &mut f64| {
            let now = std::time::Instant::now();
            *slot += (now - clock).as_secs_f64();
            clock = now;
        };
        let mut timings = std::mem::take(&mut self.timings);
        timings.ticks += 1;

        // 1. Environment and events.
        self.env.update_clock(tick, config);
        let time = tick as f64 * dt as f64;
        let effects = self.happenings.update(time, dt, config);
        self.env.clock.sun *= self.happenings.sun_factor(time);
        let mut env_events = Vec::new();
        if tick.is_multiple_of(FIELD_INTERVAL) {
            let field_dt = dt * FIELD_INTERVAL as f32;
            self.env
                .update_fields(field_dt, config, &self.terrain, &mut env_events);
            for e in &self.happenings.active {
                if let WorldEvent::Upwelling { x, width, rate, .. } = e {
                    let h = config.world.height;
                    self.env.inject_minerals(
                        Vec2::new(x - width * 0.5, h - 600.0),
                        Vec2::new(x + width * 0.5, h),
                        rate * field_dt,
                    );
                }
            }
        }

        lap(&mut timings.environment);

        // 2. Grids and light. One parallel pass over the (large) organism structs builds a
        // compact entry array; everything else in this phase reads only that.
        self.organisms
            .par_iter_mut()
            .enumerate()
            .map(|(i, o)| {
                o.prev_pos = o.pos;
                Entry {
                    index: i as u32,
                    pos: o.pos,
                    radius: o.radius,
                    hue: o.traits.hue,
                    saturation: o.traits.saturation,
                    absorptance: o.traits.absorptance,
                }
            })
            .collect_into_vec(&mut scratch.entries);
        lap(&mut timings.staging);
        scratch.grid.rebuild(&scratch.entries);
        scratch.carcass_entries.clear();
        scratch
            .carcass_entries
            .extend(self.carcasses.iter().enumerate().map(|(i, c)| Entry {
                index: i as u32,
                pos: c.pos,
                radius: c.radius,
                ..Default::default()
            }));
        scratch.carcass_grid.rebuild(&scratch.carcass_entries);
        lap(&mut timings.grids);
        {
            let light = &mut self.env.light;
            light.weight.fill(0.0);
            let geometry = light.geometry();
            for e in &scratch.entries {
                let a = e.absorptance;
                geometry.for_each_covered(e.pos, e.radius, |idx, w| light.weight[idx] += w * a);
            }
            light.propagate(self.env.clock.sun, config.light.half_depth);
        }
        lap(&mut timings.light);

        // 3. Think.
        let think_ctx = ThinkCtx {
            config,
            grid: &scratch.grid,
            carcass_grid: &scratch.carcass_grid,
            env: &self.env,
            tick,
        };
        self.organisms
            .par_iter()
            .enumerate()
            .map(|(i, o)| think(i, o, &think_ctx))
            .collect_into_vec(&mut scratch.intents);

        lap(&mut timings.think);

        // 4. Act.
        let act_ctx = ActCtx {
            config,
            env: &self.env,
            terrain: &self.terrain,
            time: (tick as f64 * dt as f64) as f32,
        };
        self.organisms
            .par_iter_mut()
            .zip(scratch.intents.par_iter())
            .for_each(|(o, intent)| act(o, intent, &act_ctx));

        lap(&mut timings.act);

        self.apply_events(effects, time);

        // 5. Feed.
        self.feed();
        lap(&mut timings.feed);

        // 6. Carcasses, reproduction, deaths, seed bank.
        self.update_carcasses();
        self.reproduce();
        self.remove_dead();
        if (self.organisms.len() as u32) < self.config.population.min_population {
            let n = self.config.population.reseed_count;
            self.spawn_ancestors(n);
            self.log(
                EventKind::Recolonization,
                format!("Life clings on: {n} ancestral microbes recolonize the shallows."),
                None,
                Some(0),
            );
        }

        // 7. Events and statistics.
        for e in env_events {
            match e {
                EnvEvent::VentOpened(p) => self.log(
                    EventKind::Environment,
                    format!("A hydrothermal vent opens on the seafloor at x={:.0}.", p.x),
                    None,
                    None,
                ),
                EnvEvent::VentDied(p) => self.log(
                    EventKind::Environment,
                    format!("The vent at x={:.0} falls silent.", p.x),
                    None,
                    None,
                ),
            }
        }
        let species_events = std::mem::take(&mut self.species.events);
        for e in species_events {
            match e {
                SpeciesEvent::Established(id) => {
                    let s = &self.species.species[&id];
                    let parent = s.parent.map(|p| self.species.name(p).to_string());
                    let text = match parent {
                        Some(p) => format!("New species: {} (descended from {p}).", s.name),
                        None => format!("New species: {}.", s.name),
                    };
                    self.log(EventKind::Species, text, None, Some(id));
                }
                SpeciesEvent::Extinct(id) => {
                    let s = &self.species.species[&id];
                    let text = format!(
                        "Extinction: {} is gone (peak population {}).",
                        s.name, s.peak
                    );
                    self.log(EventKind::Extinction, text, None, Some(id));
                }
            }
        }
        self.stats.since_sample += dt;
        if self.stats.since_sample >= self.config.stats.sample_interval {
            self.sample_stats();
        }
        lap(&mut timings.life);
        self.timings = timings;
    }

    /// Start events rolled this tick, apply ongoing ones (plague deaths, barriers), log endings.
    fn apply_events(&mut self, fx: crate::events::EventEffects, time: f64) {
        let dt = self.config.time.dt;
        if fx.start_plague {
            let total = self.organisms.len().max(1) as f32;
            let dominant = self
                .species
                .living()
                .max_by_key(|s| s.population)
                .map(|s| (s.id, s.population));
            if let Some((id, pop)) = dominant
                && pop as f32 / total >= self.config.events.plague_min_share
            {
                self.happenings.add_plague(id, time, &self.config);
                let name = self.species.name(id).to_string();
                self.log(
                    EventKind::Disaster,
                    format!("Plague! A disease sweeps through {name}, {pop} strong."),
                    None,
                    Some(id),
                );
            }
        }
        if let Some((x, _)) = fx.start_upwelling {
            self.log(
                EventKind::Environment,
                format!("Cold, mineral-rich water wells up to the surface near x={x:.0}."),
                None,
                None,
            );
        }
        if let Some(p) = fx.impact {
            let r = self.config.events.impact_radius;
            let mut killed = 0;
            for o in &mut self.organisms {
                if o.dead.is_none() && o.pos.distance(p) < r + o.radius {
                    o.dead = Some(DeathCause::Catastrophe);
                    killed += 1;
                }
            }
            self.env.inject_minerals(
                p - Vec2::splat(r),
                p + Vec2::splat(r),
                self.config.events.impact_minerals,
            );
            self.log(
                EventKind::Disaster,
                format!("A meteor strikes the ocean at x={:.0}, killing {killed}! Dust darkens the sky.", p.x),
                None,
                None,
            );
        }
        if let Some((x, _)) = fx.start_barrier {
            self.log(
                EventKind::Environment,
                format!("The seafloor heaves: a wall of rock begins to rise at x={x:.0}, splitting the ocean."),
                None,
                None,
            );
        }

        // Plague deaths (the stricken species and its unnamed offshoots).
        if self
            .happenings
            .is_active(|e| matches!(e, WorldEvent::Plague { .. }))
        {
            let mut rates: std::collections::HashMap<u32, f32> = std::collections::HashMap::new();
            for o in &mut self.organisms {
                if o.dead.is_some() {
                    continue;
                }
                let rate = *rates.entry(o.species).or_insert_with(|| {
                    let root = self
                        .species
                        .established_ancestor(o.species)
                        .unwrap_or(o.species);
                    self.happenings.plague_rate(root)
                });
                if rate > 0.0 && o.rng.chance(rate * dt) {
                    o.dead = Some(DeathCause::Disease);
                }
            }
        }

        // Barriers: rebuild terrain where a wall's height changed, or where one ended.
        let mut dirty: Vec<(f32, f32)> = Vec::new();
        let height = self.config.world.height;
        for e in &mut self.happenings.active {
            let WorldEvent::Barrier { x, width, .. } = *e else {
                continue;
            };
            let floor = self.terrain.floor_at(x);
            let top = barrier_height(e, time, floor, height).unwrap_or(floor);
            if let WorldEvent::Barrier { built, .. } = e
                && (top - *built).abs() > 24.0
            {
                *built = top;
                dirty.push((x - width * 1.5, x + width * 1.5));
            }
        }
        for e in &fx.ended {
            match e {
                WorldEvent::Barrier { x, width, .. } => {
                    dirty.push((x - width * 1.5, x + width * 1.5));
                    self.log(
                        EventKind::Environment,
                        format!("The wall at x={x:.0} has sunk back into the seafloor. The waters mix again."),
                        None,
                        None,
                    );
                }
                WorldEvent::ImpactWinter { .. } => self.log(
                    EventKind::Environment,
                    "The dust settles and the sun returns.".into(),
                    None,
                    None,
                ),
                WorldEvent::Plague { species, .. } => {
                    let name = self.species.name(*species).to_string();
                    self.log(
                        EventKind::Environment,
                        format!("The plague among {name} has run its course."),
                        None,
                        Some(*species),
                    )
                }
                WorldEvent::Upwelling { .. } => {}
            }
        }
        if !dirty.is_empty() {
            let walls: Vec<crate::terrain::Wall> = self
                .happenings
                .active
                .iter()
                .filter_map(|e| match e {
                    WorldEvent::Barrier {
                        id,
                        x,
                        width,
                        built,
                        ..
                    } => Some(crate::terrain::Wall {
                        x: *x,
                        width: *width,
                        top: *built,
                        seed: *id as u64 ^ 0xBA11,
                    }),
                    _ => None,
                })
                .collect();
            for (a, b) in dirty {
                let (a, b) = self.terrain.apply_walls(&walls, a, b);
                self.env.refresh_masks(&self.terrain, a, b);
            }
        }
    }

    fn feed(&mut self) {
        let config = &self.config;
        let fc = &config.feeding;
        let dt = config.time.dt;
        let body_energy = config.metabolism.body_energy;
        let min_mass = config.genes.min_radius * config.genes.min_radius * 0.5;
        let intents = &self.scratch.as_ref().unwrap().intents;
        let minerals = &mut self.env.minerals;
        let detritus = &mut self.env.detritus;
        let m_area = minerals.cell_area();
        let d_area = detritus.cell_area();
        let mut first_kill = None;

        for i in 0..self.organisms.len() {
            if self.organisms[i].dead.is_some() {
                continue;
            }
            let intent = intents[i];
            let o = &mut self.organisms[i];
            // Only the fields feeding needs (Traits is large; don't copy it all).
            struct Diet {
                chemo: f32,
                scavenge: f32,
                hunt: f32,
                jaws: f32,
            }
            let t = Diet {
                chemo: o.traits.chemo,
                scavenge: o.traits.scavenge,
                hunt: o.traits.hunt,
                jaws: o.traits.jaws,
            };
            let r = o.radius;
            let mut gained = 0.0;

            if t.chemo > 0.0 {
                let idx = minerals.index_at(o.pos);
                let conc = minerals.data[idx];
                if conc > 0.0 {
                    let want = fc.chemo_rate * t.chemo * r * conc * dt;
                    let take = want.min(conc * m_area);
                    minerals.data[idx] -= take / m_area;
                    gained += take * fc.chemo_yield;
                }
            }
            if t.scavenge > 0.0 {
                let idx = detritus.index_at(o.pos);
                let conc = detritus.data[idx];
                if conc > 0.0 {
                    let sweep = fc.scavenge_rate * r + fc.filter_rate * 2.0 * r * o.vel.length();
                    let want = sweep * t.scavenge * conc * dt;
                    let take = want.min(conc * d_area);
                    detritus.data[idx] -= take / d_area;
                    gained += take;
                }
            }
            if intent.carcass != NONE
                && let Some(c) = self.carcasses.get_mut(intent.carcass as usize)
                && c.energy > 0.0
                && c.pos.distance(o.pos) < c.radius + r
            {
                let skill = t.scavenge.max(0.7 * t.hunt);
                let rate = fc.carcass_rate * skill * r * r * (0.3 + 1.7 * t.jaws);
                let take = (rate * dt).min(c.energy);
                c.energy -= take;
                gained += take * fc.assimilation;
                detritus.add(o.pos, take * (1.0 - fc.assimilation) / d_area);
                o.activity = Activity::Feeding;
            }
            o.energy += gained;
            o.gain_rate += gained / dt;

            // Hunting.
            let j = intent.attack as usize;
            if intent.attack != NONE && j < self.organisms.len() && j != i {
                let (hunter, prey) = pair_mut(&mut self.organisms, i, j);
                if prey.dead.is_none()
                    && prey.pos.distance(hunter.pos) < prey.radius + hunter.radius
                {
                    let protection = 1.0 - fc.armor_protection * prey.traits.armor;
                    let mut taken_mass = 0.0;
                    let mut taken_energy = 0.0;
                    if prey.radius <= fc.engulf_ratio * hunter.radius {
                        let p = fc.engulf_rate * t.hunt * protection * dt;
                        if hunter.rng.chance(p) {
                            taken_mass = prey.mass;
                            taken_energy = prey.body_value(config);
                            prey.dead = Some(DeathCause::Eaten);
                        }
                    } else {
                        let bite =
                            fc.bite_rate * t.hunt * (0.2 + 1.8 * t.jaws) * r * r * dt * protection;
                        let dm = bite.min(prey.mass);
                        if dm > 0.0 {
                            let frac = dm / prey.mass;
                            let reserves = prey.energy.max(0.0) * frac;
                            prey.energy -= reserves;
                            taken_mass = dm;
                            taken_energy = dm * body_energy + reserves;
                            prey.hp -= dm * prey.traits.hp_factor;
                            prey.set_mass(prey.mass - dm);
                            if prey.hp <= 0.0 || prey.mass < min_mass {
                                prey.dead = Some(DeathCause::Predation);
                            }
                        }
                    }
                    if taken_energy > 0.0 {
                        let assimilated = taken_energy * fc.assimilation * t.hunt;
                        hunter.energy += assimilated;
                        hunter.gain_rate += assimilated / dt;
                        detritus.add(hunter.pos, (taken_energy - assimilated) / d_area);
                        hunter.activity = Activity::Feeding;
                        if prey.traits.spikes > 0.0 {
                            hunter.hp -= fc.spike_damage * prey.traits.spikes * taken_mass;
                            if hunter.hp <= 0.0 {
                                hunter.dead = Some(DeathCause::Spikes);
                            }
                        }
                        if prey.dead.is_some() {
                            hunter.kills += 1;
                            if first_kill.is_none() {
                                first_kill = Some((hunter.id, hunter.species));
                            }
                        }
                    }
                }
            }

            // Anything the organism can't store is lost as detritus.
            let o = &mut self.organisms[i];
            let excess = o.energy - o.max_energy();
            if excess > 0.0 {
                o.energy -= excess;
                detritus.add(o.pos, excess / d_area);
            }
        }

        if let Some((id, species)) = first_kill
            && !self.milestones.first_kill
        {
            self.milestones.first_kill = true;
            let name = self.species.name(species).to_string();
            self.log(
                EventKind::Milestone,
                format!("First blood: a {name} kills another organism. Predation has begun."),
                Some(id),
                Some(species),
            );
        }
    }

    fn update_carcasses(&mut self) {
        let config = &self.config;
        let dt = config.time.dt;
        let phys = &config.physics;
        let body_energy = config.metabolism.body_energy;
        let time = self.time() as f32;
        let detritus = &mut self.env.detritus;
        let area = detritus.cell_area();
        for c in &mut self.carcasses {
            c.vel.y = -phys.carcass_sink * c.radius.sqrt();
            c.vel.x *= 0.9;
            let water = crate::currents::flow(
                c.pos,
                time,
                config.world.height,
                &config.currents,
                config.world.seed,
            );
            c.pos += (c.vel + water) * dt;
            self.terrain.collide(&mut c.pos, &mut c.vel, c.radius);
            let lost = c.energy * phys.carcass_decay * dt;
            c.energy -= lost;
            detritus.add(c.pos, lost / area);
            c.update_radius(body_energy);
        }
        self.carcasses.retain(|c| {
            if c.energy < 1.0 {
                detritus.add(c.pos, c.energy.max(0.0) / area);
                false
            } else {
                true
            }
        });
    }

    fn reproduce(&mut self) {
        let config = &self.config;
        let met = &config.metabolism;
        let rc = &config.reproduction;
        let min_mass = config.genes.min_radius * config.genes.min_radius;
        let max_population = config.population.max_population as usize;
        let tick = self.tick;
        let mut births: Vec<Organism> = Vec::new();
        let mut parents: Vec<usize> = Vec::new();
        let mut population = self.organisms.len();

        for i in 0..self.organisms.len() {
            let o = &mut self.organisms[i];
            if o.dead.is_some()
                || !o.is_adult()
                || o.repro_cooldown > 0.0
                || o.energy < o.traits.repro_threshold * o.max_energy()
            {
                continue;
            }
            if population >= max_population {
                break;
            }
            let t = o.traits;
            let child_mass = (t.adult_mass * t.birth_fraction).max(min_mass);
            let child_reserves = child_mass * t.reserve_factor * rc.newborn_reserves;
            let cost = child_mass * met.build_cost + child_reserves;
            let count = t.brood.min((o.energy / cost) as u32);
            if count == 0 {
                continue;
            }
            for _ in 0..count {
                let mut rng = o.rng.fork();
                let mut genome = o.genome;
                let mutations = genome.mutate(&mut rng, &config.mutation);
                let species = if mutations == 0 {
                    o.species
                } else {
                    self.species.classify(o.species, &genome, tick, config)
                };
                let child_r = child_mass.sqrt();
                let mut pos = o.pos;
                for _ in 0..6 {
                    let p = o.pos + Vec2::from_angle(rng.angle()) * (o.radius + child_r + 0.5);
                    if self.terrain.is_free(p, child_r) {
                        pos = p;
                        break;
                    }
                }
                let id = self.next_id;
                self.next_id += 1;
                let mut child = Organism::new(
                    id,
                    o.id,
                    species,
                    o.generation + 1,
                    genome,
                    mutations,
                    config,
                    pos,
                    child_mass,
                    child_reserves,
                    rng,
                    tick,
                );
                child.vel = o.vel;
                self.species.on_birth(species, config, tick);
                self.stats.record_birth();
                births.push(child);
                population += 1;
            }
            o.energy -= cost * count as f32;
            o.offspring += count;
            o.repro_cooldown = rc.cooldown;
            parents.push(i);
        }

        // Milestones count only once a lineage has proven itself by reproducing.
        for &i in &parents {
            let parent = &self.organisms[i];
            check_milestones(
                &mut self.milestones,
                &mut self.events,
                parent,
                &self.species,
                config,
                tick,
            );
        }
        self.organisms.extend(births);
    }

    fn remove_dead(&mut self) {
        let config = &self.config;
        let body_energy = config.metabolism.body_energy;
        let detritus = &mut self.env.detritus;
        let area = detritus.cell_area();
        let tick = self.tick;
        for o in &self.organisms {
            let Some(cause) = o.dead else { continue };
            self.species.on_death(o.species, tick);
            self.stats.record_death(cause);
            if cause == DeathCause::Eaten {
                continue;
            }
            let value = o.mass * body_energy + o.energy.max(0.0);
            if o.radius >= config.physics.carcass_min_radius {
                let mut c = Carcass {
                    pos: o.pos,
                    vel: o.vel * 0.5,
                    energy: value,
                    radius: o.radius,
                    color: o.traits.color,
                    species: o.species,
                };
                c.update_radius(body_energy);
                self.carcasses.push(c);
            } else {
                detritus.add(o.pos, value / area);
            }
        }
        // swap_remove from the back: O(deaths) instead of shifting the whole array.
        for i in (0..self.organisms.len()).rev() {
            if self.organisms[i].dead.is_some() {
                self.organisms.swap_remove(i);
            }
        }
    }

    fn sample_stats(&mut self) {
        let mut sample = StatSample {
            tick: self.tick,
            time: self.time(),
            population: self.organisms.len() as u32,
            births: self.stats.births_since_sample,
            deaths: self.stats.deaths_since_sample,
            sun: self.env.clock.sun,
            surface_temp: self.env.clock.surface_temp,
            ..Default::default()
        };
        let mut radius_sum = 0.0;
        for o in &self.organisms {
            sample.biomass += o.mass;
            sample.diets[o.traits.main_diet()] += 1;
            if o.traits.is_motile() {
                sample.motile += 1;
            }
            radius_sum += o.radius;
            sample.max_radius = sample.max_radius.max(o.radius);
        }
        sample.mean_radius = radius_sum / self.organisms.len().max(1) as f32;
        for s in self.species.living() {
            sample.species_populations.push((s.id, s.population));
        }
        sample.species = sample.species_populations.len() as u32;
        let max = self.config.stats.max_samples;
        self.stats.push(sample, max);
    }

    /// Start an event on the next tick (hotkeys, viewer commands).
    pub fn trigger(&mut self, trigger: crate::events::Trigger) {
        self.happenings.pending.push(trigger);
    }

    /// Organism by id (linear scan; fine for UI use).
    pub fn find(&self, id: u64) -> Option<&Organism> {
        self.organisms.iter().find(|o| o.id == id)
    }

    /// Organism under `pos`, preferring the smallest one that contains it.
    pub fn pick(&self, pos: Vec2, slack: f32) -> Option<u64> {
        self.organisms
            .iter()
            .filter(|o| o.pos.distance(pos) <= o.radius + slack)
            .min_by(|a, b| {
                let da = a.pos.distance(pos) / (a.radius + slack);
                let db = b.pos.distance(pos) / (b.radius + slack);
                da.partial_cmp(&db).unwrap()
            })
            .map(|o| o.id)
    }

    /// Total energy stored in organisms, carcasses, and fields (for conservation checks).
    pub fn total_energy(&self) -> f64 {
        let body = self.config.metabolism.body_energy as f64;
        let orgs: f64 = self
            .organisms
            .iter()
            .map(|o| o.mass as f64 * body + o.energy as f64)
            .sum();
        let carcasses: f64 = self.carcasses.iter().map(|c| c.energy as f64).sum();
        let (m, d) = self.env.field_totals();
        orgs + carcasses + m + d
    }
}

fn check_milestones(
    m: &mut Milestones,
    events: &mut EventLog,
    parent: &Organism,
    species: &SpeciesRegistry,
    config: &Config,
    tick: u64,
) {
    let t = &parent.traits;
    let time = tick as f64 * config.time.dt as f64;
    let name = species
        .established_ancestor(parent.species)
        .map(|s| species.name(s).to_string())
        .unwrap_or_else(|| "microbe".into());
    let mut log = |text: String| {
        events.push(
            Event {
                tick,
                time,
                kind: EventKind::Milestone,
                text,
                organism: Some(parent.id),
                species: Some(parent.species),
            },
            config.stats.max_events,
        );
    };
    let moves = t.is_motile() && (t.restlessness > 0.05 || t.drives.iter().any(|d| d.abs() > 0.05));
    if !m.first_swimmer && moves && t.max_speed(t.adult_radius) > 1.0 {
        m.first_swimmer = true;
        log(format!(
            "A {name} that can swim reproduces — the first motile lineage."
        ));
    }
    if !m.first_chemotroph && t.main_diet() == 1 && t.chemo > 0.25 {
        m.first_chemotroph = true;
        log(format!(
            "A {name} lineage turns to chemosynthesis, feeding on vent minerals."
        ));
    }
    if !m.first_scavenger && t.main_diet() == 2 && t.scavenge > 0.25 {
        m.first_scavenger = true;
        log(format!(
            "A {name} lineage becomes a scavenger, living off the dead."
        ));
    }
    if !m.first_hunter && t.main_diet() == 3 && t.hunt > 0.25 {
        m.first_hunter = true;
        log(format!("A {name} lineage becomes a dedicated hunter."));
    }
    if !m.first_eyes && t.sense_radii * t.adult_radius > 40.0 {
        m.first_eyes = true;
        log(format!(
            "A {name} lineage can sense its surroundings at a distance."
        ));
    }
    if !m.first_armor && t.armor > 0.3 {
        m.first_armor = true;
        log(format!("A {name} lineage grows armor."));
    }
    if m.size_record == 0.0 {
        m.size_record = t.adult_radius;
    } else if t.adult_radius > m.size_record * 1.25 && t.adult_radius >= 4.0 {
        m.size_record = t.adult_radius;
        log(format!(
            "Size record: a {name} of radius {:.1} reproduces.",
            t.adult_radius
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_config() -> Config {
        let mut c = Config::default();
        c.world.width = 4000.0;
        c.world.height = 3000.0;
        c.terrain.floor_height = 900.0;
        c.terrain.shelf_depth = 700.0;
        c.terrain.trench_depth = 400.0;
        c.vents.min_depth = 1500.0;
        c.population.initial = 300;
        c.population.min_population = 10;
        c
    }

    #[test]
    fn world_runs_and_life_persists() {
        let mut world = World::new(small_config());
        for _ in 0..3000 {
            world.step();
        }
        assert!(
            world.organisms.len() > 50,
            "population {}",
            world.organisms.len()
        );
        assert!(world.stats.total_births > 0);
        for o in &world.organisms {
            assert!(o.pos.is_finite() && o.energy.is_finite() && o.hp.is_finite());
            assert!(o.energy >= 0.0 && o.energy <= o.max_energy() + 1e-3);
        }
    }

    #[test]
    fn same_seed_same_world() {
        let run = || {
            let mut w = World::new(small_config());
            for _ in 0..600 {
                w.step();
            }
            (
                w.organisms.len(),
                w.stats.total_births,
                w.organisms.iter().map(|o| o.pos.x).sum::<f32>(),
            )
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let mut w = World::new(small_config());
        for _ in 0..100 {
            w.step();
        }
        let dir = std::env::temp_dir().join(format!("evo_save_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("world.bin");
        w.save(&path).unwrap();
        let mut loaded = World::load(&path).unwrap();
        assert_eq!(loaded.tick, w.tick);
        assert_eq!(loaded.organisms.len(), w.organisms.len());
        for _ in 0..50 {
            w.step();
            loaded.step();
        }
        assert_eq!(loaded.organisms.len(), w.organisms.len());
        assert_eq!(loaded.organisms[0].pos, w.organisms[0].pos);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn events_strike_and_pass() {
        let mut config = small_config();
        config.events.plague_every = 0.0;
        config.events.upwelling_every = 0.0;
        config.events.impact_every = 0.0;
        config.events.barrier_every = 0.0;
        let mut world = World::new(config);
        for _ in 0..200 {
            world.step();
        }
        let before = world.organisms.len();
        // Meteor at the surface in the middle of the world.
        let p = Vec2::new(2000.0, 2800.0);
        let fx = crate::events::EventEffects {
            impact: Some(p),
            ..Default::default()
        };
        world.apply_events(fx, world.time());
        world.remove_dead();
        assert!(
            world.organisms.len() < before,
            "impact should kill organisms"
        );
        assert!(world.organisms.iter().all(|o| o.pos.distance(p) >= 700.0));

        // A barrier: rock appears mid-water while it stands, and is gone afterwards.
        let t = world.time();
        world.happenings.active.push(WorldEvent::Barrier {
            id: 0,
            x: 2000.0,
            width: 200.0,
            started: t,
            rise: 10.0,
            stand: 20.0,
            built: 0.0,
        });
        let probe = Vec2::new(2000.0, 2000.0);
        assert!(world.terrain.distance(probe) > 0.0);
        for _ in 0..(20.0 / world.config.time.dt) as usize {
            world.step();
        }
        assert!(world.terrain.distance(probe) < 0.0, "wall should stand");
        assert!(
            world.env.light.level_at(Vec2::new(2000.0, 1500.0)) == 0.0,
            "wall casts shade"
        );
        for _ in 0..(30.0 / world.config.time.dt) as usize {
            world.step();
        }
        assert!(world.happenings.active.is_empty());
        assert!(world.terrain.distance(probe) > 0.0, "wall should be gone");
    }

    #[test]
    fn predator_eats_prey() {
        let mut config = small_config();
        config.population.initial = 0;
        config.population.min_population = 0;
        let mut world = World::new(config.clone());
        let mut hunter_g = world.ancestor;
        hunter_g.0[crate::genome::gene::SIZE] = 0x60; // radius 8
        hunter_g.0[crate::genome::gene::PHOTO] = 0;
        hunter_g.0[crate::genome::gene::HUNT] = 0xFF;
        hunter_g.0[crate::genome::gene::HUE] = 0x00; // not kin
        let pos = Vec2::new(2000.0, 2900.0);
        let hunter = Organism::new(
            1,
            0,
            0,
            0,
            hunter_g,
            0,
            &config,
            pos,
            64.0,
            10.0,
            Rng::new(1),
            0,
        );
        let prey = Organism::new(
            2,
            0,
            0,
            0,
            world.ancestor,
            0,
            &config,
            pos + Vec2::new(3.0, 0.0),
            4.0,
            1.0,
            Rng::new(2),
            0,
        );
        world.organisms.push(hunter);
        world.organisms.push(prey);
        world.species.on_birth(0, &config, 0);
        world.species.on_birth(0, &config, 0);
        let e0 = world.organisms[0].energy;
        for _ in 0..120 {
            world.step();
            if world.organisms.len() == 1 {
                break;
            }
        }
        assert_eq!(world.organisms.len(), 1, "prey should be eaten");
        assert_eq!(world.organisms[0].id, 1);
        assert!(world.organisms[0].kills == 1);
        assert!(world.organisms[0].energy > e0 * 0.5);
    }
}

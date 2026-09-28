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
use crate::organism::{Activity, Carcass, Chem, DeathCause, Grasp, Organism};
use crate::rng::{Rng, mix64};
use crate::species::{SpeciesEvent, SpeciesRegistry};
use crate::stats::{Event, EventKind, EventLog, Milestones, StatSample, Stats};
use crate::terrain::Terrain;
use crate::traits::{MAX_BITES, MAX_EYES, MAX_GRASPS, Traits};

/// Field (minerals, detritus, vents) update interval in ticks.
const FIELD_INTERVAL: u64 = 3;
/// Max organisms considered per perception query (bounds cost in dense crowds).
const PERCEPTION_LIMIT: u32 = 96;
const NONE: u32 = u32::MAX;
/// Marks a tentacle's target as a carcass (else an organism).
const CARCASS: u32 = 1 << 31;
/// Ticks between steering decisions of motile organisms.
const STEER_INTERVAL: u64 = 2;
/// Drives weaker than this are ignored (skips sensing work for genes that drifted near neutral).
const MIN_DRIVE: f32 = 0.02;
/// Save format marker. Bumped when saves stop being loadable (v2: genome with parts and chemistry).
const SAVE_MAGIC: &[u8] = b"EVOSAVE2";
const OLD_SAVE_MAGIC: &[u8] = b"EVOSAVE1";

#[derive(Clone, Copy, Debug, Default)]
struct Intent {
    heading: f32,
    throttle: f32,
    push: Vec2,
    attack: u32,
    carcass: u32,
    /// What each tentacle grabs (organism index, or carcass index | CARCASS; NONE = nothing).
    grabs: [u32; MAX_GRASPS],
    activity: Activity,
}

/// Short-lived visual effects for the viewer (not saved).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxKind {
    /// Prey swallowed whole: its body flashes into a corpse and fades.
    Engulf,
    /// A hunter took a bite.
    Bite,
    /// A scavenger took a bite of a carcass.
    Scavenge,
    /// Meteor strike shockwave.
    Impact,
}

#[derive(Clone, Copy, Debug)]
pub struct Fx {
    pub kind: FxKind,
    pub pos: Vec2,
    pub radius: f32,
    pub color: [f32; 3],
    pub tick: u64,
    /// The body it shows, for effects that draw an organism (a swallowed prey's corpse).
    pub body: Option<FxBody>,
}

/// An organism's look, for an effect that draws it.
#[derive(Clone, Copy, Debug)]
pub struct FxBody {
    pub genome: Genome,
    pub heading: f32,
    pub seed: f32,
}

impl FxKind {
    /// How long the effect lasts, in sim seconds.
    pub fn duration(self) -> f32 {
        match self {
            FxKind::Engulf => 0.5,
            FxKind::Bite => 0.6,
            FxKind::Scavenge => 0.7,
            FxKind::Impact => 6.0,
        }
    }
}

/// Don't emit more than one bite/scavenge effect per organism within this many ticks.
const FX_THROTTLE: u64 = 12;
const MAX_FX: usize = 20_000;

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
    /// The second founding lineage at the vents, and its root species.
    #[serde(default)]
    pub vent_ancestor: Option<Genome>,
    #[serde(default)]
    pub vent_species: Option<u32>,
    pub next_id: u64,
    rng: Rng,
    #[serde(skip)]
    scratch: Option<Scratch>,
    #[serde(skip)]
    pub timings: Timings,
    /// Visual effects from recent ticks (for the viewer).
    #[serde(skip)]
    pub fx: Vec<Fx>,
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

/// Where an organism can eat: fully at its mouth (the front of its head) and at jaws away from
/// it, and poorly through the rest of its body surface. Tentacles don't eat; they grasp food
/// within reach of their bases and pull it to the mouth.
#[derive(Clone, Copy)]
struct Reach {
    center: Vec2,
    radius: f32,
    mouth: Vec2,
    mouth_radius: f32,
    /// Bite strength at the mouth (jaws there).
    jaws: f32,
    /// Jaws away from the mouth: world center, radius, strength.
    bites: [(Vec2, f32, f32); MAX_BITES],
    bite_count: usize,
    /// Tentacle grasps: world center (the tentacle's base), radius and pull strength.
    grasps: [(Vec2, f32, f32); MAX_GRASPS],
    grasp_count: usize,
    /// How well it eats through its body surface, away from the mouth (0..1).
    body: f32,
    extent: f32,
}

/// What a body in contact with an organism can be to it.
#[derive(Clone, Copy, Default)]
struct Contact {
    /// How well it can be eaten there: 1 at the mouth or a jaw, `body` elsewhere on the body.
    eat: f32,
    /// How well it can be swallowed whole: only at the mouth (or poorly, through the body).
    engulf: f32,
    /// Strength of the jaws biting it.
    jaws: f32,
    /// At the mouth.
    mouth: bool,
}

impl Reach {
    fn new(pos: Vec2, heading: f32, radius: f32, t: &Traits, body: f32) -> Reach {
        let rot = Vec2::from_angle(heading);
        let place = |x: f32, y: f32| pos + rot.rotate(Vec2::new(x, y)) * radius;
        let mouth = place(t.mouth_offset, 0.0);
        let mouth_radius = t.mouth_radius * radius;
        let mut extent = radius.max(mouth.distance(pos) + mouth_radius);
        let mut bites = [(Vec2::ZERO, 0.0, 0.0); MAX_BITES];
        let bite_count = t.bite_count as usize;
        for (b, z) in bites.iter_mut().zip(&t.bites[..bite_count]) {
            *b = (place(z.x, z.y), z.radius * radius, z.strength);
            extent = extent.max(b.0.distance(pos) + b.1);
        }
        let mut grasps = [(Vec2::ZERO, 0.0, 0.0); MAX_GRASPS];
        let grasp_count = t.grasp_count as usize;
        for (g, z) in grasps.iter_mut().zip(&t.grasps[..grasp_count]) {
            *g = (place(z.x, z.y), z.radius * radius, z.strength);
            extent = extent.max(g.0.distance(pos) + g.1);
        }
        Reach {
            center: pos,
            radius,
            mouth,
            mouth_radius,
            jaws: t.jaws,
            bites,
            bite_count,
            grasps,
            grasp_count,
            body,
            extent,
        }
    }

    /// Distance from the center that covers everything reachable.
    fn extent(&self) -> f32 {
        self.extent
    }

    /// Where a bite on a body at `pos` shows: at the mouth, or on its near edge.
    fn bite_point(&self, contact: &Contact, pos: Vec2, radius: f32) -> Vec2 {
        if contact.mouth {
            self.mouth + (pos - self.mouth).normalize_or_zero() * self.mouth_radius * 0.5
        } else {
            pos + (self.center - pos).normalize_or_zero() * radius * 0.6
        }
    }

    /// How a body at `pos` with `radius` touches this organism.
    fn contact(&self, pos: Vec2, radius: f32) -> Contact {
        let within = |c: Vec2, r: f32| pos.distance_squared(c) < (r + radius) * (r + radius);
        if within(self.mouth, self.mouth_radius) {
            return Contact {
                eat: 1.0,
                engulf: 1.0,
                jaws: self.jaws,
                mouth: true,
            };
        }
        let jaws = self.bites[..self.bite_count]
            .iter()
            .filter(|&&(c, r, _)| within(c, r))
            .map(|b| b.2)
            .fold(0.0, f32::max);
        if jaws > 0.0 {
            Contact {
                eat: 1.0,
                jaws,
                ..Default::default()
            }
        } else if within(self.center, self.radius) {
            Contact {
                eat: self.body,
                engulf: self.body,
                ..Default::default()
            }
        } else {
            Contact::default()
        }
    }

    /// Which tentacles (bit k = grasp k) can reach a body at `pos` with `radius`.
    fn grasping(&self, pos: Vec2, radius: f32) -> u8 {
        let mut mask = 0;
        for (k, &(c, r, _)) in self.grasps[..self.grasp_count].iter().enumerate() {
            if pos.distance_squared(c) < (r + radius) * (r + radius) {
                mask |= 1 << k;
            }
        }
        mask
    }

    /// Give a body in reach to the first free tentacle that reaches it; returns that tentacle.
    fn grab(&self, free: &mut u8, pos: Vec2, radius: f32) -> Option<usize> {
        if *free == 0 {
            return None;
        }
        let m = self.grasping(pos, radius) & *free;
        (m != 0).then(|| {
            let k = m.trailing_zeros() as usize;
            *free &= !(1 << k);
            k
        })
    }
}

/// Tentacles drag a held body toward the mouth, around the side of the body if it's behind; the
/// pull is shared by mass (an anchored organism doesn't budge). Returns how far each moves.
fn tentacle_pull(
    reach: &Reach,
    target: Vec2,
    target_radius: f32,
    speed: f32,
    share: f32,
) -> (Vec2, Vec2) {
    let axis = (reach.mouth - reach.center).normalize_or_zero();
    let rel = target - reach.center;
    let aim = if rel.dot(axis) < 0.0 {
        let side = if axis.perp_dot(rel) >= 0.0 { 1.0 } else { -1.0 };
        reach.center + axis.perp() * side * (reach.radius + target_radius)
    } else {
        reach.mouth
    };
    let to = aim - target;
    let dist = to.length();
    if dist < 1e-4 || speed <= 0.0 {
        return (Vec2::ZERO, Vec2::ZERO);
    }
    let step = to / dist * speed.min(dist);
    (-step * (1.0 - share), step * share)
}

/// Share of a tentacle pull that moves the held body rather than the holder.
fn pull_share(holder_mass: f32, target_mass: f32, anchored: bool) -> f32 {
    if anchored {
        1.0
    } else {
        holder_mass / (holder_mass + target_mass).max(1e-6)
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
        grabs: [NONE; MAX_GRASPS],
        activity: Activity::Drifting,
    };
    let kin_threshold = cfg.species.kin_threshold;
    let is_kin =
        |e: &Entry| color_distance(t.hue, t.saturation, e.hue, e.saturation) < kin_threshold;
    // Hunters spare look-alikes unless they're starving.
    let hunger = 1.0 - (o.energy / o.max_energy().max(1e-6)).clamp(0.0, 1.0);
    let spares = |e: &Entry| is_kin(e) && hunger < cfg.feeding.kin_hunger;

    // Contacts: soft collision push, and a prey candidate at the mouth or within tentacle reach.
    let mass = r * r;
    let hunts = t.hunt > 0.0;
    let reach = Reach::new(pos, o.heading, r, t, cfg.feeding.body_contact);
    // Eat the smallest prey in contact; each tentacle grabs one thing within its reach.
    let mut prey: Option<(u32, f32)> = None;
    let mut free: u8 = ((1u16 << reach.grasp_count) - 1) as u8;
    let mut push = Vec2::ZERO;
    ctx.grid.query(pos, reach.extent().max(r), 0.0, |e| {
        if e.index as usize == i {
            return true;
        }
        let d = e.pos - pos;
        let dist2 = d.length_squared();
        let min = r + e.radius;
        if dist2 < min * min {
            let dist = dist2.sqrt();
            let dir = if dist > 1e-4 {
                d / dist
            } else {
                let h = mix64((i as u64) << 32 | e.index as u64);
                Vec2::from_angle((h >> 40) as f32 / (1u64 << 24) as f32 * std::f32::consts::TAU)
            };
            let other_mass = e.radius * e.radius;
            push -= dir * (min - dist) * (other_mass / (mass + other_mass));
        }
        if hunts && !spares(e) {
            if reach.contact(e.pos, e.radius).eat > 0.0 && prey.is_none_or(|(_, pr)| e.radius < pr)
            {
                prey = Some((e.index, e.radius));
            }
            if let Some(k) = reach.grab(&mut free, e.pos, e.radius) {
                intent.grabs[k] = e.index;
            }
        }
        true
    });
    intent.push = saturate(push * cfg.physics.push_stiffness / r.max(1e-3)) * r;
    if let Some((j, _)) = prey {
        intent.attack = j;
    }

    // Carcass contact (at the mouth or jaws), and free tentacles grab carcasses too.
    let eats_carcasses = t.scavenge > 0.0 || t.hunt > 0.0;
    if eats_carcasses && !ctx.carcass_grid.is_empty() {
        ctx.carcass_grid.query(pos, reach.extent(), 0.0, |e| {
            if intent.carcass == NONE && reach.contact(e.pos, e.radius).eat > 0.0 {
                intent.carcass = e.index;
            }
            if let Some(k) = reach.grab(&mut free, e.pos, e.radius) {
                intent.grabs[k] = e.index | CARCASS;
            }
            intent.carcass == NONE || free != 0
        });
    }

    if !t.is_motile() {
        return intent;
    }
    // Steering decisions are made every STEER_INTERVAL ticks (staggered by id); in between,
    // keep swimming on the last decision.
    if !(o.id + ctx.tick).is_multiple_of(STEER_INTERVAL) {
        intent.heading = o.steer_heading;
        intent.throttle = o.steer_throttle;
        intent.activity = o.activity;
        return intent;
    }

    // Steering by drives.
    let d = &t.drives;
    let fc = &cfg.feeding;
    let food = fc.hunger_min + (fc.hunger_max - fc.hunger_min) * hunger;
    let probe = (2.0 * r).max(24.0);
    let env = ctx.env;

    let mut foraging = Vec2::ZERO;
    if d[drive::LIGHT].abs() > MIN_DRIVE {
        let mut g = env.light.relative_gradient(pos, probe);
        if env.any_glow {
            // Lures count as light (the anglerfish trick).
            let p = probe.max(env.glow.cell);
            let here = env.light.level_at(pos) + env.glow.sample(pos);
            g += env.glow.gradient(pos, p) * p / here.max(1e-4);
        }
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
    if d[drive::OXYGEN].abs() > MIN_DRIVE {
        let here = env.oxygen.sample(pos);
        let g = env.oxygen.gradient(pos, probe.max(env.oxygen.cell)) / (here + 0.05);
        steer += d[drive::OXYGEN] * gradient_drive(g, 30.0);
    }
    steer.y += d[drive::RISE];

    let mut hunting = Vec2::ZERO;
    let mut fleeing = Vec2::ZERO;
    let mut schooling = Vec2::ZERO;
    let cap = cfg.genes.sense_cap;
    let range = (t.sense_radii * r).min(cap);
    if range > r {
        // Smell works all around and in the dark; eyes see within their cones, and need light.
        let smell = (t.smell_radii * r).min(cap);
        let eye_count = t.eye_count as usize;
        let pc = &cfg.parts;
        let brightness = env.light.level_at(pos) / cfg.light.surface_intensity.max(1e-6);
        let sight = pc.eye_dark + (1.0 - pc.eye_dark) * (brightness / pc.eye_light).min(1.0);
        let mut cones = [(Vec2::ZERO, 0.0f32, 0.0f32); MAX_EYES];
        for (cone, eye) in cones.iter_mut().zip(&t.eyes[..eye_count]) {
            *cone = (
                Vec2::from_angle(o.heading + eye.dir),
                eye.cos_half,
                (eye.range * r * sight).min(cap),
            );
        }
        let perceives = |dir: Vec2, dist: f32, other: f32| {
            dist < smell + other
                || cones[..eye_count].iter().any(|&(axis, cos_half, reach)| {
                    dist < reach + other && dir.dot(axis) >= cos_half
                })
        };
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
            if !perceives(dir, dist, e.radius) {
                return true;
            }
            seen += 1;
            let closeness = 1.0 - dist / reach;
            if spares(e) {
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
                let delta = e.pos - pos;
                let dist = delta.length();
                if dist < best && dist > 1e-4 && perceives(delta / dist, dist, e.radius) {
                    best = dist;
                    carrion = delta / dist;
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
    let cc = &cfg.chemistry;
    let env = ctx.env;
    let t = o.traits;

    o.age += dt;
    o.repro_cooldown -= dt;
    if intent.activity != Activity::Feeding {
        o.activity = intent.activity;
    }
    o.steer_heading = intent.heading;
    o.steer_throttle = intent.throttle;

    // Movement. Bodies with legs or holdfasts check whether they touch rock: legs walk on it,
    // grip resists the current, and a holdfast anchors unless the organism is swimming off.
    let r = o.radius;
    o.touching =
        t.uses_contact() && ctx.terrain.distance(o.pos) < 1.5 * r + cfg.parts.contact_margin;
    let anchored = o.touching && t.anchor > 0.0 && intent.throttle < 0.3;
    o.anchored = anchored;
    // Turning needs an organ to turn with (legs only while walking).
    let turn_rate = if o.touching {
        t.turn_rate.max(t.walk_turn)
    } else {
        t.turn_rate
    };
    if turn_rate > 0.0 && !anchored {
        // Face so that its best way of moving points where it wants to go.
        let diff = wrap_angle(intent.heading - t.move_axis - o.heading);
        let max_turn = turn_rate * dt;
        o.heading = wrap_angle(o.heading + diff.clamp(-max_turn, max_turn));
    }
    // Organs only allow movement in some directions relative to the body; head whichever
    // allowed way gets closest to where it wants to go.
    let (direction, speed, walking) = if intent.throttle > 0.0 && t.is_motile() {
        best_move(&t, o.heading, intent.heading, o.touching)
    } else {
        (o.heading, 0.0, false)
    };
    let target = Vec2::from_angle(direction) * intent.throttle * speed * r.sqrt();
    let response = (phys.response / r.sqrt() * dt).min(1.0);
    o.vel += (target - o.vel) * response;
    if anchored {
        o.vel = Vec2::ZERO;
        o.pos += intent.push * 0.3;
    } else {
        let grip = if o.touching { t.grip } else { 0.0 };
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
        o.pos += (o.vel + (drift + brownian + water) * (1.0 - grip)) * dt + intent.push;
    }
    ctx.terrain.collide(&mut o.pos, &mut o.vel, r);

    // Energy needs.
    let m75 = o.mass.powf(0.75);
    let temp = env.temperature.sample(o.pos);
    o.temperature = temp;
    let stress = ((temp - t.temp_pref) / t.temp_tol).powi(2);
    let basal = met.basal * m75 * t.upkeep * (1.0 + met.temp_stress_cost * stress.min(4.0));
    let effort = if walking {
        0.5 * t.crawl_coeff / cfg.parts.crawl_speed.max(1e-6)
    } else {
        t.swim * t.drag
    };
    let movement = met.basal * m75 * met.move_cost * effort * intent.throttle * intent.throttle;

    // The water here (all chemistry fields share one grid).
    let idx = env.oxygen.water_index_at(o.pos);
    let oxygen = env.oxygen.data[idx];
    let mut chem = Chem::default();

    // Photosynthesis: oxygen-producing (needs CO₂) and sulfur-based (needs minerals).
    let mut photo = 0.0;
    if t.pigment > 0.0 {
        let light = &env.light;
        let mut absorbed = 0.0;
        light.for_each_covered(o.prev_pos, r * t.light_span, |i, w| {
            absorbed += light.share[i] * w
        });
        let raw = absorbed * t.pigment * cfg.light.photo_yield;
        let co2 = env.co2.data[idx];
        let k = cc.co2_half;
        let oxygenic = raw * t.oxygenic * (co2 * (1.0 + k) / (co2 + k)).max(0.0);
        let minerals = env.minerals.data[idx].max(0.0);
        let sulfur = raw * (1.0 - t.oxygenic) * minerals / (minerals + cc.sulfur_half);
        photo = oxygenic + sulfur;
        chem.oxygen += oxygenic * cc.oxygen_per_energy * dt;
        chem.co2 -= oxygenic * cc.co2_per_energy * dt;
        chem.minerals -= sulfur * cc.sulfur_minerals * dt;
    }

    // Respiration: breathe oxygen as far as capacity and the water allow, ferment the rest
    // (costlier, and it leaks organic waste). Whatever can't be met suffocates.
    let demand = basal + movement;
    let available = (oxygen / cc.oxygen_half).clamp(0.0, 1.0);
    let aerobic = t.aerobic.min(available);
    let ferment = (1.0 - aerobic).min(t.ferment);
    let shortfall = (1.0 - aerobic - ferment).max(0.0);
    let mut cost = demand * (aerobic + ferment * (1.0 + cc.ferment_penalty));
    chem.oxygen -= demand * aerobic * cc.oxygen_per_energy * dt;
    chem.co2 += demand * (aerobic + 0.5 * ferment) * cc.co2_per_energy * dt;
    chem.waste += demand * ferment * cc.ferment_penalty * cc.ferment_waste * dt;
    chem.aerobic_share = aerobic;
    chem.shortfall = shortfall;

    // Nitrogen fixation (hampered by oxygen).
    let nutrient_cap = o.max_nutrients(cfg);
    if t.fixation > 0.0 && o.nutrients < nutrient_cap {
        let inhibition = 1.0 - 0.6 * oxygen.min(1.0);
        let fixed = (cc.fixation_rate * t.fixation * o.mass * inhibition * dt)
            .min(nutrient_cap - o.nutrients);
        o.nutrients += fixed;
        cost += fixed * cc.fixation_cost / dt;
    }

    // Toxins: leak our own; suffer from others' unless they carry our hue's signature.
    if t.toxin > 0.0 {
        chem.toxin = cc.toxin_rate * t.toxin * r * dt;
    }
    let max_hp = o.max_hp();
    let mut poison = 0.0;
    let toxin = env.toxin.data[idx];
    if toxin > 1e-6 {
        let signature = Vec2::new(env.toxin_hue_x.data[idx], env.toxin_hue_y.data[idx]) / toxin;
        let hue = Vec2::from_angle(t.hue as f32 / 256.0 * std::f32::consts::TAU);
        let immunity = ((signature.dot(hue) - 0.85) / 0.1).clamp(0.0, 1.0);
        poison += cc.toxin_damage * toxin / (1.0 + r / 8.0)
            * (1.0 - immunity)
            * (1.0 - t.resistance)
            * max_hp
            * dt;
    }
    // Oxygen poisons strict fermenters.
    let strict = (1.0 - 2.0 * t.aerobic).max(0.0);
    if strict > 0.0 && oxygen > 0.5 {
        poison += cc.oxygen_toxicity * strict * strict * (oxygen - 0.5) * max_hp * dt;
    }
    chem.poisoned = poison;

    o.energy += (photo - cost) * dt;
    o.gain_rate = photo;
    o.cost_rate = cost;

    let suffocation = cc.suffocation_damage * shortfall * max_hp * dt;
    let mut temperature_damage = 0.0;
    if stress > 1.0 {
        temperature_damage = met.temp_damage * (stress - 1.0).min(10.0) * max_hp * dt;
    }
    let mut starvation = 0.0;
    if o.energy < 0.0 {
        starvation = -o.energy * met.starvation_damage;
        o.energy = 0.0;
    }
    o.hp -= temperature_damage + suffocation + poison + starvation;

    // Regeneration.
    if o.hp < max_hp && o.energy > 0.0 {
        let heal = (t.regen * max_hp * dt)
            .min(max_hp - o.hp)
            .min(o.energy / met.regen_cost);
        o.hp += heal;
        o.energy -= heal * met.regen_cost;
        o.cost_rate += heal * met.regen_cost / dt;
    }

    // Growth toward adult size, limited by stored nutrients.
    if o.mass < t.adult_mass {
        let buffer = met.growth_buffer * o.max_energy();
        if o.energy > buffer {
            let build = met.build_cost * t.build_factor;
            let invest = (o.energy - buffer)
                .min(met.growth_rate * o.mass * dt * build)
                .min((t.adult_mass - o.mass) * build)
                .min(o.nutrients.max(0.0) / cc.nutrient_per_mass * build);
            if invest > 0.0 {
                o.energy -= invest;
                let grown = invest / build;
                o.nutrients -= grown * cc.nutrient_per_mass;
                let old_max_hp = max_hp;
                o.set_mass(o.mass + grown);
                // Growing adds HP proportionally so a healthy juvenile stays healthy.
                o.hp += o.max_hp() - old_max_hp;
            }
        }
    }
    o.energy = o.energy.min(o.max_energy());
    o.hp = o.hp.min(o.max_hp());
    // Nutrients beyond the store are excreted.
    let excess = o.nutrients - o.max_nutrients(cfg);
    if excess > 0.0 {
        o.nutrients -= excess;
        chem.nutrients += excess;
    }
    o.chem = chem;

    if o.age > t.lifespan && o.rng.chance(met.old_age_death_rate * dt) {
        o.dead = Some(DeathCause::OldAge);
    } else if o.hp <= 0.0 {
        let causes = [
            (starvation, DeathCause::Starvation),
            (temperature_damage, DeathCause::Temperature),
            (suffocation, DeathCause::Suffocation),
            (poison, DeathCause::Poison),
        ];
        o.dead = Some(
            causes
                .iter()
                .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
                .filter(|c| c.0 > 0.0)
                .map(|c| c.1)
                .unwrap_or(DeathCause::Starvation),
        );
    }
}

/// The best way to move toward world angle `want` for a body facing `heading`: the world
/// direction, its speed multiplier (× √radius), and whether that's walking. Tries sixteen
/// directions around the body (and `want` itself) and keeps the one that makes the most
/// progress toward `want`.
fn best_move(
    t: &crate::traits::Traits,
    heading: f32,
    want: f32,
    touching: bool,
) -> (f32, f32, bool) {
    let want_local = wrap_angle(want - heading);
    let speed_at = |local: f32| {
        let swim = t.swim_toward(local);
        let walk = if touching { t.crawl_toward(local) } else { 0.0 };
        if walk > swim {
            (walk, true)
        } else {
            (swim, false)
        }
    };
    let mut best = (heading, 0.0, false, 0.0);
    for k in 0..=16 {
        let local = if k == 16 {
            want_local
        } else {
            k as f32 * std::f32::consts::TAU / 16.0
        };
        let (speed, walking) = speed_at(local);
        let progress = speed * (local - want_local).cos();
        if progress > best.3 {
            best = (heading + local, speed, walking, progress);
        }
    }
    (best.0, best.1, best.2)
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
        let mut species = SpeciesRegistry::new(&ancestor, &config);
        let vent_ancestor = Some(config.population.vent_ancestor.trim())
            .filter(|s| !s.is_empty())
            .map(|s| Genome::parse(s).expect("config validated"));
        let vent_species = vent_ancestor.map(|g| species.add_root(&g, &config, 0));
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
            vent_ancestor,
            vent_species,
            next_id: 1,
            rng,
            scratch: None,
            timings: Timings::default(),
            fx: Vec::new(),
        };
        let initial = world.config.population.initial;
        world.spawn_ancestors(initial);
        let vents: Vec<Vec2> = world.env.vents.iter().map(|v| v.pos).collect();
        for p in vents {
            world.seed_vent(p, world.config.population.vent_initial);
        }
        world.log(
            EventKind::Milestone,
            if initial == 1 {
                "Life begins: a single microbe drifts in the sunlit shallows.".into()
            } else {
                "Life begins: the first microbes drift in the sunlit shallows.".into()
            },
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
        if bytes.starts_with(OLD_SAVE_MAGIC) {
            return Err(format!(
                "{}: this save is from an older version (before body parts and chemistry) \
                 and can't be loaded; start a new world with --new",
                path.display()
            ));
        }
        let body = bytes
            .strip_prefix(SAVE_MAGIC)
            .ok_or_else(|| format!("{}: not an evolution save file", path.display()))?;
        let packed = lz4_flex::decompress_size_prepended(body).map_err(|e| e.to_string())?;
        let mut world: World =
            rmp_serde::from_slice(&packed).map_err(|e| format!("{}: {e}", path.display()))?;
        world.after_load();
        Ok(world)
    }

    /// Swap in a new config for a running world (e.g. after tuning). The world size must match.
    pub fn replace_config(&mut self, config: Config) -> Result<(), String> {
        if config.world.width != self.config.world.width
            || config.world.height != self.config.world.height
        {
            return Err("the new config has a different world size".into());
        }
        self.config = config;
        self.scratch = None;
        self.after_load();
        Ok(())
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
        self.log_at(kind, text, organism, species, None);
    }

    fn log_at(
        &mut self,
        kind: EventKind,
        text: String,
        organism: Option<u64>,
        species: Option<u32>,
        pos: Option<Vec2>,
    ) {
        let event = Event {
            tick: self.tick,
            time: self.time(),
            kind,
            text,
            organism,
            species,
            pos,
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

    /// Settle `count` vent ancestors around a vent at `pos`.
    fn seed_vent(&mut self, pos: Vec2, count: u32) {
        let (Some(genome), Some(species)) = (self.vent_ancestor, self.vent_species) else {
            return;
        };
        let cfg = &self.config;
        let traits = crate::traits::Traits::decode(&genome, cfg);
        let r = traits.adult_radius;
        for _ in 0..count {
            let mut rng = self.rng.fork();
            let mut spot = None;
            for _ in 0..30 {
                let p = pos + Vec2::new(rng.range(-90.0, 90.0), rng.range(0.0, 60.0));
                if self.terrain.is_free(p, r) {
                    spot = Some(p);
                    break;
                }
            }
            let Some(p) = spot else { continue };
            let mass = traits.adult_mass;
            let energy = 0.5 * mass * traits.reserve_factor;
            let id = self.next_id;
            self.next_id += 1;
            let o = Organism::new(
                id, 0, species, 0, genome, 0, cfg, p, mass, energy, rng, self.tick,
            );
            self.species.on_birth(species, cfg, self.tick);
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
                    let (min, max) = (
                        Vec2::new(x - width * 0.5, h - 600.0),
                        Vec2::new(x + width * 0.5, h),
                    );
                    self.env.inject_minerals(min, max, rate * field_dt);
                    let nutrients = rate * field_dt * config.chemistry.upwelling_nutrients;
                    self.env.inject_nutrients(min, max, nutrients);
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
                    light_radius: o.radius * o.traits.light_span,
                    lure: o.traits.lure,
                }
            })
            .collect_into_vec(&mut scratch.entries);
        self.env.rebuild_glow(
            scratch
                .entries
                .iter()
                .filter(|e| e.lure > 0.0)
                .map(|e| (e.pos, e.lure)),
            config,
        );
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
                geometry
                    .for_each_covered(e.pos, e.light_radius, |idx, w| light.weight[idx] += w * a);
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
        let keep_after = self.tick.saturating_sub((6.0 / dt) as u64);
        self.fx.retain(|f| {
            f.tick >= keep_after && (self.tick - f.tick) as f32 * dt <= f.kind.duration()
        });
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
                if n == 1 {
                    "Life is gone. A single new microbe appears in the shallows and starts again."
                        .to_string()
                } else {
                    format!("Life clings on: {n} ancestral microbes recolonize the shallows.")
                },
                None,
                Some(0),
            );
        }

        // 7. Events and statistics.
        for e in env_events {
            match e {
                EnvEvent::VentOpened(p) => {
                    self.log_at(
                        EventKind::Environment,
                        format!("A hydrothermal vent opens on the seafloor at x={:.0}.", p.x),
                        None,
                        None,
                        Some(p),
                    );
                    // Drifting spores settle there if no vent life lives nearby.
                    let settled = self.organisms.iter().any(|o| {
                        o.traits.chemo > 0.3 && o.pos.distance_squared(p) < 1500.0 * 1500.0
                    });
                    if !settled {
                        self.seed_vent(p, self.config.population.vent_seed);
                    }
                }
                EnvEvent::VentDied(p) => self.log_at(
                    EventKind::Environment,
                    format!("The vent at x={:.0} falls silent.", p.x),
                    None,
                    None,
                    Some(p),
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
            let pos = Vec2::new(x, self.config.world.height - 400.0);
            self.log_at(
                EventKind::Environment,
                format!("Cold, mineral-rich water wells up to the surface near x={x:.0}."),
                None,
                None,
                Some(pos),
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
            self.fx.push(Fx {
                kind: FxKind::Impact,
                pos: p,
                radius: r,
                color: [1.0, 0.7, 0.3],
                tick: self.tick,
                body: None,
            });
            self.log_at(
                EventKind::Disaster,
                format!("A meteor strikes the ocean at x={:.0}, killing {killed}! Dust darkens the sky.", p.x),
                None,
                None,
                Some(p),
            );
        }
        if let Some((x, _)) = fx.start_barrier {
            let pos = Vec2::new(x, self.terrain.floor_at(x) + 400.0);
            self.log_at(
                EventKind::Environment,
                format!("The seafloor heaves: a wall of rock begins to rise at x={x:.0}, splitting the ocean."),
                None,
                None,
                Some(pos),
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
                    let pos = Vec2::new(*x, self.terrain.floor_at(*x) + 400.0);
                    self.log_at(
                        EventKind::Environment,
                        format!("The wall at x={x:.0} has sunk back into the seafloor. The waters mix again."),
                        None,
                        None,
                        Some(pos),
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
        let cc = &config.chemistry;
        let dt = config.time.dt;
        let body_energy = config.metabolism.body_energy;
        let min_mass = config.genes.min_radius * config.genes.min_radius * 0.5;
        let intents = &self.scratch.as_ref().unwrap().intents;
        let env = &mut self.env;
        let minerals = &mut env.minerals;
        let detritus = &mut env.detritus;
        let oxygen = &mut env.oxygen;
        let co2 = &mut env.co2;
        let nutrients = &mut env.nutrients;
        let toxin = &mut env.toxin;
        let toxin_x = &mut env.toxin_hue_x;
        let toxin_y = &mut env.toxin_hue_y;
        let toxin_present = &mut env.toxin_present;
        let m_area = minerals.cell_area();
        let d_area = detritus.cell_area();
        let mut first_kill = None;
        let tick = self.tick;
        let fx = &mut self.fx;
        let terrain = &self.terrain;

        for i in 0..self.organisms.len() {
            let intent = intents[i];
            let o = &mut self.organisms[i];

            // What it exchanged with the water during act (even if it died doing so).
            let idx = minerals.water_index_at(o.pos);
            let c = o.chem;
            oxygen.data[idx] = (oxygen.data[idx] + c.oxygen / m_area).max(0.0);
            co2.data[idx] = (co2.data[idx] + c.co2 / m_area).max(0.0);
            minerals.data[idx] = (minerals.data[idx] + c.minerals / m_area).max(0.0);
            nutrients.data[idx] += c.nutrients / m_area;
            if c.toxin > 0.0 {
                *toxin_present = true;
                let hue = Vec2::from_angle(o.traits.hue as f32 / 256.0 * std::f32::consts::TAU);
                toxin.data[idx] += c.toxin / m_area;
                toxin_x.data[idx] += c.toxin * hue.x / m_area;
                toxin_y.data[idx] += c.toxin * hue.y / m_area;
            }
            let d_idx = detritus.water_index_at(o.pos);
            if c.waste > 0.0 {
                detritus.data[d_idx] += c.waste / d_area;
            }
            if o.dead.is_some() {
                continue;
            }

            // Only the fields feeding needs (Traits is large; don't copy it all).
            struct Diet {
                chemo: f32,
                scavenge: f32,
                hunt: f32,
                filter: f32,
                uptake: f32,
                uptake_area: f32,
                resistance: f32,
            }
            let t = Diet {
                chemo: o.traits.chemo,
                scavenge: o.traits.scavenge,
                hunt: o.traits.hunt,
                filter: o.traits.filter,
                uptake: o.traits.uptake,
                uptake_area: o.traits.uptake_area,
                resistance: o.traits.resistance,
            };
            let r = o.radius;
            let reach = Reach::new(o.pos, o.heading, r, &o.traits, fc.body_contact);
            let mut gained = 0.0;
            let mut absorbed = 0.0;
            let mut grazed = 0.0;

            if t.chemo > 0.0 {
                let conc = minerals.data[idx];
                if conc > 0.0 {
                    let want = fc.chemo_rate * t.chemo * r * conc * dt;
                    let take = want.min(conc * m_area);
                    minerals.data[idx] -= take / m_area;
                    gained += take * fc.chemo_yield;
                    absorbed += take * fc.chemo_yield;
                }
            }
            // Nutrients from the water (a holdfast also draws them from the rock).
            let nutrient_cap = o.max_nutrients(config);
            if t.uptake > 0.0 && o.nutrients < nutrient_cap {
                let conc = nutrients.data[idx];
                if conc > 0.0 {
                    let anchor = if o.anchored { cc.anchor_uptake } else { 1.0 };
                    let want = cc.uptake_rate * t.uptake * r * t.uptake_area * anchor * conc * dt;
                    let take = want.min(conc * m_area).min(nutrient_cap - o.nutrients);
                    nutrients.data[idx] -= take / m_area;
                    o.nutrients += take;
                }
            }
            if t.scavenge > 0.0 {
                let conc = detritus.data[d_idx];
                if conc > 0.0 {
                    let sweep = fc.scavenge_rate * r * (1.0 + t.filter)
                        + fc.filter_rate * 2.0 * r * o.vel.length();
                    let want = sweep * t.scavenge * conc * dt;
                    let take = want.min(conc * d_area);
                    detritus.data[d_idx] -= take / d_area;
                    gained += take;
                    grazed += take;
                }
            }
            // Label what it's living on, when that's a meaningful share of its needs.
            let need = o.cost_rate.max(1e-6) * dt;
            if grazed > need * 0.5 && grazed >= absorbed {
                o.activity = Activity::Grazing;
                o.ate_tick = tick;
            } else if absorbed > need * 0.5 {
                o.activity = Activity::Absorbing;
            }
            if intent.carcass != NONE
                && let Some(c) = self.carcasses.get_mut(intent.carcass as usize)
                && c.energy > 0.0
                && let contact = reach.contact(c.pos, c.radius)
                && contact.eat > 0.0
            {
                let skill = t.scavenge.max(0.7 * t.hunt);
                let rate =
                    fc.carcass_rate * skill * r * r * (0.3 + 1.7 * contact.jaws) * contact.eat;
                let take = (rate * dt).min(c.energy);
                let share = take / c.energy;
                let n = c.nutrients * share;
                c.nutrients -= n;
                o.nutrients += n;
                c.energy -= take;
                gained += take * fc.assimilation;
                detritus.add(o.pos, take * (1.0 - fc.assimilation) / d_area);
                o.activity = Activity::Scavenging;
                if tick >= o.ate_tick + FX_THROTTLE && fx.len() < MAX_FX {
                    fx.push(Fx {
                        kind: FxKind::Scavenge,
                        pos: reach.bite_point(&contact, c.pos, c.radius),
                        radius: r.max(c.radius * 0.5),
                        color: c.color,
                        tick,
                        body: None,
                    });
                }
                o.ate_tick = tick;
                c.hurt_tick = tick;
            }
            o.energy += gained;
            o.gain_rate += gained / dt;

            // Hunting.
            let j = intent.attack as usize;
            if intent.attack != NONE && j < self.organisms.len() && j != i {
                let (hunter, prey) = pair_mut(&mut self.organisms, i, j);
                let contact = reach.contact(prey.pos, prey.radius);
                if prey.dead.is_none() && contact.eat > 0.0 {
                    // Plates and spines only guard the side the attack comes from; jaws bite
                    // through part of a shell.
                    let from = hunter.pos - prey.pos;
                    let (armor, spikes) = prey
                        .traits
                        .defense_toward(from.y.atan2(from.x) - prey.heading);
                    let protection =
                        1.0 - fc.armor_protection * armor * (1.0 - fc.jaw_pierce * contact.jaws);
                    let mut taken_mass = 0.0;
                    let mut taken_energy = 0.0;
                    if prey.radius <= fc.engulf_ratio * hunter.radius && contact.engulf > 0.0 {
                        let p = fc.engulf_rate * t.hunt * protection * contact.engulf * dt;
                        if hunter.rng.chance(p) {
                            taken_mass = prey.mass;
                            taken_energy = prey.body_value(config);
                            hunter.nutrients += prey.body_nutrients(config);
                            prey.nutrients = 0.0;
                            prey.dead = Some(DeathCause::Eaten);
                            if fx.len() < MAX_FX {
                                fx.push(Fx {
                                    kind: FxKind::Engulf,
                                    pos: prey.pos,
                                    radius: prey.radius,
                                    color: prey.traits.color,
                                    tick,
                                    body: Some(FxBody {
                                        genome: prey.genome,
                                        heading: prey.heading,
                                        seed: ((prey.id.wrapping_mul(2_654_435_761) >> 8) % 1000)
                                            as f32
                                            / 1000.0,
                                    }),
                                });
                            }
                        }
                    } else {
                        let bite = fc.bite_rate
                            * t.hunt
                            * (0.2 + 1.8 * contact.jaws)
                            * r
                            * r
                            * dt
                            * protection
                            * contact.eat;
                        let dm = bite.min(prey.mass);
                        if dm > 0.0 {
                            let frac = dm / prey.mass;
                            let reserves = prey.energy.max(0.0) * frac;
                            prey.energy -= reserves;
                            let stored = prey.nutrients.max(0.0) * frac;
                            prey.nutrients -= stored;
                            hunter.nutrients += dm * cc.nutrient_per_mass + stored;
                            taken_mass = dm;
                            taken_energy = dm * body_energy + reserves;
                            prey.hp -= dm * prey.traits.hp_factor;
                            prey.set_mass(prey.mass - dm);
                            if tick >= hunter.ate_tick + FX_THROTTLE && fx.len() < MAX_FX {
                                fx.push(Fx {
                                    kind: FxKind::Bite,
                                    pos: reach.bite_point(&contact, prey.pos, prey.radius),
                                    radius: hunter.radius.max(prey.radius * 0.5),
                                    color: prey.traits.color,
                                    tick,
                                    body: None,
                                });
                            }
                            prey.hurt_tick = tick;
                            if prey.hp <= 0.0 || prey.mass < min_mass {
                                prey.dead = Some(DeathCause::Predation);
                            }
                        }
                    }
                    if taken_energy > 0.0 {
                        // Even a weak hunter digests much of what it catches.
                        let assimilated = taken_energy * fc.assimilation * (0.4 + 0.6 * t.hunt);
                        hunter.energy += assimilated;
                        hunter.gain_rate += assimilated / dt;
                        detritus.add(hunter.pos, (taken_energy - assimilated) / d_area);
                        hunter.activity = Activity::Feeding;
                        hunter.ate_tick = tick;
                        if spikes > 0.0 {
                            hunter.hp -= fc.spike_damage * spikes * taken_mass;
                            if hunter.hp <= 0.0 {
                                hunter.dead = Some(DeathCause::Spikes);
                            }
                        }
                        if prey.traits.toxin > 0.0 {
                            hunter.hp -= cc.poison_damage
                                * prey.traits.toxin
                                * taken_mass
                                * (1.0 - t.resistance);
                            if hunter.hp <= 0.0 && hunter.dead.is_none() {
                                hunter.dead = Some(DeathCause::Poison);
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

            // Each tentacle drags what it holds toward the mouth.
            for (k, &target) in intent.grabs[..reach.grasp_count].iter().enumerate() {
                if target == NONE {
                    continue;
                }
                let (base, grasp_r, strength) = reach.grasps[k];
                let speed = fc.tentacle_pull * strength * r.sqrt() * dt;
                let reaches = |pos: Vec2, radius: f32| {
                    pos.distance_squared(base) < (grasp_r + radius) * (grasp_r + radius)
                };
                let (holder, pos, radius) = if target & CARCASS != 0 {
                    let Some(c) = self.carcasses.get_mut((target & !CARCASS) as usize) else {
                        continue;
                    };
                    let o = &mut self.organisms[i];
                    if c.energy <= 0.0 || o.dead.is_some() || !reaches(c.pos, c.radius) {
                        continue;
                    }
                    let share = pull_share(o.mass, c.radius * c.radius, o.anchored);
                    let (moved, dragged) = tentacle_pull(&reach, c.pos, c.radius, speed, share);
                    o.pos += moved;
                    c.pos += dragged;
                    terrain.collide(&mut o.pos, &mut o.vel, r);
                    terrain.collide(&mut c.pos, &mut c.vel, c.radius);
                    (o, c.pos, c.radius)
                } else {
                    let j = target as usize;
                    if j == i || j >= self.organisms.len() {
                        continue;
                    }
                    let (o, prey) = pair_mut(&mut self.organisms, i, j);
                    if prey.dead.is_some() || o.dead.is_some() || !reaches(prey.pos, prey.radius) {
                        continue;
                    }
                    let share = pull_share(o.mass, prey.mass, o.anchored);
                    let (moved, dragged) =
                        tentacle_pull(&reach, prey.pos, prey.radius, speed, share);
                    o.pos += moved;
                    prey.pos += dragged;
                    terrain.collide(&mut o.pos, &mut o.vel, r);
                    terrain.collide(&mut prey.pos, &mut prey.vel, prey.radius);
                    (o, prey.pos, prey.radius)
                };
                if holder.grasps.len() < reach.grasp_count {
                    holder.grasps.resize(reach.grasp_count, Grasp::default());
                }
                holder.grasps[k].hold(pos, radius, tick);
            }

            // Anything the organism can't store is lost: energy as detritus, nutrients excreted.
            let o = &mut self.organisms[i];
            let excess = o.energy - o.max_energy();
            if excess > 0.0 {
                o.energy -= excess;
                detritus.add(o.pos, excess / d_area);
            }
            let excess = o.nutrients - o.max_nutrients(config);
            if excess > 0.0 {
                o.nutrients -= excess;
                nutrients.data[idx] += excess / m_area;
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
        let time = self.time() as f32;
        let detritus = &mut self.env.detritus;
        let nutrients = &mut self.env.nutrients;
        let area = detritus.cell_area();
        let n_area = nutrients.cell_area();
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
            // Small bodies decompose faster (microbes within about a minute).
            let rate = phys.carcass_decay * (6.0 / c.radius.max(0.5)).clamp(1.0, 5.0);
            let lost = c.energy * rate * dt;
            c.energy -= lost;
            detritus.add(c.pos, lost / area);
            // Rotting releases the body's nutrients.
            let released = c.nutrients * rate * dt;
            c.nutrients -= released;
            nutrients.add(c.pos, released / n_area);
        }
        self.carcasses.retain(|c| {
            if c.energy < 0.3 || c.consumed() > 0.92 {
                detritus.add(c.pos, c.energy.max(0.0) / area);
                nutrients.add(c.pos, c.nutrients.max(0.0) / n_area);
                false
            } else {
                true
            }
        });
    }

    fn reproduce(&mut self) {
        let config = &self.config;
        let rc = &config.reproduction;
        let cc = &config.chemistry;
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
            // Budding: the child's body comes out of the parent's, which then regrows. The
            // parent keeps at least half its adult mass (fission-sized offspring: one child,
            // two halves). Building the body is paid later, as the parent regrows.
            let child_mass = (t.adult_mass * t.birth_fraction).max(min_mass);
            let child_reserves = child_mass * t.reserve_factor * rc.newborn_reserves;
            let child_store = 0.25 * child_mass * cc.nutrient_store;
            let spare_mass = o.mass - 0.5 * t.adult_mass;
            let count = t
                .brood
                .min(((spare_mass + 1e-3) / child_mass) as u32)
                .min((o.energy / child_reserves.max(1e-6)) as u32)
                .min((o.nutrients.max(0.0) / child_store.max(1e-6)) as u32);
            if count == 0 {
                continue;
            }
            let old_mass = o.mass;
            let old_radius = o.radius;
            let new_mass = old_mass - child_mass * count as f32;
            o.hp *= new_mass / old_mass;
            o.set_mass(new_mass);
            o.energy -= child_reserves * count as f32;
            o.nutrients -= child_store * count as f32;
            // Reserves and nutrients the smaller parent can't hold go to the children.
            let child_energy_room = child_mass * t.reserve_factor - child_reserves;
            let spare_energy = (o.energy - o.max_energy()).max(0.0);
            o.energy -= spare_energy;
            let extra_energy = (spare_energy / count as f32).min(child_energy_room);
            let child_store_room = child_mass * cc.nutrient_store - child_store;
            let spare_nutrients = (o.nutrients - o.max_nutrients(config)).max(0.0);
            o.nutrients -= spare_nutrients;
            let extra_nutrients = (spare_nutrients / count as f32).min(child_store_room);
            // Anything left over (rare) is released into the water.
            let leftover_energy = spare_energy - extra_energy * count as f32;
            let leftover_nutrients = spare_nutrients - extra_nutrients * count as f32;
            if leftover_energy > 1e-6 {
                self.env
                    .detritus
                    .add(o.pos, leftover_energy / self.env.detritus.cell_area());
            }
            if leftover_nutrients > 1e-6 {
                self.env
                    .nutrients
                    .add(o.pos, leftover_nutrients / self.env.nutrients.cell_area());
            }
            o.budded_tick = tick;
            o.bud_scale = old_radius / o.radius;
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
                    child_reserves + extra_energy,
                    rng,
                    tick,
                );
                child.vel = o.vel;
                child.nutrients = child_store + extra_nutrients;
                child.bud_offset = pos - o.pos;
                self.species.on_birth(species, config, tick);
                self.stats.record_birth();
                births.push(child);
                population += 1;
            }
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
        let nutrients = &mut self.env.nutrients;
        let area = detritus.cell_area();
        let n_area = nutrients.cell_area();
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
                self.carcasses.push(Carcass {
                    pos: o.pos,
                    vel: o.vel * 0.5,
                    energy: value,
                    radius: o.radius,
                    color: o.traits.color,
                    species: o.species,
                    initial_energy: value,
                    nutrients: o.body_nutrients(config),
                    heading: o.heading,
                    genome: o.genome,
                    seed: ((o.id.wrapping_mul(2_654_435_761) >> 8) % 1000) as f32 / 1000.0,
                    hurt_tick: 0,
                    died_tick: tick,
                });
            } else {
                detritus.add(o.pos, value / area);
                nutrients.add(o.pos, o.body_nutrients(config) / n_area);
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
        let mut part_sum = 0u64;
        for o in &self.organisms {
            sample.biomass += o.mass;
            if o.traits.part_count > 0 {
                sample.with_parts += 1;
            }
            part_sum += o.traits.part_count as u64;
            sample.diets[o.traits.main_diet()] += 1;
            if o.traits.is_motile() {
                sample.motile += 1;
            }
            radius_sum += o.radius;
            sample.max_radius = sample.max_radius.max(o.radius);
        }
        sample.mean_radius = radius_sum / self.organisms.len().max(1) as f32;
        sample.mean_parts = part_sum as f32 / self.organisms.len().max(1) as f32;
        let env = &self.env;
        sample.air_oxygen = env.atmosphere.oxygen;
        sample.air_co2 = env.atmosphere.co2;
        let f = &env.oxygen;
        let top_rows = ((400.0 / f.cell) as usize).clamp(1, f.ny);
        let (mut o2, mut nut, mut n) = (0.0f64, 0.0f64, 0u32);
        for y in f.ny - top_rows..f.ny {
            for x in 0..f.nx {
                let i = f.index(x, y);
                if !f.solid[i] {
                    o2 += f.data[i] as f64;
                    nut += env.nutrients.data[i] as f64;
                    n += 1;
                }
            }
        }
        sample.surface_oxygen = (o2 / n.max(1) as f64) as f32;
        sample.surface_nutrients = (nut / n.max(1) as f64) as f32;
        let water = f.solid.iter().filter(|s| !**s).count().max(1);
        let anoxic = f
            .data
            .iter()
            .zip(&f.solid)
            .filter(|(v, s)| !**s && **v < 0.1)
            .count();
        sample.anoxic_fraction = anoxic as f32 / water as f32;
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
                pos: None,
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
    if !m.first_eyes && t.eye_count > 0 && t.sense_radii * t.adult_radius > 40.0 {
        m.first_eyes = true;
        log(format!("A {name} lineage can see its surroundings."));
    }
    if !m.first_armor && t.armor > 0.3 {
        m.first_armor = true;
        log(format!("A {name} lineage grows a shell."));
    }
    // The first time each kind of part is carried by a lineage that breeds.
    for p in crate::parts::decode_parts(&parent.genome) {
        let bit = 1u32 << p.kind.code();
        if m.parts_seen & bit == 0 && p.size >= 0.25 {
            m.parts_seen |= bit;
            log(format!(
                "A {name} lineage grows the first {}.",
                if p.count > 1 {
                    p.kind.plural()
                } else {
                    p.kind.name()
                }
            ));
        }
    }
    if !m.first_walker && parent.touching && t.crawl_coeff > 2.0 {
        m.first_walker = true;
        log(format!("A {name} lineage walks along the seafloor."));
    }
    if !m.first_anchored && parent.anchored {
        m.first_anchored = true;
        log(format!("A {name} lineage anchors itself to rock."));
    }
    if !m.first_anaerobe && t.aerobic < 0.3 {
        m.first_anaerobe = true;
        log(format!(
            "A {name} lineage gives up breathing oxygen and lives by fermentation."
        ));
    }
    if !m.first_sulfur && t.photo > 0.3 && t.oxygenic < 0.3 {
        m.first_sulfur = true;
        log(format!(
            "A {name} lineage photosynthesizes with minerals instead of CO₂, releasing no oxygen."
        ));
    }
    if !m.first_fixer && t.fixation > 0.4 {
        m.first_fixer = true;
        log(format!(
            "A {name} lineage makes its own nutrients (nitrogen fixation)."
        ));
    }
    if !m.first_toxic && t.toxin > 0.4 {
        m.first_toxic = true;
        log(format!(
            "A {name} lineage turns toxic, poisoning the water around it."
        ));
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
    use crate::genome::gene;
    use crate::parts::PartKind;

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
        c.population.vent_initial = 0;
        c.population.vent_seed = 0;
        c.terrain.island = false;
        c.terrain.shelf_vents = false;
        c
    }

    #[test]
    fn optional_vent_lineage_starts_at_vents() {
        let mut c = small_config();
        c.population.vent_initial = 30;
        c.population.vent_ancestor = "B:207060300000 C:10C040 D:00FF0000 G:400080000000 H:2030 \
            N:80808080808080808000 P:086010 000000 000000 000000 000000 000000 000000 000000 \
            R:FF00B0 T:FFFF"
            .into();
        let world = World::new(c);
        let vent_species = world.vent_species.expect("a vent lineage");
        assert_ne!(vent_species, 0);
        let at_vents: Vec<_> = world
            .organisms
            .iter()
            .filter(|o| o.species == vent_species)
            .collect();
        assert!(!at_vents.is_empty());
        for o in at_vents {
            assert!(o.traits.chemo > 0.9);
            let near = world
                .env
                .vents
                .iter()
                .any(|v| v.pos.distance(o.pos) < 150.0);
            assert!(near, "vent life starts at a vent");
        }
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
        // It can't swim, so it has to face its prey: it eats with its mouth.
        world.organisms[0].heading = 0.0;
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

    /// An empty small world plus a helper to add organisms.
    fn empty_world() -> World {
        let mut config = small_config();
        config.population.initial = 0;
        config.population.min_population = 0;
        config.events.enabled = false;
        World::new(config)
    }

    fn add(world: &mut World, genome: Genome, pos: Vec2) -> usize {
        let id = world.next_id;
        world.next_id += 1;
        let t = crate::traits::Traits::decode(&genome, &world.config);
        let o = Organism::new(
            id,
            0,
            0,
            0,
            genome,
            0,
            &world.config,
            pos,
            t.adult_mass,
            t.adult_mass * t.reserve_factor * 0.8,
            Rng::new(id),
            0,
        );
        world.species.on_birth(0, &world.config, 0);
        world.organisms.push(o);
        world.organisms.len() - 1
    }

    fn with_part(
        mut g: Genome,
        slot: usize,
        kind: PartKind,
        sym: u8,
        size: u8,
        place: u8,
    ) -> Genome {
        let o = crate::parts::slot_offset(slot);
        g.0[o] = sym << 4 | kind.code();
        g.0[o + 1] = size;
        g.0[o + 2] = place;
        g
    }

    #[test]
    fn photosynthesis_releases_oxygen_and_breathing_uses_it() {
        let mut world = empty_world();
        let h = world.config.world.height;
        let ancestor = world.ancestor;
        let plant = add(&mut world, ancestor, Vec2::new(2000.0, h - 20.0));
        let mut animal_g = world.ancestor;
        animal_g.0[gene::PHOTO] = 0;
        animal_g.0[gene::SCAVENGE] = 0xFF;
        animal_g.0[gene::RESPIRATION] = 0xFF;
        let animal = add(&mut world, animal_g, Vec2::new(2600.0, h - 20.0));
        world.step();
        let (p, a) = (&world.organisms[plant].chem, &world.organisms[animal].chem);
        assert!(p.oxygen > 0.0 && p.co2 < 0.0, "plant {p:?}");
        assert!(a.oxygen < 0.0 && a.co2 > 0.0, "animal {a:?}");
        assert!(a.aerobic_share > 0.99);
    }

    #[test]
    fn aerobes_suffocate_without_oxygen_and_fermenters_do_not() {
        let mut world = empty_world();
        let h = world.config.world.height;
        let mut aerobe = world.ancestor;
        aerobe.0[gene::RESPIRATION] = 0xFF;
        let mut fermenter = world.ancestor;
        fermenter.0[gene::RESPIRATION] = 0x00;
        let a = add(&mut world, aerobe, Vec2::new(1000.0, h - 1500.0));
        let f = add(&mut world, fermenter, Vec2::new(3000.0, h - 1500.0));
        world.env.oxygen.data.fill(0.0);
        world.step();
        let (ca, cf) = (world.organisms[a].chem, world.organisms[f].chem);
        assert!(ca.shortfall > 0.5, "aerobe {ca:?}");
        assert_eq!(cf.shortfall, 0.0, "fermenter {cf:?}");
        assert!(cf.waste > 0.0, "fermentation leaks organic waste");
    }

    #[test]
    fn holdfasts_anchor_and_legs_walk_on_rock() {
        let mut world = empty_world();
        let x = 2000.0;
        let floor = world.terrain.floor_at(x);
        // Cold-hardy, so the deep floor doesn't kill them.
        let mut base = world.ancestor;
        base.0[gene::TEMP_PREF] = 0x20;
        base.0[gene::TEMP_TOL] = 0xFF;
        // Sinks, has a holdfast.
        let mut g = with_part(base, 0, PartKind::Holdfast, 0, 0xC0, 24);
        g.0[gene::BUOYANCY] = 0x00;
        let anchored = add(&mut world, g, Vec2::new(x, floor + 60.0));
        // A walker: legs only, wanders.
        let mut w = with_part(base, 0, PartKind::Leg, 1, 0xFF, 6 << 5 | 12);
        w.0[gene::BUOYANCY] = 0x00;
        w.0[gene::RESTLESSNESS] = 0xFF;
        let walker = add(&mut world, w, Vec2::new(x + 400.0, floor + 60.0));
        let t = &world.organisms[walker].traits;
        assert!(
            t.crawl_coeff > 0.0 && t.speed_coeff > 0.0,
            "legs walk and row"
        );
        for _ in 0..(40.0 / world.config.time.dt) as usize {
            world.step();
        }
        let o = &world.organisms[anchored];
        assert!(o.anchored, "should have settled and anchored");
        let pos = o.pos;
        for _ in 0..(10.0 / world.config.time.dt) as usize {
            world.step();
        }
        assert!(
            world.organisms[anchored].pos.distance(pos) < 1.0,
            "anchored bodies stay put"
        );
        let wk = &world.organisms[walker];
        assert!(wk.touching, "walker should be on the floor");
        assert!(
            wk.vel.length() > 1.0,
            "walker moves along the rock: {}",
            wk.vel.length()
        );
    }

    #[test]
    fn toxins_harm_strangers_but_not_kin() {
        let mut world = empty_world();
        let h = world.config.world.height;
        let mut producer = world.ancestor;
        producer.0[gene::TOXIN] = 0xFF;
        let mut stranger = world.ancestor;
        stranger.0[gene::HUE] = world.ancestor.0[gene::HUE].wrapping_add(128);
        let pos = Vec2::new(2000.0, h - 200.0);
        for k in 0..30 {
            add(
                &mut world,
                producer,
                pos + Vec2::new(k as f32 % 6.0 * 5.0, (k / 6) as f32 * 5.0),
            );
        }
        let s = add(&mut world, stranger, pos + Vec2::new(12.0, 12.0));
        let ancestor = world.ancestor;
        let kin = add(&mut world, ancestor, pos + Vec2::new(14.0, 8.0));
        let (sid, kid) = (world.organisms[s].id, world.organisms[kin].id);
        for _ in 0..60 {
            world.step();
        }
        let poisoned = |id| world.find(id).map(|o| o.chem.poisoned).unwrap_or(1.0);
        assert!(poisoned(sid) > 0.0, "the stranger is poisoned");
        assert_eq!(poisoned(kid), 0.0, "kin are immune");
    }

    #[test]
    fn growth_needs_nutrients() {
        let mut world = empty_world();
        let h = world.config.world.height;
        let mut g = world.ancestor;
        g.0[gene::UPTAKE] = 0;
        let a = add(&mut world, g, Vec2::new(2000.0, h - 20.0));
        let b = add(&mut world, g, Vec2::new(2600.0, h - 20.0));
        for i in [a, b] {
            let o = &mut world.organisms[i];
            o.set_mass(o.traits.adult_mass * 0.5);
        }
        world.organisms[a].nutrients = 0.0;
        for _ in 0..300 {
            world.step();
        }
        let (ma, mb) = (world.organisms[a].mass, world.organisms[b].mass);
        assert!(mb > ma * 1.2, "fed {mb} starved {ma}");
    }

    #[test]
    fn eyes_see_only_where_they_look() {
        let run = |eye_angle: u8| {
            let mut world = empty_world();
            let h = world.config.world.height;
            let mut hunter = with_part(world.ancestor, 0, PartKind::Eye, 0, 0xFF, eye_angle);
            hunter = with_part(hunter, 1, PartKind::Flagellum, 0, 0xFF, 16);
            hunter.0[gene::SIZE] = 0x60;
            hunter.0[gene::PHOTO] = 0;
            hunter.0[gene::HUNT] = 0xFF;
            hunter.0[gene::DRIVES + drive::SMALLER] = 0xFF;
            let pos = Vec2::new(2000.0, h - 300.0);
            let i = add(&mut world, hunter, pos);
            world.organisms[i].heading = 0.0;
            world.organisms[i].steer_heading = 0.0;
            let mut prey = world.ancestor;
            prey.0[gene::HUE] = 0xD0;
            add(&mut world, prey, pos + Vec2::new(80.0, 0.0));
            world.step();
            world.step();
            world.organisms[i].activity
        };
        assert_eq!(run(0), Activity::Hunting, "an eye facing the prey sees it");
        assert_ne!(run(16), Activity::Hunting, "an eye facing away doesn't");
    }

    #[test]
    fn mouths_eat_in_front_and_the_body_eats_poorly() {
        // A sessile hunter facing right biting large prey in front of it or against its back.
        let bite = |behind: bool| {
            let mut world = empty_world();
            let h = world.config.world.height;
            let mut g = world.ancestor;
            g.0[gene::SIZE] = 0x60;
            g.0[gene::PHOTO] = 0x00;
            g.0[gene::HUNT] = 0xFF;
            let pos = Vec2::new(2000.0, h - 300.0);
            let hunter = add(&mut world, g, pos);
            world.organisms[hunter].heading = 0.0;
            let mut prey = world.ancestor;
            prey.0[gene::SIZE] = 0x58;
            prey.0[gene::HUE] = 0xD0;
            let side = if behind { -1.0 } else { 1.0 };
            let p = add(&mut world, prey, pos + Vec2::new(side * 12.0, 0.0));
            let m0 = world.organisms[p].mass;
            world.step();
            m0 - world.organisms[p].mass
        };
        let (front, back) = (bite(false), bite(true));
        assert!(front > 0.0 && back > 0.0, "front {front} back {back}");
        assert!(
            back < front * 0.5,
            "the body eats poorly: front {front} back {back}"
        );
    }

    #[test]
    fn tentacles_grab_near_their_base() {
        // One tentacle on the left side: it grabs things on the left, not on the right.
        let world = empty_world();
        let mut g = world.ancestor;
        g.0[gene::SIZE] = 0x60;
        g = with_part(g, 0, PartKind::Tentacle, 0, 0xFF, 8);
        let t = crate::traits::Traits::decode(&g, &world.config);
        assert_eq!(t.grasp_count, 1);
        assert!(
            t.grasps[0].y > 0.5,
            "the grasp is around the base: {:?}",
            t.grasps[0]
        );
        let r = t.adult_radius;
        let reach = Reach::new(Vec2::ZERO, 0.0, r, &t, 0.3);
        let out = r * (1.0 + t.grasps[0].radius * 0.8);
        let (left, right) = (Vec2::new(0.0, out), Vec2::new(0.0, -out));
        assert_eq!(reach.grasping(left, 1.0), 1, "grabbed on its side");
        assert_eq!(reach.contact(left, 1.0).eat, 0.0, "but not eaten there");
        assert_eq!(reach.grasping(right, 1.0), 0, "out of reach on the other side");
    }

    #[test]
    fn each_tentacle_holds_one_thing() {
        // A ring of three tentacles with a small prey beside each, and a fourth prey beside the
        // first: three are held, one per tentacle.
        let mut world = empty_world();
        let h = world.config.world.height;
        let mut g = world.ancestor;
        g.0[gene::SIZE] = 0x60;
        g.0[gene::PHOTO] = 0x00;
        g.0[gene::HUNT] = 0xFF;
        g = with_part(g, 0, PartKind::Tentacle, 2, 0xFF, 16);
        let pos = Vec2::new(2000.0, h - 300.0);
        let hunter = add(&mut world, g, pos);
        world.organisms[hunter].heading = 0.0;
        let t = world.organisms[hunter].traits;
        assert_eq!(t.grasp_count, 3);
        let r = world.organisms[hunter].radius;
        let mut prey = world.ancestor;
        prey.0[gene::SIZE] = 0x10;
        prey.0[gene::HUE] = 0xD0;
        for (k, z) in t.grasps[..3].iter().enumerate() {
            let base = Vec2::new(z.x, z.y) * r;
            let out = base.normalize() * (z.radius * r * 0.6);
            add(&mut world, prey, pos + base + out);
            if k == 0 {
                add(&mut world, prey, pos + base + out * 1.3);
            }
        }
        for o in &mut world.organisms {
            o.repro_cooldown = 1e9;
        }
        world.step();
        let o = &world.organisms[hunter];
        let holding: Vec<&Grasp> = o.grasps.iter().filter(|g| g.tick == world.tick).collect();
        assert_eq!(holding.len(), 3, "each tentacle holds one: {:?}", o.grasps);
        for (a, b) in [(0, 1), (0, 2), (1, 2)] {
            assert!(holding[a].pos.distance(holding[b].pos) > 1.0, "different prey");
        }
    }

    #[test]
    fn tentacles_pull_food_around_to_the_mouth() {
        // Large prey held just behind a sessile hunter gets dragged around to its mouth.
        let run = |tentacles: bool| {
            let mut world = empty_world();
            let h = world.config.world.height;
            let mut g = world.ancestor;
            g.0[gene::SIZE] = 0x60;
            g.0[gene::PHOTO] = 0x00;
            g.0[gene::HUNT] = 0xFF;
            if tentacles {
                g = with_part(g, 0, PartKind::Tentacle, 2, 0xFF, 16);
            }
            let pos = Vec2::new(2000.0, h - 300.0);
            let hunter = add(&mut world, g, pos);
            world.organisms[hunter].heading = 0.0;
            let r = world.organisms[hunter].radius;
            let mut prey = world.ancestor;
            prey.0[gene::SIZE] = 0x58;
            prey.0[gene::HUE] = 0xD0;
            let pr = crate::traits::Traits::decode(&prey, &world.config).adult_radius;
            let p = add(&mut world, prey, pos - Vec2::new(r + pr + 0.3 * r, 0.0));
            let id = world.organisms[p].id;
            let m0 = world.organisms[p].mass;
            // Nobody buds during the test.
            for o in &mut world.organisms {
                o.repro_cooldown = 1e9;
            }
            for _ in 0..90 {
                world.step();
            }
            world.find(id).map(|o| m0 - o.mass).unwrap_or(m0)
        };
        let (with, without) = (run(true), run(false));
        assert!(
            with > 3.0 * without,
            "tentacles bring it to be eaten: {with} vs {without}"
        );
    }

    #[test]
    fn jaws_bite_where_they_are_and_crack_shells() {
        let world = empty_world();
        let base = {
            let mut g = world.ancestor;
            g.0[gene::SIZE] = 0x60;
            g.0[gene::PHOTO] = 0x00;
            g.0[gene::HUNT] = 0xFF;
            g
        };
        let decode = |g: &Genome| crate::traits::Traits::decode(g, &world.config);
        // Jaws at the front strengthen the mouth; a jaw on the back bites there.
        let front = decode(&with_part(base, 0, PartKind::Jaw, 1, 0xC0, 2));
        assert!(front.jaws > 0.5 && front.bite_count == 0);
        let back = decode(&with_part(base, 0, PartKind::Jaw, 0, 0xC0, 16));
        assert!(back.jaws == 0.0 && back.bite_count == 1);
        let r = back.adult_radius;
        let reach = Reach::new(Vec2::ZERO, 0.0, r, &back, 0.3);
        let behind = reach.contact(Vec2::new(-1.6 * r, 0.0), 0.5 * r);
        assert!(behind.eat == 1.0 && behind.jaws > 0.5 && behind.engulf == 0.0);

        // Jawed mouths bite through part of a shell.
        let bite = |pierce: f32| {
            let mut world = empty_world();
            world.config.feeding.jaw_pierce = pierce;
            let h = world.config.world.height;
            let pos = Vec2::new(2000.0, h - 300.0);
            let hunter = add(
                &mut world,
                with_part(base, 0, PartKind::Jaw, 1, 0xFF, 2),
                pos,
            );
            world.organisms[hunter].heading = 0.0;
            let mut prey = with_part(world.ancestor, 0, PartKind::Shell, 5, 0xFF, 0);
            prey.0[gene::SIZE] = 0x58;
            prey.0[gene::HUE] = 0xD0;
            let p = add(&mut world, prey, pos + Vec2::new(12.0, 0.0));
            let m0 = world.organisms[p].mass;
            world.step();
            m0 - world.organisms[p].mass
        };
        let (cracked, blocked) = (bite(0.6), bite(0.0));
        assert!(
            cracked > 1.5 * blocked && blocked > 0.0,
            "cracked {cracked} blocked {blocked}"
        );
    }

    #[test]
    fn shells_guard_the_side_they_cover() {
        // Prey with a plate on its back, bitten from behind or from the front.
        let bites = |from_behind: bool| {
            let mut world = empty_world();
            let h = world.config.world.height;
            let mut hunter = world.ancestor;
            hunter.0[gene::SIZE] = 0x60;
            hunter.0[gene::PHOTO] = 0;
            hunter.0[gene::HUNT] = 0xFF;
            let mut prey = with_part(world.ancestor, 0, PartKind::Shell, 0, 0xFF, 16);
            prey.0[gene::SIZE] = 0x58;
            prey.0[gene::HUE] = 0xD0;
            let pos = Vec2::new(2000.0, h - 300.0);
            let hi = add(&mut world, hunter, pos);
            world.organisms[hi].heading = 0.0;
            let pi = add(&mut world, prey, pos + Vec2::new(14.0, 0.0));
            // The prey faces away from the hunter (back toward it) or toward it.
            world.organisms[pi].heading = if from_behind {
                0.0
            } else {
                std::f32::consts::PI
            };
            let m0 = world.organisms[pi].mass;
            world.step();
            m0 - world.organisms[pi].mass
        };
        let (behind, front) = (bites(true), bites(false));
        assert!(
            front > 0.0 && behind < front * 0.6,
            "behind {behind} front {front}"
        );
    }

    #[test]
    fn organs_set_which_way_a_body_can_move() {
        // Flagella pointing up and down: it swims up without facing up.
        let mut world = empty_world();
        let h = world.config.world.height;
        let mut g = with_part(world.ancestor, 0, PartKind::Flagellum, 1, 0xFF, 8);
        g.0[gene::BUOYANCY] = 0x80;
        g.0[gene::DRIVES + drive::RISE] = 0xFF;
        let i = add(&mut world, g, Vec2::new(2000.0, h - 1500.0));
        for _ in 0..60 {
            world.step();
        }
        let o = &world.organisms[i];
        assert!(o.vel.y > 3.0, "swims up: {:?}", o.vel);
        let facing = Vec2::from_angle(o.heading);
        assert!(
            facing.y.abs() < 0.5,
            "sideways to its motion: heading {}",
            o.heading
        );

        // No turning organ (a fluke facing back gives some): without any, it can't turn.
        let mut world = empty_world();
        let mut g = world.ancestor;
        g.0[gene::DRIVES + drive::RISE] = 0xFF;
        let i = add(&mut world, g, Vec2::new(2000.0, h - 1500.0));
        let heading = world.organisms[i].heading;
        for _ in 0..30 {
            world.step();
        }
        assert_eq!(world.organisms[i].heading, heading);
    }

    #[test]
    fn budding_takes_the_body_from_the_parent() {
        let run = |offspring_size: u8| {
            let mut world = empty_world();
            let h = world.config.world.height;
            let mut g = world.ancestor;
            g.0[gene::OFFSPRING_SIZE] = offspring_size;
            g.0[gene::BROOD] = 0;
            g.0[gene::SIZE] = 0x60;
            let i = add(&mut world, g, Vec2::new(2000.0, h - 50.0));
            let o = &mut world.organisms[i];
            o.energy = o.max_energy();
            o.nutrients = o.max_nutrients(&world.config);
            let (mass, energy) = (o.mass, o.energy);
            world.reproduce();
            assert_eq!(world.organisms.len(), 2, "one child");
            let (p, c) = (&world.organisms[0], &world.organisms[1]);
            // Mass moves from parent to child; no reserves are lost.
            assert!((p.mass + c.mass - mass).abs() < 1e-3);
            assert!((p.energy + c.energy - energy).abs() < 1e-3);
            assert!(p.bud_scale > 1.0 && c.bud_offset.length() > 0.0);
            (p.mass / mass, c.mass / mass)
        };
        let (parent, child) = run(0xFF);
        assert!(
            (parent - 0.5).abs() < 0.01 && (child - 0.5).abs() < 0.01,
            "fission halves"
        );
        let (parent, child) = run(0x90);
        assert!(
            parent > 0.9 && child < 0.1,
            "a small bud: parent {parent}, child {child}"
        );
    }
}

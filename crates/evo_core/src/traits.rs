//! Decoded traits (phenotype) of a genome. Computed once at birth.
//!
//! Body parts (P chromosome) are laid out once here and summed into plain numbers
//! (thrust, grip, eye cones, …), so the per-tick simulation never looks at parts.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::genome::{Genome, drive, gene};
use crate::parts::{self, BodyShape, Instance, PartKind};

/// Most eyes whose view cones the simulation tracks (the largest ones win).
pub const MAX_EYES: usize = 6;
/// Most tentacle grasps and jaw bite sites the simulation tracks (the largest ones win).
pub const MAX_GRASPS: usize = 8;
pub const MAX_BITES: usize = 4;

/// A body-local circle where a part works: a tentacle's grasp around its base, or where a jaw
/// away from the mouth bites. In body radii (0,0 = body center, +x = forward).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Zone {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    /// How strong the part is there, 0..1 (jaw bite strength, tentacle pull).
    pub strength: f32,
    /// Index of the part instance in the body layout (to draw the tentacle that's holding).
    pub part: u8,
}

/// One eye's view cone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Eye {
    /// Body-local direction (radians, 0 = forward).
    pub dir: f32,
    /// Cosine of half the view cone.
    pub cos_half: f32,
    /// Range in body radii.
    pub range: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Traits {
    pub adult_radius: f32,
    pub adult_mass: f32,

    /// Body shape: 0..1 elongation and 1..6 segments.
    pub elongation: f32,
    pub segments: u8,
    /// Number of part instances on the body (after symmetry and repetition).
    pub part_count: u8,

    /// Damage blocked by shell plates, 0..1, averaged over all sides.
    pub armor: f32,
    /// Damage reflected onto attackers by spines, 0..1, averaged over all sides.
    pub spikes: f32,
    /// Armor and spikes by side: front, left (+y), back, right (−y). Plates and spines guard
    /// the side they're on; see [`Traits::defense_toward`].
    pub armor_sides: [f32; 4],
    pub spikes_sides: [f32; 4],
    /// The mouth: its center's distance ahead of the body center, and its radius (body radii).
    /// Food is eaten well only at the mouth or at jaws; tentacles pull it to the mouth.
    pub mouth_offset: f32,
    pub mouth_radius: f32,
    /// Bite strength of the jaws at the mouth, 0..1.
    pub jaws: f32,
    /// Jaws away from the mouth: each bites (but can't swallow) where it sits.
    pub bites: [Zone; MAX_BITES],
    pub bite_count: u8,
    /// −1 (sinks) .. +1 (floats), after the weight of parts.
    pub buoyancy: f32,
    /// Energy storage per unit of mass.
    pub reserve_factor: f32,
    /// Seconds.
    pub lifespan: f32,

    /// Diet capabilities after the generalist penalty.
    pub photo: f32,
    pub chemo: f32,
    pub scavenge: f32,
    pub hunt: f32,

    /// Fraction of max HP regenerated per second.
    pub regen: f32,
    /// Max HP per unit of mass.
    pub hp_factor: f32,

    /// Swimming strength 0..1 in the best direction (thrust against drag).
    pub swim: f32,
    /// Max swimming speed multiplier in the best direction; × √radius for units/s.
    pub speed_coeff: f32,
    /// Swimming speed multiplier in eight body-relative directions (0 = forward, then every
    /// 45° counterclockwise). Organs only enable movement along (flagella, flukes) or across
    /// (fins, cilia) the way they face.
    pub swim_dirs: [f32; 8],
    /// Best walking speed multiplier on rock and seafloor (legs); × √radius for units/s.
    pub crawl_coeff: f32,
    /// Walking speed multiplier in the same eight directions.
    pub crawl_dirs: [f32; 8],
    /// Body-relative direction it moves best in (0 = forward). It turns so that this points
    /// where it wants to go.
    pub move_axis: f32,
    /// Fraction of the current resisted while touching rock (legs, tentacles, holdfasts).
    pub grip: f32,
    /// Holdfast strength: > 0 means it anchors when it touches rock.
    pub anchor: f32,
    /// Drag of the body and its parts (multiplies the cost of swimming).
    pub drag: f32,
    /// Radians per second while swimming (0 = no organ to turn with).
    pub turn_rate: f32,
    /// Radians per second while walking on rock (legs).
    pub walk_turn: f32,
    pub restlessness: f32,

    /// Signed drive weights (see `genome::drive`).
    pub drives: [f32; drive::COUNT],

    /// Newborn mass as a fraction of adult mass.
    pub birth_fraction: f32,
    pub brood: u32,
    /// Reserve fullness required to reproduce.
    pub repro_threshold: f32,

    /// View cones of the eyes (`eye_count` used).
    pub eyes: [Eye; MAX_EYES],
    pub eye_count: u8,
    /// Smell range in body radii, in every direction (antennae).
    pub smell_radii: f32,
    /// Longest sense range in body radii (0 = blind).
    pub sense_radii: f32,

    /// What each tentacle can grasp: a circle around its base. Each holds one thing at a time
    /// and pulls it toward the mouth with its own strength.
    pub grasps: [Zone; MAX_GRASPS],
    pub grasp_count: u8,
    /// Extra marine-snow sweep from cilia and tentacles (multiplier − 1).
    pub filter: f32,
    /// Width that catches light, as a multiple of the body's (fronds).
    pub light_span: f32,
    /// Surface that takes up nutrients, as a multiple of the body's (fronds).
    pub uptake_area: f32,
    /// Glow of lures, 0..1.
    pub lure: f32,
    /// Cost of building body mass relative to a body without parts (parts weigh something).
    pub build_factor: f32,

    /// Chemistry. Aerobic respiration capacity 0..1 (the rest must be fermented).
    pub aerobic: f32,
    /// Fermentation capacity 0..1.
    pub ferment: f32,
    /// Share of photosynthesis that is oxygen-producing (the rest is sulfur-based).
    pub oxygenic: f32,
    /// Nutrient uptake from the water, 0..1.
    pub uptake: f32,
    /// Nitrogen fixation, 0..1.
    pub fixation: f32,
    /// Toxin production, 0..1.
    pub toxin: f32,
    /// Resistance to toxins, 0..1 (fraction of damage avoided).
    pub resistance: f32,

    /// °C.
    pub temp_pref: f32,
    pub temp_tol: f32,

    /// Basal metabolism multiplier: 1 + Σ upkeep.
    pub upkeep: f32,
    /// Light absorptance of the body (0..1).
    pub absorptance: f32,
    /// Part of the absorptance due to photosynthetic pigment.
    pub pigment: f32,

    /// Display color, sRGB 0..1.
    pub color: [f32; 3],
    pub hue: u8,
    pub saturation: u8,
    pub pattern: u8,
}

/// Diet capabilities with the generalist penalty (docs/DESIGN.md §2): each diet is
/// weakened in proportion to the investment in the others, `c_i = x_i (1 − k Σ_{j≠i} x_j)`.
/// Entry into a new diet is linear (gentle), while true generalists lose overall.
pub fn diet_capabilities(genes: [f32; 4], penalty: f32) -> [f32; 4] {
    let total: f32 = genes.iter().sum();
    genes.map(|x| (x * (1.0 - penalty * (total - x))).max(0.0))
}

/// HSV → sRGB with fixed value; hue in 0..1.
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h6 = (h.rem_euclid(1.0)) * 6.0;
    let i = h6.floor() as i32;
    let f = h6 - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match i.rem_euclid(6) {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Swimming efficiency of flagella and cilia: excellent for microbes, useless for giants.
fn small_swimmer(radius: f32, half: f32) -> f32 {
    1.0 / (1.0 + (radius / half).powf(1.5))
}

/// Swimming efficiency of fins and flukes: poor for microbes, best for big bodies.
fn large_swimmer(radius: f32, half: f32) -> f32 {
    0.3 + 0.7 * radius / (radius + half)
}

/// `1 − e^(−x)`: turns a sum of contributions into a 0..1 level with diminishing returns.
fn saturating(x: f32) -> f32 {
    1.0 - (-x).exp()
}

/// How much a plate or spine facing `dir` guards side `k` (0 front, 1 left, 2 back, 3 right):
/// mostly the side it faces, plus `around` everywhere (a broad plate covers more than a spine).
fn side_weight(dir: f32, k: usize, around: f32) -> f32 {
    let side = k as f32 * std::f32::consts::FRAC_PI_2;
    (dir - side).cos().max(0.0) + around
}

/// The eight body-relative movement directions (0 = forward, counterclockwise).
pub const MOVE_DIRS: usize = 8;

fn move_dir(k: usize) -> f32 {
    k as f32 * std::f32::consts::TAU / MOVE_DIRS as f32
}

/// Add `amount` of thrust in every direction, weighted by how well an organ facing `dir`
/// moves the body that way (`weight` of the angle between them).
fn add_thrust(into: &mut [f32; 8], dir: f32, amount: f32, weight: fn(f32) -> f32) {
    for (k, v) in into.iter_mut().enumerate() {
        *v += amount * weight(move_dir(k) - dir);
    }
}

/// Along the organ's axis, either way (a whip pushes or pulls).
fn along(delta: f32) -> f32 {
    delta.cos().abs()
}

/// Only away from where the organ points (a tail stroke pushes the body forward).
fn pushing(delta: f32) -> f32 {
    (-delta.cos()).max(0.0)
}

/// Across the organ's axis (fins row, cilia beat and legs step sideways to where they point).
fn across(delta: f32) -> f32 {
    delta.sin().abs()
}

/// Angle wrapped to (−π, π].
fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// Linear interpolation between the eight directions.
fn toward(v: &[f32; 8], angle: f32) -> f32 {
    let s = angle.rem_euclid(std::f32::consts::TAU) / (std::f32::consts::TAU / MOVE_DIRS as f32);
    let i = (s.floor() as usize) % MOVE_DIRS;
    let j = (i + 1) % MOVE_DIRS;
    lerp(v[i], v[j], s - s.floor())
}

/// Sums of what the parts do.
#[derive(Default)]
struct PartSums {
    /// Swimming and walking thrust in the eight body-relative directions.
    thrust: [f32; 8],
    crawl: [f32; 8],
    turning: f32,
    walk_turning: f32,
    drag: f32,
    grip: f32,
    holdfast: f32,
    spines: [f32; 4],
    shell: [f32; 4],
    antennae: f32,
    lure: f32,
    grasps: Vec<Zone>,
    /// Jaws: where each sits, where it bites (center and radius, body radii), and its size.
    jaws_at: Vec<(Vec2, Vec2, f32, f32)>,
    filter: f32,
    fronds: f32,
    mass: f32,
    upkeep: f32,
    eyes: Vec<Eye>,
}

fn sum_parts(instances: &[Instance], radius: f32, config: &Config) -> PartSums {
    let pc = &config.parts;
    let small = small_swimmer(radius, pc.small_swimmer_radius);
    let large = large_swimmer(radius, pc.large_swimmer_radius);
    let mut p = PartSums::default();
    for (index, inst) in instances.iter().enumerate() {
        let s = inst.size;
        let spec = inst.kind.spec();
        // Even a tiny part weighs and costs something.
        let bulk = 0.2 + 0.8 * s;
        p.mass += spec.mass * bulk;
        p.upkeep += spec.upkeep * bulk;
        p.drag += spec.drag * bulk;
        let raised = if inst.parent == Some(PartKind::Stalk) {
            pc.stalk_bonus
        } else {
            1.0
        };
        match inst.kind {
            PartKind::Flagellum => {
                // A whip pushes the body away from the side it's on.
                add_thrust(&mut p.thrust, inst.dir, 1.0 * s * small, pushing);
                p.turning += 0.3 * s;
            }
            PartKind::Cilia => {
                add_thrust(&mut p.thrust, inst.dir, 0.35 * s * small, across);
                p.turning += 0.4 * s;
                p.filter += 0.6 * s;
            }
            PartKind::Tentacle => {
                add_thrust(&mut p.thrust, inst.dir, 0.1 * s, along);
                p.turning += 0.1 * s;
                p.grip += 0.1 * s;
                p.filter += 0.15 * s;
                // It bends, so it can grasp anything within most of its length of its base.
                p.grasps.push(Zone {
                    x: inst.base.x,
                    y: inst.base.y,
                    radius: inst.length * pc.tentacle_reach,
                    strength: saturating(1.5 * s),
                    part: index.min(255) as u8,
                });
            }
            PartKind::Frond => p.fronds += inst.length * raised,
            PartKind::Fin => {
                add_thrust(&mut p.thrust, inst.dir, 0.5 * s * large, across);
                p.turning += 0.6 * s;
            }
            PartKind::Fluke => {
                add_thrust(&mut p.thrust, inst.dir, 1.4 * s * large, pushing);
                p.turning += 0.2 * s;
            }
            PartKind::Leg => {
                // Jointed legs walk on rock and row through open water (like a copepod's).
                add_thrust(&mut p.crawl, inst.dir, 0.8 * s, across);
                add_thrust(&mut p.thrust, inst.dir, 0.3 * s, across);
                p.turning += 0.15 * s;
                p.walk_turning += 0.5 * s;
                p.grip += 0.35 * s;
            }
            PartKind::Holdfast => p.holdfast += s,
            PartKind::Stalk => {}
            PartKind::Eye => {
                let half = (55.0 - 20.0 * s).to_radians();
                p.eyes.push(Eye {
                    dir: inst.dir,
                    cos_half: half.cos(),
                    range: pc.eye_range * (0.15 + 0.85 * s) * raised,
                });
            }
            PartKind::Antenna => p.antennae += s * raised,
            PartKind::Lure => p.lure += s * raised,
            PartKind::Spine => {
                for (k, v) in p.spines.iter_mut().enumerate() {
                    *v += side_weight(inst.dir, k, 0.15) * s;
                }
            }
            PartKind::Jaw => {
                let site = inst.base + Vec2::from_angle(inst.dir) * (0.5 * inst.length);
                p.jaws_at.push((inst.base, site, 0.5 * inst.length + 0.15, s));
            }
            PartKind::Shell => {
                for (k, v) in p.shell.iter_mut().enumerate() {
                    *v += side_weight(inst.dir, k, 0.3) * (0.4 + 0.6 * s);
                }
            }
        }
    }
    p
}

impl Traits {
    pub fn decode(genome: &Genome, config: &Config) -> Traits {
        let gc = &config.genes;
        let pc = &config.parts;
        let up = &config.upkeep;
        let light = &config.light;
        let f = |i: usize| genome.frac(i);

        let adult_radius =
            gc.min_radius * 2f32.powf(genome.get(gene::SIZE) as f32 / gc.size_doubling);
        let adult_mass = adult_radius * adult_radius;

        let body = BodyShape::from_genome(genome);
        let mut instances = Vec::with_capacity(parts::MAX_INSTANCES);
        parts::layout(&parts::decode_parts(genome), &body, &mut instances);
        let mut p = sum_parts(&instances, adult_radius, config);

        let armor_sides = p.shell.map(|v| saturating(0.9 * v));
        let spikes_sides = p.spines.map(|v| saturating(1.2 * v));
        let armor = armor_sides.iter().sum::<f32>() / 4.0;
        let spikes = spikes_sides.iter().sum::<f32>() / 4.0;
        // The mouth sits at the front of the head. A round cell eats across most of its front
        // half; a long body only at its nose.
        let mouth_radius = (0.85 * body.head_width()).max(0.4);
        let mouth_offset = body.nose() - 0.6 * mouth_radius;
        let extra_segments = (body.segments - 1) as f32;
        // Jaws at the mouth strengthen its bite; each jaw elsewhere bites where it sits (a pair
        // side by side bites together).
        let mouth_at = Vec2::new(mouth_offset, 0.0);
        let mut mouth_jaws = 0.0;
        let mut sites: Vec<(Vec2, f32, f32)> = Vec::new();
        for &(base, at, radius, size) in &p.jaws_at {
            if base.distance(mouth_at) < mouth_radius {
                mouth_jaws += size;
            } else if let Some(site) = sites
                .iter_mut()
                .find(|(c, r, _)| c.distance(at) < 0.5 * (r + radius))
            {
                site.2 += size;
            } else {
                sites.push((at, radius, size));
            }
        }
        let jaws = saturating(1.2 * mouth_jaws);
        sites.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        let mut bites = [Zone::default(); MAX_BITES];
        let bite_count = sites.len().min(MAX_BITES);
        for (zone, &(at, radius, size)) in bites.iter_mut().zip(&sites) {
            *zone = Zone {
                x: at.x,
                y: at.y,
                radius,
                strength: saturating(1.2 * size),
                part: 0,
            };
        }
        // Tentacles: keep the widest grasps.
        p.grasps.sort_by(|a, b| {
            b.radius
                .partial_cmp(&a.radius)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut grasps = [Zone::default(); MAX_GRASPS];
        let grasp_count = p.grasps.len().min(MAX_GRASPS);
        grasps[..grasp_count].copy_from_slice(&p.grasps[..grasp_count]);
        let buoyancy_gene = genome.signed(gene::BUOYANCY);
        let buoyancy = (buoyancy_gene - p.mass * config.physics.part_density).clamp(-1.0, 1.0);
        let reserves = f(gene::RESERVES);
        let longevity = f(gene::LONGEVITY);

        let [photo, chemo, scavenge, hunt_raw] = diet_capabilities(
            [
                f(gene::PHOTO),
                f(gene::CHEMO),
                f(gene::SCAVENGE),
                f(gene::HUNT),
            ],
            gc.generalist_penalty,
        );
        // Trace hunting genes from drift don't bite.
        let hunt = if hunt_raw < config.feeding.min_hunt {
            0.0
        } else {
            hunt_raw
        };

        let regen = f(gene::REGEN);
        let toughness = f(gene::TOUGHNESS);
        let restlessness = f(gene::RESTLESSNESS);

        let mut drives = [0.0; drive::COUNT];
        for (k, d) in drives.iter_mut().enumerate() {
            // Drives reach full strength halfway to either end of the byte's range, so a few
            // mutations are enough to change behavior.
            *d = (genome.signed(gene::DRIVES + k) * 2.0).clamp(-1.0, 1.0);
        }

        let birth_fraction =
            0.5 * 2f32.powf(-((255 - genome.get(gene::OFFSPRING_SIZE)) as f32) / 32.0);
        let brood = 1 + (genome.get(gene::BROOD) >> 5) as u32;
        let repro_threshold = 0.3 + 0.7 * f(gene::PATIENCE);

        // Swimming: thrust against drag in each direction. Long bodies slip through the water
        // lengthwise but not sideways, and turn slowly.
        let forward_drag = 1.0 - 0.45 * body.elongation;
        let side_drag = 1.0 + 0.6 * body.elongation + 0.1 * extra_segments;
        let drag = forward_drag + p.drag;
        let armor_penalty = 1.0 - config.physics.armor_speed_penalty * armor;
        let mut swim_dirs = [0.0; MOVE_DIRS];
        let mut crawl_dirs = [0.0; MOVE_DIRS];
        for k in 0..MOVE_DIRS {
            let sideways = move_dir(k).sin().abs();
            let d = lerp(forward_drag, side_drag, sideways) + p.drag;
            swim_dirs[k] = saturating(p.thrust[k] / d) * gc.speed_scale * armor_penalty;
            crawl_dirs[k] = pc.crawl_speed * saturating(p.crawl[k]);
        }
        let speed_coeff = swim_dirs.iter().cloned().fold(0.0, f32::max);
        let swim = speed_coeff / (gc.speed_scale * armor_penalty).max(1e-6);
        let crawl_coeff = crawl_dirs.iter().cloned().fold(0.0, f32::max);
        // The best direction, preferring forward when others are about as good.
        let axis_of = |v: &[f32; MOVE_DIRS]| {
            (0..MOVE_DIRS)
                .max_by(|&a, &b| {
                    let score = |k: usize| v[k] * (1.0 + 0.05 * move_dir(k).cos());
                    score(a)
                        .partial_cmp(&score(b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|k| wrap(move_dir(k)))
                .unwrap_or(0.0)
        };
        let move_axis = if speed_coeff > 0.0 {
            axis_of(&swim_dirs)
        } else {
            axis_of(&crawl_dirs)
        };
        // Turning needs an organ to turn with.
        let shape_turn = (1.0 - 0.35 * body.elongation) / (1.0 + 0.1 * extra_segments);
        let turn_with = |turning: f32| {
            if turning > 0.0 {
                lerp(gc.turn_min, gc.turn_max, saturating(turning)) * shape_turn
            } else {
                0.0
            }
        };
        let turn_rate = turn_with(p.turning);
        let walk_turn = turn_with(p.walk_turning);
        let anchor = saturating(2.0 * p.holdfast);
        let grip = saturating(p.grip + 2.0 * p.holdfast);

        // Senses: keep the longest-range eyes.
        p.eyes.sort_by(|a, b| {
            b.range
                .partial_cmp(&a.range)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut eyes = [Eye::default(); MAX_EYES];
        let eye_count = p.eyes.len().min(MAX_EYES);
        eyes[..eye_count].copy_from_slice(&p.eyes[..eye_count]);
        // Everyone smells what's right next to them; antennae reach much farther.
        let smell_radii = pc.base_smell + pc.smell_range * saturating(1.2 * p.antennae);
        let sense_radii = eyes[..eye_count]
            .iter()
            .map(|e| e.range)
            .fold(smell_radii, f32::max);

        let tolerance = f(gene::TEMP_TOL);
        let aerobic = f(gene::RESPIRATION);
        let uptake = f(gene::UPTAKE);
        let fixation = f(gene::FIXATION);
        // Trace values from drift make no toxin (and cost nothing).
        let toxin = if genome.get(gene::TOXIN) < 6 {
            0.0
        } else {
            f(gene::TOXIN)
        };
        let resistance = f(gene::RESISTANCE);

        let upkeep = 1.0
            + up.buoyancy * f(gene::BUOYANCY)
            + up.reserves * reserves
            + up.longevity * longevity
            + up.photo * photo
            + up.chemo * chemo
            + up.scavenge * scavenge
            + up.hunt * hunt
            + up.regen * regen
            + up.toughness * toughness
            + up.tolerance * tolerance
            + up.uptake * uptake
            + up.fixation * fixation
            + up.toxin * toxin
            + up.resistance * resistance
            + up.segments * extra_segments
            + up.parts * p.upkeep;

        let pigment = light.pigment_absorptance * photo;
        let absorptance = (light.body_absorptance + pigment).min(0.98);

        let hue = genome.get(gene::HUE);
        let saturation = genome.get(gene::SATURATION);
        let color = hsv_to_rgb(
            hue as f32 / 256.0,
            lerp(0.2, 1.0, saturation as f32 / 255.0),
            0.92,
        );

        Traits {
            adult_radius,
            adult_mass,
            elongation: body.elongation,
            segments: body.segments,
            part_count: instances.len() as u8,
            armor,
            spikes,
            armor_sides,
            spikes_sides,
            mouth_offset,
            mouth_radius,
            jaws,
            bites,
            bite_count: bite_count as u8,
            buoyancy,
            reserve_factor: lerp(gc.reserves_min, gc.reserves_max, reserves),
            lifespan: gc.lifespan_base
                * adult_radius.sqrt()
                * lerp(1.0, gc.longevity_max, longevity),
            photo,
            chemo,
            scavenge,
            hunt,
            regen: lerp(gc.regen_min, gc.regen_max, regen),
            hp_factor: lerp(gc.toughness_min, gc.toughness_max, toughness),
            swim,
            speed_coeff,
            swim_dirs,
            crawl_coeff,
            crawl_dirs,
            move_axis,
            grip,
            anchor,
            drag,
            turn_rate,
            walk_turn,
            restlessness,
            drives,
            birth_fraction,
            brood,
            repro_threshold,
            eyes,
            eye_count: eye_count as u8,
            smell_radii,
            sense_radii,
            grasps,
            grasp_count: grasp_count as u8,
            filter: p.filter,
            light_span: 1.0 + pc.frond_span * p.fronds,
            uptake_area: 1.0 + pc.frond_uptake * p.fronds,
            lure: saturating(1.5 * p.lure),
            build_factor: 1.0 + p.mass + gc.segment_cost * extra_segments,
            aerobic,
            ferment: (1.0 - aerobic + config.chemistry.ferment_floor).min(1.0),
            oxygenic: f(gene::OXYGENIC),
            uptake,
            fixation,
            toxin,
            resistance: 0.9 * resistance,
            temp_pref: f(gene::TEMP_PREF) * gc.temp_max,
            temp_tol: lerp(gc.tolerance_min, gc.tolerance_max, tolerance),
            upkeep,
            absorptance,
            pigment,
            color,
            hue,
            saturation,
            pattern: genome.get(gene::PATTERN),
        }
    }

    #[inline]
    pub fn max_speed(&self, radius: f32) -> f32 {
        self.speed_coeff * radius.sqrt()
    }

    /// Swimming speed multiplier toward a body-relative angle (0 = forward).
    #[inline]
    pub fn swim_toward(&self, angle: f32) -> f32 {
        toward(&self.swim_dirs, angle)
    }

    /// Walking speed multiplier toward a body-relative angle (0 = forward).
    #[inline]
    pub fn crawl_toward(&self, angle: f32) -> f32 {
        toward(&self.crawl_dirs, angle)
    }

    /// Armor and spikes facing an attack coming from `angle` (radians, body-local: 0 = from the
    /// front, π = from behind), blended between the neighboring sides.
    pub fn defense_toward(&self, angle: f32) -> (f32, f32) {
        let s = angle.rem_euclid(std::f32::consts::TAU) / std::f32::consts::FRAC_PI_2;
        let i = (s.floor() as usize) % 4;
        let j = (i + 1) % 4;
        let t = s - s.floor();
        (
            lerp(self.armor_sides[i], self.armor_sides[j], t),
            lerp(self.spikes_sides[i], self.spikes_sides[j], t),
        )
    }

    /// Can move under its own power (swimming, or walking on rock).
    #[inline]
    pub fn is_motile(&self) -> bool {
        self.speed_coeff > 0.0 || self.crawl_coeff > 0.0
    }

    /// Needs to know whether it touches rock (to walk, grip, or anchor).
    #[inline]
    pub fn uses_contact(&self) -> bool {
        self.crawl_coeff > 0.0 || self.grip > 0.0
    }

    /// Dominant diet as an index 0..4 (photo, chemo, scavenge, hunt).
    pub fn main_diet(&self) -> usize {
        let caps = [self.photo, self.chemo, self.scavenge, self.hunt];
        let mut best = 0;
        for i in 1..4 {
            if caps[i] > caps[best] {
                best = i;
            }
        }
        best
    }

    /// Widest angle any eye sees, in degrees (for display).
    pub fn eyes_span_deg(&self) -> f32 {
        self.eyes[..self.eye_count as usize]
            .iter()
            .map(|e| 2.0 * e.cos_half.acos().to_degrees())
            .sum::<f32>()
            .min(360.0)
    }
}

/// Plain-English meaning of one gene value, for the inspector.
pub fn describe_gene(index: usize, genome: &Genome, traits: &Traits, config: &Config) -> String {
    let v = genome.get(index);
    let pct = |x: f32| format!("{:.0}%", x * 100.0);
    let signed = |x: f32| {
        if x.abs() < 0.02 {
            "indifferent".to_string()
        } else if x > 0.0 {
            format!("toward {:+.0}%", x * 100.0)
        } else {
            format!("away {:+.0}%", x * 100.0)
        }
    };
    if (gene::PARTS..gene::PARTS + 3 * parts::SLOTS).contains(&index) {
        return describe_part_gene(index, genome);
    }
    match index {
        gene::SIZE => format!("adult radius {:.1}", traits.adult_radius),
        gene::BUOYANCY => {
            let b = genome.signed(gene::BUOYANCY);
            if b.abs() < 0.05 {
                "neutral".into()
            } else if b > 0.0 {
                format!("floats {}", pct(b))
            } else {
                format!("sinks {}", pct(-b))
            }
        }
        gene::RESERVES => format!("stores {:.1}× body mass", traits.reserve_factor),
        gene::LONGEVITY => format!("lives ~{}", format_duration(traits.lifespan)),
        gene::ELONGATION => {
            let aspect = 1.0 + 3.0 * traits.elongation;
            if aspect < 1.15 {
                "round".into()
            } else {
                format!("{aspect:.1}× as long as wide")
            }
        }
        gene::SEGMENTS => match traits.segments {
            1 => "1 segment".into(),
            n => format!("{n} segments"),
        },
        gene::HUE => format!("{:.0}°", v as f32 / 256.0 * 360.0),
        gene::SATURATION => pct(lerp(0.2, 1.0, v as f32 / 255.0)),
        gene::PATTERN => ["plain", "spots", "stripes", "rings"][(v >> 6) as usize].into(),
        gene::PHOTO => format!("{} effective", pct(traits.photo)),
        gene::CHEMO => format!("{} effective", pct(traits.chemo)),
        gene::SCAVENGE => format!("{} effective", pct(traits.scavenge)),
        gene::HUNT => format!("{} effective", pct(traits.hunt)),
        gene::RESPIRATION => {
            let a = traits.aerobic;
            if a > 0.85 {
                "breathes oxygen (needs it)".into()
            } else if a < 0.15 {
                "ferments (oxygen is poison)".into()
            } else {
                format!("{} oxygen, the rest fermented", pct(a))
            }
        }
        gene::OXYGENIC => format!(
            "{} releases O₂, {} uses minerals",
            pct(traits.oxygenic),
            pct(1.0 - traits.oxygenic)
        ),
        gene::UPTAKE => pct(traits.uptake),
        gene::FIXATION => {
            if traits.fixation < 0.02 {
                "none".into()
            } else {
                format!("makes its own nutrients ({})", pct(traits.fixation))
            }
        }
        gene::TOXIN => {
            if traits.toxin < 0.02 {
                "none".into()
            } else {
                format!("leaks poison ({})", pct(traits.toxin))
            }
        }
        gene::RESISTANCE => format!("ignores {} of poison", pct(traits.resistance)),
        gene::REGEN => format!("{:.1}% HP/s", traits.regen * 100.0),
        gene::TOUGHNESS => format!("{:.2} HP per mass", traits.hp_factor),
        gene::RESTLESSNESS => pct(traits.restlessness),
        i if (gene::DRIVES..gene::DRIVES + drive::COUNT).contains(&i) => {
            signed(traits.drives[i - gene::DRIVES])
        }
        gene::OFFSPRING_SIZE => format!(
            "newborn {} of adult mass",
            pct_precise(traits.birth_fraction)
        ),
        gene::BROOD => format!("{} per brood", traits.brood),
        gene::PATIENCE => format!("breeds at {} reserves", pct(traits.repro_threshold)),
        gene::TEMP_PREF => format!("{:.1} °C", traits.temp_pref),
        gene::TEMP_TOL => format!("±{:.1} °C", traits.temp_tol),
        _ => {
            let _ = config;
            String::new()
        }
    }
}

/// Meaning of one byte of a part slot.
fn describe_part_gene(index: usize, genome: &Genome) -> String {
    let slot = (index - gene::PARTS) / 3;
    let o = parts::slot_offset(slot);
    let kind_byte = genome.get(o);
    let Some(kind) = PartKind::from_code(kind_byte) else {
        return if index == o {
            "empty slot".into()
        } else {
            "unused".into()
        };
    };
    match index - o {
        0 => match parts::symmetry_count(kind_byte) {
            1 => kind.name().to_string(),
            2 => format!("pair of {}", kind.plural()),
            n => format!("ring of {n} {}", kind.plural()),
        },
        1 => format!("size {:.0}%", genome.frac(index) * 100.0),
        _ => {
            let (anchor, angle) = parts::decode_anchor(genome.get(index));
            let place = match anchor {
                parts::Anchor::Segment(k) => format!("segment {}", k + 1),
                parts::Anchor::EverySegment => "every segment".into(),
                parts::Anchor::OnPrevious => "on previous part".into(),
            };
            let deg = angle.to_degrees().round() as i32;
            let deg = if deg > 180 { deg - 360 } else { deg };
            format!("{place}, {deg:+}°")
        }
    }
}

fn pct_precise(x: f32) -> String {
    if x >= 0.1 {
        format!("{:.0}%", x * 100.0)
    } else {
        format!("{:.1}%", x * 100.0)
    }
}

/// `90 s`, `12 min`, `3.5 h`.
pub fn format_duration(seconds: f32) -> String {
    if seconds < 120.0 {
        format!("{seconds:.0} s")
    } else if seconds < 7200.0 {
        format!("{:.0} min", seconds / 60.0)
    } else {
        format!("{:.1} h", seconds / 3600.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ancestor() -> (Genome, Config) {
        let config = Config::default();
        (Genome::parse(&config.population.ancestor).unwrap(), config)
    }

    #[test]
    fn ancestor_decodes_as_designed() {
        let (g, config) = ancestor();
        let t = Traits::decode(&g, &config);
        assert!((t.adult_radius - 2.0).abs() < 1e-4);
        assert!((t.photo - 1.0).abs() < 1e-6);
        assert_eq!(t.hunt, 0.0);
        assert!(!t.is_motile());
        assert_eq!(t.brood, 1);
        assert!((t.birth_fraction - 0.5).abs() < 1e-6);
        assert!(t.buoyancy > 0.0);
        assert_eq!(t.eye_count, 0);
        assert_eq!(
            t.sense_radii, config.parts.base_smell,
            "it only smells what's close"
        );
        assert_eq!(t.part_count, 0);
        assert!(t.aerobic > 0.5 && t.oxygenic > 0.9);
        assert!(
            t.color[1] > t.color[0] && t.color[1] > t.color[2],
            "ancestor should be green"
        );
    }

    #[test]
    fn size_gene_doubles_every_0x20() {
        let (mut g, config) = ancestor();
        g.0[gene::SIZE] = 0x40;
        let r4 = Traits::decode(&g, &config).adult_radius;
        g.0[gene::SIZE] = 0x60;
        let r8 = Traits::decode(&g, &config).adult_radius;
        assert!((r4 - 4.0).abs() < 1e-3 && (r8 - 8.0).abs() < 1e-3);
        g.0[gene::SIZE] = 0xFF;
        assert!(Traits::decode(&g, &config).adult_radius > 240.0);
    }

    #[test]
    fn generalists_are_penalized() {
        let spec = diet_capabilities([1.0, 0.0, 0.0, 0.0], 0.6);
        let generalist = diet_capabilities([1.0, 0.0, 0.0, 1.0], 0.6);
        assert_eq!(spec[0], 1.0);
        assert!((generalist[0] - 0.4).abs() < 1e-6 && (generalist[3] - 0.4).abs() < 1e-6);
        assert!(
            generalist.iter().sum::<f32>() < 1.0,
            "generalists must lose overall"
        );
        // A small second diet costs the main diet little and works linearly.
        let trace = diet_capabilities([1.0, 0.0, 0.0, 0.05], 0.6);
        assert!(trace[0] > 0.96 && trace[3] > 0.015);
    }

    #[test]
    fn capabilities_cost_upkeep() {
        let (mut g, config) = ancestor();
        let base = Traits::decode(&g, &config).upkeep;
        g.0[gene::TOXIN] = 0xFF;
        let toxic = Traits::decode(&g, &config).upkeep;
        assert!(toxic > base);
        // Parts cost upkeep too.
        let o = parts::slot_offset(0);
        g.0[o] = PartKind::Shell.code();
        g.0[o + 1] = 0x80;
        assert!(Traits::decode(&g, &config).upkeep > toxic);
    }

    fn with_part(kind: PartKind, sym: u8, size: u8, place: u8) -> (Genome, Config) {
        let (mut g, config) = ancestor();
        let o = parts::slot_offset(0);
        g.0[o] = sym << 4 | kind.code();
        g.0[o + 1] = size;
        g.0[o + 2] = place;
        (g, config)
    }

    #[test]
    fn parts_give_abilities() {
        // No organs: can't swim or turn.
        let (g, config) = ancestor();
        let t = Traits::decode(&g, &config);
        assert!(!t.is_motile() && t.turn_rate == 0.0 && t.walk_turn == 0.0);
        // A flagellum at the back pushes a microbe forward (not back, not sideways).
        let (g, config) = with_part(PartKind::Flagellum, 0, 0x80, 16);
        let t = Traits::decode(&g, &config);
        assert!(t.is_motile() && t.max_speed(t.adult_radius) > 5.0);
        assert!(t.turn_rate > 0.0);
        let half_pi = std::f32::consts::FRAC_PI_2;
        assert!(t.swim_toward(0.0) > 5.0 * t.swim_toward(half_pi));
        assert_eq!(t.swim_toward(std::f32::consts::PI), 0.0);
        // One at the front pushes it backward, so it swims tail-first.
        let (g, config) = with_part(PartKind::Flagellum, 0, 0x80, 0);
        let t = Traits::decode(&g, &config);
        assert!(t.swim_toward(std::f32::consts::PI) > 0.0 && t.swim_toward(0.0) == 0.0);
        assert!((t.move_axis.abs() - std::f32::consts::PI).abs() < 1e-4);
        // A pair of flagella pointing up and down lets it swim vertically.
        let (g, config) = with_part(PartKind::Flagellum, 1, 0x80, 8);
        let t = Traits::decode(&g, &config);
        assert!(t.swim_toward(half_pi) > 5.0 * t.swim_toward(0.0));
        assert!(
            (t.move_axis.abs() - half_pi).abs() < 1e-4,
            "it swims sideways best"
        );
        // Fins on the sides row it forward; a fluke only pushes away from where it points.
        let (g, config) = with_part(PartKind::Fin, 1, 0x80, 8);
        let t = Traits::decode(&g, &config);
        assert!(t.swim_toward(0.0) > 5.0 * t.swim_toward(half_pi));
        let (g, config) = with_part(PartKind::Fluke, 0, 0x80, 16);
        let t = Traits::decode(&g, &config);
        assert!(t.swim_toward(0.0) > 0.0 && t.swim_toward(std::f32::consts::PI) == 0.0);
        // The same flagellum on a giant barely moves it; a fluke does better.
        let (mut big, _) = with_part(PartKind::Flagellum, 0, 0x80, 16);
        big.0[gene::SIZE] = 0xC0;
        let flagellum = Traits::decode(&big, &config).swim;
        big.0[parts::slot_offset(0)] = PartKind::Fluke.code();
        let fluke = Traits::decode(&big, &config).swim;
        assert!(
            fluke > 2.0 * flagellum,
            "fluke {fluke} flagellum {flagellum}"
        );

        // A pair of eyes gives two view cones.
        let (g, config) = with_part(PartKind::Eye, 1, 0xC0, 3);
        let t = Traits::decode(&g, &config);
        assert_eq!(t.eye_count, 2);
        assert!(t.sense_radii > 20.0);
        assert!((t.eyes[0].dir + t.eyes[1].dir).abs() < 1e-4, "mirrored");

        // Legs walk, holdfasts anchor, fronds widen the light catch.
        let (g, config) = with_part(PartKind::Leg, 1, 0xC0, 6 << 5 | 8);
        let t = Traits::decode(&g, &config);
        assert!(t.crawl_coeff > 0.0 && t.grip > 0.0 && t.is_motile());
        assert!(
            t.walk_turn > t.turn_rate && t.speed_coeff > 0.0,
            "legs row in open water and turn best while walking"
        );
        let (g, config) = with_part(PartKind::Holdfast, 0, 0x80, 16);
        assert!(Traits::decode(&g, &config).anchor > 0.5);
        let (g, config) = with_part(PartKind::Frond, 1, 0xFF, 8);
        assert!(Traits::decode(&g, &config).light_span > 2.0);

        // A plate on the back guards against attacks from behind, not from the front.
        let (g, config) = with_part(PartKind::Shell, 0, 0xFF, 16);
        let t = Traits::decode(&g, &config);
        let (behind, _) = t.defense_toward(std::f32::consts::PI);
        let (ahead, _) = t.defense_toward(0.0);
        assert!(behind > 0.5 && ahead < 0.3, "behind {behind} ahead {ahead}");
        // The mouth is at the front, farther out on a long body.
        let (mut long, config) = with_part(PartKind::Fin, 1, 0x80, 8);
        long.0[gene::ELONGATION] = 0xFF;
        long.0[gene::SEGMENTS] = 0xFF;
        let (round, _) = with_part(PartKind::Fin, 1, 0x80, 8);
        let (l, r) = (
            Traits::decode(&long, &config),
            Traits::decode(&round, &config),
        );
        assert!(l.mouth_offset > 3.0 * r.mouth_offset);
        assert!(l.turn_rate < r.turn_rate, "long bodies turn slowly");

        // A ring of shell plates is heavy armor that sinks.
        let (g, config) = with_part(PartKind::Shell, 5, 0xFF, 0);
        let t = Traits::decode(&g, &config);
        assert!(t.armor > 0.8 && t.buoyancy < 0.0);
    }

    #[test]
    fn every_gene_has_a_description() {
        let (g, config) = ancestor();
        let t = Traits::decode(&g, &config);
        for i in 0..crate::genome::GENOME_LEN {
            assert!(!describe_gene(i, &g, &t, &config).is_empty(), "gene {i}");
        }
    }
}

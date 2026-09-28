//! `--gallery`: a paused world with rows of organisms, each row sweeping one gene from 00 to FF.
//! Used to check that the art for each part and trait reads clearly and scales smoothly.

use evo_core::genome::gene;
use evo_core::parts::{PartKind, slot_offset};
use evo_core::rng::Rng;
use evo_core::{Config, Genome, Organism, World};

const COLUMNS: usize = 9;
const SPACING: f32 = 48.0;
const TOP: f32 = 180.0;
const LEFT: f32 = 1000.0;

/// Camera x and zoom that frame the gallery (y comes from `view_for`).
const VIEW: (f32, f32, f32) = (LEFT + SPACING * 4.0, 0.0, 0.3);

/// Rows: label and how to set the swept gene (value 0..=255).
type Setter = fn(&mut Genome, u8);

/// Put a part in slot `k`: kind, symmetry (0 single, 1 pair, 2–5 rings of 3–6), size, placement
/// (`where` 0–5 segment, 6 every segment, 7 on the previous part; angle 0–31, 0 = forward).
fn set_part(g: &mut Genome, k: usize, kind: PartKind, sym: u8, size: u8, place: u8, angle: u8) {
    let o = slot_offset(k);
    g.0[o] = sym << 4 | kind.code();
    g.0[o + 1] = size;
    g.0[o + 2] = place << 5 | angle;
}

pub fn rows() -> Vec<(&'static str, Setter)> {
    vec![
        ("Flagellum", |g, v| {
            set_part(g, 0, PartKind::Flagellum, 0, v, 0, 16)
        }),
        ("Cilia bands", |g, v| {
            set_part(g, 0, PartKind::Cilia, 1, v, 0, 8)
        }),
        ("Tentacles", |g, v| {
            set_part(g, 0, PartKind::Tentacle, 2, v, 0, 2)
        }),
        ("Fronds", |g, v| set_part(g, 0, PartKind::Frond, 1, v, 0, 7)),
        ("Fins", |g, v| set_part(g, 0, PartKind::Fin, 1, v, 0, 11)),
        ("Tail fluke", |g, v| {
            g.0[gene::ELONGATION] = 0x60;
            set_part(g, 0, PartKind::Fluke, 0, v, 0, 16)
        }),
        ("Legs", |g, v| {
            g.0[gene::ELONGATION] = 0x80;
            g.0[gene::SEGMENTS] = 0x60;
            set_part(g, 0, PartKind::Leg, 1, v, 6, 10)
        }),
        ("Holdfast", |g, v| {
            set_part(g, 0, PartKind::Holdfast, 0, v, 0, 24)
        }),
        ("Stalked eye", |g, v| {
            set_part(g, 0, PartKind::Stalk, 0, v, 0, 5);
            set_part(g, 1, PartKind::Eye, 0, 0xC0, 7, 0);
        }),
        ("Eyes", |g, v| set_part(g, 0, PartKind::Eye, 1, v, 0, 3)),
        ("Antennae", |g, v| {
            set_part(g, 0, PartKind::Antenna, 1, v, 0, 2)
        }),
        ("Lure", |g, v| set_part(g, 0, PartKind::Lure, 0, v, 0, 4)),
        ("Spines", |g, v| set_part(g, 0, PartKind::Spine, 5, v, 0, 3)),
        ("Jaws", |g, v| {
            g.0[gene::HUNT] = 0xC0;
            g.0[gene::PHOTO] = 0x20;
            set_part(g, 0, PartKind::Jaw, 1, v, 0, 2)
        }),
        ("Shell plates", |g, v| {
            set_part(g, 0, PartKind::Shell, 4, v, 0, 0)
        }),
        ("Elongation", |g, v| g.0[gene::ELONGATION] = v),
        ("Segments", |g, v| {
            g.0[gene::ELONGATION] = 0x20;
            g.0[gene::SEGMENTS] = v;
        }),
        ("Hunting mouth", |g, v| {
            g.0[gene::HUNT] = v;
            g.0[gene::PHOTO] = 255 - v;
            // A pink body with fronds: leaves should stay leaf-coloured as photosynthesis weakens.
            g.0[gene::HUE] = 0xEA;
            g.0[gene::SATURATION] = 0xC0;
            set_part(g, 0, PartKind::Frond, 1, 0x80, 0, 7)
        }),
        ("Scavenging", |g, v| {
            g.0[gene::SCAVENGE] = v;
            g.0[gene::PHOTO] = 255 - v;
        }),
        ("Oxygenic share", |g, v| {
            g.0[gene::OXYGENIC] = v;
            set_part(g, 0, PartKind::Frond, 1, 0x80, 0, 7)
        }),
        ("Toxin", |g, v| g.0[gene::TOXIN] = v),
        ("Nitrogen fixation", |g, v| g.0[gene::FIXATION] = v),
    ]
}

pub fn build() -> World {
    let mut config = Config::default();
    config.population.initial = 0;
    config.population.min_population = 0;
    config.events.enabled = false;
    let mut world = World::new(config);
    let config = world.config.clone();
    let h = config.world.height;
    let base = world.ancestor;
    let mut id = 1_000_000;
    for (row, (_, set)) in rows().into_iter().enumerate() {
        for col in 0..COLUMNS {
            let value = ((col * 255) / (COLUMNS - 1)) as u8;
            let mut genome = base;
            genome.0[gene::SIZE] = 0x40; // radius 4, big enough to see detail
            set(&mut genome, value);
            let x = LEFT + col as f32 * SPACING;
            let y = h - TOP - row as f32 * SPACING;
            let traits = evo_core::Traits::decode(&genome, &config);
            let mut o = Organism::new(
                id,
                0,
                0,
                0,
                genome,
                0,
                &config,
                glam::Vec2::new(x, y),
                traits.adult_mass,
                traits.adult_mass,
                Rng::new(id),
                0,
            );
            o.heading = 0.0;
            o.steer_heading = 0.0;
            world.organisms.push(o);
            id += 1;
        }
    }
    animation_rows(&mut world, rows().len(), &mut id);
    world
}

/// Rows below the gene rows showing the budding, dying, swallowing and grasping animations
/// frozen at nine stages.
fn animation_rows(world: &mut World, first_row: usize, id: &mut u64) {
    use crate::render::{BUD_TIME, DEATH_FADE, GRASP_TIME};
    use evo_core::world::{Fx, FxBody, FxKind};
    let config = world.config.clone();
    let dt = config.time.dt;
    let h = config.world.height;
    // The world is paused, so each animation stays at the stage it started at.
    world.tick = 10_000;
    let now = world.tick;
    let ago = |secs: f32| now - (secs / dt).round() as u64;
    let mut base = world.ancestor;
    base.0[gene::SIZE] = 0x40;
    base.0[gene::PATTERN] = 0x50;
    let y_of = |row: usize| h - TOP - row as f32 * SPACING;

    // Budding: a small bud, then fission (two halves).
    for (k, offspring_size) in [0xD0u8, 0xFF].into_iter().enumerate() {
        let mut genome = base;
        genome.0[gene::OFFSPRING_SIZE] = offspring_size;
        let traits = evo_core::Traits::decode(&genome, &config);
        let child_mass = traits.adult_mass * traits.birth_fraction;
        let parent_mass = traits.adult_mass - child_mass;
        let y = y_of(first_row + k);
        for col in 0..COLUMNS {
            let stage = col as f32 / (COLUMNS - 1) as f32;
            let x = LEFT + col as f32 * SPACING - 6.0;
            let make = |id: u64, pos: glam::Vec2, mass: f32| {
                let mut o = Organism::new(
                    id,
                    0,
                    0,
                    0,
                    genome,
                    0,
                    &config,
                    pos,
                    mass,
                    mass,
                    Rng::new(id),
                    0,
                );
                o.heading = 0.0;
                o.steer_heading = 0.0;
                o
            };
            let mut parent = make(*id, glam::Vec2::new(x, y), parent_mass);
            parent.budded_tick = ago(stage * BUD_TIME);
            parent.bud_scale = (traits.adult_mass / parent_mass).sqrt();
            let offset = glam::Vec2::new(parent.radius + child_mass.sqrt() + 0.5, 0.0);
            let mut child = make(*id + 1, parent.pos + offset, child_mass);
            child.born = ago(stage * BUD_TIME);
            child.bud_offset = offset;
            world.organisms.push(parent);
            world.organisms.push(child);
            *id += 2;
        }
    }

    // Dying: the colours drain to grey.
    let traits = evo_core::Traits::decode(&base, &config);
    let y = y_of(first_row + 2);
    for col in 0..COLUMNS {
        let stage = col as f32 / (COLUMNS - 1) as f32;
        let pos = glam::Vec2::new(LEFT + col as f32 * SPACING, y);
        let value = traits.adult_mass * 2.0;
        world.carcasses.push(evo_core::organism::Carcass {
            pos,
            vel: glam::Vec2::ZERO,
            energy: value,
            radius: traits.adult_radius,
            color: traits.color,
            species: 0,
            initial_energy: value,
            nutrients: 0.0,
            heading: 0.0,
            genome: base,
            seed: col as f32 * 0.11,
            hurt_tick: 0,
            died_tick: ago(stage * DEATH_FADE),
        });
    }

    // Swallowed whole: the prey flashes into a corpse and fades away.
    let y = y_of(first_row + 3);
    for col in 0..COLUMNS {
        let stage = col as f32 / (COLUMNS - 1) as f32;
        world.fx.push(Fx {
            kind: FxKind::Engulf,
            pos: glam::Vec2::new(LEFT + col as f32 * SPACING, y),
            radius: traits.adult_radius,
            color: traits.color,
            tick: ago(stage * FxKind::Engulf.duration() * 0.999),
            body: Some(FxBody {
                genome: base,
                heading: 0.0,
                seed: col as f32 * 0.13,
            }),
        });
    }

    // Grasping: a tentacle closing on prey behind (first columns), then holding prey around the
    // body, one more at a time, each tentacle its own.
    let mut hunter = base;
    hunter.0[gene::PHOTO] = 0x20;
    hunter.0[gene::HUNT] = 0xC0;
    hunter.0[gene::HUE] = 0xE0;
    set_part(&mut hunter, 0, PartKind::Tentacle, 2, 0xB0, 0, 16);
    let traits = evo_core::Traits::decode(&hunter, &config);
    let mut prey = base;
    prey.0[gene::SIZE] = 0x28;
    let prey_mass = evo_core::Traits::decode(&prey, &config).adult_mass;
    let y = y_of(first_row + 4);
    for col in 0..COLUMNS {
        let x = LEFT + col as f32 * SPACING;
        let closing = (col as f32 / 4.0).min(1.0);
        let held = if col < 5 { 1 } else { (col - 3).min(3) };
        let mut o = Organism::new(
            *id,
            0,
            0,
            0,
            hunter,
            0,
            &config,
            glam::Vec2::new(x, y),
            traits.adult_mass,
            1.0,
            Rng::new(*id),
            0,
        );
        o.heading = 0.0;
        o.steer_heading = 0.0;
        *id += 1;
        for (k, zone) in traits.grasps[..traits.grasp_count as usize]
            .iter()
            .enumerate()
            .take(held)
        {
            let base_at = glam::Vec2::new(zone.x, zone.y) * o.radius;
            let turn = if col < 5 { 0.5 } else { 0.9 - 0.2 * col as f32 };
            let dir = glam::Vec2::from_angle(base_at.to_angle() + turn);
            let at = o.pos + base_at + dir * (zone.radius * o.radius * 0.7);
            o.grasps.resize(k + 1, evo_core::organism::Grasp::default());
            o.grasps[k] = evo_core::organism::Grasp {
                pos: at,
                radius: prey_mass.sqrt(),
                since: ago(closing * GRASP_TIME),
                tick: now,
            };
            let mut p = Organism::new(
                *id,
                0,
                0,
                0,
                prey,
                0,
                &config,
                at,
                prey_mass,
                prey_mass,
                Rng::new(*id),
                0,
            );
            p.heading = 0.0;
            world.organisms.push(p);
            *id += 1;
        }
        world.organisms.push(o);
    }
}

/// The gallery's view centre depends on the world height, so compute it at runtime.
pub fn view_for(config: &Config) -> (f32, f32, f32) {
    let rows = rows().len() as f32 + 5.0;
    (
        VIEW.0,
        config.world.height - TOP - (rows - 1.0) * SPACING * 0.5,
        VIEW.2,
    )
}

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
    world
}

/// The gallery's view centre depends on the world height, so compute it at runtime.
pub fn view_for(config: &Config) -> (f32, f32, f32) {
    let rows = rows().len() as f32;
    (
        VIEW.0,
        config.world.height - TOP - (rows - 1.0) * SPACING * 0.5,
        VIEW.2,
    )
}

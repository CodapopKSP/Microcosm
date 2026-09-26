//! `--gallery`: a paused world with rows of organisms, each row sweeping one gene from 00 to FF.
//! Used to check that the art for each trait reads clearly and scales smoothly.

use evo_core::genome::gene;
use evo_core::rng::Rng;
use evo_core::{Config, Genome, Organism, World};

const COLUMNS: usize = 9;
const SPACING: f32 = 30.0;
const TOP: f32 = 180.0;
const LEFT: f32 = 1000.0;

/// Camera x and zoom that frame the gallery (y comes from `view_for`).
const VIEW: (f32, f32, f32) = (LEFT + SPACING * 4.0, 0.0, 0.2);

/// Rows: label and how to set the swept gene (value 0..=255).
type Setter = fn(&mut Genome, u8);

fn rows() -> Vec<(&'static str, Setter)> {
    vec![
        ("Eye range (fov 00)", |g, v| g.0[gene::SENSE_RANGE] = v),
        ("Field of view (range C0)", |g, v| {
            g.0[gene::SENSE_RANGE] = 0xC0;
            g.0[gene::FOV] = v;
        }),
        ("Hunting (jaws 00)", |g, v| {
            g.0[gene::HUNT] = v;
            g.0[gene::PHOTO] = 255 - v;
        }),
        ("Jaws (hunting C0)", |g, v| {
            g.0[gene::HUNT] = 0xC0;
            g.0[gene::PHOTO] = 0x20;
            g.0[gene::JAWS] = v;
        }),
        ("Scavenging", |g, v| {
            g.0[gene::SCAVENGE] = v;
            g.0[gene::PHOTO] = 255 - v;
        }),
        ("Speed", |g, v| {
            g.0[gene::SPEED] = v;
            g.0[gene::RESTLESSNESS] = 0xFF;
        }),
        ("Agility (speed A0)", |g, v| {
            g.0[gene::SPEED] = 0xA0;
            g.0[gene::RESTLESSNESS] = 0xFF;
            g.0[gene::AGILITY] = v;
        }),
        ("Armor", |g, v| g.0[gene::ARMOR] = v),
        ("Spikes", |g, v| g.0[gene::SPIKES] = v),
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

//! Water inspector: what the water is like in one tile (right-click a spot).

use bevy_egui::egui;
use evo_core::World;
use glam::Vec2;

use super::{DIET_NAMES, diet_color};

/// One row of the table: name, value, and a short note on what it means for life.
fn row(ui: &mut egui::Ui, name: &str, value: String, note: String) {
    ui.label(egui::RichText::new(name).weak());
    ui.label(egui::RichText::new(value).strong());
    ui.label(note);
    ui.end_row();
}

pub fn draw(ui: &mut egui::Ui, world: &World, pos: Vec2) {
    let cfg = &world.config;
    let env = &world.env;
    let height = cfg.world.height;
    let depth = (height - pos.y).max(0.0);

    // The chemistry fields share one grid; that cell is "the tile".
    let grid = &env.oxygen;
    let cell = grid.cell;
    let (cx, cy) = grid.coords(pos);
    let (x0, y0) = (cx as f32 * cell, cy as f32 * cell);
    ui.label(format!(
        "x {:.0}, depth {:.0} · tile x {:.0}–{:.0}, depth {:.0}–{:.0}",
        pos.x,
        depth,
        x0,
        x0 + cell,
        (height - y0 - cell).max(0.0),
        height - y0,
    ));
    if world.terrain.is_solid(pos) {
        ui.label("Solid rock.");
        return;
    }
    let rock = world.terrain.distance(pos);
    if rock < 200.0 {
        ui.label(format!("Rock {rock:.0} units away."));
    }
    ui.separator();

    egui::Grid::new("probe_table")
        .num_columns(3)
        .striped(true)
        .spacing([10.0, 4.0])
        .show(ui, |ui| {
            // Light.
            let light = env.light.level_at(pos);
            let noon = cfg.light.surface_intensity.max(1e-9);
            let now = env.clock.sun.max(1e-9);
            row(
                ui,
                "Light",
                format!("{:.1}%", light / noon * 100.0),
                format!(
                    "of noon surface light ({:.0}% of the surface right now)",
                    (light / now * 100.0).min(100.0)
                ),
            );
            let geometry = env.light.geometry();
            let weight = env.light.weight[geometry.index_at(pos)];
            if weight > 1e-4 {
                row(
                    ui,
                    "Shade",
                    format!("{:.0}%", (1.0 - (-weight).exp()) * 100.0),
                    "of the light here is caught by bodies".into(),
                );
            }
            if env.any_glow {
                let glow = env.glow.sample(pos);
                if glow > 1e-6 {
                    row(
                        ui,
                        "Lure glow",
                        format!("{:.1}%", glow / noon * 100.0),
                        "of noon light, from lures nearby".into(),
                    );
                }
            }

            // Temperature.
            let temp = env.temperature.sample(pos);
            row(
                ui,
                "Temperature",
                format!("{temp:.1} °C"),
                format!("surface {:.1} °C", env.clock.surface_temp),
            );

            // Chemistry.
            let cc = &cfg.chemistry;
            let oxygen = grid.get(pos);
            let breath = (oxygen / cc.oxygen_half).clamp(0.0, 1.0);
            row(
                ui,
                "Oxygen",
                format!("{oxygen:.2}"),
                if breath >= 1.0 {
                    "enough to breathe (1.0 = surface water)".into()
                } else {
                    format!("aerobes get {:.0}% of their breath", breath * 100.0)
                },
            );
            let co2 = env.co2.get(pos);
            let k = cc.co2_half;
            let fix = (co2 * (1.0 + k) / (co2 + k)).max(0.0);
            row(
                ui,
                "CO₂",
                format!("{co2:.2}"),
                format!("oxygen-making photosynthesis at {:.0}%", fix * 100.0),
            );
            let minerals = env.minerals.get(pos).max(0.0);
            row(
                ui,
                "Minerals",
                format!("{minerals:.3}"),
                format!(
                    "chemosynthesis food; sulfur photosynthesis at {:.0}%",
                    minerals / (minerals + cc.sulfur_half) * 100.0
                ),
            );
            row(
                ui,
                "Nutrients",
                format!("{:.3}", env.nutrients.get(pos)),
                "needed to build body mass".into(),
            );
            row(
                ui,
                "Detritus",
                format!("{:.4}", env.detritus.get(pos)),
                "marine snow (scavenger food)".into(),
            );
            if !env.snow_nutrients.data.is_empty() {
                row(
                    ui,
                    "Snow nutrients",
                    format!("{:.4}", env.snow_nutrients.get(pos)),
                    "in the marine snow, sinking; released as it rots".into(),
                );
            }
            let toxin = env.toxin.get(pos);
            if toxin >= 5e-5 {
                row(
                    ui,
                    "Toxin",
                    format!("{toxin:.4}"),
                    "harms all but the hue that made it".into(),
                );
            }

            // Current.
            let flow = evo_core::currents::flow(
                pos,
                world.time() as f32,
                height,
                &cfg.currents,
                cfg.world.seed,
            );
            let speed = flow.length();
            row(
                ui,
                "Current",
                format!("{speed:.1} u/s"),
                if speed > 0.05 {
                    format!("toward {}", compass(flow))
                } else {
                    "still".into()
                },
            );

            // Nearest vent.
            if let Some(v) = env
                .vents
                .iter()
                .min_by(|a, b| a.pos.distance(pos).total_cmp(&b.pos.distance(pos)))
            {
                let d = v.pos.distance(pos);
                if d < 1500.0 {
                    let kind = if v.fixed { "permanent vent" } else { "vent" };
                    row(
                        ui,
                        "Vent",
                        format!("{d:.0} u"),
                        format!("{kind}, at {:.0}% strength", v.strength * 100.0),
                    );
                }
            }
        });

    // Life in the tile.
    ui.separator();
    let inside = |p: Vec2| p.x >= x0 && p.x < x0 + cell && p.y >= y0 && p.y < y0 + cell;
    let mut diets = [0u32; 4];
    let mut count = 0u32;
    let mut mass = 0.0;
    for o in world.organisms.iter().filter(|o| inside(o.pos)) {
        count += 1;
        mass += o.mass;
        diets[o.traits.main_diet()] += 1;
    }
    let carcasses = world.carcasses.iter().filter(|c| inside(c.pos)).count();
    if count == 0 {
        ui.label("No organisms in this tile.");
    } else {
        ui.label(format!("{count} organisms in this tile (mass {mass:.0}):"));
        ui.horizontal_wrapped(|ui| {
            for (k, n) in diets.iter().enumerate().filter(|(_, n)| **n > 0) {
                ui.colored_label(diet_color(k), format!("{} {n}", DIET_NAMES[k]));
            }
        });
    }
    if carcasses > 0 {
        ui.label(format!("{carcasses} carcasses"));
    }
}

/// Rough direction of a vector, as a word.
fn compass(v: Vec2) -> &'static str {
    let names = [
        "the right",
        "up-right",
        "up",
        "up-left",
        "the left",
        "down-left",
        "down",
        "down-right",
    ];
    let a = v.y.atan2(v.x).rem_euclid(std::f32::consts::TAU);
    names[((a / std::f32::consts::FRAC_PI_4).round() as usize) % 8]
}

//! Organism inspector: vitals and the genome, chromosome by chromosome, in plain English.

use bevy_egui::egui;
use evo_core::World;
use evo_core::genome::{CHROMOSOMES, gene_code};
use evo_core::traits::{describe_gene, format_duration};

use super::{DIET_NAMES, diet_color, swatch};
use crate::sim::Selection;

const MUTATED: egui::Color32 = egui::Color32::from_rgb(255, 170, 60);

fn bar(ui: &mut egui::Ui, label: &str, fraction: f32, text: String, color: egui::Color32) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [62.0, 16.0],
            egui::Label::new(egui::RichText::new(label).weak()),
        );
        let bar = egui::ProgressBar::new(fraction.clamp(0.0, 1.0))
            .fill(color)
            .text(text)
            .desired_width(ui.available_width());
        ui.add(bar);
    });
}

pub fn draw(ui: &mut egui::Ui, world: &World, id: u64, selection: &mut Selection) {
    let Some(o) = world.find(id) else {
        ui.heading("Gone");
        ui.label("This organism has died.");
        if ui.button("Close").clicked() {
            selection.organism = None;
            selection.follow = false;
        }
        return;
    };
    let t = &o.traits;
    let species = world.species.get(o.species);

    ui.horizontal(|ui| {
        swatch(ui, t.color, 18.0);
        let name = world.species.name(o.species);
        let established = species.map(|s| s.established).unwrap_or(false);
        let title = if established {
            name.to_string()
        } else {
            match world.species.established_ancestor(o.species) {
                Some(a) => format!("Variant of {}", world.species.name(a)),
                None => "Unnamed variant".into(),
            }
        };
        ui.heading(title);
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("#{} · generation {}", o.id, o.generation)).weak());
        if let Some(s) = species
            && ui.small_button("Highlight species").clicked()
        {
            selection.species = Some(s.id);
        }
        let follow_label = if selection.follow {
            "Following"
        } else {
            "Follow"
        };
        if ui
            .selectable_label(selection.follow, follow_label)
            .clicked()
        {
            selection.follow = !selection.follow;
        }
        if ui.small_button("✖").clicked() {
            selection.organism = None;
            selection.follow = false;
        }
    });
    ui.label(format!(
        "{} · {} · {}",
        o.activity.label(),
        DIET_NAMES[t.main_diet()],
        if o.is_adult() { "adult" } else { "juvenile" }
    ));
    ui.add_space(4.0);

    bar(
        ui,
        "Energy",
        o.energy / o.max_energy().max(1e-6),
        format!("{:.2} / {:.2}", o.energy, o.max_energy()),
        egui::Color32::from_rgb(200, 170, 50),
    );
    bar(
        ui,
        "Health",
        o.hp / o.max_hp().max(1e-6),
        format!("{:.2} / {:.2}", o.hp, o.max_hp()),
        egui::Color32::from_rgb(190, 70, 70),
    );
    bar(
        ui,
        "Growth",
        o.mass / t.adult_mass,
        format!("radius {:.1} / {:.1}", o.radius, t.adult_radius),
        egui::Color32::from_rgb(80, 150, 200),
    );
    bar(
        ui,
        "Age",
        o.age / t.lifespan,
        format!(
            "{} / {}",
            format_duration(o.age),
            format_duration(t.lifespan)
        ),
        egui::Color32::from_rgb(120, 120, 150),
    );

    egui::Grid::new("vitals")
        .num_columns(4)
        .spacing([12.0, 2.0])
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Gaining").weak());
            ui.label(format!("{:.3}/s", o.gain_rate));
            ui.label(egui::RichText::new("Spending").weak());
            ui.label(format!("{:.3}/s", o.cost_rate));
            ui.end_row();
            ui.label(egui::RichText::new("Water").weak());
            ui.label(format!("{:.1} °C", o.temperature));
            ui.label(egui::RichText::new("Comfort").weak());
            ui.label(format!("{:.0} ± {:.0} °C", t.temp_pref, t.temp_tol));
            ui.end_row();
            ui.label(egui::RichText::new("Offspring").weak());
            ui.label(o.offspring.to_string());
            ui.label(egui::RichText::new("Kills").weak());
            ui.label(o.kills.to_string());
            ui.end_row();
            ui.label(egui::RichText::new("Depth").weak());
            ui.label(format!("{:.0}", world.config.world.height - o.pos.y));
            ui.label(egui::RichText::new("Upkeep").weak());
            ui.label(format!("×{:.2}", t.upkeep));
            ui.end_row();
        });

    // Diet capabilities as small bars.
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let caps = [t.photo, t.chemo, t.scavenge, t.hunt];
        for (i, cap) in caps.iter().enumerate() {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(86.0, 14.0), egui::Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(rect, 2.0, egui::Color32::from_gray(40));
            let mut fill = rect;
            fill.set_width(rect.width() * cap.clamp(0.0, 1.0));
            painter.rect_filled(fill, 2.0, diet_color(i));
            response.on_hover_text(format!("{}: {:.0}%", DIET_NAMES[i], cap * 100.0));
        }
    });

    ui.separator();
    ui.horizontal(|ui| {
        ui.heading("Genome");
        if ui.small_button("Copy").clicked() {
            ui.ctx().copy_text(o.genome.to_string());
        }
    });
    if o.mutations != 0 {
        ui.label(
            egui::RichText::new(format!(
                "{} gene(s) mutated from its parent (orange).",
                o.mutations.count_ones()
            ))
            .color(MUTATED),
        );
    }

    for c in &CHROMOSOMES {
        let bytes: Vec<String> = c
            .range()
            .map(|i| format!("{:02X}", o.genome.0[i]))
            .collect();
        let any_mutated = c.range().any(|i| o.mutations & (1 << i) != 0);
        let header =
            egui::RichText::new(format!("{}  {:<13} {}", c.letter, c.name, bytes.join(" ")))
                .monospace()
                .color(if any_mutated {
                    MUTATED
                } else {
                    egui::Color32::from_gray(220)
                });
        egui::CollapsingHeader::new(header)
            .id_salt(c.letter)
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new(("genes", c.letter))
                    .num_columns(4)
                    .spacing([8.0, 1.0])
                    .show(ui, |ui| {
                        for (k, i) in c.range().enumerate() {
                            let info = c.genes[k];
                            let value = o.genome.0[i];
                            let mutated = o.mutations & (1 << i) != 0;
                            let color = if mutated {
                                MUTATED
                            } else {
                                egui::Color32::from_gray(200)
                            };
                            ui.label(egui::RichText::new(gene_code(i)).monospace().weak());
                            ui.label(egui::RichText::new(info.name).color(color));
                            ui.label(
                                egui::RichText::new(format!("{value:02X}"))
                                    .monospace()
                                    .color(color),
                            );
                            let desc = describe_gene(i, &o.genome, t, &world.config);
                            ui.horizontal(|ui| {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(46.0, 8.0),
                                    egui::Sense::hover(),
                                );
                                let painter = ui.painter();
                                painter.rect_filled(rect, 1.0, egui::Color32::from_gray(45));
                                if info.signed {
                                    let mid = rect.center().x;
                                    let v = o.genome.signed(i);
                                    let x = mid + v * rect.width() * 0.5;
                                    let r = egui::Rect::from_min_max(
                                        egui::pos2(mid.min(x), rect.top()),
                                        egui::pos2(mid.max(x), rect.bottom()),
                                    );
                                    let c = if v >= 0.0 {
                                        egui::Color32::from_rgb(110, 190, 120)
                                    } else {
                                        egui::Color32::from_rgb(200, 110, 110)
                                    };
                                    painter.rect_filled(r, 1.0, c);
                                } else {
                                    let mut r = rect;
                                    r.set_width(rect.width() * value as f32 / 255.0);
                                    painter.rect_filled(
                                        r,
                                        1.0,
                                        egui::Color32::from_rgb(110, 150, 200),
                                    );
                                }
                                ui.label(egui::RichText::new(desc).small());
                            });
                            ui.end_row();
                        }
                    });
            });
    }
}

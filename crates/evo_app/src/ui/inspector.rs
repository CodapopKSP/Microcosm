//! Organism inspector: vitals and the genome, chromosome by chromosome, in plain English.

use bevy_egui::egui;
use evo_core::World;
use evo_core::genome::{CHROMOSOMES, gene_code};
use evo_core::parts::{self, BodyShape};
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
        "Nutrients",
        o.nutrients / o.max_nutrients(&world.config).max(1e-6),
        format!("{:.2} / {:.2}", o.nutrients, o.max_nutrients(&world.config)),
        egui::Color32::from_rgb(90, 170, 90),
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

    body_plan(ui, world, o);
    chemistry(ui, world, o);

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
                                if info.signed() {
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

/// Body shape, parts, and what they add up to.
fn body_plan(ui: &mut egui::Ui, world: &World, o: &evo_core::Organism) {
    let t = &o.traits;
    let shape = BodyShape::from_genome(&o.genome);
    let list = parts::decode_parts(&o.genome);
    let aspect = 1.0 + 3.0 * t.elongation;
    let form = if aspect < 1.15 {
        "Round".to_string()
    } else {
        format!("Elongated ({aspect:.1}:1)")
    };
    let segments = if t.segments > 1 {
        format!(", {} segments", t.segments)
    } else {
        String::new()
    };
    let header = format!(
        "Body plan: {} part{}",
        t.part_count,
        if t.part_count == 1 { "" } else { "s" }
    );
    egui::CollapsingHeader::new(egui::RichText::new(header).strong())
        .id_salt("body_plan")
        .default_open(true)
        .show(ui, |ui| {
            ui.label(format!("{form}{segments} body."));
            if list.is_empty() {
                ui.label(
                    egui::RichText::new("No body parts: it drifts and can't sense anything.")
                        .weak(),
                );
            }
            for p in &list {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(format!("P{}", p.slot + 1))
                            .monospace()
                            .weak(),
                    );
                    ui.label(parts::describe(p, &shape))
                        .on_hover_text(p.kind.role());
                });
            }
            ui.add_space(2.0);
            let mut abilities: Vec<String> = Vec::new();
            if t.speed_coeff > 0.0 {
                abilities.push(format!("swims {:.0} u/s", t.max_speed(o.radius)));
            }
            if t.crawl_coeff > 0.0 {
                abilities.push(format!(
                    "walks {:.0} u/s on rock",
                    t.crawl_coeff * o.radius.sqrt()
                ));
            }
            if t.is_motile() {
                abilities.push(format!("turns {:.0}°/s", t.turn_rate.to_degrees()));
            }
            if t.anchor > 0.0 {
                abilities.push(if o.anchored {
                    "anchored to rock".into()
                } else {
                    "anchors when it touches rock".into()
                });
            } else if t.grip > 0.05 {
                abilities.push(format!("grips rock ({:.0}%)", t.grip * 100.0));
            }
            let cap = world.config.genes.sense_cap;
            if t.eye_count > 0 {
                let range = t.eyes[..t.eye_count as usize]
                    .iter()
                    .map(|e| e.range)
                    .fold(0.0, f32::max);
                abilities.push(format!(
                    "{} eye{} see {:.0} u over {:.0}°",
                    t.eye_count,
                    if t.eye_count == 1 { "" } else { "s" },
                    (range * o.radius).min(cap),
                    t.eyes_span_deg()
                ));
            }
            if t.smell_radii > 0.0 {
                abilities.push(format!(
                    "smells {:.0} u around",
                    (t.smell_radii * o.radius).min(cap)
                ));
            }
            if t.reach > 0.0 {
                abilities.push(format!("reaches {:.0} u", t.reach * o.radius));
            }
            if t.light_span > 1.01 {
                abilities.push(format!("catches light ×{:.1} wide", t.light_span));
            }
            if t.filter > 0.01 {
                abilities.push(format!("filters marine snow ×{:.1}", 1.0 + t.filter));
            }
            if t.armor > 0.01 {
                abilities.push(format!(
                    "shell blocks {:.0}%",
                    t.armor * world.config.feeding.armor_protection * 100.0
                ));
            }
            if t.spikes > 0.01 {
                abilities.push(format!("spines hurt attackers ({:.0}%)", t.spikes * 100.0));
            }
            if t.jaws > 0.01 {
                abilities.push(format!("bite ×{:.1}", 0.2 + 1.8 * t.jaws));
            }
            if t.lure > 0.01 {
                abilities.push(format!("glowing lure ({:.0}%)", t.lure * 100.0));
            }
            if !abilities.is_empty() {
                ui.label(egui::RichText::new(abilities.join(" · ")).small());
            }
        });
}

/// Breathing, what it trades with the water, and the water around it.
fn chemistry(ui: &mut egui::Ui, world: &World, o: &evo_core::Organism) {
    let t = &o.traits;
    let c = &o.chem;
    let dt = world.config.time.dt;
    egui::CollapsingHeader::new(egui::RichText::new("Chemistry").strong())
        .id_salt("chemistry")
        .default_open(true)
        .show(ui, |ui| {
            let breathing = if c.shortfall > 0.01 {
                format!(
                    "Suffocating: {:.0}% of its needs unmet (not enough oxygen)",
                    c.shortfall * 100.0
                )
            } else if c.aerobic_share > 0.99 {
                "Breathing oxygen".into()
            } else if c.aerobic_share < 0.01 {
                "Fermenting (no oxygen used)".into()
            } else {
                format!(
                    "Breathing {:.0}%, fermenting {:.0}%",
                    c.aerobic_share * 100.0,
                    (1.0 - c.aerobic_share) * 100.0
                )
            };
            ui.label(breathing);
            let rate = |amount: f32, gives: &str, takes: &str| {
                let r = amount / dt;
                if r.abs() < 1e-5 {
                    None
                } else if r > 0.0 {
                    Some(format!("{gives} {r:.3}/s"))
                } else {
                    Some(format!("{takes} {:.3}/s", -r))
                }
            };
            let trades: Vec<String> = [
                rate(c.oxygen, "releases O₂", "uses O₂"),
                rate(c.co2, "releases CO₂", "uses CO₂"),
                rate(c.minerals, "releases minerals", "uses minerals"),
                rate(c.toxin, "leaks toxin", ""),
                rate(c.waste, "leaks waste", ""),
            ]
            .into_iter()
            .flatten()
            .collect();
            if !trades.is_empty() {
                ui.label(egui::RichText::new(trades.join(" · ")).small());
            }
            let poison_rate = c.poisoned / dt / o.max_hp().max(1e-6);
            if poison_rate >= 0.0005 {
                ui.label(
                    egui::RichText::new(format!(
                        "Poisoned: losing {:.1}% HP/s",
                        poison_rate * 100.0
                    ))
                    .color(egui::Color32::from_rgb(230, 90, 80)),
                );
            }
            let env = &world.env;
            let i = env.oxygen.index_at(o.pos);
            ui.label(
                egui::RichText::new(format!(
                    "Water here: O₂ {:.0}% · CO₂ {:.0}% · nutrients {:.2} · minerals {:.2}{}",
                    env.oxygen.data[i] * 100.0,
                    env.co2.data[i] * 100.0,
                    env.nutrients.data[i],
                    env.minerals.data[i],
                    if env.toxin.data[i] > 0.001 {
                        format!(" · toxin {:.3}", env.toxin.data[i])
                    } else {
                        String::new()
                    }
                ))
                .small()
                .weak(),
            );
            let _ = t;
        });
}

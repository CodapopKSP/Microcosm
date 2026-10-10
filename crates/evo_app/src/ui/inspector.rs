//! Organism inspector: a card beside the toolbar with the organism's name and actions, and
//! tabs for an overview (vitals, stats, diet), its body plan, its chemistry and its genome.

use bevy_egui::egui::{self, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2, vec2};
use evo_core::genome::{CHROMOSOMES, gene_code};
use evo_core::parts::{self, BodyShape};
use evo_core::traits::{describe_gene, format_game_duration};
use evo_core::{Organism, World};

use super::hud::{self, ACCENT, CARD_FILL, MUTED, TEXT, round_button};
use super::{DIET_NAMES, color32, diet_color};
use crate::sim::Selection;

/// Width of the inspector card.
pub const WIDTH: f32 = 380.0;

const MUTATED: Color32 = Color32::from_rgb(255, 170, 60);
const TRACK: Color32 = Color32::from_rgba_premultiplied(14, 16, 18, 18);
const DANGER: Color32 = Color32::from_rgb(235, 105, 95);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Overview,
    Body,
    Chemistry,
    Genome,
}

const TABS: [(Tab, &str); 4] = [
    (Tab::Overview, "Overview"),
    (Tab::Body, "Body"),
    (Tab::Chemistry, "Chemistry"),
    (Tab::Genome, "Genome"),
];

/// What the viewer asked for from the inspector.
pub enum Action {
    /// Sketch this organism in the field notebook.
    Record,
    /// It's already in the notebook: open it there.
    OpenNotebook,
}

// ---- Icons ----

fn icon_close(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let stroke = Stroke::new(1.8, color);
    let d = s * 0.6;
    p.line_segment([c + vec2(-d, -d), c + vec2(d, d)], stroke);
    p.line_segment([c + vec2(d, -d), c + vec2(-d, d)], stroke);
}

/// Crosshair: follow with the camera.
fn icon_follow(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let stroke = Stroke::new(1.5, color);
    p.circle_stroke(c, s * 0.6, stroke);
    for k in 0..4 {
        let d = Vec2::angled(k as f32 * std::f32::consts::FRAC_PI_2);
        p.line_segment([c + d * s * 0.35, c + d * s * 1.0], stroke);
    }
    p.circle_filled(c, s * 0.15, color);
}

/// A dot in a dashed ring: highlight the species on the map.
fn icon_highlight(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    p.circle_filled(c, s * 0.3, color);
    for k in 0..8 {
        let a = k as f32 / 8.0 * std::f32::consts::TAU;
        let (d0, d1) = (Vec2::angled(a), Vec2::angled(a + 0.45));
        p.line_segment([c + d0 * s * 0.85, c + d1 * s * 0.85], Stroke::new(1.5, color));
    }
}

// ---- Widgets ----

/// A rounded button with an icon and a label, lit when `on`.
fn pill(
    ui: &mut egui::Ui,
    text: &str,
    on: bool,
    icon: impl FnOnce(&egui::Painter, Pos2, f32, Color32),
) -> egui::Response {
    let font = FontId::proportional(13.0);
    let width = ui
        .painter()
        .layout_no_wrap(text.to_string(), font.clone(), TEXT)
        .size()
        .x;
    let (rect, response) = ui.allocate_exact_size(vec2(width + 38.0, 28.0), Sense::click());
    crate::ui_sfx::hover_sound(&response);
    let hovered = response.hovered();
    let fill = if on {
        Color32::from_rgb(38, 86, 77)
    } else if hovered {
        Color32::from_rgb(32, 49, 56)
    } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, 8)
    };
    let stroke = if on {
        ACCENT
    } else {
        Color32::from_rgba_unmultiplied(150, 240, 210, if hovered { 110 } else { 45 })
    };
    let p = ui.painter();
    p.rect(rect, 14.0, fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
    let color = if on || hovered { ACCENT } else { TEXT };
    icon(p, Pos2::new(rect.left() + 16.0, rect.center().y), 7.0, color);
    p.text(
        Pos2::new(rect.left() + 28.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        font,
        color,
    );
    response
}

/// A small tag with a coloured dot.
fn chip(ui: &mut egui::Ui, text: &str, dot: Option<Color32>) {
    let font = FontId::proportional(12.0);
    let width = ui
        .painter()
        .layout_no_wrap(text.to_string(), font.clone(), TEXT)
        .size()
        .x;
    let pad = if dot.is_some() { 25.0 } else { 14.0 };
    let (rect, _) = ui.allocate_exact_size(vec2(width + pad, 20.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 10.0, Color32::from_rgba_unmultiplied(255, 255, 255, 12));
    let mut x = rect.left() + 7.0;
    if let Some(c) = dot {
        p.circle_filled(Pos2::new(x + 3.0, rect.center().y), 3.5, c);
        x += 11.0;
    }
    p.text(Pos2::new(x, rect.center().y), egui::Align2::LEFT_CENTER, text, font, TEXT);
}

/// A section heading: small capitals and a faint rule.
fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(title.to_uppercase()).size(11.0).color(MUTED).strong());
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(150, 240, 210, 30));
    ui.add_space(4.0);
}

/// A labelled meter: a rounded track, filled by `fraction`, with the value at its end.
fn meter(ui: &mut egui::Ui, label: &str, fraction: f32, value: &str, color: Color32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 20.0), Sense::hover());
    let p = ui.painter();
    p.text(
        Pos2::new(rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.5),
        MUTED,
    );
    let track = Rect::from_min_max(
        Pos2::new(rect.left() + 74.0, rect.center().y - 6.0),
        Pos2::new(rect.right(), rect.center().y + 6.0),
    );
    p.rect_filled(track, 6.0, TRACK);
    let f = fraction.clamp(0.0, 1.0);
    if f > 0.0 {
        let mut fill = track;
        fill.set_width((track.width() * f).max(track.height()));
        p.rect_filled(fill, 6.0, color.gamma_multiply(0.85));
    }
    p.text(
        Pos2::new(track.right() - 6.0, track.center().y),
        egui::Align2::RIGHT_CENTER,
        value,
        FontId::proportional(11.5),
        TEXT,
    );
}

/// Pairs of (label, value) in two columns.
fn stats(ui: &mut egui::Ui, id: &str, rows: &[(&str, String)]) {
    egui::Grid::new(id)
        .num_columns(4)
        .spacing([10.0, 3.0])
        .min_col_width(40.0)
        .show(ui, |ui| {
            for pair in rows.chunks(2) {
                for (label, value) in pair {
                    ui.label(RichText::new(*label).size(12.5).color(MUTED));
                    ui.label(RichText::new(value).size(12.5));
                }
                ui.end_row();
            }
        });
}

/// A bulleted line.
fn bullet(ui: &mut egui::Ui, text: impl Into<String>, color: Color32) {
    ui.horizontal_wrapped(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(10.0, 14.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 2.5, color);
        ui.label(RichText::new(text.into()).size(12.5));
    });
}

/// The tab bar: one rounded track with the open tab lit.
fn tabs(ui: &mut egui::Ui, tab: &mut Tab) {
    hud::tab_bar(ui, tab, &TABS);
}

// ---- The card ----

/// Show the inspector for organism `id` inside `rect`.
pub fn show(
    ctx: &egui::Context,
    rect: Rect,
    world: &World,
    id: u64,
    selection: &mut Selection,
    tab: &mut Tab,
) -> Option<Action> {
    let mut action = None;
    egui::Area::new(egui::Id::new("inspector"))
        .fixed_pos(rect.min)
        .show(ctx, |ui| {
            // An area's room is last frame's size; give it the whole card's space.
            ui.set_max_size(rect.size());
            hud::card().show(ui, |ui| {
                ui.style_mut().visuals.override_text_color = Some(TEXT);
                let inner_w = rect.width() - 24.0;
                ui.set_width(inner_w);
                let Some(o) = world.find(id) else {
                    gone(ui, selection);
                    return;
                };
                action = header(ui, world, o, selection);
                ui.add_space(8.0);
                tabs(ui, tab);
                ui.add_space(4.0);
                // Down to the bottom of the card's space, less its margin.
                let room = rect.bottom() - ui.cursor().top() - 10.0;
                egui::ScrollArea::vertical()
                    .max_height(room.max(120.0))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.set_width(inner_w - 10.0);
                        match tab {
                            Tab::Overview => overview(ui, world, o),
                            Tab::Body => body(ui, world, o),
                            Tab::Chemistry => chemistry(ui, world, o),
                            Tab::Genome => genome(ui, world, o),
                        }
                        ui.add_space(4.0);
                    });
            });
        });
    action
}

fn gone(ui: &mut egui::Ui, selection: &mut Selection) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Gone").size(18.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if round_button(ui, false, 14.0, icon_close).on_hover_text("Close (Esc)").clicked() {
                selection.organism = None;
                selection.follow = false;
            }
        });
    });
    ui.label(RichText::new("This organism has died.").color(MUTED));
}

/// Name, who it is, and the actions.
fn header(ui: &mut egui::Ui, world: &World, o: &Organism, selection: &mut Selection) -> Option<Action> {
    let t = &o.traits;
    let species = world.species.get(o.species);
    let named = species.map(|s| s.established).unwrap_or(false);
    let title = if named {
        world.species.name(o.species).to_string()
    } else {
        match world.species.established_ancestor(o.species) {
            Some(a) => format!("Variant of {}", world.species.name(a)),
            None => "Unnamed variant".into(),
        }
    };
    let mut action = None;
    ui.horizontal(|ui| {
        // The body colour, as a cell.
        let (r, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
        let p = ui.painter();
        p.circle_filled(r.center(), 12.0, color32(t.color));
        p.circle_stroke(r.center(), 15.0, Stroke::new(1.2, ACCENT.gamma_multiply(0.5)));
        p.circle_filled(r.center() + vec2(-3.0, 1.0), 3.5, Color32::from_black_alpha(70));
        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - 40.0);
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.label(RichText::new(&title).size(17.0).strong());
            ui.label(
                RichText::new(format!("#{}  ·  generation {}", o.id, o.generation))
                    .size(12.0)
                    .color(MUTED),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if round_button(ui, false, 14.0, icon_close).on_hover_text("Close (Esc)").clicked() {
                selection.organism = None;
                selection.follow = false;
            }
        });
    });
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(5.0, 5.0);
        chip(ui, o.activity.label(), None);
        let diet = t.main_diet();
        chip(ui, DIET_NAMES[diet], Some(diet_color(diet)));
        chip(ui, if o.is_adult() { "Adult" } else { "Juvenile" }, None);
    });
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        if pill(ui, if selection.follow { "Following" } else { "Follow" }, selection.follow, icon_follow)
            .on_hover_text("Keep the camera on it (F)")
            .clicked()
        {
            selection.follow = !selection.follow;
        }
        if let Some(s) = species {
            let on = selection.species == Some(s.id);
            if pill(ui, "Highlight species", on, icon_highlight)
                .on_hover_text("Ring every member of its species on the map")
                .clicked()
            {
                selection.species = if on { None } else { Some(s.id) };
            }
        }
        let recorded = world.notebook.has_organism(o.id);
        let label = if recorded { "In notebook" } else { "Record" };
        let tip = if recorded {
            "Sketched in your Field Notebook: click to open it there"
        } else {
            "Sketch it in your Field Notebook, under its species"
        };
        if pill(ui, label, recorded, hud::icon_book).on_hover_text(tip).clicked() {
            action = Some(if recorded { Action::OpenNotebook } else { Action::Record });
        }
    });
    action
}

fn overview(ui: &mut egui::Ui, world: &World, o: &Organism) {
    let t = &o.traits;
    let tc = &world.config.time;
    section(ui, "Vitals");
    meter(ui, "Energy", o.energy / o.max_energy().max(1e-6), &format!("{:.2} / {:.2}", o.energy, o.max_energy()), Color32::from_rgb(225, 190, 80));
    meter(ui, "Health", o.hp / o.max_hp().max(1e-6), &format!("{:.2} / {:.2}", o.hp, o.max_hp()), Color32::from_rgb(220, 95, 95));
    meter(ui, "Growth", o.mass / t.adult_mass, &format!("radius {:.1} / {:.1}", o.radius, t.adult_radius), Color32::from_rgb(95, 165, 225));
    let max_n = o.max_nutrients(&world.config).max(1e-6);
    meter(ui, "Nutrients", o.nutrients / max_n, &format!("{:.2} / {:.2}", o.nutrients, max_n), Color32::from_rgb(105, 195, 115));
    meter(
        ui,
        "Age",
        o.age / t.lifespan,
        &format!("{} / {}", format_game_duration(o.age, tc), format_game_duration(t.lifespan, tc)),
        Color32::from_rgb(150, 150, 195),
    );

    section(ui, "Now");
    stats(
        ui,
        "inspector_stats",
        &[
            ("Gaining", format!("{:.3}/s", o.gain_rate)),
            ("Spending", format!("{:.3}/s", o.cost_rate)),
            ("Water", format!("{:.1} °C", o.temperature)),
            ("Comfort", format!("{:.0} ± {:.0} °C", t.temp_pref, t.temp_tol)),
            ("Depth", format!("{:.0}", world.config.world.height - o.pos.y)),
            ("Upkeep", format!("×{:.2}", t.upkeep)),
            ("Offspring", o.offspring.to_string()),
            ("Kills", o.kills.to_string()),
        ],
    );

    section(ui, "Diet");
    let caps = [t.photo, t.chemo, t.scavenge, t.hunt];
    let names = ["Sunlight", "Minerals", "Dead matter", "Prey"];
    for (i, cap) in caps.iter().enumerate() {
        meter(ui, names[i], *cap, &format!("{:.0}%", cap * 100.0), diet_color(i));
    }
}

pub(crate) fn body(ui: &mut egui::Ui, world: &World, o: &Organism) {
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
        format!(" in {} segments", t.segments)
    } else {
        String::new()
    };
    let count = match t.part_count {
        0 => "no parts".to_string(),
        1 => "1 part".to_string(),
        n => format!("{n} parts"),
    };
    section(ui, "Shape");
    ui.label(RichText::new(format!("{form} body{segments}, with {count}.")).size(12.5));

    section(ui, "Parts");
    if list.is_empty() {
        ui.label(RichText::new("No body parts: it drifts, sensing only what's right around it.").size(12.5).color(MUTED));
    }
    for p in &list {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(26.0, 18.0), Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(r, 5.0, Color32::from_rgba_unmultiplied(150, 240, 210, 28));
            painter.text(r.center(), egui::Align2::CENTER_CENTER, format!("P{}", p.slot + 1), FontId::monospace(11.0), ACCENT);
            ui.add(egui::Label::new(RichText::new(parts::describe(p, &shape)).size(12.5)).wrap())
                .on_hover_text(p.kind.role());
        });
    }

    section(ui, "Abilities");
    for line in abilities(world, o) {
        bullet(ui, line, ACCENT.gamma_multiply(0.8));
    }
}

/// What its body lets it do, one line each.
fn abilities(world: &World, o: &Organism) -> Vec<String> {
    let t = &o.traits;
    let mut out: Vec<String> = Vec::new();
    // Organs only allow movement in some directions relative to the body.
    let directions = |at: &dyn Fn(f32) -> f32| {
        let pi = std::f32::consts::PI;
        let sides = at(pi * 0.5).max(at(-pi * 0.5));
        let speed = |x: f32| x * o.radius.sqrt();
        let mut v = vec![format!("forward {:.0}", speed(at(0.0)))];
        if at(pi) > 0.05 {
            v.push(format!("back {:.0}", speed(at(pi))));
        }
        if sides > 0.05 {
            v.push(format!("sideways {:.0}", speed(sides)));
        }
        format!("{} u/s", v.join(", "))
    };
    if t.speed_coeff > 0.0 {
        out.push(format!("Swims {}", directions(&|a| t.swim_toward(a))));
    }
    if t.crawl_coeff > 0.0 {
        out.push(format!("Walks on rock {}", directions(&|a| t.crawl_toward(a))));
    }
    if t.turn_rate > 0.0 {
        out.push(format!("Turns {:.0}°/s", t.turn_rate.to_degrees()));
    } else if t.walk_turn > 0.0 {
        out.push(format!("Turns {:.0}°/s while walking", t.walk_turn.to_degrees()));
    } else if t.is_motile() {
        out.push("Can't turn".into());
    }
    if t.anchor > 0.0 {
        out.push(if o.anchored { "Anchored to rock".into() } else { "Anchors when it touches rock".into() });
    } else if t.grip > 0.05 {
        out.push(format!("Grips rock ({:.0}%)", t.grip * 100.0));
    }
    let cap = world.config.genes.sense_cap;
    let organs = t.eye_count + t.antenna_count;
    out.push(format!(
        "Senses light, water, bodies and carcasses {:.0} u around{}",
        (t.sense_radii * o.radius).min(cap),
        match organs {
            0 => String::new(),
            1 => " (widened by 1 sense organ)".into(),
            n => format!(" (widened by {n} sense organs)"),
        }
    ));
    if t.grasp_count > 0 {
        let reach = t.grasps[..t.grasp_count as usize].iter().map(|z| z.radius).fold(0.0, f32::max);
        out.push(format!("Tentacles grab within {:.0} u and pull to the mouth", reach * o.radius));
    }
    if t.light_span > 1.01 {
        out.push(format!("Catches light ×{:.1} wide", t.light_span));
    }
    if t.filter > 0.01 {
        out.push(format!("Filters marine snow ×{:.1}", 1.0 + t.filter));
    }
    // Plates and spines guard the side they're on.
    let sides = |v: &[f32; 4], scale: f32| {
        let names = ["front", "left", "back", "right"];
        let max = v.iter().cloned().fold(0.0, f32::max);
        if v.iter().all(|x| (x - max).abs() < 0.05) {
            format!("{:.0}% all round", max * scale * 100.0)
        } else {
            names
                .iter()
                .zip(v)
                .map(|(n, x)| format!("{n} {:.0}%", x * scale * 100.0))
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    if t.armor > 0.01 {
        out.push(format!("Shell blocks {}", sides(&t.armor_sides, world.config.feeding.armor_protection)));
    }
    if t.spikes > 0.01 {
        out.push(format!("Spines hurt attackers: {}", sides(&t.spikes_sides, 1.0)));
    }
    if t.jaws > 0.01 {
        out.push(format!("Bite ×{:.1}", 0.2 + 1.8 * t.jaws));
    }
    if t.lure > 0.01 {
        out.push(format!("Glowing lure ({:.0}%)", t.lure * 100.0));
    }
    out
}

/// Breathing, what it trades with the water, and the water around it.
fn chemistry(ui: &mut egui::Ui, world: &World, o: &Organism) {
    let c = &o.chem;
    let dt = world.config.time.dt;
    section(ui, "Breathing");
    let (text, color) = if c.shortfall > 0.01 {
        (format!("Suffocating: {:.0}% of its needs unmet (not enough oxygen)", c.shortfall * 100.0), DANGER)
    } else if c.aerobic_share > 0.99 {
        ("Breathing oxygen".into(), TEXT)
    } else if c.aerobic_share < 0.01 {
        ("Fermenting (no oxygen used)".into(), TEXT)
    } else {
        (format!("Breathing {:.0}%, fermenting {:.0}%", c.aerobic_share * 100.0, (1.0 - c.aerobic_share) * 100.0), TEXT)
    };
    ui.label(RichText::new(text).size(12.5).color(color));
    meter(ui, "Oxygen", c.aerobic_share, &format!("{:.0}%", c.aerobic_share * 100.0), Color32::from_rgb(120, 190, 240));
    let poison_rate = c.poisoned / dt / o.max_hp().max(1e-6);
    if poison_rate >= 0.0005 {
        bullet(ui, format!("Poisoned: losing {:.1}% HP/s", poison_rate * 100.0), DANGER);
    }

    section(ui, "Exchange with the water");
    let rate = |amount: f32| amount / dt;
    let mut rows: Vec<(&str, String)> = Vec::new();
    let mut add = |label: &'static str, r: f32, out_word: &str, in_word: &str| {
        if r.abs() >= 1e-5 {
            rows.push((label, if r > 0.0 { format!("{out_word} {r:.3}/s") } else { format!("{in_word} {:.3}/s", -r) }));
        }
    };
    add("O₂", rate(c.oxygen), "gives", "takes");
    add("CO₂", rate(c.co2), "gives", "takes");
    add("Minerals", rate(c.minerals), "gives", "takes");
    add("Toxin", rate(c.toxin), "leaks", "takes");
    add("Waste", rate(c.waste), "leaks", "takes");
    if rows.is_empty() {
        ui.label(RichText::new("Nothing right now.").size(12.5).color(MUTED));
    } else {
        egui::Grid::new("exchange").num_columns(2).spacing([14.0, 3.0]).show(ui, |ui| {
            for (label, value) in rows {
                ui.label(RichText::new(label).size(12.5).color(MUTED));
                ui.label(RichText::new(value).size(12.5));
                ui.end_row();
            }
        });
    }

    section(ui, "The water here");
    let env = &world.env;
    let i = env.oxygen.index_at(o.pos);
    let mut rows = vec![
        ("Oxygen", format!("{:.0}%", env.oxygen.data[i] * 100.0)),
        ("CO₂", format!("{:.0}%", env.co2.data[i] * 100.0)),
        ("Nutrients", format!("{:.2}", env.nutrients.data[i])),
        ("Minerals", format!("{:.2}", env.minerals.data[i])),
    ];
    if env.toxin.data[i] > 0.001 {
        rows.push(("Toxin", format!("{:.3}", env.toxin.data[i])));
    }
    stats(ui, "water_here", &rows);
}

fn genome(ui: &mut egui::Ui, world: &World, o: &Organism) {
    let t = &o.traits;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let mutated = o.mutations.count_ones();
        if mutated > 0 {
            let (r, _) = ui.allocate_exact_size(vec2(10.0, 14.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 4.0, MUTATED);
            ui.label(
                RichText::new(format!("{mutated} gene{} mutated from its parent", if mutated == 1 { "" } else { "s" }))
                    .size(12.5)
                    .color(MUTATED),
            );
        } else {
            ui.label(RichText::new("Same genes as its parent").size(12.5).color(MUTED));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if pill(ui, "Copy", false, |p, c, s, col| {
                let r = Rect::from_center_size(c + vec2(1.5, 1.5), vec2(s * 1.3, s * 1.5));
                p.rect_stroke(r, 2.0, Stroke::new(1.3, col), egui::StrokeKind::Middle);
                let r2 = r.translate(vec2(-3.0, -3.0));
                p.rect_filled(r2, 2.0, CARD_FILL);
                p.rect_stroke(r2, 2.0, Stroke::new(1.3, col), egui::StrokeKind::Middle);
            })
            .on_hover_text("Copy the genome as text")
            .clicked()
            {
                ui.ctx().copy_text(o.genome.to_string());
            }
        });
    });

    for c in &CHROMOSOMES {
        // Part slots are three bytes each: group them, as the genome is written elsewhere.
        let bytes: Vec<String> = if c.letter == 'P' {
            c.range()
                .collect::<Vec<_>>()
                .chunks(3)
                .map(|k| k.iter().map(|&i| format!("{:02X}", o.genome.0[i])).collect::<String>())
                .collect()
        } else {
            c.range().map(|i| format!("{:02X}", o.genome.0[i])).collect()
        };
        let bytes = bytes.join(" ");
        let long = bytes.len() > 30;
        let any_mutated = c.range().any(|i| o.mutations & (1 << i) != 0);
        let id = ui.make_persistent_id(("chromosome", c.letter));
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
        ui.add_space(4.0);
        let header = ui.horizontal(|ui| {
            let height = if long { 38.0 } else { 24.0 };
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
            let p = ui.painter();
            let hovered = resp.hovered();
            p.rect_filled(r, 8.0, Color32::from_rgba_unmultiplied(255, 255, 255, if hovered { 16 } else { 8 }));
            let badge = Rect::from_min_size(r.min + vec2(4.0, 3.0), vec2(18.0, 18.0));
            p.rect_filled(badge, 5.0, if any_mutated { MUTATED.gamma_multiply(0.35) } else { Color32::from_rgba_unmultiplied(150, 240, 210, 35) });
            p.text(badge.center(), egui::Align2::CENTER_CENTER, c.letter.to_string(), FontId::monospace(12.0), if any_mutated { MUTATED } else { ACCENT });
            let row_y = badge.center().y;
            p.text(Pos2::new(badge.right() + 8.0, row_y), egui::Align2::LEFT_CENTER, c.name, FontId::proportional(13.0), TEXT);
            let byte_color = if any_mutated { MUTATED } else { MUTED };
            if long {
                // Too long for the name's row: on a line of its own below it.
                p.text(
                    Pos2::new(badge.left() + 2.0, row_y + 17.0),
                    egui::Align2::LEFT_CENTER,
                    &bytes,
                    FontId::monospace(10.0),
                    byte_color,
                );
            } else {
                p.text(Pos2::new(r.right() - 22.0, row_y), egui::Align2::RIGHT_CENTER, &bytes, FontId::monospace(11.5), byte_color);
            }
            // Open/closed chevron.
            let cx = Pos2::new(r.right() - 10.0, row_y);
            let open = state.is_open();
            let pts = if open {
                vec![cx + vec2(-4.0, -2.0), cx + vec2(0.0, 2.5), cx + vec2(4.0, -2.0)]
            } else {
                vec![cx + vec2(-2.0, -4.0), cx + vec2(2.5, 0.0), cx + vec2(-2.0, 4.0)]
            };
            p.add(egui::Shape::line(pts, Stroke::new(1.5, MUTED)));
            resp
        });
        crate::ui_sfx::silent(&header.inner);
        if header.inner.clicked() {
            state.toggle(ui);
        }
        state.show_body_unindented(ui, |ui| {
            // Each gene on two lines: its code, name, a bar and its value; then what it means
            // underneath (descriptions can be long, so they get the card's whole width).
            for (k, i) in c.range().enumerate() {
                let info = c.genes[k];
                let value = o.genome.0[i];
                let mutated = o.mutations & (1 << i) != 0;
                let color = if mutated { MUTATED } else { TEXT };
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [28.0, 16.0],
                        egui::Label::new(RichText::new(gene_code(i)).monospace().size(11.0).color(MUTED)),
                    );
                    ui.label(RichText::new(info.name).size(12.5).color(color));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(4.0);
                        ui.label(RichText::new(format!("{value:02X}")).monospace().size(12.0).color(color));
                        let (rect, _) = ui.allocate_exact_size(vec2(56.0, 6.0), Sense::hover());
                        let painter = ui.painter();
                        painter.rect_filled(rect, 3.0, TRACK);
                        if info.signed() {
                            let mid = rect.center().x;
                            let v = o.genome.signed(i);
                            let x = mid + v * rect.width() * 0.5;
                            let r = Rect::from_min_max(egui::pos2(mid.min(x), rect.top()), egui::pos2(mid.max(x), rect.bottom()));
                            let c = if v >= 0.0 { Color32::from_rgb(110, 200, 130) } else { DANGER };
                            painter.rect_filled(r, 3.0, c);
                        } else {
                            let mut r = rect;
                            r.set_width(rect.width() * value as f32 / 255.0);
                            painter.rect_filled(r, 3.0, ACCENT.gamma_multiply(0.75));
                        }
                    });
                });
                let desc = describe_gene(i, &o.genome, t, &world.config);
                ui.horizontal(|ui| {
                    // Lined up under the gene's name (after its code and the gap).
                    ui.add_space(28.0 + ui.spacing().item_spacing.x);
                    ui.add(egui::Label::new(RichText::new(desc).size(11.5).color(MUTED)).wrap());
                });
            }
            ui.add_space(2.0);
        });
    }
}

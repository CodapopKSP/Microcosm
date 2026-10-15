//! The heads-up display over the world: clock and speed, weather, life, what's happening now
//! (top left), and round panel buttons down the right edge.

use bevy::prelude::{ResMut, Vec2};
use bevy_egui::egui::{self, Color32, Pos2, Rect, Stroke, vec2};
use evo_core::World;

use super::{MapMode, UiState};
use crate::camera::CameraState;
use crate::settings::Settings;
use crate::sim::{SPEEDS, Selection, SimState};

pub(super) const TEXT: Color32 = Color32::from_rgb(222, 238, 236);
pub(super) const MUTED: Color32 = Color32::from_rgb(150, 180, 182);
pub(super) const ACCENT: Color32 = Color32::from_rgb(150, 240, 210);
pub(super) const DIM: Color32 = Color32::from_rgb(70, 88, 92);
pub(super) const WARN: Color32 = Color32::from_rgb(240, 185, 90);
const SUN: Color32 = Color32::from_rgb(255, 214, 110);
const MOON: Color32 = Color32::from_rgb(190, 205, 235);
const WARM: Color32 = Color32::from_rgb(240, 130, 100);
const BUBBLE: Color32 = Color32::from_rgb(130, 200, 245);
pub(super) const CARD_FILL: Color32 = Color32::from_rgb(12, 21, 27);
/// Gap between the HUD and the edges of the view, and between cards.
pub(super) const GAP: f32 = 10.0;
pub(super) const BUTTON_RADIUS: f32 = 19.0;
/// Width of the left column: the clock card and the species list under it.
pub(super) const COLUMN_WIDTH: f32 = 290.0;
/// Width the round buttons take down the right edge.
pub(super) const TOOLBAR_WIDTH: f32 = 2.0 * BUTTON_RADIUS + 2.0 * GAP;

/// The HUD's card style (also used by the event ticker).
pub(super) fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(CARD_FILL)
        .stroke(Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(150, 240, 210, 36),
        ))
        .corner_radius(12)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .shadow(egui::Shadow {
            offset: [0, 2],
            blur: 10,
            spread: 0,
            color: Color32::from_black_alpha(90),
        })
}

/// A card at a fixed spot; returns its screen rect.
fn show_card(
    ctx: &egui::Context,
    id: &str,
    pos: Pos2,
    pivot: egui::Align2,
    add: impl FnOnce(&mut egui::Ui),
) -> Rect {
    egui::Area::new(egui::Id::new(id))
        .fixed_pos(pos)
        .pivot(pivot)
        .show(ctx, |ui| {
            card().show(ui, |ui| {
                ui.style_mut().visuals.override_text_color = Some(TEXT);
                add(ui);
            });
        })
        .response
        .rect
}

/// "12,345".
fn grouped(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

// ---- Icons (painted, so they don't depend on font glyphs) ----

fn icon_sun(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    p.circle_filled(c, s * 0.42, color);
    for k in 0..8 {
        let a = k as f32 * std::f32::consts::TAU / 8.0;
        let d = vec2(a.cos(), a.sin());
        p.line_segment(
            [c + d * s * 0.6, c + d * s * 0.9],
            Stroke::new((s * 0.12).max(1.2), color),
        );
    }
}

fn icon_moon(p: &egui::Painter, c: Pos2, s: f32, color: Color32, bg: Color32) {
    p.circle_filled(c, s * 0.62, color);
    p.circle_filled(c + vec2(s * 0.32, -s * 0.22), s * 0.52, bg);
}

fn icon_thermometer(p: &egui::Painter, c: Pos2, s: f32, color: Color32, fill: f32) {
    let stem = Rect::from_center_size(c + vec2(0.0, -s * 0.2), vec2(s * 0.36, s * 1.3));
    p.rect_stroke(
        stem,
        s * 0.18,
        Stroke::new(1.2, MUTED),
        egui::StrokeKind::Middle,
    );
    let bulb = c + vec2(0.0, s * 0.55);
    p.circle_filled(bulb, s * 0.3, color);
    let top = stem.bottom() - (stem.height() - s * 0.2) * fill.clamp(0.0, 1.0);
    p.line_segment(
        [Pos2::new(c.x, bulb.y), Pos2::new(c.x, top)],
        Stroke::new(s * 0.18, color),
    );
}

fn icon_bubbles(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let stroke = Stroke::new(1.4, color);
    p.circle_stroke(c + vec2(-s * 0.25, s * 0.3), s * 0.42, stroke);
    p.circle_stroke(c + vec2(s * 0.42, -s * 0.2), s * 0.28, stroke);
    p.circle_stroke(c + vec2(-s * 0.05, -s * 0.62), s * 0.18, stroke);
}

/// A few round cells (organisms, the species list).
pub(super) fn icon_cells(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    for (d, r) in [
        (vec2(-0.38, 0.28), 0.36),
        (vec2(0.4, 0.3), 0.3),
        (vec2(0.02, -0.4), 0.33),
    ] {
        let center = c + d * s;
        p.circle_filled(center, r * s, color.gamma_multiply(0.35));
        p.circle_stroke(center, r * s, Stroke::new(1.3, color));
        p.circle_filled(center, r * s * 0.3, color);
    }
}

/// A branching tree (species).
fn icon_branches(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let stroke = Stroke::new(1.6, color);
    let root = c + vec2(0.0, s * 0.85);
    let fork = c + vec2(0.0, s * 0.15);
    p.line_segment([root, fork], stroke);
    for (end, mid) in [
        (vec2(-0.7, -0.75), vec2(-0.35, -0.2)),
        (vec2(0.7, -0.75), vec2(0.35, -0.2)),
        (vec2(0.0, -0.85), vec2(0.0, -0.3)),
    ] {
        let m = c + mid * s;
        let e = c + end * s;
        p.line_segment([fork, m], stroke);
        p.line_segment([m, e], stroke);
        p.circle_filled(e, s * 0.17, color);
    }
}

pub(super) fn icon_chart(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let base = c.y + s * 0.75;
    for (i, h) in [0.55f32, 1.0, 0.75, 1.4].iter().enumerate() {
        let x = c.x - s * 0.75 + i as f32 * s * 0.5;
        let r = Rect::from_min_max(Pos2::new(x, base - h * s), Pos2::new(x + s * 0.32, base));
        p.rect_filled(r, 1.5, color);
    }
    p.line_segment(
        [
            Pos2::new(c.x - s * 0.95, base + 1.5),
            Pos2::new(c.x + s * 0.95, base + 1.5),
        ],
        Stroke::new(1.2, color.gamma_multiply(0.7)),
    );
}

/// A closed notebook with an elastic band (the field notebook).
pub(super) fn icon_book(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let cover = Rect::from_center_size(c, vec2(s * 1.5, s * 1.9));
    let stroke = Stroke::new(1.5, color);
    p.rect_filled(cover, 2.5, color.gamma_multiply(0.25));
    p.rect_stroke(cover, 2.5, stroke, egui::StrokeKind::Middle);
    // Spine.
    let spine = Rect::from_min_max(
        cover.min,
        Pos2::new(cover.left() + s * 0.35, cover.bottom()),
    );
    p.rect_filled(spine, 2.0, color);
    // Elastic band and a pencil line on the cover.
    let x = cover.right() - s * 0.35;
    p.line_segment(
        [Pos2::new(x, cover.top()), Pos2::new(x, cover.bottom())],
        Stroke::new(1.2, color),
    );
    p.line_segment(
        [
            cover.center() + vec2(-s * 0.15, -s * 0.3),
            cover.center() + vec2(s * 0.25, -s * 0.3),
        ],
        Stroke::new(1.2, color),
    );
}

/// Lines with bullets (the event ticker).
fn icon_feed(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    for i in 0..3 {
        let y = c.y + (i as f32 - 1.0) * s * 0.6;
        p.circle_filled(Pos2::new(c.x - s * 0.7, y), s * 0.13, color);
        let len = [1.4, 1.0, 1.2][i];
        p.line_segment(
            [
                Pos2::new(c.x - s * 0.4, y),
                Pos2::new(c.x - s * 0.4 + len * s, y),
            ],
            Stroke::new(1.6, color),
        );
    }
}

/// A water drop (the chemistry map).
fn icon_drop(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let center = c + vec2(0.0, s * 0.25);
    let r = s * 0.6;
    let mut pts = vec![c + vec2(0.0, -s * 0.95)];
    // The round bottom, from where the sides meet it tangentially.
    let n = 16;
    let start = -0.25f32;
    for k in 0..=n {
        let a = std::f32::consts::PI * (start + (1.0 - 2.0 * start) * k as f32 / n as f32);
        pts.push(center + vec2(a.cos(), a.sin()) * r);
    }
    p.add(egui::Shape::convex_polygon(
        pts,
        color.gamma_multiply(0.35),
        Stroke::new(1.4, color),
    ));
}

fn icon_camera(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let body = Rect::from_center_size(c + vec2(0.0, s * 0.1), vec2(s * 1.8, s * 1.2));
    let stroke = Stroke::new(1.5, color);
    p.rect_stroke(body, 3.0, stroke, egui::StrokeKind::Middle);
    let bump = Rect::from_min_size(body.min + vec2(s * 0.3, -s * 0.3), vec2(s * 0.6, s * 0.3));
    p.rect_filled(bump, 1.5, color);
    p.circle_stroke(body.center(), s * 0.36, stroke);
    p.circle_filled(body.center(), s * 0.12, color);
}

fn icon_menu(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    for i in 0..3 {
        let y = c.y + (i as f32 - 1.0) * s * 0.55;
        p.line_segment(
            [Pos2::new(c.x - s * 0.75, y), Pos2::new(c.x + s * 0.75, y)],
            Stroke::new(1.8, color),
        );
    }
}

pub(super) fn icon_text(p: &egui::Painter, c: Pos2, s: f32, color: Color32, text: &str) {
    p.text(
        c,
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(s * 1.7),
        color,
    );
}

fn icon_pause(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    for dx in [-0.3, 0.3] {
        let r = Rect::from_center_size(c + vec2(dx * s, 0.0), vec2(s * 0.32, s * 1.2));
        p.rect_filled(r, 1.5, color);
    }
}

fn icon_play(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    p.add(egui::Shape::convex_polygon(
        vec![
            c + vec2(-s * 0.45, -s * 0.65),
            c + vec2(s * 0.65, 0.0),
            c + vec2(-s * 0.45, s * 0.65),
        ],
        color,
        Stroke::NONE,
    ));
}

/// A round button with a painted icon. `on` = its panel is open.
/// A cross (close).
pub(super) fn icon_close(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let stroke = Stroke::new(1.8, color);
    let d = s * 0.6;
    p.line_segment([c + vec2(-d, -d), c + vec2(d, d)], stroke);
    p.line_segment([c + vec2(d, -d), c + vec2(-d, d)], stroke);
}

/// A tab bar: one rounded track across the card with the open tab lit (as in the organism
/// card and the charts).
pub(super) fn tab_bar<T: PartialEq + Copy>(ui: &mut egui::Ui, current: &mut T, tabs: &[(T, &str)]) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 28.0), egui::Sense::hover());
    ui.painter().rect_filled(
        rect,
        14.0,
        Color32::from_rgba_unmultiplied(255, 255, 255, 10),
    );
    let seg = rect.width() / tabs.len().max(1) as f32;
    for (i, (t, name)) in tabs.iter().enumerate() {
        let r = Rect::from_min_size(
            Pos2::new(rect.left() + seg * i as f32, rect.top()),
            vec2(seg, rect.height()),
        )
        .shrink(2.0);
        let response = ui.interact(r, ui.id().with(("tab", i)), egui::Sense::click());
        let on = *current == *t;
        let p = ui.painter();
        if on {
            p.rect(
                r,
                12.0,
                Color32::from_rgb(38, 86, 77),
                Stroke::new(1.0, ACCENT.gamma_multiply(0.6)),
                egui::StrokeKind::Inside,
            );
        } else if response.hovered() {
            p.rect_filled(r, 12.0, Color32::from_rgba_unmultiplied(255, 255, 255, 14));
        }
        p.text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            *name,
            egui::FontId::proportional(12.5),
            if on { ACCENT } else { TEXT },
        );
        if response.clicked() {
            *current = *t;
        }
    }
}

pub(super) fn round_button(
    ui: &mut egui::Ui,
    on: bool,
    radius: f32,
    icon: impl FnOnce(&egui::Painter, Pos2, f32, Color32),
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(radius * 2.0, radius * 2.0), egui::Sense::click());
    crate::ui_sfx::hover_sound(&response);
    let p = ui.painter();
    let c = rect.center();
    let hovered = response.hovered();
    let fill = match (on, hovered) {
        (true, _) => Color32::from_rgb(38, 86, 77),
        (false, true) => Color32::from_rgb(32, 49, 56),
        (false, false) => CARD_FILL,
    };
    p.circle_filled(c + vec2(0.0, 2.0), radius, Color32::from_black_alpha(70));
    p.circle_filled(c, radius, fill);
    let ring = if on {
        ACCENT
    } else if hovered {
        Color32::from_rgba_unmultiplied(150, 240, 210, 120)
    } else {
        Color32::from_rgba_unmultiplied(150, 240, 210, 45)
    };
    p.circle_stroke(
        c,
        radius - 0.5,
        Stroke::new(if on { 1.6 } else { 1.0 }, ring),
    );
    let color = if on || hovered { ACCENT } else { TEXT };
    icon(p, c, radius * 0.5, color);
    response
}

// ---- Cards ----

fn speed_label(speed: f64) -> String {
    if speed == 0.0 {
        "Paused".into()
    } else if speed < 1.0 {
        "½×".into()
    } else {
        format!("{speed:.0}×")
    }
}

/// Date and time, with the speed controls under it.
fn clock_card(ui: &mut egui::Ui, sim: &mut SimState) {
    let world = &sim.world;
    let c = &world.env.clock;
    let (h, m) = c.time_of_day();
    // Moon from 6 pm to 6 am.
    let day = (6..18).contains(&h);
    // As wide as the species list under it (less the card's margins).
    ui.set_min_width(COLUMN_WIDTH - 24.0);
    ui.horizontal(|ui| {
        let (rect, r) = ui.allocate_exact_size(vec2(34.0, 34.0), egui::Sense::hover());
        if day {
            icon_sun(ui.painter(), rect.center(), 16.0, SUN);
        } else {
            icon_moon(ui.painter(), rect.center(), 16.0, MOON, CARD_FILL);
        }
        r.on_hover_text(if day { "Day" } else { "Night" });
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(
                egui::RichText::new(format!("{h:02}:{m:02}"))
                    .size(22.0)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(evo_core::traits::format_game_date(
                    c.time,
                    &world.config.time,
                ))
                .size(12.5)
                .color(MUTED),
            );
        });
    });
    ui.add_space(4.0);
    speed_controls(ui, sim);
}

/// Pause button, a row of arrows (one per speed; click to pick), and the current speed.
fn speed_controls(ui: &mut egui::Ui, sim: &mut SimState) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let paused = sim.paused();
        let r = round_button(ui, paused, 13.0, |p, c, s, color| {
            if paused {
                icon_play(p, c, s, color)
            } else {
                icon_pause(p, c, s, color)
            }
        })
        .on_hover_text(if paused {
            "Resume (Space)"
        } else {
            "Pause (Space)"
        });
        if r.clicked() {
            sim.toggle_pause();
        }
        ui.add_space(6.0);
        let current = sim.speed_index;
        let mut hovered_index = None;
        for (i, speed) in SPEEDS.iter().enumerate().skip(1) {
            let (rect, response) = ui.allocate_exact_size(vec2(14.0, 26.0), egui::Sense::click());
            crate::ui_sfx::hover_sound(&response);
            if response.hovered() {
                hovered_index = Some(i);
            }
            let lit = !paused && i <= current;
            let color = if lit {
                // Teal for slow, warming toward gold for the fastest.
                let t = (i - 1) as f32 / (SPEEDS.len() - 2) as f32;
                lerp_color(ACCENT, WARN, t)
            } else {
                DIM
            };
            let color = if hovered_index.is_some_and(|h| i <= h) && !lit {
                color.gamma_multiply(1.8)
            } else {
                color
            };
            let c = rect.center();
            let w = 7.0;
            let hgt = 8.0;
            ui.painter().add(egui::Shape::convex_polygon(
                vec![
                    c + vec2(-w * 0.6, -hgt),
                    c + vec2(w * 0.8, 0.0),
                    c + vec2(-w * 0.6, hgt),
                ],
                color,
                Stroke::NONE,
            ));
            if response.clicked() {
                sim.speed_index = i;
            }
            response.on_hover_text(format!("{} speed", speed_label(*speed)));
        }
        ui.add_space(8.0);
        // The speed it actually runs at, when the CPU can't keep up with the one picked.
        let lagging = !paused && sim.actual_speed < sim.speed() * 0.9 && sim.tps > 0.0;
        let text = if lagging {
            if sim.actual_speed < 10.0 {
                format!("{:.1}×", sim.actual_speed)
            } else {
                format!("{:.0}×", sim.actual_speed)
            }
        } else {
            speed_label(sim.speed())
        };
        let (rect, response) = ui.allocate_exact_size(vec2(46.0, 26.0), egui::Sense::hover());
        ui.painter().text(
            rect.left_center(),
            egui::Align2::LEFT_CENTER,
            text,
            egui::FontId::proportional(if paused { 15.0 } else { 18.0 }),
            if paused || lagging { WARN } else { TEXT },
        );
        if lagging {
            response.on_hover_text(format!(
                "Set to {}, but this computer can only keep up with about {:.1}×.",
                speed_label(sim.speed()),
                sim.actual_speed
            ));
        }
    });
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// One reading: an icon, a value, and a caption under it.
fn stat(
    ui: &mut egui::Ui,
    value: &str,
    caption: &str,
    tip: &str,
    icon: impl FnOnce(&egui::Painter, Pos2, f32),
) {
    ui.horizontal_top(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(26.0, 34.0), egui::Sense::hover());
        icon(ui.painter(), rect.center(), 11.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(egui::RichText::new(value).size(16.0).strong());
            ui.label(egui::RichText::new(caption).size(11.5).color(MUTED));
        });
    })
    .response
    .on_hover_text(tip);
}

fn weather_card(ui: &mut egui::Ui, world: &World) {
    let c = &world.env.clock;
    let air = &world.env.atmosphere;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        let sun = c.sun / world.config.light.surface_intensity;
        stat(
            ui,
            &format!("{:.0}%", sun * 100.0),
            "Sunlight",
            "Sunlight at the surface, relative to a normal noon.",
            |p, center, s| {
                if c.sun > 0.0 {
                    icon_sun(p, center, s, SUN.gamma_multiply((0.4 + sun).min(1.0)));
                } else {
                    icon_moon(p, center, s, MOON, CARD_FILL);
                }
            },
        );
        let warmth = ((c.surface_temp - 0.0) / 35.0).clamp(0.05, 1.0);
        stat(
            ui,
            &format!("{:.1} °C", c.surface_temp),
            "Surface",
            "Water temperature at the surface. It changes with the seasons and the climate.",
            |p, center, s| icon_thermometer(p, center, s, WARM, warmth),
        );
        stat(
            ui,
            &format!("{:.0}% · {:.0}%", air.oxygen * 100.0, air.co2 * 100.0),
            "Air O₂ · CO₂",
            "Oxygen and CO₂ in the air, relative to today's Earth. Photosynthesis releases \
             oxygen and uses CO₂; breathing and rotting do the opposite. More CO₂ warms the \
             climate.",
            |p, center, s| icon_bubbles(p, center, s, BUBBLE),
        );
    });
}

fn life_card(ui: &mut egui::Ui, world: &World) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        stat(
            ui,
            &grouped(world.organisms.len()),
            "Organisms",
            "Living organisms in the world.",
            |p, c, s| icon_cells(p, c, s, ACCENT),
        );
        stat(
            ui,
            &grouped(world.species.living().count()),
            "Species",
            "Named species alive now.",
            |p, c, s| icon_branches(p, c, s, ACCENT),
        );
    });
}

/// Events going on now, as clickable chips. Hovering a chip widens it to show an X inside that
/// ends the event (`call_off` gets its index).
fn now_card(
    ui: &mut egui::Ui,
    world: &World,
    goto: &mut Option<Vec2>,
    call_off: &mut Option<usize>,
) {
    use evo_core::events::WorldEvent;
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Now").size(12.5).color(MUTED));
        let h = world.config.world.height;
        for (index, e) in world.happenings.active.iter().enumerate() {
            let (text, color, target) = match e {
                WorldEvent::Plague { species, .. } => (
                    format!("Plague: {}", world.species.name(*species)),
                    Color32::from_rgb(255, 120, 60),
                    world
                        .organisms
                        .iter()
                        .find(|o| world.species.established_ancestor(o.species) == Some(*species))
                        .map(|o| Vec2::new(o.pos.x, o.pos.y)),
                ),
                WorldEvent::ImpactWinter { .. } => (
                    format!(
                        "Impact winter ({:.0}% sun)",
                        world.happenings.sun_factor(world.time()) * 100.0
                    ),
                    Color32::from_rgb(255, 120, 60),
                    None,
                ),
                WorldEvent::Upwelling { x, .. } => (
                    format!("Upwelling at x={x:.0}"),
                    Color32::from_rgb(120, 190, 240),
                    Some(Vec2::new(*x, h - 500.0)),
                ),
                WorldEvent::Barrier { x, built, .. } => (
                    format!("Rock wall at x={x:.0}"),
                    Color32::from_rgb(200, 170, 130),
                    Some(Vec2::new(*x, built.min(h) - 300.0)),
                ),
            };
            if let Some(action) = event_chip(ui, index, &text, color, target.is_some()) {
                match action {
                    ChipAction::Look => *goto = target,
                    ChipAction::End => *call_off = Some(index),
                }
            }
        }
    });
}

enum ChipAction {
    Look,
    End,
}

/// One event in the Now card: a rounded chip with its text. While hovered it widens to show an
/// X at its right end that ends the event; a click anywhere else looks at the event (if
/// `lookable`).
fn event_chip(
    ui: &mut egui::Ui,
    index: usize,
    text: &str,
    color: Color32,
    lookable: bool,
) -> Option<ChipAction> {
    const X_SPACE: f32 = 18.0;
    let id = ui.id().with(("now_chip", index));
    // Widened if it was hovered last frame (the size has to be known before it's laid out).
    let open = ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        egui::FontId::proportional(12.5),
        color,
    );
    let pad = ui.spacing().button_padding;
    let size = vec2(
        galley.size().x + 2.0 * pad.x + if open { X_SPACE } else { 0.0 },
        galley.size().y + 2.0 * pad.y,
    );
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    ui.ctx().data_mut(|d| d.insert_temp(id, response.hovered()));
    let x_rect = egui::Rect::from_min_max(
        egui::pos2(rect.right() - X_SPACE - 2.0, rect.top()),
        rect.right_bottom(),
    );
    let on_x = open && response.hover_pos().is_some_and(|p| x_rect.contains(p));

    let painter = ui.painter();
    let fill = if response.hovered() && !on_x && lookable { 0.25 } else { 0.15 };
    painter.rect(
        rect,
        10.0,
        color.gamma_multiply(fill),
        Stroke::new(1.0, color.gamma_multiply(0.6)),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.left_top() + pad, galley, color);
    if open {
        let c = x_rect.center() - vec2(1.0, 0.0);
        let r = 7.0;
        if on_x {
            painter.circle_filled(c, r, color.gamma_multiply(0.35));
        }
        let x_color = if on_x { Color32::WHITE } else { color };
        let d = r * 0.45;
        let stroke = Stroke::new(1.5, x_color);
        painter.line_segment([c + vec2(-d, -d), c + vec2(d, d)], stroke);
        painter.line_segment([c + vec2(-d, d), c + vec2(d, -d)], stroke);
    }

    crate::ui_sfx::hover_sound(&response);
    let response = if on_x {
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("End this event")
    } else if lookable {
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Click to look")
    } else {
        response
    };
    if !response.clicked() {
        None
    } else if on_x {
        Some(ChipAction::End)
    } else if lookable {
        Some(ChipAction::Look)
    } else {
        None
    }
}

const MAP_MODES: [(MapMode, &str); 6] = [
    (MapMode::None, "Off"),
    (MapMode::Oxygen, "Oxygen"),
    (MapMode::Co2, "CO₂"),
    (MapMode::Nutrients, "Nutrients"),
    (MapMode::Minerals, "Minerals"),
    (MapMode::Toxin, "Toxin"),
];

/// Everything floating over the world view `view` (the screen minus side panels). The cards
/// keep to `content`: the view minus the toolbar column and the inspector, when it's open.
/// Returns the bottom of the cards in the leftmost column (the species list goes below).
/// Cards under the top row start right of `column_right` (the species list's edge).
#[allow(clippy::too_many_arguments)]
pub fn draw(
    ctx: &egui::Context,
    view: Rect,
    content: Rect,
    column_right: f32,
    ui_state: &mut UiState,
    sim: &mut SimState,
    cam: &mut CameraState,
    selection: &mut Selection,
    settings: &mut ResMut<Settings>,
) -> f32 {
    // Top left: clock and speed, weather, life. A card that doesn't fit beside the previous
    // one (narrow window, side panels open) goes on the next row.
    let top_left = content.left_top() + vec2(GAP, GAP);
    let right_edge = content.right();
    let next_pos = |id: &str, prev: Rect, row_bottom: f32| -> Pos2 {
        let width = ctx
            .memory(|m| m.area_rect(egui::Id::new(id)))
            .map_or(0.0, |r| r.width());
        if prev.right() + GAP + width > right_edge {
            Pos2::new(top_left.x, row_bottom + GAP)
        } else {
            Pos2::new(prev.right() + GAP, prev.top())
        }
    };
    let clock = show_card(ctx, "hud_clock", top_left, egui::Align2::LEFT_TOP, |ui| {
        clock_card(ui, sim)
    });
    let at = next_pos("hud_weather", clock, clock.bottom());
    let weather = show_card(ctx, "hud_weather", at, egui::Align2::LEFT_TOP, |ui| {
        weather_card(ui, &sim.world)
    });
    let at = next_pos("hud_life", weather, clock.bottom().max(weather.bottom()));
    let life = show_card(ctx, "hud_life", at, egui::Align2::LEFT_TOP, |ui| {
        life_card(ui, &sim.world)
    });
    // The leftmost column's cards, for the species list to sit under.
    let in_left_column = |r: Rect| (r.left() - top_left.x).abs() < 1.0;
    let mut left_bottom = [clock, weather, life]
        .into_iter()
        .filter(|r| in_left_column(*r))
        .map(|r| r.bottom())
        .fold(clock.bottom(), f32::max);

    // What's happening now, then the map legend, right under the weather.
    let mut below = weather.bottom() + GAP;
    let under_x = weather.left().max(column_right + GAP);
    if !sim.world.happenings.active.is_empty() {
        let mut goto = None;
        let mut call_off = None;
        // In the slot beside the clock card: its bottom lines up with the clock's (when the
        // weather card is beside the clock), and it's slim so it fits.
        let height = ctx
            .memory(|m| m.area_rect(egui::Id::new("hud_now")))
            .map_or(30.0, |r| r.height());
        let top = if (weather.top() - clock.top()).abs() < 1.0 {
            (clock.bottom() - height).max(weather.bottom() + 6.0)
        } else {
            below
        };
        let rect = egui::Area::new(egui::Id::new("hud_now"))
            .fixed_pos(Pos2::new(under_x, top))
            .show(ctx, |ui| {
                ui.set_max_width((right_edge - under_x).max(200.0));
                card()
                    .inner_margin(egui::Margin::symmetric(10, 4))
                    .show(ui, |ui| now_card(ui, &sim.world, &mut goto, &mut call_off));
            })
            .response
            .rect;
        below = rect.bottom() + GAP;
        if in_left_column(rect) {
            left_bottom = left_bottom.max(rect.bottom());
        }
        if let Some(p) = goto {
            cam.director = false;
            selection.follow = false;
            cam.focus(p, Some(2.0));
        }
        if let Some(index) = call_off {
            sim.world.call_off_event(index);
        }
    }

    if ui_state.map_mode != MapMode::None {
        let rect = show_card(
            ctx,
            "hud_map_legend",
            Pos2::new(under_x, below),
            egui::Align2::LEFT_TOP,
            |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::hover());
                    icon_drop(ui.painter(), rect.center(), 8.0, BUBBLE);
                    ui.label(ui_state.map_mode.label());
                });
            },
        );
        if in_left_column(rect) {
            left_bottom = left_bottom.max(rect.bottom());
        }
    }

    toolbar(ctx, view, ui_state, cam, settings);

    // Bottom right: performance, and the last save.
    egui::Area::new(egui::Id::new("hud_perf"))
        .fixed_pos(content.right_bottom() + vec2(0.0, -6.0))
        .pivot(egui::Align2::RIGHT_BOTTOM)
        .interactable(false)
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if let Some((msg, at)) = &ui_state.toast
                    && at.elapsed().as_secs_f32() < 4.0
                {
                    ui.label(egui::RichText::new(msg).color(ACCENT));
                }
                if let Some((msg, at)) = &sim.last_save_message
                    && at.elapsed().as_secs_f32() < 4.0
                {
                    ui.label(egui::RichText::new(msg).color(ACCENT));
                }
                ui.label(
                    egui::RichText::new(format!(
                        "{:.0} fps · {:.0} ticks/s · sim {:.1} ms",
                        ui_state.fps, sim.tps, sim.sim_ms
                    ))
                    .size(11.0)
                    .color(Color32::from_white_alpha(90)),
                );
            });
        });
    left_bottom
}

/// Round buttons down the right edge of the view.
fn toolbar(
    ctx: &egui::Context,
    view: Rect,
    ui_state: &mut UiState,
    cam: &mut CameraState,
    settings: &mut ResMut<Settings>,
) {
    let mut map_button = None;
    let mut menu_button = None;
    egui::Area::new(egui::Id::new("hud_toolbar"))
        .fixed_pos(view.right_top() + vec2(-GAP, GAP))
        .pivot(egui::Align2::RIGHT_TOP)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.vertical(|ui| {
                let r = BUTTON_RADIUS;
                let response =
                    round_button(ui, ui_state.game_menu, r, icon_menu).on_hover_text("Settings");
                if response.clicked() {
                    ui_state.game_menu = !ui_state.game_menu;
                }
                menu_button = Some(response.rect);
                ui.add_space(10.0);
                if round_button(ui, ui_state.show_notebook, r, icon_book)
                    .on_hover_text("Field Notebook")
                    .clicked()
                {
                    ui_state.show_notebook = !ui_state.show_notebook;
                }
                if round_button(ui, ui_state.show_charts, r, icon_chart)
                    .on_hover_text("Charts")
                    .clicked()
                {
                    ui_state.show_charts = !ui_state.show_charts;
                }
                if round_button(ui, ui_state.show_species, r, icon_cells)
                    .on_hover_text("Species")
                    .clicked()
                {
                    ui_state.show_species = !ui_state.show_species;
                }
                if round_button(ui, ui_state.show_events, r, icon_feed)
                    .on_hover_text("Events")
                    .clicked()
                {
                    ui_state.show_events = !ui_state.show_events;
                }
                let map_on = ui_state.map_mode != MapMode::None || ui_state.map_menu;
                let response =
                    round_button(ui, map_on, r, icon_drop).on_hover_text("Chemistry map");
                if response.clicked() {
                    ui_state.map_menu = !ui_state.map_menu;
                }
                map_button = Some(response.rect);
                if round_button(ui, cam.director, r, icon_camera)
                    .on_hover_text("Auto camera")
                    .clicked()
                {
                    cam.director = !cam.director;
                }
                let response = round_button(ui, ui_state.show_acts, r, |p, c, s, color| {
                    icon_bolt(p, c, s * 0.9, color)
                })
                .on_hover_text("Acts of God");
                if response.clicked() {
                    ui_state.show_acts = !ui_state.show_acts;
                }
                ui_state.acts_button = Some(response.rect);
            });
        });

    // Settings fly out to the left of their button, level with its top.
    if let (true, Some(button)) = (ui_state.game_menu, menu_button) {
        let response = egui::Area::new(egui::Id::new("hud_game_menu"))
            .fixed_pos(button.left_top() + vec2(-GAP, 0.0))
            .pivot(egui::Align2::RIGHT_TOP)
            .show(ctx, |ui| game_menu(ui, ui_state, settings))
            .response;
        // Clicking elsewhere closes it.
        let pointer = ctx.pointer_hover_pos().unwrap_or_default();
        if ctx.input(|i| i.pointer.any_click())
            && !response.rect.contains(pointer)
            && !button.contains(pointer)
        {
            ui_state.game_menu = false;
        }
    }

    // The map's choices fly out to the left of its button.
    if let (true, Some(button)) = (ui_state.map_menu, map_button) {
        let response = egui::Area::new(egui::Id::new("hud_map_menu"))
            .fixed_pos(button.left_center() + vec2(-GAP, 0.0))
            .pivot(egui::Align2::RIGHT_CENTER)
            .show(ctx, |ui| {
                card().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (mode, name) in MAP_MODES {
                            let on = ui_state.map_mode == mode;
                            let text =
                                egui::RichText::new(name).color(if on { ACCENT } else { TEXT });
                            if ui.selectable_label(on, text).clicked() {
                                ui_state.map_mode = mode;
                                ui_state.map_menu = false;
                            }
                        }
                    });
                });
            })
            .response;
        // Clicking elsewhere closes it.
        let pointer = ctx.pointer_hover_pos().unwrap_or_default();
        if ctx.input(|i| i.pointer.any_click())
            && !response.rect.contains(pointer)
            && !button.contains(pointer)
        {
            ui_state.map_menu = false;
        }
    }
}

/// The menu: back to the main menu, help, and the master volume (kept between sessions).
fn game_menu(ui: &mut egui::Ui, ui_state: &mut UiState, settings: &mut ResMut<Settings>) {
    card().show(ui, |ui| {
        ui.set_width(220.0);
        ui.style_mut().visuals.override_text_color = Some(TEXT);
        let w = ui.available_width();
        if menu_item(ui, w, "Main menu", icon_door)
            .on_hover_text("Save, and go back to the main menu")
            .clicked()
        {
            ui_state.game_menu = false;
            ui_state.to_menu = true;
        }
        ui.add_space(2.0);
        if menu_item(ui, w, "Help", |p, c, s, col| icon_text(p, c, s, col, "?"))
            .on_hover_text("Controls and keys (H)")
            .clicked()
        {
            ui_state.game_menu = false;
            ui_state.show_help = true;
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            // The speaker mutes and unmutes.
            let (rect, speaker) = ui.allocate_exact_size(vec2(24.0, 22.0), egui::Sense::click());
            crate::ui_sfx::hover_sound(&speaker);
            let color = if speaker.hovered() { ACCENT } else { MUTED };
            icon_speaker(ui.painter(), rect.center(), 8.0, color, settings.level());
            let speaker = speaker.on_hover_text(if settings.muted { "Unmute" } else { "Mute" });
            if speaker.clicked() {
                settings.muted = !settings.muted;
                settings.save();
            }
            ui.label(egui::RichText::new("Volume").size(14.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let text = if settings.muted {
                    "Muted".to_string()
                } else {
                    format!("{:.0}%", settings.volume * 100.0)
                };
                ui.label(egui::RichText::new(text).size(13.0).color(MUTED));
            });
        });
        let mut percent = settings.volume * 100.0;
        ui.spacing_mut().slider_width = w;
        // In the game's colours: a dark rail filled teal up to the handle.
        let v = &mut ui.style_mut().visuals;
        // (Greyed while muted.)
        let fill = if settings.muted {
            MUTED.gamma_multiply(0.45)
        } else {
            ACCENT
        };
        v.selection.bg_fill = fill.gamma_multiply(0.75);
        v.widgets.inactive.bg_fill = Color32::from_rgb(30, 44, 48);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, fill);
        v.widgets.hovered.bg_fill = ACCENT;
        v.widgets.active.bg_fill = ACCENT;
        let slider = ui.add(
            egui::Slider::new(&mut percent, 0.0..=100.0)
                .show_value(false)
                .trailing_fill(true),
        );
        if slider.changed() {
            settings.volume = (percent / 100.0).clamp(0.0, 1.0);
            // Turning it up (or down) means wanting to hear it.
            settings.muted = false;
        }
        // Saved once it's let go of (not on every step of a drag).
        if slider.drag_stopped() || (slider.changed() && !slider.dragged()) {
            settings.save();
        }
    });
}

/// One of the menu's choices: an icon and a name on a dark rounded tile that lights up under the
/// pointer, like the game's other buttons.
fn menu_item(
    ui: &mut egui::Ui,
    width: f32,
    text: &str,
    icon: impl FnOnce(&egui::Painter, Pos2, f32, Color32),
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 36.0), egui::Sense::click());
    crate::ui_sfx::hover_sound(&response);
    let hovered = response.hovered();
    let p = ui.painter();
    p.rect(
        rect,
        10.0,
        if hovered {
            Color32::from_rgb(32, 49, 56)
        } else {
            Color32::from_rgb(22, 32, 36)
        },
        Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(150, 240, 210, if hovered { 120 } else { 40 }),
        ),
        egui::StrokeKind::Inside,
    );
    let color = if hovered { ACCENT } else { TEXT };
    icon(p, rect.left_center() + vec2(20.0, 0.0), 7.0, color);
    p.text(
        rect.left_center() + vec2(38.0, 0.0),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(15.0),
        color,
    );
    response
}

/// A door with an arrow leaving through it (back to the main menu).
fn icon_door(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let stroke = Stroke::new(1.5, color);
    let door = Rect::from_center_size(c + vec2(-s * 0.35, 0.0), vec2(s * 1.1, s * 2.0));
    p.line_segment([door.right_top(), door.left_top()], stroke);
    p.line_segment([door.left_top(), door.left_bottom()], stroke);
    p.line_segment([door.left_bottom(), door.right_bottom()], stroke);
    let tip = c + vec2(s * 1.1, 0.0);
    p.line_segment([c + vec2(-s * 0.2, 0.0), tip], stroke);
    p.line_segment([tip, tip + vec2(-s * 0.45, -s * 0.45)], stroke);
    p.line_segment([tip, tip + vec2(-s * 0.45, s * 0.45)], stroke);
}

/// A speaker with as many sound waves as the volume is loud.
fn icon_speaker(p: &egui::Painter, c: Pos2, s: f32, color: Color32, volume: f32) {
    let body = [
        c + vec2(-s, -s * 0.35),
        c + vec2(-s * 0.45, -s * 0.35),
        c + vec2(0.0, -s * 0.85),
        c + vec2(0.0, s * 0.85),
        c + vec2(-s * 0.45, s * 0.35),
        c + vec2(-s, s * 0.35),
    ];
    p.add(egui::Shape::convex_polygon(
        body.to_vec(),
        color,
        Stroke::NONE,
    ));
    let waves = if volume <= 0.0 {
        0
    } else {
        1 + (volume * 2.99) as usize
    };
    for k in 0..waves {
        let r = s * (0.45 + 0.35 * k as f32);
        let points: Vec<Pos2> = (0..=8)
            .map(|i| {
                let a = -0.8 + 1.6 * i as f32 / 8.0;
                c + vec2(s * 0.15 + a.cos() * r, a.sin() * r)
            })
            .collect();
        p.add(egui::Shape::line(points, Stroke::new(1.4, color)));
    }
    if waves == 0 {
        let x = c + vec2(s * 0.6, 0.0);
        let d = s * 0.35;
        p.line_segment([x + vec2(-d, -d), x + vec2(d, d)], Stroke::new(1.4, color));
        p.line_segment([x + vec2(-d, d), x + vec2(d, -d)], Stroke::new(1.4, color));
    }
}

/// A lightning bolt.
pub(super) fn icon_bolt(p: &egui::Painter, c: egui::Pos2, s: f32, color: egui::Color32) {
    let pt = |x: f32, y: f32| c + egui::vec2(x, y) * s;
    // Two overlapping wedges: the top one points up and right, the bottom one down and left.
    for tri in [
        [pt(0.3, -1.0), pt(-0.5, 0.15), pt(0.1, 0.15)],
        [pt(-0.3, 1.0), pt(0.5, -0.15), pt(-0.1, -0.15)],
    ] {
        p.add(egui::Shape::convex_polygon(tri.to_vec(), color, egui::Stroke::NONE));
    }
}

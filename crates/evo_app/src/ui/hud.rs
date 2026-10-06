//! The heads-up display over the world: clock and speed, weather, life, what's happening now
//! (top left), and round panel buttons down the right edge.

use bevy::prelude::Vec2;
use bevy_egui::egui::{self, Color32, Pos2, Rect, Stroke, vec2};
use evo_core::World;

use super::{MapMode, UiState};
use crate::camera::CameraState;
use crate::sim::{SPEEDS, Selection, SimState};

const TEXT: Color32 = Color32::from_rgb(222, 238, 236);
const MUTED: Color32 = Color32::from_rgb(150, 180, 182);
const ACCENT: Color32 = Color32::from_rgb(150, 240, 210);
const DIM: Color32 = Color32::from_rgb(70, 88, 92);
const WARN: Color32 = Color32::from_rgb(240, 185, 90);
const SUN: Color32 = Color32::from_rgb(255, 214, 110);
const MOON: Color32 = Color32::from_rgb(190, 205, 235);
const WARM: Color32 = Color32::from_rgb(240, 130, 100);
const BUBBLE: Color32 = Color32::from_rgb(130, 200, 245);
const CARD_FILL: Color32 = Color32::from_rgba_premultiplied(10, 18, 24, 220);
/// Gap between the HUD and the edges of the view, and between cards.
const GAP: f32 = 10.0;
const BUTTON_RADIUS: f32 = 19.0;

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
fn icon_cells(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
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

fn icon_chart(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
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

fn icon_clock(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let stroke = Stroke::new(1.6, color);
    p.circle_stroke(c, s * 0.85, stroke);
    p.line_segment([c, c + vec2(0.0, -s * 0.55)], stroke);
    p.line_segment([c, c + vec2(s * 0.4, s * 0.15)], stroke);
    p.circle_filled(c, 1.6, color);
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

fn icon_text(p: &egui::Painter, c: Pos2, s: f32, color: Color32, text: &str) {
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
fn round_button(
    ui: &mut egui::Ui,
    on: bool,
    radius: f32,
    icon: impl FnOnce(&egui::Painter, Pos2, f32, Color32),
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(radius * 2.0, radius * 2.0), egui::Sense::click());
    let p = ui.painter();
    let c = rect.center();
    let hovered = response.hovered();
    let fill = match (on, hovered) {
        (true, _) => Color32::from_rgba_unmultiplied(40, 92, 82, 235),
        (false, true) => Color32::from_rgba_unmultiplied(34, 52, 60, 235),
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
    let day = c.sun > 0.0;
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
                egui::RichText::new(format!(
                    "Year {} · {} · Day {}",
                    c.year + 1,
                    c.season_name(),
                    c.day + 1
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

/// Events going on now, as clickable chips.
fn now_card(ui: &mut egui::Ui, world: &World, goto: &mut Option<Vec2>) {
    use evo_core::events::WorldEvent;
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Now").size(12.5).color(MUTED));
        let h = world.config.world.height;
        for e in &world.happenings.active {
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
            let chip = egui::Button::new(egui::RichText::new(text).color(color))
                .fill(color.gamma_multiply(0.15))
                .stroke(Stroke::new(1.0, color.gamma_multiply(0.6)))
                .corner_radius(10);
            let response = ui.add(chip);
            let response = if target.is_some() {
                response.on_hover_text("Click to look")
            } else {
                response
            };
            if response.clicked() && target.is_some() {
                *goto = target;
            }
        }
    });
}

const MAP_MODES: [(MapMode, &str); 6] = [
    (MapMode::None, "Off"),
    (MapMode::Oxygen, "Oxygen"),
    (MapMode::Co2, "CO₂"),
    (MapMode::Nutrients, "Nutrients"),
    (MapMode::Minerals, "Minerals"),
    (MapMode::Toxin, "Toxin"),
];

/// Everything floating over the world view `view` (the screen minus side panels).
pub fn draw(
    ctx: &egui::Context,
    view: Rect,
    ui_state: &mut UiState,
    sim: &mut SimState,
    cam: &mut CameraState,
    selection: &mut Selection,
) {
    // Top left: clock and speed, weather, life.
    let top_left = view.left_top() + vec2(GAP, GAP);
    let clock = show_card(ctx, "hud_clock", top_left, egui::Align2::LEFT_TOP, |ui| {
        clock_card(ui, sim)
    });
    let weather = show_card(
        ctx,
        "hud_weather",
        Pos2::new(clock.right() + GAP, clock.top()),
        egui::Align2::LEFT_TOP,
        |ui| weather_card(ui, &sim.world),
    );
    let life = show_card(
        ctx,
        "hud_life",
        Pos2::new(weather.right() + GAP, clock.top()),
        egui::Align2::LEFT_TOP,
        |ui| life_card(ui, &sim.world),
    );
    let mut below = clock.bottom().max(weather.bottom()).max(life.bottom()) + GAP;

    if !sim.world.happenings.active.is_empty() {
        let mut goto = None;
        let rect = egui::Area::new(egui::Id::new("hud_now"))
            .fixed_pos(Pos2::new(top_left.x, below))
            .show(ctx, |ui| {
                ui.set_max_width((view.width() - 2.0 * GAP - 60.0).max(200.0));
                card().show(ui, |ui| now_card(ui, &sim.world, &mut goto));
            })
            .response
            .rect;
        below = rect.bottom() + GAP;
        if let Some(p) = goto {
            cam.director = false;
            selection.follow = false;
            cam.focus(p, Some(2.0));
        }
    }

    if ui_state.map_mode != MapMode::None {
        show_card(
            ctx,
            "hud_map_legend",
            Pos2::new(top_left.x, below),
            egui::Align2::LEFT_TOP,
            |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::hover());
                    icon_drop(ui.painter(), rect.center(), 8.0, BUBBLE);
                    ui.label(ui_state.map_mode.label());
                });
            },
        );
    }

    toolbar(ctx, view, ui_state, cam);

    // Bottom right: performance, and the last save.
    egui::Area::new(egui::Id::new("hud_perf"))
        .fixed_pos(view.right_bottom() + vec2(-GAP, -6.0))
        .pivot(egui::Align2::RIGHT_BOTTOM)
        .interactable(false)
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
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
}

/// Round buttons down the right edge of the view.
fn toolbar(ctx: &egui::Context, view: Rect, ui_state: &mut UiState, cam: &mut CameraState) {
    let mut map_button = None;
    egui::Area::new(egui::Id::new("hud_toolbar"))
        .fixed_pos(view.right_top() + vec2(-GAP, GAP))
        .pivot(egui::Align2::RIGHT_TOP)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.vertical(|ui| {
                let r = BUTTON_RADIUS;
                if round_button(ui, ui_state.show_species, r, icon_cells)
                    .on_hover_text("Species list (L)")
                    .clicked()
                {
                    ui_state.show_species = !ui_state.show_species;
                }
                if round_button(ui, ui_state.show_charts, r, icon_chart)
                    .on_hover_text("Charts and family tree (G)")
                    .clicked()
                {
                    ui_state.show_charts = !ui_state.show_charts;
                }
                if round_button(ui, ui_state.show_history, r, icon_clock)
                    .on_hover_text("History (E)")
                    .clicked()
                {
                    ui_state.show_history = !ui_state.show_history;
                }
                if round_button(ui, ui_state.show_events, r, icon_feed)
                    .on_hover_text("Event ticker (T)")
                    .clicked()
                {
                    ui_state.show_events = !ui_state.show_events;
                }
                let map_on = ui_state.map_mode != MapMode::None || ui_state.map_menu;
                let response =
                    round_button(ui, map_on, r, icon_drop).on_hover_text("Chemistry map (M)");
                if response.clicked() {
                    ui_state.map_menu = !ui_state.map_menu;
                }
                map_button = Some(response.rect);
                if round_button(ui, cam.director, r, icon_camera)
                    .on_hover_text("Auto camera: follows the action (C)")
                    .clicked()
                {
                    cam.director = !cam.director;
                }
                if round_button(ui, ui_state.show_help, r, |p, c, s, col| {
                    icon_text(p, c, s, col, "?")
                })
                .on_hover_text("Help (H)")
                .clicked()
                {
                    ui_state.show_help = !ui_state.show_help;
                }
                ui.add_space(10.0);
                if round_button(ui, false, r, icon_menu)
                    .on_hover_text("Save, and go back to the main menu")
                    .clicked()
                {
                    ui_state.to_menu = true;
                }
            });
        });

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

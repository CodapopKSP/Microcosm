//! Acts of God: the toolbar's lightning bolt. A panel of tools that act where the player
//! points: move or kill organisms (one, or every one in a dragged box), strike with a plague,
//! an upwelling, a meteor or a rock wall, and pour substances into the water.
//!
//! While a tool is in hand the cursor turns into a crosshair, the area the act will reach is
//! outlined in the world, and a label beside the cursor says what a click will do. Escape (or
//! clicking the tool again) puts it down.

use bevy::input::ButtonInput;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::input::EguiWantsInput;
use bevy_egui::{EguiContexts, egui};
use evo_core::environment::Substance;
use evo_core::god::SUBSTANCE_RADIUS;

use super::hud::{self, ACCENT, GAP, MUTED, TEXT, WARN};
use crate::camera::{CameraState, MainCamera};
use crate::sim::{Selection, SimState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Move,
    Kill,
    Plague,
    Upwelling,
    Meteor,
    RockWall,
    Pour,
}

impl Act {
    fn label(self) -> &'static str {
        match self {
            Act::Move => "Move",
            Act::Kill => "Kill",
            Act::Plague => "Plague",
            Act::Upwelling => "Upwelling",
            Act::Meteor => "Meteor",
            Act::RockWall => "Rock wall",
            Act::Pour => "Pour",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Act::Move => {
                "Drag an organism to move it. To move several, drag a box around them (hold \
                 Shift to start the box on an organism), then drag any of them. Click empty \
                 water to let go."
            }
            Act::Kill => "Click an organism to kill it, or drag a box to kill everything in it.",
            Act::Plague => "Click an organism: its species falls sick.",
            Act::Upwelling => "Click where minerals and nutrients should well up to the surface.",
            Act::Meteor => "Click where the meteor strikes. Everything in the circle dies.",
            Act::RockWall => "Click where a wall of rock rises from the seafloor.",
            Act::Pour => "Click to pour it into the water there.",
        }
    }
}

/// A drag in progress with the Move or Kill tool.
#[derive(Clone, Copy, Debug)]
enum Drag {
    /// A box from this world point to the cursor.
    Box { start: glam::Vec2 },
    /// Carrying the picked-up organisms; the cursor was last here.
    Carry { last: glam::Vec2 },
}

#[derive(Resource)]
pub struct ActsOfGod {
    pub tool: Option<Act>,
    substance: Substance,
    /// How far a pour spreads (units).
    radius: f32,
    /// Organisms picked up with the Move tool (moved together).
    group: Vec<u64>,
    drag: Option<Drag>,
    /// Where the left button went down (screen), to tell a click from a drag.
    press: Option<Vec2>,
    /// For the egui pass: the cursor to show over the world, and the label beside it.
    cursor: egui::CursorIcon,
    label: Option<String>,
}

impl Default for ActsOfGod {
    fn default() -> Self {
        Self {
            tool: None,
            substance: Substance::Nutrients,
            radius: SUBSTANCE_RADIUS,
            group: Vec::new(),
            drag: None,
            press: None,
            cursor: egui::CursorIcon::Crosshair,
            label: None,
        }
    }
}

impl ActsOfGod {
    /// Put the tool down and let go of everything.
    fn put_down(&mut self) {
        self.tool = None;
        self.group.clear();
        self.drag = None;
        self.press = None;
    }
}

/// Escape puts the tool down.
pub(super) fn acts_keys(keys: Res<ButtonInput<KeyCode>>, mut acts: ResMut<ActsOfGod>) {
    if keys.just_pressed(KeyCode::Escape) && acts.tool.is_some() {
        acts.put_down();
    }
}

fn to_bevy(p: glam::Vec2) -> Vec2 {
    Vec2::new(p.x, p.y)
}

/// What a click or drag in the world does with the tool in hand. Runs before the camera, and
/// takes the clicks (and, for Move and Kill, the drags) it uses so they don't also select or pan.
#[allow(clippy::too_many_arguments)]
pub(crate) fn acts_pointer(
    mut acts: ResMut<ActsOfGod>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    egui_input: Res<EguiWantsInput>,
    mut sim: ResMut<SimState>,
    mut selection: ResMut<Selection>,
    cam: Res<CameraState>,
    ui_state: Res<super::UiState>,
    mut gizmos: Gizmos,
) {
    acts.label = None;
    acts.cursor = egui::CursorIcon::Crosshair;
    if !ui_state.show_acts || !ui_state.show_ui {
        if acts.tool.is_some() {
            acts.put_down();
        }
        return;
    }
    let Some(tool) = acts.tool else {
        return;
    };
    let (Ok(window), Ok((camera, transform))) = (windows.single(), cameras.single()) else {
        return;
    };
    let screen = window.cursor_position();
    let Some(at) = screen
        .and_then(|c| camera.viewport_to_world_2d(transform, c).ok())
        .map(|p| glam::Vec2::new(p.x, p.y))
    else {
        return;
    };
    let free = !egui_input.wants_any_pointer_input();
    let zoom = cam.zoom;
    let world = &mut sim.world;
    let hovered = world
        .pick(at, 6.0 * zoom)
        .and_then(|id| world.find(id))
        .map(|o| (o.id, o.pos, o.radius, o.species));

    // A press in the world. Move and Kill take it (their drags aren't pans); the others leave
    // it to the camera, so the view can still be dragged about.
    let pressed = mouse.just_pressed(MouseButton::Left) && free;
    if pressed {
        acts.press = screen;
        if matches!(tool, Act::Move | Act::Kill) {
            mouse.clear_just_pressed(MouseButton::Left);
        }
    }
    let released = mouse.just_released(MouseButton::Left) && acts.press.is_some();
    let clicked = released
        && matches!((acts.press, screen), (Some(a), Some(b)) if a.distance(b) <= 4.0);
    if released {
        acts.press = None;
        if clicked || matches!(tool, Act::Move | Act::Kill) {
            // Not a selection click for the camera.
            mouse.clear_just_released(MouseButton::Left);
        }
    }

    let ring = |gizmos: &mut Gizmos, p: glam::Vec2, r: f32, color: Color| {
        gizmos.circle_2d(Isometry2d::from_translation(to_bevy(p)), r * 1.3 + 3.0 * zoom, color);
    };
    let rect = |gizmos: &mut Gizmos, a: glam::Vec2, b: glam::Vec2, color: Color| {
        let (min, max) = (a.min(b), a.max(b));
        gizmos.rect_2d(
            Isometry2d::from_translation(to_bevy((min + max) * 0.5)),
            to_bevy(max - min),
            color,
        );
    };
    let cfg = world.config.events.clone();
    let height = world.config.world.height;

    match tool {
        Act::Move => {
            let gold = Color::srgba(1.0, 0.85, 0.4, 0.9);
            // Shift always starts a box, so one can be drawn in a crowd.
            let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
            if pressed {
                acts.drag = Some(match hovered.filter(|_| !shift) {
                    Some((id, ..)) => {
                        if !acts.group.contains(&id) {
                            acts.group = vec![id];
                        }
                        selection.follow = false;
                        Drag::Carry { last: at }
                    }
                    None => Drag::Box { start: at },
                });
            }
            match acts.drag {
                Some(Drag::Carry { last }) => {
                    let delta = at - last;
                    if delta != glam::Vec2::ZERO {
                        for &id in &acts.group {
                            if let Some(p) = world.find(id).map(|o| o.pos) {
                                world.move_organism(id, p + delta);
                            }
                        }
                    }
                    acts.drag = Some(Drag::Carry { last: at });
                    acts.cursor = egui::CursorIcon::Grabbing;
                    if released {
                        acts.drag = None;
                    }
                }
                Some(Drag::Box { start }) => {
                    rect(&mut gizmos, start, at, gold);
                    if released {
                        acts.drag = None;
                        acts.group = if clicked {
                            Vec::new()
                        } else {
                            world.organisms_in(start, at)
                        };
                    }
                }
                None => {
                    if let Some((_, p, r, _)) = hovered {
                        ring(&mut gizmos, p, r, gold);
                        acts.cursor = egui::CursorIcon::Grab;
                    }
                }
            }
            acts.group.retain(|id| world.find(*id).is_some());
            for &id in acts.group.iter().take(2000) {
                if let Some(o) = world.find(id) {
                    ring(&mut gizmos, o.pos, o.radius, gold);
                }
            }
            if !acts.group.is_empty() && acts.drag.is_none() {
                acts.label = Some(format!("{} picked up", acts.group.len()));
            }
        }
        Act::Kill => {
            let red = Color::srgba(1.0, 0.35, 0.3, 0.9);
            if pressed {
                acts.drag = Some(Drag::Box { start: at });
            }
            if let Some(Drag::Box { start }) = acts.drag {
                // A box once the pointer has moved off the press point (else it's a click).
                let boxed = matches!((acts.press, screen), (Some(a), Some(b)) if a.distance(b) > 4.0)
                    || (released && !clicked);
                if boxed {
                    rect(&mut gizmos, start, at, red);
                    let inside = world.organisms_in(start, at);
                    for &id in inside.iter().take(2000) {
                        if let Some(o) = world.find(id) {
                            ring(&mut gizmos, o.pos, o.radius, red);
                        }
                    }
                    acts.label = Some(format!("Kill {}", inside.len()));
                }
                if released {
                    acts.drag = None;
                    let ids = if clicked {
                        hovered.map(|h| vec![h.0]).unwrap_or_default()
                    } else {
                        world.organisms_in(start, at)
                    };
                    if !ids.is_empty() {
                        world.smite(&ids);
                        if selection.organism.is_some_and(|s| ids.contains(&s)) {
                            selection.organism = None;
                            selection.follow = false;
                        }
                    }
                }
            } else if let Some((_, p, r, _)) = hovered {
                ring(&mut gizmos, p, r, red);
            }
        }
        Act::Plague => {
            let sick = Color::srgba(1.0, 0.55, 0.25, 0.9);
            if let Some((_, p, r, species)) = hovered {
                ring(&mut gizmos, p, r, sick);
                let root = world.species.established_ancestor(species).unwrap_or(species);
                let name = world.species.name(root);
                acts.label = Some(if world.happenings.plague_rate(root) > 0.0 {
                    format!("{name} already has a plague")
                } else {
                    format!("Plague: {name}")
                });
                if clicked {
                    world.act_plague(species);
                }
            } else {
                acts.label = Some("Point at an organism".into());
            }
        }
        Act::Upwelling => {
            let half = cfg.upwelling_width * 0.5;
            let blue = Color::srgba(0.5, 0.8, 1.0, 0.8);
            rect(
                &mut gizmos,
                glam::Vec2::new(at.x - half, height - 600.0),
                glam::Vec2::new(at.x + half, height),
                blue,
            );
            acts.label = Some("Upwelling".into());
            if clicked {
                world.act_upwelling(at.x);
            }
        }
        Act::Meteor => {
            let fire = Color::srgba(1.0, 0.6, 0.25, 0.9);
            gizmos
                .circle_2d(Isometry2d::from_translation(to_bevy(at)), cfg.impact_radius, fire)
                .resolution(128);
            acts.label = Some(format!("Meteor: kills {}", world.organisms_within(at, cfg.impact_radius)));
            if clicked {
                world.act_meteor(at);
                if selection.organism.is_some_and(|id| world.find(id).is_none()) {
                    selection.organism = None;
                    selection.follow = false;
                }
            }
        }
        Act::RockWall => {
            let half = cfg.barrier_width * 0.5;
            let floor = world.terrain.floor_at(at.x);
            let stone = Color::srgba(0.85, 0.72, 0.55, 0.9);
            rect(
                &mut gizmos,
                glam::Vec2::new(at.x - half, floor),
                glam::Vec2::new(at.x + half, height),
                stone,
            );
            acts.label = Some("Rock wall".into());
            if clicked {
                world.act_rock_wall(at.x);
            }
        }
        Act::Pour => {
            let color = Color::srgba(0.6, 1.0, 0.85, 0.8);
            gizmos
                .circle_2d(Isometry2d::from_translation(to_bevy(at)), acts.radius, color)
                .resolution(128);
            acts.label = Some(format!("Pour {}", acts.substance.name()));
            if clicked {
                let (substance, radius) = (acts.substance, acts.radius);
                world.add_substance(substance, at, radius);
            }
        }
    }
}

/// The panel, beside its toolbar button, and the cursor and its label over the world.
pub(super) fn acts_panel(
    mut contexts: EguiContexts,
    mut acts: ResMut<ActsOfGod>,
    mut ui_state: ResMut<super::UiState>,
) -> Result {
    if !ui_state.show_acts || !ui_state.show_ui {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?.clone();
    let acts = &mut *acts;
    let anchor = ui_state
        .acts_button
        .map_or(ctx.content_rect().right_top() + egui::vec2(-80.0, 80.0), |r| {
            r.left_top() + egui::vec2(-GAP, 0.0)
        });
    let mut open = true;
    egui::Area::new(egui::Id::new("acts_of_god"))
        .fixed_pos(anchor)
        .pivot(egui::Align2::RIGHT_TOP)
        .show(&ctx, |ui| {
            hud::card().show(ui, |ui| {
                ui.set_width(PANEL_WIDTH);
                ui.style_mut().visuals.override_text_color = Some(TEXT);
                ui.spacing_mut().item_spacing.y = 6.0;

                // Title and close.
                ui.horizontal(|ui| {
                    let (r, _) =
                        ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
                    hud::icon_bolt(ui.painter(), r.center(), 9.0, WARN);
                    ui.label(egui::RichText::new("Acts of God").size(17.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if hud::round_button(ui, false, 12.0, hud::icon_close).clicked() {
                            open = false;
                        }
                    });
                });

                // Tools, two to a row.
                let pick = |acts: &mut ActsOfGod, t: Act| {
                    let again = acts.tool == Some(t);
                    acts.put_down();
                    if !again {
                        acts.tool = Some(t);
                    }
                };
                section(ui, "Organisms");
                tile_grid(ui, 2, TOOL_HEIGHT, &[Act::Move, Act::Kill], |ui, rect, &t| {
                    let r = tool_tile(ui, rect, t.label(), acts.tool == Some(t), |p, c, s, color| {
                        tool_icon(t, p, c, s, color)
                    });
                    if r.clicked() {
                        pick(acts, t);
                    }
                });
                section(ui, "Disasters");
                let disasters = [Act::Plague, Act::Upwelling, Act::Meteor, Act::RockWall];
                tile_grid(ui, 2, TOOL_HEIGHT, &disasters, |ui, rect, &t| {
                    let r = tool_tile(ui, rect, t.label(), acts.tool == Some(t), |p, c, s, color| {
                        tool_icon(t, p, c, s, color)
                    });
                    if r.clicked() {
                        pick(acts, t);
                    }
                });

                // Substances: picking one is the pour tool.
                section(ui, "Pour into the water");
                tile_grid(ui, 2, SUBSTANCE_HEIGHT, &Substance::ALL, |ui, rect, &sub| {
                    let on = acts.tool == Some(Act::Pour) && acts.substance == sub;
                    let r = tool_tile(ui, rect, sub.name(), on, |p, c, s, _| {
                        p.circle_filled(c, s * 0.55, substance_color(sub));
                    });
                    if r.clicked() {
                        if on {
                            acts.put_down();
                        } else {
                            acts.put_down();
                            acts.substance = sub;
                            acts.tool = Some(Act::Pour);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Radius").size(12.5).color(MUTED));
                    ui.spacing_mut().slider_width = (ui.available_width() - 70.0).max(60.0);
                    ui.add(
                        egui::Slider::new(&mut acts.radius, SUBSTANCE_RADIUS..=800.0)
                            .logarithmic(true)
                            .fixed_decimals(0)
                            .suffix(" u"),
                    )
                    .on_hover_text("How far each pour spreads: the same amount, thinner when wider. Click again to pour more.");
                });

                // What the tool does, and what the last act did.
                ui.add_space(4.0);
                let hint = match acts.tool {
                    Some(Act::Pour) => format!(
                        "Click to pour {} into the water there.",
                        acts.substance.name().to_lowercase()
                    ),
                    Some(t) => t.hint().to_string(),
                    None => "Pick an act, then click in the world. Escape puts it down.".into(),
                };
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(16, 28, 34))
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(egui::RichText::new(hint).size(12.0).color(MUTED));
                    });
            });
        });
    if !open {
        ui_state.show_acts = false;
        acts.put_down();
        return Ok(());
    }

    // Over the world: the tool's cursor, and what a click will do.
    if acts.tool.is_some() && !ctx.is_pointer_over_egui() {
        ctx.set_cursor_icon(acts.cursor);
        if let (Some(label), Some(pointer)) = (&acts.label, ctx.pointer_hover_pos()) {
            egui::Area::new(egui::Id::new("acts_label"))
                .fixed_pos(pointer + egui::vec2(18.0, 14.0))
                .order(egui::Order::Tooltip)
                .interactable(false)
                .show(&ctx, |ui| {
                    egui::Frame::new()
                        .fill(hud::CARD_FILL.gamma_multiply(0.92))
                        .corner_radius(6)
                        .inner_margin(egui::Margin::symmetric(8, 3))
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new(label).size(12.5).color(TEXT))
                                    .extend(),
                            );
                        });
                });
        }
    }
    Ok(())
}

const PANEL_WIDTH: f32 = 286.0;
const TOOL_HEIGHT: f32 = 40.0;
const SUBSTANCE_HEIGHT: f32 = 32.0;
const TILE_GAP: f32 = 6.0;

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(title.to_uppercase()).size(11.0).color(MUTED));
}

/// Lay `items` out in rows of `columns` equal tiles, `height` tall, filling the width.
fn tile_grid<T>(
    ui: &mut egui::Ui,
    columns: usize,
    height: f32,
    items: &[T],
    mut add: impl FnMut(&mut egui::Ui, egui::Rect, &T),
) {
    let width = ui.available_width();
    let w = (width - TILE_GAP * (columns - 1) as f32) / columns as f32;
    for row in items.chunks(columns) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
        for (k, item) in row.iter().enumerate() {
            let x = rect.left() + k as f32 * (w + TILE_GAP);
            let tile = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w, height));
            add(ui, tile, item);
        }
    }
}

/// A tile: an icon at the left and a label, lit when `on`.
fn tool_tile(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    label: &str,
    on: bool,
    icon: impl FnOnce(&egui::Painter, egui::Pos2, f32, egui::Color32),
) -> egui::Response {
    let response = ui.interact(rect, ui.id().with(label), egui::Sense::click());
    crate::ui_sfx::hover_sound(&response);
    let hovered = response.hovered();
    let (fill, stroke) = match (on, hovered) {
        (true, _) => (egui::Color32::from_rgb(38, 86, 77), ACCENT),
        (false, true) => (
            egui::Color32::from_rgb(32, 49, 56),
            egui::Color32::from_rgba_unmultiplied(150, 240, 210, 110),
        ),
        (false, false) => (
            egui::Color32::from_rgb(18, 32, 40),
            egui::Color32::from_rgba_unmultiplied(150, 240, 210, 40),
        ),
    };
    let p = ui.painter();
    p.rect(rect, 9.0, fill, egui::Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
    let color = if on || hovered { ACCENT } else { TEXT };
    let s = (rect.height() * 0.22).min(9.0);
    let icon_center = egui::pos2(rect.left() + 10.0 + s, rect.center().y);
    icon(p, icon_center, s, color);
    p.text(
        egui::pos2(icon_center.x + s + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        color,
    );
    response
}

/// The colour each substance has on the chemistry map (toxin red, nutrients green, ...).
fn substance_color(s: Substance) -> egui::Color32 {
    match s {
        Substance::Nutrients => egui::Color32::from_rgb(110, 220, 90),
        Substance::Minerals => egui::Color32::from_rgb(80, 220, 240),
        Substance::Oxygen => egui::Color32::from_rgb(110, 160, 245),
        Substance::Co2 => egui::Color32::from_rgb(235, 190, 70),
        Substance::Toxin => egui::Color32::from_rgb(235, 80, 70),
        Substance::Detritus => egui::Color32::from_rgb(190, 180, 165),
    }
}

/// A small drawing for each tool, centred on `c`, about `s` in radius.
fn tool_icon(t: Act, p: &egui::Painter, c: egui::Pos2, s: f32, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.6, color);
    let pt = |x: f32, y: f32| c + egui::vec2(x, y) * s;
    let arrow = |from: egui::Pos2, to: egui::Pos2| {
        p.line_segment([from, to], stroke);
        let d = (to - from).normalized() * s * 0.4;
        let n = egui::vec2(-d.y, d.x);
        p.line_segment([to, to - d + n], stroke);
        p.line_segment([to, to - d - n], stroke);
    };
    match t {
        Act::Move => {
            for dir in [egui::vec2(1.0, 0.0), egui::vec2(-1.0, 0.0), egui::vec2(0.0, 1.0), egui::vec2(0.0, -1.0)] {
                arrow(c + dir * s * 0.2, c + dir * s);
            }
        }
        Act::Kill => {
            p.circle_stroke(c, s, stroke);
            p.line_segment([pt(-0.5, -0.5), pt(0.5, 0.5)], stroke);
            p.line_segment([pt(-0.5, 0.5), pt(0.5, -0.5)], stroke);
        }
        Act::Plague => {
            p.circle_stroke(c, s * 0.55, stroke);
            for k in 0..8 {
                let a = k as f32 / 8.0 * std::f32::consts::TAU;
                let dir = egui::vec2(a.cos(), a.sin());
                p.line_segment([c + dir * s * 0.55, c + dir * s * 0.9], stroke);
                p.circle_filled(c + dir * s, s * 0.12, color);
            }
        }
        Act::Upwelling => {
            arrow(pt(-0.45, 0.8), pt(-0.45, -0.8));
            arrow(pt(0.45, 0.8), pt(0.45, -0.8));
        }
        Act::Meteor => {
            p.circle_filled(pt(-0.35, 0.35), s * 0.45, color);
            for (x, y) in [(0.0, -0.4), (0.25, -0.1), (-0.3, -0.65)] {
                p.line_segment([pt(x, y), pt(x + 0.7, y - 0.7)], stroke);
            }
        }
        Act::RockWall => {
            let r = egui::Rect::from_center_size(c, egui::vec2(s * 1.1, s * 2.0));
            p.rect_stroke(r, 1.0, stroke, egui::StrokeKind::Middle);
            for k in 1..4 {
                let y = r.top() + r.height() * k as f32 / 4.0;
                p.line_segment([egui::pos2(r.left(), y), egui::pos2(r.right(), y)], stroke);
            }
        }
        Act::Pour => {
            p.circle_filled(c, s * 0.55, color);
        }
    }
}


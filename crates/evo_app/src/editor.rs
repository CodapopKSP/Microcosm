//! The map editor: paint rock, place vent nodes, set the water level and the map's size, and
//! save the map under a name for the new-world screen (in the player's maps folder; see
//! `paths.rs`).


use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use egui::{Color32, Pos2, Rect, Stroke, Vec2, pos2, vec2};
use evo_core::map::PaintedMap;

use crate::AppState;
use crate::menu::{ACCENT, ERROR, MUTED, TEXT, caption, card, pill, rgba, style_menu};

pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Editor>()
            .add_systems(OnEnter(AppState::Editor), enter)
            .add_systems(
                EguiPrimaryContextPass,
                draw_editor.run_if(in_state(AppState::Editor)),
            );
    }
}

/// Most undo steps kept.
const UNDO_STEPS: usize = 40;
/// Brush sizes (world units), for the slider.
const BRUSH_MIN: f32 = 8.0;
const BRUSH_MAX: f32 = 2400.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Ground,
    Erase,
    Cave,
    VentSpawn,
    PermanentVent,
    StartArea,
}

const TOOLS: [(Tool, &str, &str, &str); 6] = [
    (Tool::Ground, "Ground", "B", "Paint rock"),
    (Tool::Erase, "Remove ground", "E", "Paint open water (also removes vent nodes it touches)"),
    (
        Tool::Cave,
        "Cave",
        "C",
        "Paint cave: open water that's dark (no sunlight) and keeps filling with detritus and nutrients. A home for scavengers that live without oxygen.",
    ),
    (
        Tool::VentSpawn,
        "Vent spawn",
        "V",
        "Click to place a spot where a vent may open for a while. Right-click a node to remove it.",
    ),
    (
        Tool::PermanentVent,
        "Permanent vent",
        "P",
        "Click to place a vent that never dies out. Right-click a node to remove it.",
    ),
    (
        Tool::StartArea,
        "Start area",
        "A",
        "Drag a box where a new world's first organisms can start, then name it. Its name shows on the New world screen. Right-click an area to remove it.",
    ),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Round,
    Square,
}

/// A new map's starting point.
#[derive(Clone, PartialEq)]
enum Template {
    Flat,
    Empty,
}

#[derive(Resource)]
pub struct Editor {
    map: PaintedMap,
    name: String,
    tool: Tool,
    shape: Shape,
    size: f32,
    /// World point at the canvas centre, and world units per screen pixel.
    center: egui::Vec2,
    zoom: f32,
    /// Fit the whole map in view next frame.
    fit: bool,
    texture: Option<egui::TextureHandle>,
    /// The texture needs rebuilding from scratch (new map or size).
    rebuild: bool,
    undo: Vec<PaintedMap>,
    redo: Vec<PaintedMap>,
    /// Where the last painting step was (to fill in between frames), while a stroke is on.
    last: Option<egui::Vec2>,
    /// Map size settings being edited (applied with "Resize").
    width_edit: f32,
    height_edit: f32,
    /// Maps to open (the player's and the built-in ones).
    saved: Vec<String>,
    template: Template,
    new_open: bool,
    /// The save window is open (asking for a name and description).
    save_open: bool,
    /// A start area being dragged out (world corner where the drag began).
    area_from: Option<egui::Vec2>,
    /// A start area just drawn, waiting for its name (min, max), and the name being typed.
    area_pending: Option<([f32; 2], [f32; 2])>,
    area_name: String,
    unsaved: bool,
    message: Option<(String, bool, std::time::Instant)>,
    /// Back to the menu (after this frame).
    leave: bool,
}

impl Default for Editor {
    fn default() -> Self {
        let map = new_flat();
        Editor {
            width_edit: map.width,
            height_edit: map.height,
            map,
            name: "my-map".into(),
            tool: Tool::Ground,
            shape: Shape::Round,
            size: 160.0,
            center: egui::Vec2::ZERO,
            zoom: 1.0,
            fit: true,
            texture: None,
            rebuild: true,
            undo: Vec::new(),
            redo: Vec::new(),
            last: None,
            saved: Vec::new(),
            template: Template::Flat,
            new_open: false,
            save_open: false,
            area_from: None,
            area_pending: None,
            area_name: String::new(),
            unsaved: false,
            message: None,
            leave: false,
        }
    }
}

fn new_flat() -> PaintedMap {
    PaintedMap::flat(12000.0, 4400.0, 4000.0, 700.0)
}

/// A name that's safe as a file name (letters, digits, spaces, - and _).
fn clean_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '-' })
        .collect()
}

fn enter(mut commands: Commands, mut editor: ResMut<Editor>) {
    // The menu's live backdrop isn't needed here.
    commands.remove_resource::<crate::sim::SimState>();
    editor.saved = crate::paths::all_map_names();
    editor.rebuild = true;
    editor.fit = true;
    editor.leave = false;
}

impl Editor {
    fn say(&mut self, text: impl Into<String>, error: bool) {
        self.message = Some((text.into(), error, std::time::Instant::now()));
    }

    /// Remember the map as it is, for undo.
    fn checkpoint(&mut self) {
        self.undo.push(self.map.clone());
        if self.undo.len() > UNDO_STEPS {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.unsaved = true;
    }

    fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            let now = std::mem::replace(&mut self.map, prev);
            self.redo.push(now);
            self.after_swap();
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            let now = std::mem::replace(&mut self.map, next);
            self.undo.push(now);
            self.after_swap();
        }
    }

    /// Resize the map to the size settings, if they differ from it.
    fn apply_size(&mut self) {
        let (w, h) = (self.width_edit, self.height_edit);
        if (w - self.map.width).abs() < self.map.cell && (h - self.map.height).abs() < self.map.cell {
            return;
        }
        self.checkpoint();
        self.map.resize(w, h);
        self.width_edit = self.map.width;
        self.height_edit = self.map.height;
        self.rebuild = true;
        self.fit = true;
    }

    fn after_swap(&mut self) {
        self.rebuild = true;
        self.width_edit = self.map.width;
        self.height_edit = self.map.height;
        self.unsaved = true;
    }

    fn replace(&mut self, map: PaintedMap, name: &str) {
        self.map = map;
        self.name = name.to_string();
        self.undo.clear();
        self.redo.clear();
        self.after_swap();
        self.unsaved = false;
        self.fit = true;
    }

    /// Save under the current name; true if it worked.
    fn save(&mut self) -> bool {
        let name = clean_name(&self.name);
        if name.is_empty() {
            self.say("Give the map a name first.", true);
            return false;
        }
        self.name = name.clone();
        match self.map.save(&crate::paths::user_map_path(&name)) {
            Ok(()) => {
                self.unsaved = false;
                self.saved = crate::paths::all_map_names();
                self.say(format!("Saved \"{name}\". It's in the New world map list."), false);
                true
            }
            Err(e) => {
                self.say(format!("Couldn't save: {e}"), true);
                false
            }
        }
    }

    fn open(&mut self, name: &str) {
        match crate::paths::load_painted(name) {
            Ok(map) => {
                self.replace(map, name);
                self.say(format!("Opened \"{name}\"."), false);
            }
            Err(e) => self.say(e, true),
        }
    }

    fn create(&mut self) {
        let map = match &self.template {
            Template::Flat => new_flat(),
            Template::Empty => PaintedMap::new(12000.0, 4400.0, 4000.0),
        };
        self.replace(map, "my-map");
        self.unsaved = true;
    }

    /// Paint with the brush at world point `p`: rock, open water or cave. Returns the cells
    /// changed, as a rectangle (x0, y0, x1, y1), if any.
    fn paint(&mut self, p: egui::Vec2, kind: Paint) -> Option<[usize; 4]> {
        let m = &mut self.map;
        let r = self.size * 0.5;
        let c = m.cell;
        let x0 = ((p.x - r) / c).floor().max(0.0) as usize;
        let y0 = ((p.y - r) / c).floor().max(0.0) as usize;
        let x1 = (((p.x + r) / c).ceil() as usize).min(m.nx);
        let y1 = (((p.y + r) / c).ceil() as usize).min(m.ny);
        if x0 >= x1 || y0 >= y1 {
            return None;
        }
        let mut changed = false;
        for y in y0..y1 {
            for x in x0..x1 {
                let cc = m.cell_center(x, y);
                let d = vec2(cc.x - p.x, cc.y - p.y);
                let inside = match self.shape {
                    Shape::Round => d.length() <= r.max(c * 0.5),
                    Shape::Square => d.x.abs() <= r.max(c * 0.5) && d.y.abs() <= r.max(c * 0.5),
                };
                if !inside {
                    continue;
                }
                let now = if m.get(x, y) {
                    Paint::Rock
                } else if m.is_cave(x, y) {
                    Paint::Cave
                } else {
                    Paint::Water
                };
                if now != kind {
                    match kind {
                        Paint::Rock => m.set(x, y, true),
                        Paint::Water => m.set(x, y, false),
                        Paint::Cave => m.set_cave(x, y),
                    }
                    changed = true;
                }
            }
        }
        if kind == Paint::Water {
            // Erasing also clears vent nodes under the brush.
            let shape = self.shape;
            let hit = |v: &[f32; 2]| {
                let d = vec2(v[0] - p.x, v[1] - p.y);
                match shape {
                    Shape::Round => d.length() <= r,
                    Shape::Square => d.x.abs() <= r && d.y.abs() <= r,
                }
            };
            let before = m.vent_spawns.len() + m.permanent_vents.len();
            m.vent_spawns.retain(|v| !hit(v));
            m.permanent_vents.retain(|v| !hit(v));
            changed |= before != m.vent_spawns.len() + m.permanent_vents.len();
        }
        changed.then_some([x0, y0, x1, y1])
    }
}

/// What a brush stroke paints.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Paint {
    Rock,
    Water,
    Cave,
}

/// The colour of a cell in the editor's picture.
fn cell_color(m: &PaintedMap, x: usize, y: usize) -> Color32 {
    let wy = (y as f32 + 0.5) * m.cell;
    let depth = m.water_level - wy;
    if m.get(x, y) {
        // Rock: speckled, lighter above the water.
        let speck = ((x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663)) % 17) as f32 / 17.0;
        let shade = if depth < 0.0 { 1.2 } else { 0.75 + 0.25 * (-depth / 2500.0).exp() };
        let s = shade * (0.92 + 0.08 * speck);
        Color32::from_rgb((112.0 * s) as u8, (98.0 * s) as u8, (82.0 * s) as u8)
    } else if m.is_cave(x, y) {
        // Cave: the rock's colour, much darker (deep and unlit), faintly speckled.
        let speck = ((x.wrapping_mul(83_492_791) ^ y.wrapping_mul(2_654_435_761)) % 13) as f32 / 13.0;
        let s = 0.36 + 0.05 * speck;
        Color32::from_rgb((112.0 * s) as u8, (98.0 * s) as u8, (82.0 * s) as u8)
    } else if depth < 0.0 {
        // Air above the water.
        let t = (-depth / 1500.0).min(1.0);
        Color32::from_rgb((70.0 - 40.0 * t) as u8, (110.0 - 60.0 * t) as u8, (150.0 - 70.0 * t) as u8)
    } else {
        let l = (-depth / 1800.0).exp();
        Color32::from_rgb(
            (6.0 + 34.0 * l) as u8,
            (18.0 + 110.0 * l) as u8,
            (36.0 + 120.0 * l) as u8,
        )
    }
}

/// The picture of cells x0..x1, y0..y1 (texture rows run top down).
fn region_image(m: &PaintedMap, x0: usize, y0: usize, x1: usize, y1: usize) -> egui::ColorImage {
    let (w, h) = (x1 - x0, y1 - y0);
    let mut pixels = Vec::with_capacity(w * h);
    for row in 0..h {
        let y = y1 - 1 - row;
        for x in x0..x1 {
            pixels.push(cell_color(m, x, y));
        }
    }
    egui::ColorImage::new([w, h], pixels)
}

fn draw_editor(
    mut contexts: EguiContexts,
    mut editor: ResMut<Editor>,
    mut next: ResMut<NextState<AppState>>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let ed = &mut *editor;
    if ed.leave {
        next.set(AppState::Menu);
        return Ok(());
    }
    if ed.rebuild || ed.texture.is_none() {
        let m = &ed.map;
        let image = region_image(m, 0, 0, m.nx, m.ny);
        ed.texture = Some(ctx.load_texture("map_editor", image, egui::TextureOptions::NEAREST));
        ed.rebuild = false;
    }
    shortcuts(&ctx, ed);

    let screen = ctx.content_rect();
    // The canvas fills the screen behind the panels.
    egui::Area::new(egui::Id::new("editor_canvas"))
        .fixed_pos(screen.min)
        .order(egui::Order::Background)
        .show(&ctx, |ui| canvas(ui, screen, ed));

    let panel_w = 300.0;
    egui::Area::new(egui::Id::new("editor_tools"))
        .fixed_pos(screen.left_top() + vec2(14.0, 14.0))
        .show(&ctx, |ui| {
            style_menu(ui);
            ui.set_width(panel_w);
            card().show(ui, |ui| {
                ui.set_width(panel_w - 32.0);
                tools_panel(ui, ed);
            });
        });
    if ed.new_open {
        new_dialog(&ctx, ed);
    }
    if ed.save_open {
        save_dialog(&ctx, ed);
    }
    if ed.area_pending.is_some() {
        area_dialog(&ctx, ed);
    }
    if let Some((text, error, at)) = &ed.message
        && at.elapsed().as_secs_f32() < 5.0
    {
        egui::Area::new(egui::Id::new("editor_message"))
            .fixed_pos(screen.center_bottom() + vec2(0.0, -20.0))
            .pivot(egui::Align2::CENTER_BOTTOM)
            .show(&ctx, |ui| {
                style_menu(ui);
                card().show(ui, |ui| {
                    ui.label(egui::RichText::new(text).color(if *error { ERROR } else { ACCENT }));
                });
            });
    }
    Ok(())
}

fn shortcuts(ctx: &egui::Context, ed: &mut Editor) {
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    let (undo, redo, keys) = ctx.input(|i| {
        let cmd = i.modifiers.command;
        let undo = cmd && !i.modifiers.shift && i.key_pressed(egui::Key::Z);
        let redo = cmd
            && (i.key_pressed(egui::Key::Y) || (i.modifiers.shift && i.key_pressed(egui::Key::Z)));
        let keys = [
            egui::Key::B,
            egui::Key::E,
            egui::Key::C,
            egui::Key::V,
            egui::Key::P,
            egui::Key::A,
            egui::Key::OpenBracket,
            egui::Key::CloseBracket,
        ]
        .map(|k| !cmd && i.key_pressed(k));
        (undo, redo, keys)
    });
    if undo {
        ed.undo();
    }
    if redo {
        ed.redo();
    }
    for (k, tool) in [
        Tool::Ground,
        Tool::Erase,
        Tool::Cave,
        Tool::VentSpawn,
        Tool::PermanentVent,
        Tool::StartArea,
    ]
        .into_iter()
        .enumerate()
    {
        if keys[k] {
            ed.tool = tool;
        }
    }
    if keys[6] {
        ed.size = (ed.size / 1.25).max(BRUSH_MIN);
    }
    if keys[7] {
        ed.size = (ed.size * 1.25).min(BRUSH_MAX);
    }
}

/// The map itself: pan, zoom, paint and place nodes.
fn canvas(ui: &mut egui::Ui, screen: Rect, ed: &mut Editor) {
    let (rect, response) = ui.allocate_exact_size(screen.size(), egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_rgb(4, 8, 14));
    let (mw, mh) = (ed.map.width, ed.map.height);
    if ed.fit {
        // Fit into the space right of the tool panel, centred there.
        let panel = 330.0;
        ed.zoom = (mw / (rect.width() - panel - 60.0).max(300.0)).max(mh / (rect.height() - 120.0).max(200.0));
        ed.center = vec2(mw * 0.5 - panel * 0.5 * ed.zoom, mh * 0.5);
        ed.fit = false;
    }
    let to_world = |s: Pos2, center: egui::Vec2, zoom: f32| {
        let d = (s - rect.center()) * zoom;
        vec2(center.x + d.x, center.y - d.y)
    };

    // Zoom toward the cursor; pan with the right or middle button.
    if let Some(hover) = response.hover_pos() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let before = to_world(hover, ed.center, ed.zoom);
            ed.zoom = (ed.zoom * (-scroll * 0.0025).exp()).clamp(0.25, 80.0);
            let after = to_world(hover, ed.center, ed.zoom);
            ed.center += before - after;
        }
    }
    let (secondary, middle, delta) = ui.input(|i| {
        (
            i.pointer.button_down(egui::PointerButton::Secondary),
            i.pointer.button_down(egui::PointerButton::Middle),
            i.pointer.delta(),
        )
    });
    if response.hovered() && (middle || (secondary && response.dragged_by(egui::PointerButton::Secondary))) {
        ed.center += vec2(-delta.x, delta.y) * ed.zoom;
    }

    let (center, zoom) = (ed.center, ed.zoom);
    let to_screen = |p: egui::Vec2| rect.center() + vec2(p.x - center.x, center.y - p.y) / zoom;

    // The map picture.
    let map_rect = Rect::from_min_max(to_screen(vec2(0.0, mh)), to_screen(vec2(mw, 0.0)));
    if let Some(tex) = &ed.texture {
        painter.image(tex.id(), map_rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    painter.rect_stroke(map_rect, 0.0, Stroke::new(1.0, rgba(150, 240, 210, 0.4)), egui::StrokeKind::Outside);
    // The new size while it's being changed.
    if (ed.width_edit - mw).abs() >= ed.map.cell || (ed.height_edit - mh).abs() >= ed.map.cell {
        let r = Rect::from_min_max(to_screen(vec2(0.0, ed.height_edit)), to_screen(vec2(ed.width_edit, 0.0)));
        let dash = |a: Pos2, b: Pos2| {
            painter.extend(egui::Shape::dashed_line(&[a, b], Stroke::new(1.5, ACCENT), 8.0, 6.0));
        };
        dash(r.left_top(), r.right_top());
        dash(r.right_top(), r.right_bottom());
        dash(r.left_bottom(), r.right_bottom());
        dash(r.left_top(), r.left_bottom());
    }
    // The water surface.
    let wl = ed.map.water_level;
    let y = to_screen(vec2(0.0, wl)).y;
    painter.line_segment(
        [pos2(map_rect.left(), y), pos2(map_rect.right(), y)],
        Stroke::new(2.0, Color32::from_rgb(130, 210, 255)),
    );
    painter.text(
        pos2(map_rect.left() + 6.0, y - 4.0),
        egui::Align2::LEFT_BOTTOM,
        "water level",
        egui::FontId::proportional(12.0),
        Color32::from_rgb(170, 225, 255),
    );
    // A scale: one mark every 1000 units along the bottom.
    let mut x = 0.0;
    while x <= mw {
        let s = to_screen(vec2(x, 0.0));
        painter.line_segment([s, s + vec2(0.0, 6.0)], Stroke::new(1.0, MUTED));
        if (x as i32) % 2000 == 0 {
            painter.text(s + vec2(0.0, 8.0), egui::Align2::CENTER_TOP, format!("{x:.0}"), egui::FontId::proportional(11.0), MUTED);
        }
        x += 1000.0;
    }
    // Start areas: dashed green boxes with their names.
    let area_color = Color32::from_rgb(150, 255, 180);
    let dashed_box = |r: Rect, color: Color32| {
        for (a, b) in [
            (r.left_top(), r.right_top()),
            (r.right_top(), r.right_bottom()),
            (r.right_bottom(), r.left_bottom()),
            (r.left_bottom(), r.left_top()),
        ] {
            painter.extend(egui::Shape::dashed_line(&[a, b], Stroke::new(1.5, color), 7.0, 5.0));
        }
    };
    for area in &ed.map.spawn_areas {
        let r = Rect::from_two_pos(
            to_screen(vec2(area.min[0], area.min[1])),
            to_screen(vec2(area.max[0], area.max[1])),
        );
        painter.rect_filled(r, 0.0, rgba(150, 255, 180, 0.08));
        dashed_box(r, area_color);
        painter.text(
            r.left_top() + vec2(5.0, 4.0),
            egui::Align2::LEFT_TOP,
            &area.name,
            egui::FontId::proportional(12.5),
            area_color,
        );
    }

    // Vent nodes.
    for v in &ed.map.vent_spawns {
        let c = to_screen(vec2(v[0], v[1]));
        painter.circle_stroke(c, 7.0, Stroke::new(2.0, Color32::from_rgb(255, 190, 90)));
        painter.circle_filled(c, 2.5, Color32::from_rgb(255, 190, 90));
    }
    for v in &ed.map.permanent_vents {
        let c = to_screen(vec2(v[0], v[1]));
        painter.circle_filled(c, 7.0, Color32::from_rgb(255, 120, 40));
        painter.circle_stroke(c, 10.0, Stroke::new(1.5, Color32::from_rgb(255, 200, 120)));
    }

    // The tool under the pointer.
    let hover = response.hover_pos();
    let world = hover.map(|h| to_world(h, center, zoom));
    if let (Some(h), Some(w)) = (hover, world) {
        match ed.tool {
            Tool::Ground | Tool::Erase | Tool::Cave => {
                let r = ed.size * 0.5 / ed.zoom;
                let color = match ed.tool {
                    Tool::Ground => ACCENT,
                    Tool::Cave => Color32::from_rgb(200, 170, 120),
                    _ => ERROR,
                };
                match ed.shape {
                    Shape::Round => painter.circle_stroke(h, r.max(2.0), Stroke::new(1.5, color)),
                    Shape::Square => painter.rect_stroke(
                        Rect::from_center_size(h, Vec2::splat(r.max(2.0) * 2.0)),
                        0.0,
                        Stroke::new(1.5, color),
                        egui::StrokeKind::Middle,
                    ),
                };
            }
            Tool::VentSpawn | Tool::PermanentVent => {
                painter.circle_stroke(h, 9.0, Stroke::new(1.5, TEXT));
            }
            Tool::StartArea => {
                // The box being dragged out.
                if let Some(from) = ed.area_from {
                    let r = Rect::from_two_pos(to_screen(from), h);
                    painter.rect_filled(r, 0.0, rgba(150, 255, 180, 0.12));
                    dashed_box(r, area_color);
                } else {
                    painter.line_segment([h - vec2(8.0, 0.0), h + vec2(8.0, 0.0)], Stroke::new(1.5, area_color));
                    painter.line_segment([h - vec2(0.0, 8.0), h + vec2(0.0, 8.0)], Stroke::new(1.5, area_color));
                }
            }
        }
        // Where the pointer is.
        let depth = ed.map.water_level - w.y;
        let label = if depth >= 0.0 {
            format!("x {:.0} · depth {:.0}", w.x, depth)
        } else {
            format!("x {:.0} · {:.0} above water", w.x, -depth)
        };
        let font = egui::FontId::proportional(12.5);
        let size = painter.layout_no_wrap(label.clone(), font.clone(), TEXT).size();
        let chip = Rect::from_center_size(
            rect.center_bottom() + vec2(0.0, -86.0),
            size + vec2(18.0, 8.0),
        );
        painter.rect(chip, 10.0, Color32::from_rgb(12, 21, 27), Stroke::new(1.0, rgba(150, 240, 210, 0.18)), egui::StrokeKind::Inside);
        painter.text(chip.center(), egui::Align2::CENTER_CENTER, label, font, TEXT);
    }

    // Using the tool.
    let primary_down = ui.input(|i| i.pointer.primary_down());
    match ed.tool {
        Tool::Ground | Tool::Erase | Tool::Cave => {
            if response.drag_started_by(egui::PointerButton::Primary)
                || response.clicked_by(egui::PointerButton::Primary)
                || (response.is_pointer_button_down_on() && primary_down && ed.last.is_none())
            {
                if ed.last.is_none() {
                    ed.checkpoint();
                }
                ed.last = world;
            }
            if let (Some(last), Some(w)) = (ed.last, world)
                && primary_down
            {
                // Fill in the path since last frame so fast strokes don't skip.
                let kind = match ed.tool {
                    Tool::Ground => Paint::Rock,
                    Tool::Cave => Paint::Cave,
                    _ => Paint::Water,
                };
                let steps = ((w - last).length() / (ed.size * 0.25).max(ed.map.cell)).ceil().max(1.0) as usize;
                let mut dirty: Option<[usize; 4]> = None;
                for k in 1..=steps {
                    let p = last + (w - last) * (k as f32 / steps as f32);
                    if let Some(d) = ed.paint(p, kind) {
                        dirty = Some(match dirty {
                            None => d,
                            Some(o) => [o[0].min(d[0]), o[1].min(d[1]), o[2].max(d[2]), o[3].max(d[3])],
                        });
                    }
                }
                if let Some([x0, y0, x1, y1]) = dirty
                    && let Some(tex) = &mut ed.texture
                {
                    let image = region_image(&ed.map, x0, y0, x1, y1);
                    tex.set_partial([x0, ed.map.ny - y1], image, egui::TextureOptions::NEAREST);
                }
                ed.last = Some(w);
            }
            if !primary_down {
                ed.last = None;
            }
        }
        Tool::VentSpawn | Tool::PermanentVent => {
            if response.clicked_by(egui::PointerButton::Primary)
                && let Some(w) = world
            {
                if w.x < 0.0 || w.x > mw || w.y < 0.0 || w.y > ed.map.water_level {
                    ed.say("Vents go in the water.", true);
                } else {
                    ed.checkpoint();
                    let list = if ed.tool == Tool::VentSpawn {
                        &mut ed.map.vent_spawns
                    } else {
                        &mut ed.map.permanent_vents
                    };
                    list.push([w.x, w.y]);
                }
            }
            if response.clicked_by(egui::PointerButton::Secondary)
                && let Some(w) = world
            {
                // Remove the nearest node within a few pixels.
                let reach = 14.0 * ed.zoom;
                let near = |v: &[f32; 2]| vec2(v[0] - w.x, v[1] - w.y).length() < reach;
                let before = ed.map.vent_spawns.len() + ed.map.permanent_vents.len();
                let snapshot = ed.map.clone();
                ed.map.vent_spawns.retain(|v| !near(v));
                ed.map.permanent_vents.retain(|v| !near(v));
                if before != ed.map.vent_spawns.len() + ed.map.permanent_vents.len() {
                    ed.undo.push(snapshot);
                    ed.redo.clear();
                    ed.unsaved = true;
                }
            }
        }
        Tool::StartArea => {
            if response.drag_started_by(egui::PointerButton::Primary) {
                ed.area_from = world;
            }
            if response.drag_stopped_by(egui::PointerButton::Primary)
                && let (Some(from), Some(to)) = (ed.area_from.take(), world)
            {
                // Kept to the water (life starts there).
                let water = ed.map.water_level;
                let min = [from.x.min(to.x).clamp(0.0, mw), from.y.min(to.y).clamp(0.0, water)];
                let max = [from.x.max(to.x).clamp(0.0, mw), from.y.max(to.y).clamp(0.0, water)];
                if max[0] - min[0] < 20.0 || max[1] - min[1] < 20.0 {
                    ed.say("Drag out a bigger box, in the water.", true);
                } else {
                    ed.area_pending = Some((min, max));
                    ed.area_name = format!("Area {}", ed.map.spawn_areas.len() + 1);
                }
            }
            if response.clicked_by(egui::PointerButton::Secondary)
                && let Some(w) = world
            {
                // Remove the smallest area under the pointer.
                let hit = ed
                    .map
                    .spawn_areas
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| w.x >= a.min[0] && w.x <= a.max[0] && w.y >= a.min[1] && w.y <= a.max[1])
                    .min_by(|a, b| {
                        let size = |a: &evo_core::map::SpawnArea| (a.max[0] - a.min[0]) * (a.max[1] - a.min[1]);
                        size(a.1).partial_cmp(&size(b.1)).unwrap()
                    })
                    .map(|(i, _)| i);
                if let Some(i) = hit {
                    ed.checkpoint();
                    let removed = ed.map.spawn_areas.remove(i);
                    ed.say(format!("Removed start area \"{}\".", removed.name), false);
                }
            }
        }
    }
}


// ---- Panel widgets ----

const TILE_FILL: Color32 = Color32::from_rgb(16, 30, 38);
const TILE_HOVER: Color32 = Color32::from_rgb(32, 49, 56);
const TILE_ON: Color32 = Color32::from_rgb(38, 86, 77);
const ROCK: Color32 = Color32::from_rgb(150, 130, 108);
const VENT: Color32 = Color32::from_rgb(255, 150, 60);
const AREA: Color32 = Color32::from_rgb(150, 255, 180);

/// A tool's icon, drawn at `c` about `s` across.
fn tool_icon(p: &egui::Painter, tool: Tool, c: Pos2, s: f32, color: Color32) {
    let stroke = Stroke::new(1.5, color);
    match tool {
        Tool::Ground => {
            // A rocky mound.
            let pts = vec![
                c + vec2(-s, s * 0.6),
                c + vec2(-s * 0.55, -s * 0.1),
                c + vec2(-s * 0.15, -s * 0.55),
                c + vec2(s * 0.35, -s * 0.2),
                c + vec2(s, s * 0.6),
            ];
            p.add(egui::Shape::convex_polygon(pts, ROCK.gamma_multiply(0.85), stroke));
        }
        Tool::Erase => {
            // A mound with a bite out of it.
            let pts = vec![
                c + vec2(-s, s * 0.6),
                c + vec2(-s * 0.55, -s * 0.1),
                c + vec2(-s * 0.15, -s * 0.55),
                c + vec2(s * 0.35, -s * 0.2),
                c + vec2(s, s * 0.6),
            ];
            p.add(egui::Shape::convex_polygon(pts, ROCK.gamma_multiply(0.35), stroke));
            p.circle_filled(c + vec2(s * 0.15, -s * 0.05), s * 0.42, TILE_FILL);
            p.circle_stroke(c + vec2(s * 0.15, -s * 0.05), s * 0.42, Stroke::new(1.5, ERROR));
        }
        Tool::Cave => {
            // Rock with a dark hollow inside.
            let r = Rect::from_center_size(c, vec2(s * 2.0, s * 1.5));
            p.rect_filled(r, 3.0, ROCK.gamma_multiply(0.85));
            p.add(egui::Shape::ellipse_filled(c + vec2(0.0, s * 0.1), vec2(s * 0.62, s * 0.4), Color32::from_rgb(30, 25, 22)));
            p.rect_stroke(r, 3.0, stroke, egui::StrokeKind::Middle);
        }
        Tool::VentSpawn => {
            p.circle_stroke(c, s * 0.55, Stroke::new(2.0, VENT));
            p.circle_filled(c, s * 0.18, VENT);
        }
        Tool::PermanentVent => {
            p.circle_filled(c, s * 0.5, VENT);
            p.circle_stroke(c, s * 0.8, Stroke::new(1.2, VENT.gamma_multiply(0.6)));
        }
        Tool::StartArea => {
            let r = Rect::from_center_size(c, vec2(s * 1.9, s * 1.3));
            p.rect_filled(r, 2.0, AREA.gamma_multiply(0.15));
            for (a, b) in [
                (r.left_top(), r.right_top()),
                (r.right_top(), r.right_bottom()),
                (r.right_bottom(), r.left_bottom()),
                (r.left_bottom(), r.left_top()),
            ] {
                p.extend(egui::Shape::dashed_line(&[a, b], Stroke::new(1.5, AREA), 4.0, 3.0));
            }
        }
    }
}

/// A tool tile: icon, name, and its key in the corner.
fn tool_tile(ui: &mut egui::Ui, tool: Tool, name: &str, key: &str, on: bool, width: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 58.0), egui::Sense::click());
    let hovered = response.hovered();
    let p = ui.painter();
    let fill = if on { TILE_ON } else if hovered { TILE_HOVER } else { TILE_FILL };
    let stroke = if on { ACCENT } else { rgba(150, 240, 210, if hovered { 0.4 } else { 0.14 }) };
    p.rect(rect, 10.0, fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
    let color = if on || hovered { ACCENT } else { TEXT };
    tool_icon(p, tool, pos2(rect.center().x, rect.top() + 20.0), 10.0, color);
    p.text(
        pos2(rect.center().x, rect.bottom() - 13.0),
        egui::Align2::CENTER_CENTER,
        name,
        egui::FontId::proportional(12.5),
        color,
    );
    p.text(
        rect.right_top() + vec2(-8.0, 7.0),
        egui::Align2::RIGHT_TOP,
        key,
        egui::FontId::monospace(10.5),
        MUTED,
    );
    response
}

/// A small round button with a painted icon.
fn round_icon(
    ui: &mut egui::Ui,
    enabled: bool,
    icon: impl FnOnce(&egui::Painter, Pos2, Color32),
) -> egui::Response {
    let sense = if enabled { egui::Sense::click() } else { egui::Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(vec2(32.0, 32.0), sense);
    let hovered = enabled && response.hovered();
    let p = ui.painter();
    p.circle(
        rect.center(),
        15.0,
        if hovered { TILE_HOVER } else { TILE_FILL },
        Stroke::new(1.0, rgba(150, 240, 210, if hovered { 0.45 } else { 0.18 })),
    );
    let color = if !enabled {
        MUTED.gamma_multiply(0.5)
    } else if hovered {
        ACCENT
    } else {
        TEXT
    };
    icon(p, rect.center(), color);
    response
}

/// An arrow curling round (undo, or redo when `forward`).
fn icon_curl(p: &egui::Painter, c: Pos2, color: Color32, forward: bool) {
    let sign = if forward { -1.0 } else { 1.0 };
    let pts: Vec<Pos2> = (0..=12)
        .map(|i| {
            let a = std::f32::consts::PI * (0.15 + 0.85 * i as f32 / 12.0);
            c + vec2(-sign * a.cos() * 6.0, -a.sin() * 6.0 + 2.0)
        })
        .collect();
    let tip = pts[0];
    p.add(egui::Shape::line(pts, Stroke::new(1.6, color)));
    p.line_segment([tip, tip + vec2(-sign * 4.0, -2.0)], Stroke::new(1.6, color));
    p.line_segment([tip, tip + vec2(-sign * 1.0, -5.0)], Stroke::new(1.6, color));
}

/// Four corners (fit the view).
fn icon_fit(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = 6.0;
    let k = 3.0;
    let stroke = Stroke::new(1.6, color);
    for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let corner = c + vec2(dx * s, dy * s);
        p.line_segment([corner, corner - vec2(dx * k, 0.0)], stroke);
        p.line_segment([corner, corner - vec2(0.0, dy * k)], stroke);
    }
}

fn icon_back(p: &egui::Painter, c: Pos2, color: Color32) {
    let stroke = Stroke::new(2.0, color);
    p.line_segment([c + vec2(3.0, -6.0), c + vec2(-3.0, 0.0)], stroke);
    p.line_segment([c + vec2(-3.0, 0.0), c + vec2(3.0, 6.0)], stroke);
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(12.0).color(MUTED));
}

fn tools_panel(ui: &mut egui::Ui, ed: &mut Editor) {
    // Header: back, title, and the map's name.
    ui.horizontal(|ui| {
        if round_icon(ui, true, icon_back).on_hover_text("Back to the menu").clicked() {
            ed.leave = true;
        }
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(egui::RichText::new("Map editor").size(20.0).color(TEXT));
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&ed.name).size(12.5).color(ACCENT));
                if ed.unsaved {
                    ui.label(egui::RichText::new("· unsaved").size(12.5).color(MUTED));
                }
            });
        });
    });

    caption(ui, "Tools");
    let tile_w = (ui.available_width() - 6.0) / 2.0;
    egui::Grid::new("tools").num_columns(2).spacing([6.0, 6.0]).show(ui, |ui| {
        for (i, (tool, name, key, tip)) in TOOLS.into_iter().enumerate() {
            if tool_tile(ui, tool, name, key, ed.tool == tool, tile_w)
                .on_hover_text(tip)
                .clicked()
            {
                ed.tool = tool;
            }
            if i % 2 == 1 {
                ui.end_row();
            }
        }
    });

    match ed.tool {
        Tool::Ground | Tool::Erase | Tool::Cave => {
            caption(ui, "Brush");
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if pill(ui, "Round", ed.shape == Shape::Round).clicked() {
                    ed.shape = Shape::Round;
                }
                if pill(ui, "Square", ed.shape == Shape::Square).clicked() {
                    ed.shape = Shape::Square;
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().slider_width = ui.available_width() - 70.0;
                ui.add(
                    egui::Slider::new(&mut ed.size, BRUSH_MIN..=BRUSH_MAX)
                        .logarithmic(true)
                        .show_value(false),
                )
                .on_hover_text("[ and ] make it smaller or bigger");
                ui.label(egui::RichText::new(format!("{:.0} u", ed.size)).size(13.0));
            });
        }
        Tool::StartArea => {
            caption(ui, "Start areas");
            hint(ui, "Drag a box, then name it. Right-click one to remove it. The first is the default for new worlds.");
            ui.add_space(4.0);
            if ed.map.spawn_areas.is_empty() {
                hint(ui, "None yet.");
            }
            let mut remove = None;
            for (i, area) in ed.map.spawn_areas.iter().enumerate() {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::hover());
                    ui.painter().rect_filled(r.shrink(2.0), 2.0, AREA.gamma_multiply(0.35));
                    ui.label(egui::RichText::new(&area.name).size(13.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                });
            }
            if let Some(i) = remove {
                ed.checkpoint();
                ed.map.spawn_areas.remove(i);
            }
        }
        Tool::VentSpawn | Tool::PermanentVent => {
            caption(ui, "Vents");
            hint(ui, "Click in the water to place one, right-click one to remove it.");
            ui.label(
                egui::RichText::new(format!(
                    "{} spawn point{} · {} permanent",
                    ed.map.vent_spawns.len(),
                    if ed.map.vent_spawns.len() == 1 { "" } else { "s" },
                    ed.map.permanent_vents.len(),
                ))
                .size(13.0),
            );
        }
    }

    caption(ui, "Water level");
    let mut level = ed.map.water_level;
    let max = ed.map.height;
    let slider = ui
        .horizontal(|ui| {
            ui.spacing_mut().slider_width = ui.available_width() - 70.0;
            let r = ui.add(
                egui::Slider::new(&mut level, 1000.0f32.min(max)..=max)
                    .step_by(ed.map.cell as f64)
                    .show_value(false),
            );
            ui.label(egui::RichText::new(format!("{level:.0} u")).size(13.0));
            r
        })
        .inner
        .on_hover_text("How high the water reaches. Rock above it stands out of the sea as land.");
    // One undo step per drag (or per typed value).
    if slider.drag_started() || (slider.changed() && !slider.dragged()) {
        ed.checkpoint();
    }
    if slider.changed() && level != ed.map.water_level {
        ed.map.water_level = level;
        // Vents must stay in the water.
        ed.map.vent_spawns.retain(|v| v[1] < level);
        ed.map.permanent_vents.retain(|v| v[1] < level);
        ed.rebuild = true;
    }

    caption(ui, "Map size");
    // Applied as soon as a value is finished: when a drag ends, or a typed value is entered.
    let mut apply = false;
    ui.horizontal(|ui| {
        let r = ui.add(egui::DragValue::new(&mut ed.width_edit).range(2000.0..=40000.0).speed(50.0).suffix(" u"));
        apply |= r.drag_stopped() || (r.changed() && !r.dragged());
        ui.label(egui::RichText::new("×").color(MUTED));
        let r = ui.add(egui::DragValue::new(&mut ed.height_edit).range(1200.0..=10000.0).speed(20.0).suffix(" u"));
        apply |= r.drag_stopped() || (r.changed() && !r.dragged());
    });
    if apply {
        ed.apply_size();
    }
    hint(ui, "Width × height. New space is water; the painting stays put from the bottom left.");

    // View and history.
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if round_icon(ui, !ed.undo.is_empty(), |p, c, col| icon_curl(p, c, col, false))
            .on_hover_text("Undo (Ctrl+Z)")
            .clicked()
        {
            ed.undo();
        }
        if round_icon(ui, !ed.redo.is_empty(), |p, c, col| icon_curl(p, c, col, true))
            .on_hover_text("Redo (Ctrl+Y)")
            .clicked()
        {
            ed.redo();
        }
        if round_icon(ui, true, icon_fit).on_hover_text("Fit the map in view").clicked() {
            ed.fit = true;
        }
        ui.add_space(4.0);
        hint(ui, "Scroll to zoom, right-drag to pan");
    });

    // File.
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        let w = (ui.available_width() - 8.0) / 2.0;
        if ui
            .add_sized([w, 36.0], egui::Button::new(egui::RichText::new("Save").size(15.0)))
            .clicked()
        {
            ed.save_open = true;
            ed.new_open = false;
        }
        if ui
            .add_sized([w, 36.0], egui::Button::new(egui::RichText::new("New / open").size(15.0)))
            .clicked()
        {
            ed.new_open = true;
            ed.save_open = false;
        }
    });
}

fn new_dialog(ctx: &egui::Context, ed: &mut Editor) {
    egui::Area::new(egui::Id::new("editor_new"))
        .fixed_pos(ctx.content_rect().center())
        .pivot(egui::Align2::CENTER_CENTER)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            style_menu(ui);
            card().show(ui, |ui| {
                ui.set_width(340.0);
                ui.label(egui::RichText::new("New or open").size(20.0).color(TEXT));
                ui.add_space(6.0);
                ui.radio_value(&mut ed.template, Template::Flat, "Flat seafloor");
                ui.radio_value(&mut ed.template, Template::Empty, "Empty (all water)");
                let mut open = None;
                if !ed.saved.is_empty() {
                    caption(ui, "Open a map");
                    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                        let mine = crate::paths::user_map_names();
                        for name in &ed.saved {
                            let label = if crate::paths::is_builtin_map(name) && !mine.contains(name) {
                                format!("{name}  (comes with the game)")
                            } else {
                                name.clone()
                            };
                            if ui
                                .add_sized([ui.available_width(), 26.0], egui::Button::selectable(*name == ed.name, label))
                                .clicked()
                            {
                                open = Some(name.clone());
                            }
                        }
                    });
                }
                if let Some(name) = open {
                    ed.open(&name);
                    ed.new_open = false;
                }
                if ed.unsaved {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Unsaved changes to this map will be lost.").size(12.5).color(ERROR));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add_sized([120.0, 32.0], egui::Button::new("Create")).clicked() {
                        ed.create();
                        ed.new_open = false;
                    }
                    if ui.add_sized([120.0, 32.0], egui::Button::new("Cancel")).clicked() {
                        ed.new_open = false;
                    }
                });
            });
        });
}

/// Asks for the map's name and description, then saves it.
fn save_dialog(ctx: &egui::Context, ed: &mut Editor) {
    egui::Area::new(egui::Id::new("editor_save"))
        .fixed_pos(ctx.content_rect().center())
        .pivot(egui::Align2::CENTER_CENTER)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            style_menu(ui);
            card().show(ui, |ui| {
                ui.set_width(360.0);
                ui.label(egui::RichText::new("Save map").size(20.0).color(TEXT));
                caption(ui, "Name");
                let name = ui.add(egui::TextEdit::singleline(&mut ed.name).desired_width(f32::INFINITY));
                let path_name = clean_name(&ed.name);
                let mine = crate::paths::user_map_names().contains(&path_name);
                let note = if mine {
                    Some("You have a map with this name; it will be replaced.")
                } else if crate::paths::is_builtin_map(&path_name) {
                    Some("This saves your own copy, used instead of the one that comes with the game.")
                } else {
                    None
                };
                if let Some(note) = note {
                    ui.label(egui::RichText::new(note).size(12.0).color(MUTED));
                }
                caption(ui, "Description");
                ui.add(
                    egui::TextEdit::multiline(&mut ed.map.description)
                        .hint_text("Shown in the New world map list")
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(10.0);
                let enter = name.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| {
                    let ok = !path_name.is_empty();
                    if ui.add_enabled_ui(ok, |ui| ui.add_sized([120.0, 32.0], egui::Button::new("Save"))).inner.clicked()
                        || (enter && ok)
                    {
                        if ed.save() {
                            ed.save_open = false;
                        }
                    }
                    if ui.add_sized([120.0, 32.0], egui::Button::new("Cancel")).clicked() {
                        ed.save_open = false;
                    }
                });
            });
        });
}

/// Names a start area that was just drawn.
fn area_dialog(ctx: &egui::Context, ed: &mut Editor) {
    egui::Area::new(egui::Id::new("editor_area"))
        .fixed_pos(ctx.content_rect().center())
        .pivot(egui::Align2::CENTER_CENTER)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            style_menu(ui);
            card().show(ui, |ui| {
                ui.set_width(340.0);
                ui.label(egui::RichText::new("Name this start area").size(20.0).color(TEXT));
                ui.label(
                    egui::RichText::new("It's offered on the New world screen as a place for life to start.")
                        .size(12.5)
                        .color(MUTED),
                );
                ui.add_space(6.0);
                let field = ui.add(
                    egui::TextEdit::singleline(&mut ed.area_name).desired_width(f32::INFINITY),
                );
                field.request_focus();
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let name = ed.area_name.trim().to_string();
                    let ok = !name.is_empty();
                    let add = ui
                        .add_enabled(ok, egui::Button::new("Add").min_size(vec2(120.0, 32.0)))
                        .clicked();
                    if (add || (enter && ok))
                        && let Some((min, max)) = ed.area_pending.take()
                    {
                        ed.checkpoint();
                        ed.map.spawn_areas.push(evo_core::map::SpawnArea { name, min, max });
                    }
                    if ui.add_sized([120.0, 32.0], egui::Button::new("Cancel")).clicked() {
                        ed.area_pending = None;
                    }
                });
            });
        });
}

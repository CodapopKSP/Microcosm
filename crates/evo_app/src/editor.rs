//! The map editor: paint rock, place vent nodes, set the water level and the map's size, and
//! save the map under a name for the new-world screen (`configs/maps/<name>.map`).

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use egui::{Color32, Pos2, Rect, Stroke, Vec2, pos2, vec2};
use evo_core::Config;
use evo_core::map::PaintedMap;

use crate::AppState;
use crate::menu::{ACCENT, ERROR, MUTED, TEXT, card, rgba, style_menu};

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

const MAPS_DIR: &str = "configs/maps";
/// Most undo steps kept.
const UNDO_STEPS: usize = 40;
/// Brush sizes (world units), for the slider.
const BRUSH_MIN: f32 = 8.0;
const BRUSH_MAX: f32 = 2400.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Ground,
    Erase,
    VentSpawn,
    PermanentVent,
}

const TOOLS: [(Tool, &str, &str, &str); 4] = [
    (Tool::Ground, "Ground", "B", "Paint rock"),
    (Tool::Erase, "Remove ground", "E", "Paint water (also removes vent nodes it touches)"),
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
    /// A copy of a built-in (generated) map.
    Copy(String),
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
    /// Painted maps saved so far (names).
    saved: Vec<String>,
    /// Generated maps, to start from.
    builtin: Vec<String>,
    template: Template,
    new_open: bool,
    /// The save window is open (asking for a name and description).
    save_open: bool,
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
            builtin: Vec::new(),
            template: Template::Flat,
            new_open: false,
            save_open: false,
            unsaved: false,
            message: None,
            leave: false,
        }
    }
}

fn new_flat() -> PaintedMap {
    PaintedMap::flat(12000.0, 4400.0, 4000.0, 700.0)
}

fn map_path(name: &str) -> PathBuf {
    Path::new(MAPS_DIR).join(format!("{name}.map"))
}

/// Map names in the maps folder with this extension.
fn list(ext: &str) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(MAPS_DIR)
        .map(|d| {
            d.flatten()
                .filter_map(|e| {
                    let p = e.path();
                    (p.extension()?.to_str()? == ext)
                        .then(|| p.file_stem()?.to_str().map(str::to_string))
                        .flatten()
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// A name that's safe as a file name.
fn clean_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect()
}

fn enter(mut commands: Commands, mut editor: ResMut<Editor>) {
    // The menu's live backdrop isn't needed here.
    commands.remove_resource::<crate::sim::SimState>();
    editor.saved = list("map");
    editor.builtin = list("toml");
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
        if self.builtin.contains(&name) {
            self.say(format!("\"{name}\" is a built-in map; pick another name."), true);
            return false;
        }
        self.name = name.clone();
        match self.map.save(&map_path(&name)) {
            Ok(()) => {
                self.unsaved = false;
                self.saved = list("map");
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
        match PaintedMap::load(&map_path(name)) {
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
            Template::Copy(name) => {
                match Config::load_with_map(Path::new("configs/default.toml"), Some(name)) {
                    Ok(config) => PaintedMap::from_config(&config),
                    Err(e) => {
                        self.say(e, true);
                        return;
                    }
                }
            }
        };
        let name = match &self.template {
            Template::Copy(n) => format!("{n}-copy"),
            _ => "my-map".into(),
        };
        self.replace(map, &name);
        self.unsaved = true;
    }

    /// Paint (or erase) with the brush at world point `p`. Returns the cells changed, as a
    /// rectangle (x0, y0, x1, y1), if any.
    fn paint(&mut self, p: egui::Vec2, rock: bool) -> Option<[usize; 4]> {
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
                if inside && m.get(x, y) != rock {
                    m.set(x, y, rock);
                    changed = true;
                }
            }
        }
        if !rock {
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

    let panel_w = 290.0;
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
            egui::Key::V,
            egui::Key::P,
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
    for (k, tool) in [Tool::Ground, Tool::Erase, Tool::VentSpawn, Tool::PermanentVent]
        .into_iter()
        .enumerate()
    {
        if keys[k] {
            ed.tool = tool;
        }
    }
    if keys[4] {
        ed.size = (ed.size / 1.25).max(BRUSH_MIN);
    }
    if keys[5] {
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
            Tool::Ground | Tool::Erase => {
                let r = ed.size * 0.5 / ed.zoom;
                let color = if ed.tool == Tool::Ground { ACCENT } else { ERROR };
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
        }
        // Where the pointer is.
        let depth = ed.map.water_level - w.y;
        let label = if depth >= 0.0 {
            format!("x {:.0} · depth {:.0}", w.x, depth)
        } else {
            format!("x {:.0} · {:.0} above water", w.x, -depth)
        };
        painter.text(
            rect.center_bottom() + vec2(0.0, -70.0),
            egui::Align2::CENTER_BOTTOM,
            label,
            egui::FontId::proportional(13.0),
            MUTED,
        );
    }

    // Using the tool.
    let primary_down = ui.input(|i| i.pointer.primary_down());
    match ed.tool {
        Tool::Ground | Tool::Erase => {
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
                let rock = ed.tool == Tool::Ground;
                let steps = ((w - last).length() / (ed.size * 0.25).max(ed.map.cell)).ceil().max(1.0) as usize;
                let mut dirty: Option<[usize; 4]> = None;
                for k in 1..=steps {
                    let p = last + (w - last) * (k as f32 / steps as f32);
                    if let Some(d) = ed.paint(p, rock) {
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
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(egui::RichText::new(title.to_uppercase()).size(11.5).strong().color(MUTED));
    ui.add_space(2.0);
}

fn tools_panel(ui: &mut egui::Ui, ed: &mut Editor) {
    ui.horizontal(|ui| {
        if ui.add_sized([80.0, 30.0], egui::Button::new("Back")).clicked() {
            ed.leave = true;
        }
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Map editor").size(22.0).color(TEXT));
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(&ed.name).size(13.0).color(MUTED));
        if ed.unsaved {
            ui.label(egui::RichText::new("· unsaved").size(13.0).color(MUTED));
        }
    });
    section(ui, "Tool");
    for (tool, name, key, tip) in TOOLS {
        ui.horizontal(|ui| {
            let on = ed.tool == tool;
            if ui
                .add_sized([200.0, 28.0], egui::Button::selectable(on, name))
                .on_hover_text(tip)
                .clicked()
            {
                ed.tool = tool;
            }
            ui.label(egui::RichText::new(key).monospace().color(MUTED));
        });
    }
    if matches!(ed.tool, Tool::Ground | Tool::Erase) {
        section(ui, "Brush");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut ed.shape, Shape::Round, "Round");
            ui.selectable_value(&mut ed.shape, Shape::Square, "Square");
        });
        ui.add(
            egui::Slider::new(&mut ed.size, BRUSH_MIN..=BRUSH_MAX)
                .logarithmic(true)
                .suffix(" u")
                .text("size"),
        )
        .on_hover_text("[ and ] make it smaller or bigger");
    } else {
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Click to place, right-click a node to remove it.").size(12.5).color(MUTED));
    }

    section(ui, "Water level");
    let mut level = ed.map.water_level;
    let max = ed.map.height;
    let slider = ui
        .add(egui::Slider::new(&mut level, 1000.0f32.min(max)..=max).step_by(ed.map.cell as f64).suffix(" u"))
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

    section(ui, "Map size");
    // Applied as soon as a value is finished: when a drag ends, or a typed value is entered.
    let mut apply = false;
    egui::Grid::new("size").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        ui.label("Width");
        let r = ui.add(egui::DragValue::new(&mut ed.width_edit).range(2000.0..=40000.0).speed(50.0).suffix(" u"));
        apply |= r.drag_stopped() || (r.changed() && !r.dragged());
        ui.end_row();
        ui.label("Height");
        let r = ui.add(egui::DragValue::new(&mut ed.height_edit).range(1200.0..=10000.0).speed(20.0).suffix(" u"));
        apply |= r.drag_stopped() || (r.changed() && !r.dragged());
        ui.end_row();
    });
    if apply {
        ed.apply_size();
    }
    ui.label(
        egui::RichText::new("New space is water; what's painted stays put from the bottom left.")
            .size(12.0)
            .color(MUTED),
    );
    ui.label(
        egui::RichText::new(format!(
            "{:.0} × {:.0}, water {:.0} deep",
            ed.map.width, ed.map.height, ed.map.water_level
        ))
        .size(12.5)
        .color(MUTED),
    );

    section(ui, "View");
    ui.horizontal(|ui| {
        if ui.button("Fit").clicked() {
            ed.fit = true;
        }
        if ui.add_enabled(!ed.undo.is_empty(), egui::Button::new("Undo")).on_hover_text("Ctrl+Z").clicked() {
            ed.undo();
        }
        if ui.add_enabled(!ed.redo.is_empty(), egui::Button::new("Redo")).on_hover_text("Ctrl+Y").clicked() {
            ed.redo();
        }
    });
    ui.label(
        egui::RichText::new("Scroll to zoom, right- or middle-drag to pan.")
            .size(12.0)
            .color(MUTED),
    );
    ui.label(
        egui::RichText::new(format!(
            "{} vent spawn{}, {} permanent vent{}",
            ed.map.vent_spawns.len(),
            if ed.map.vent_spawns.len() == 1 { "" } else { "s" },
            ed.map.permanent_vents.len(),
            if ed.map.permanent_vents.len() == 1 { "" } else { "s" },
        ))
        .size(12.0)
        .color(MUTED),
    );

    ui.add_space(14.0);
    ui.horizontal(|ui| {
        if ui.add_sized([118.0, 34.0], egui::Button::new("Save")).clicked() {
            ed.save_open = true;
            ed.new_open = false;
        }
        if ui.add_sized([118.0, 34.0], egui::Button::new("New…")).clicked() {
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
                let builtin = ed.builtin.clone();
                for name in builtin {
                    let label = format!("Copy of the \"{name}\" map");
                    ui.radio_value(&mut ed.template, Template::Copy(name), label);
                }
                let mut open = None;
                if !ed.saved.is_empty() {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("OPEN A SAVED MAP").size(11.5).strong().color(MUTED));
                    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                        for name in &ed.saved {
                            if ui
                                .add_sized([ui.available_width(), 26.0], egui::Button::selectable(*name == ed.name, name))
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
                section(ui, "Name");
                let name = ui.add(egui::TextEdit::singleline(&mut ed.name).desired_width(f32::INFINITY));
                let path_name = clean_name(&ed.name);
                let exists = ed.saved.contains(&path_name);
                if exists {
                    ui.label(egui::RichText::new("A saved map has this name; it will be replaced.").size(12.0).color(MUTED));
                }
                section(ui, "Description");
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

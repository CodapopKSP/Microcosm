//! egui panels: HUD, organism inspector (chromosomes), species list, events, charts, help.

mod charts;
mod hud;
mod inspector;
mod notebook;
mod probe;
mod sketch;
mod species;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use evo_core::World;
use evo_core::stats::EventKind;
use evo_core::traits::format_game_date;

use crate::camera::CameraState;
use crate::sim::{Selection, SimState};

/// Water overlay showing one chemical (key M cycles).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MapMode {
    #[default]
    None = 0,
    Oxygen = 1,
    Co2 = 2,
    Nutrients = 3,
    Minerals = 4,
    Toxin = 5,
}

impl MapMode {
    pub fn next(self) -> MapMode {
        match self {
            MapMode::None => MapMode::Oxygen,
            MapMode::Oxygen => MapMode::Co2,
            MapMode::Co2 => MapMode::Nutrients,
            MapMode::Nutrients => MapMode::Minerals,
            MapMode::Minerals => MapMode::Toxin,
            MapMode::Toxin => MapMode::None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MapMode::None => "",
            MapMode::Oxygen => "Oxygen map: blue = rich, purple = dead zone",
            MapMode::Co2 => "CO₂ map: blue = low, gold = high",
            MapMode::Nutrients => "Nutrient map: brighter green = richer",
            MapMode::Minerals => "Mineral map: brighter cyan = richer (vents, upwellings)",
            MapMode::Toxin => "Toxin map: red = poisoned water",
        }
    }
}

#[derive(Resource)]
pub struct UiState {
    /// Hide everything except the minimal stream overlay.
    pub show_ui: bool,
    pub show_species: bool,
    pub show_charts: bool,
    pub show_help: bool,
    pub show_events: bool,
    pub show_history: bool,
    pub show_notebook: bool,
    notebook: notebook::NotebookState,
    inspector_tab: inspector::Tab,
    /// A short message in the corner (e.g. "recorded in the notebook") and when it appeared.
    pub toast: Option<(String, std::time::Instant)>,
    history_filter: [bool; 4],
    pub show_senses: bool,
    pub map_mode: MapMode,
    /// The map button's list of overlays is open.
    map_menu: bool,
    charts: charts::ChartState,
    fps: f64,
    /// The egui style has been set up (quicker tooltips, the handwriting font).
    styled: bool,
    /// The handwriting font is usable (fonts take effect the frame after they're set).
    fonts_ready: bool,
    /// The Menu button was pressed: save and go back to the menus.
    to_menu: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            show_ui: true,
            show_species: true,
            show_charts: false,
            show_help: false,
            show_events: true,
            show_history: false,
            show_notebook: false,
            notebook: Default::default(),
            inspector_tab: Default::default(),
            toast: None,
            history_filter: [true; 4],
            show_senses: true,
            map_mode: MapMode::None,
            map_menu: false,
            charts: charts::ChartState::default(),
            fps: 0.0,
            styled: false,
            fonts_ready: false,
            to_menu: false,
        }
    }
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiState>()
            .add_systems(Startup, apply_stream_option)
            .add_systems(Update, ui_keys.run_if(in_state(crate::AppState::Running)))
            .add_systems(
                EguiPrimaryContextPass,
                draw_ui.run_if(in_state(crate::AppState::Running)),
            );
    }
}

fn apply_stream_option(options: Res<crate::ViewerOptions>, mut ui: ResMut<UiState>) {
    if options.stream {
        ui.show_ui = false;
    }
    ui.show_charts = options.charts;
    ui.map_mode = match options.map.as_deref() {
        Some("oxygen") => MapMode::Oxygen,
        Some("co2") => MapMode::Co2,
        Some("nutrients") => MapMode::Nutrients,
        Some("minerals") => MapMode::Minerals,
        Some("toxin") => MapMode::Toxin,
        _ => MapMode::None,
    };
    if options.tree {
        ui.charts.tab = charts::ChartTab::Tree;
    }
}

fn ui_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<UiState>,
    egui_input: Res<bevy_egui::input::EguiWantsInput>,
) {
    if egui_input.wants_any_keyboard_input() {
        return;
    }
    if keys.just_pressed(KeyCode::Tab) {
        ui.show_ui = !ui.show_ui;
    }
    if keys.just_pressed(KeyCode::KeyM) {
        ui.map_mode = ui.map_mode.next();
    }
    if keys.just_pressed(KeyCode::KeyH) || keys.just_pressed(KeyCode::F1) {
        ui.show_help = !ui.show_help;
    }
    if keys.just_pressed(KeyCode::KeyG) {
        ui.show_charts = !ui.show_charts;
    }
    if keys.just_pressed(KeyCode::KeyL) {
        ui.show_species = !ui.show_species;
    }
    if keys.just_pressed(KeyCode::KeyE) {
        ui.show_history = !ui.show_history;
    }
    if keys.just_pressed(KeyCode::KeyT) {
        ui.show_events = !ui.show_events;
    }
    if keys.just_pressed(KeyCode::KeyN) {
        ui.show_notebook = !ui.show_notebook;
    }
}

pub fn diet_color(i: usize) -> egui::Color32 {
    match i {
        0 => egui::Color32::from_rgb(90, 200, 90),
        1 => egui::Color32::from_rgb(70, 210, 230),
        2 => egui::Color32::from_rgb(190, 140, 80),
        _ => egui::Color32::from_rgb(235, 80, 70),
    }
}

pub const DIET_NAMES: [&str; 4] = ["Photosynthesis", "Chemosynthesis", "Scavenging", "Hunting"];

pub fn color32(rgb: [f32; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(
        (rgb[0].clamp(0.0, 1.0) * 255.0) as u8,
        (rgb[1].clamp(0.0, 1.0) * 255.0) as u8,
        (rgb[2].clamp(0.0, 1.0) * 255.0) as u8,
    )
}

pub fn swatch(ui: &mut egui::Ui, rgb: [f32; 3], size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    ui.painter()
        .circle_filled(rect.center(), size * 0.5, color32(rgb));
    response
}

/// "Day 3, 14:20" style clock.
pub fn clock_text(world: &World) -> String {
    let c = &world.env.clock;
    let (h, m) = c.time_of_day();
    format!(
        "Year {} · {} · Day {} · {:02}:{:02}",
        c.year + 1,
        c.season_name(),
        c.day + 1,
        h,
        m
    )
}

pub fn event_color(kind: EventKind) -> egui::Color32 {
    match kind {
        EventKind::Species => egui::Color32::from_rgb(140, 220, 140),
        EventKind::Extinction => egui::Color32::from_rgb(230, 120, 110),
        EventKind::Environment => egui::Color32::from_rgb(120, 190, 240),
        EventKind::Disaster => egui::Color32::from_rgb(255, 120, 60),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_ui(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<UiState>,
    mut sim: ResMut<SimState>,
    mut selection: ResMut<Selection>,
    mut cam: ResMut<CameraState>,
    time: Res<Time<Real>>,
    mut next: ResMut<NextState<crate::AppState>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    if std::mem::take(&mut ui_state.to_menu) {
        sim.save("autosave.bin");
        next.set(crate::AppState::Menu);
    }
    ui_state.fonts_ready = ui_state.styled;
    if !ui_state.styled {
        // Tooltips appear almost at once (the default waits half a second).
        ctx.all_styles_mut(|style| {
            style.interaction.tooltip_delay = 0.05;
            style.interaction.show_tooltips_only_when_still = false;
        });
        notebook::install_fonts(ctx);
        ui_state.styled = true;
    }
    let dt = time.delta_secs_f64();
    if dt > 0.0 {
        ui_state.fps = ui_state.fps * 0.95 + (1.0 / dt) * 0.05;
    }

    if !ui_state.show_ui {
        stream_overlay(ctx, &sim, &selection);
        return Ok(());
    }

    if let Some(pos) = selection.probe {
        let mut open = true;
        egui::Window::new("Water here")
            .open(&mut open)
            .default_pos([20.0, 90.0])
            .default_width(380.0)
            .resizable(false)
            .show(ctx, |ui| probe::draw(ui, &sim.world, pos));
        if !open {
            selection.probe = None;
        }
    }

    // The world view. The inspector card sits beside the toolbar on the right, and the HUD
    // cards keep to what's left of it (`content`). The species list goes in the left column,
    // under the clock and above the event ticker.
    let view = ctx.content_rect();
    let inspector_rect = selection.organism.map(|_| {
        egui::Rect::from_min_max(
            egui::pos2(
                view.right() - hud::TOOLBAR_WIDTH - inspector::WIDTH,
                view.top() + hud::GAP,
            ),
            egui::pos2(view.right() - hud::TOOLBAR_WIDTH, view.bottom() - hud::GAP),
        )
    });
    let content = egui::Rect::from_min_max(
        view.min,
        egui::pos2(
            inspector_rect.map_or(view.right() - hud::TOOLBAR_WIDTH, |r| r.left() - hud::GAP),
            view.bottom(),
        ),
    );
    let column_right = if ui_state.show_species {
        view.left() + hud::GAP + species::WIDTH
    } else {
        view.left()
    };
    let left_bottom = hud::draw(
        ctx,
        view,
        content,
        column_right,
        &mut ui_state,
        &mut sim,
        &mut cam,
        &mut selection,
    );
    if ui_state.show_species {
        // Down to the event ticker (last frame's), or the bottom of the view.
        let ticker_top = (ui_state.show_events && !sim.world.events.entries.is_empty())
            .then(|| ctx.memory(|m| m.area_rect(egui::Id::new("events"))))
            .flatten()
            .map_or(view.bottom(), |r| r.top());
        let rect = egui::Rect::from_min_max(
            egui::pos2(view.left() + hud::GAP, left_bottom + hud::GAP),
            egui::pos2(view.left() + hud::GAP + species::WIDTH, ticker_top - hud::GAP),
        );
        if rect.height() > 80.0 && !species::show(ctx, rect, &sim.world, &mut selection, &mut cam) {
            ui_state.show_species = false;
        }
    }

    let mut record = None;
    if let (Some(id), Some(rect)) = (selection.organism, inspector_rect) {
        match inspector::show(ctx, rect, &sim.world, id, &mut selection, &mut ui_state.inspector_tab) {
            Some(inspector::Action::Record) => record = Some(id),
            Some(inspector::Action::OpenNotebook) => {
                if let Some(i) = sim
                    .world
                    .notebook
                    .entries
                    .iter()
                    .position(|e| e.specimens.iter().any(|s| s.organism == id))
                {
                    ui_state.notebook.show_entry(i);
                }
                ui_state.show_notebook = true;
            }
            None => {}
        }
    }

    if ui_state.show_events && !sim.world.events.entries.is_empty() {
        event_ticker(ctx, content, &sim.world, &mut selection, &mut cam);
    }

    if ui_state.show_charts {
        let mut open = true;
        let charts_state = &mut ui_state.charts;
        egui::Window::new("Charts")
            .open(&mut open)
            .default_size([760.0, 560.0])
            .min_size([360.0, 240.0])
            .resizable(true)
            .show(ctx, |ui| {
                charts::draw(ui, &sim.world, &mut selection, charts_state, &mut cam)
            });
        ui_state.show_charts = open;
    }

    if ui_state.show_history {
        let mut open = true;
        let mut filter = ui_state.history_filter;
        egui::Window::new("History")
            .open(&mut open)
            .default_size([520.0, 480.0])
            .show(ctx, |ui| {
                history(ui, &sim.world, &mut filter, &mut selection, &mut cam)
            });
        ui_state.history_filter = filter;
        ui_state.show_history = open;
    }

    if let Some(id) = record {
        use evo_core::notebook::Recorded;
        let result = sim.world.record_specimen(id);
        let notebook = &sim.world.notebook;
        if let Some(i) = notebook
            .entries
            .iter()
            .position(|e| e.specimens.iter().any(|s| s.organism == id))
        {
            let name = &notebook.entries[i].name;
            let msg = match result {
                Recorded::NewEntry => Some(format!("New notebook entry: {name} (N to open)")),
                Recorded::Specimen => Some(format!("Another {name} sketched in the notebook")),
                _ => None,
            };
            if let Some(msg) = msg {
                ui_state.toast = Some((msg, std::time::Instant::now()));
            }
            ui_state.notebook.show_entry(i);
        }
    }

    if ui_state.show_notebook && ui_state.fonts_ready {
        let mut open = true;
        if let Some(notebook::Action::TearOut(species)) =
            notebook::draw(ctx, &sim.world, &mut ui_state.notebook, &mut open)
        {
            sim.world.notebook.remove(species);
        }
        ui_state.show_notebook = open;
    }

    if ui_state.show_help {
        let mut open = true;
        egui::Window::new("Help")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, help);
        ui_state.show_help = open;
    }
    Ok(())
}

fn event_ticker(
    ctx: &egui::Context,
    view: egui::Rect,
    world: &World,
    selection: &mut Selection,
    cam: &mut CameraState,
) {
    egui::Area::new(egui::Id::new("events"))
        .fixed_pos(view.left_bottom() + egui::vec2(10.0, -10.0))
        .pivot(egui::Align2::LEFT_BOTTOM)
        .interactable(true)
        .show(ctx, |ui| {
            hud::card().show(ui, |ui| {
                ui.set_max_width(520.0);
                for e in world.events.entries.iter().rev().take(6).rev() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(format_game_date(e.time, &world.config.time))
                                .weak()
                                .monospace(),
                        );
                        let response = ui.add(
                            egui::Label::new(
                                egui::RichText::new(&e.text).color(event_color(e.kind)),
                            )
                            .sense(egui::Sense::click()),
                        );
                        if response.clicked() {
                            goto_event(e, world, selection, cam);
                        }
                    });
                }
            });
        });
}

const EVENT_KINDS: [(EventKind, &str); 4] = [
    (EventKind::Species, "New species"),
    (EventKind::Extinction, "Extinctions"),
    (EventKind::Disaster, "Disasters"),
    (EventKind::Environment, "Environment"),
];

/// Full event log with filters.
fn history(
    ui: &mut egui::Ui,
    world: &World,
    filter: &mut [bool; 4],
    selection: &mut Selection,
    cam: &mut CameraState,
) {
    ui.horizontal_wrapped(|ui| {
        for (i, (kind, name)) in EVENT_KINDS.iter().enumerate() {
            ui.checkbox(
                &mut filter[i],
                egui::RichText::new(*name).color(event_color(*kind)),
            );
        }
    });
    ui.label(
        egui::RichText::new(format!(
            "{} events so far (the oldest are trimmed). Click an event to find the species.",
            world.events.count
        ))
        .weak(),
    );
    ui.separator();
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for e in &world.events.entries {
                let shown = EVENT_KINDS
                    .iter()
                    .position(|(k, _)| *k == e.kind)
                    .map(|i| filter[i])
                    .unwrap_or(true);
                if !shown {
                    continue;
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(format_game_date(e.time, &world.config.time))
                            .monospace()
                            .weak(),
                    );
                    let response = ui.add(
                        egui::Label::new(egui::RichText::new(&e.text).color(event_color(e.kind)))
                            .sense(egui::Sense::click()),
                    );
                    if response.clicked() {
                        goto_event(e, world, selection, cam);
                    }
                });
            }
        });
}

/// Jump the camera to where an event happened (or to a member of its species).
fn goto_event(
    e: &evo_core::stats::Event,
    world: &World,
    selection: &mut Selection,
    cam: &mut CameraState,
) {
    if let Some(p) = e.pos {
        selection.follow = false;
        cam.director = false;
        cam.focus(Vec2::new(p.x, p.y), Some(2.0));
    } else if let Some(sp) = e.species {
        selection.species = Some(sp);
        if let Some(o) = world.organisms.iter().find(|o| o.species == sp) {
            selection.organism = Some(o.id);
            selection.follow = true;
            cam.focus(Vec2::new(o.pos.x, o.pos.y), None);
        }
    }
}

/// Minimal overlay for stream mode (UI hidden).
fn stream_overlay(ctx: &egui::Context, sim: &SimState, selection: &Selection) {
    let world = &sim.world;
    egui::Area::new(egui::Id::new("stream_top"))
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(14.0, 12.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(egui::Color32::from_black_alpha(140))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(clock_text(world)).size(18.0).strong());
                    ui.label(
                        egui::RichText::new(format!(
                            "{} organisms · {} species",
                            world.organisms.len(),
                            world.species.living().count()
                        ))
                        .size(15.0),
                    );
                });
        });
    egui::Area::new(egui::Id::new("stream_events"))
        .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(14.0, -12.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(egui::Color32::from_black_alpha(140))
                .show(ui, |ui| {
                    ui.set_max_width(560.0);
                    for e in world.events.entries.iter().rev().take(4).rev() {
                        ui.label(
                            egui::RichText::new(&e.text)
                                .color(event_color(e.kind))
                                .size(15.0),
                        );
                    }
                });
        });
    if let Some(o) = selection.organism.and_then(|id| world.find(id)) {
        egui::Area::new(egui::Id::new("stream_card"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-14.0, -12.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(egui::Color32::from_black_alpha(140))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            swatch(ui, o.traits.color, 16.0);
                            ui.label(
                                egui::RichText::new(world.species.name(o.species))
                                    .size(17.0)
                                    .strong(),
                            );
                        });
                        ui.label(
                            egui::RichText::new(format!(
                                "{} · {} · generation {}",
                                o.activity.label(),
                                DIET_NAMES[o.traits.main_diet()],
                                o.generation
                            ))
                            .size(14.0),
                        );
                        ui.label(
                            egui::RichText::new(o.genome.to_string())
                                .monospace()
                                .size(13.0),
                        );
                    });
            });
    }
}

/// One piece of a shortcut in the help window.
enum Key {
    /// A keyboard key.
    K(&'static str),
    /// A mouse action.
    Mouse(&'static str),
    /// "or" between alternatives.
    Or,
    /// "+" between keys held together.
    Plus,
    /// A range of keys ("1 – 7").
    To,
}

const HELP_CAMERA: egui::Color32 = egui::Color32::from_rgb(110, 180, 245);
const HELP_WATCH: egui::Color32 = egui::Color32::from_rgb(120, 210, 120);
const HELP_TIME: egui::Color32 = egui::Color32::from_rgb(240, 205, 100);
const HELP_PANELS: egui::Color32 = egui::Color32::from_rgb(190, 150, 240);
const HELP_EVENTS: egui::Color32 = egui::Color32::from_rgb(240, 120, 110);
const HELP_GENOME: egui::Color32 = egui::Color32::from_rgb(255, 170, 60);
/// Width of the shortcut column.
const HELP_KEYS_WIDTH: f32 = 172.0;

/// A key drawn as a keycap: a raised face over a darker edge.
fn key_cap(ui: &mut egui::Ui, text: &str, mouse: bool) {
    let font = egui::FontId::monospace(12.0);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font, egui::Color32::WHITE);
    let pad = egui::vec2(6.0, 2.0);
    let size = egui::vec2(
        (galley.size().x + 2.0 * pad.x).max(22.0),
        galley.size().y + 2.0 * pad.y + 3.0,
    );
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let (face, edge, border) = if mouse {
        (
            egui::Color32::from_rgb(45, 62, 82),
            egui::Color32::from_rgb(20, 28, 38),
            egui::Color32::from_rgb(90, 125, 160),
        )
    } else {
        (
            egui::Color32::from_gray(66),
            egui::Color32::from_gray(26),
            egui::Color32::from_gray(115),
        )
    };
    let radius = if mouse { 9.0 } else { 4.0 };
    let painter = ui.painter();
    painter.rect_filled(rect, radius, edge);
    let top = egui::Rect::from_min_max(rect.min, rect.max - egui::vec2(0.0, 3.0));
    painter.rect(
        top,
        radius,
        face,
        egui::Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    );
    painter.galley(
        top.center() - galley.size() / 2.0,
        galley,
        egui::Color32::WHITE,
    );
}

fn help_section(ui: &mut egui::Ui, title: &str, color: egui::Color32) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(4.0, 18.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 2.0, color);
        ui.label(egui::RichText::new(title).strong().size(15.0).color(color));
    });
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, color.gamma_multiply(0.35));
    ui.add_space(4.0);
}

/// One shortcut: its keys in a fixed-width column, then what it does.
fn help_row(ui: &mut egui::Ui, keys: &[Key], what: &str) {
    help_row_width(ui, keys, what, HELP_KEYS_WIDTH);
}

fn help_row_width(ui: &mut egui::Ui, keys: &[Key], what: &str, width: f32) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(width, 20.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(width);
                ui.spacing_mut().item_spacing.x = 3.0;
                for k in keys {
                    match k {
                        Key::K(t) => key_cap(ui, t, false),
                        Key::Mouse(t) => key_cap(ui, t, true),
                        Key::Or => {
                            ui.label(egui::RichText::new("/").weak());
                        }
                        Key::Plus => {
                            ui.label(egui::RichText::new("+").weak());
                        }
                        Key::To => {
                            ui.label(egui::RichText::new("–").weak());
                        }
                    }
                }
            },
        );
        ui.add(egui::Label::new(what).wrap());
    });
}

fn help(ui: &mut egui::Ui) {
    use Key::*;
    ui.set_width(760.0);
    ui.columns(2, |cols| {
        let ui = &mut cols[0];
        help_section(ui, "Camera", HELP_CAMERA);
        help_row(ui, &[Mouse("Drag")], "Pan (any mouse button)");
        help_row(
            ui,
            &[K("W"), K("A"), K("S"), K("D"), Or, K("Arrows")],
            "Pan",
        );
        help_row(ui, &[Mouse("Scroll")], "Zoom");
        help_row(ui, &[K("Home")], "Show the whole world");
        help_row(ui, &[K("C")], "Auto camera: follows the action on its own");

        help_section(ui, "Watching", HELP_WATCH);
        help_row(
            ui,
            &[Mouse("Left click")],
            "Inspect an organism and follow it",
        );
        help_row(ui, &[K("F")], "Follow on / off");
        help_row(ui, &[K("Esc")], "Stop inspecting");
        help_row(
            ui,
            &[Mouse("Right click")],
            "Inspect the water there: light, temperature, oxygen and more",
        );

        help_section(ui, "Time", HELP_TIME);
        help_row(ui, &[K("Space")], "Pause / resume");
        help_row(ui, &[K("1"), To, K("7")], "Speed, from ½× up to 64×");
        help_row(ui, &[K("+"), Or, K("-")], "Faster / slower");
        help_row(ui, &[K("F5")], "Quicksave");

        let ui = &mut cols[1];
        // Short keys here, so a narrower key column.
        let help_row =
            |ui: &mut egui::Ui, keys: &[Key], what: &str| help_row_width(ui, keys, what, 100.0);
        help_section(ui, "Panels", HELP_PANELS);
        help_row(ui, &[K("Tab")], "Hide the UI (stream view)");
        help_row(ui, &[K("F11")], "Fullscreen");
        help_row(ui, &[K("L")], "Species list");
        help_row(ui, &[K("G")], "Charts and family tree");
        help_row(ui, &[K("E")], "History");
        help_row(ui, &[K("N")], "Field notebook");
        help_row(ui, &[K("T")], "Event ticker");
        help_row(
            ui,
            &[K("M")],
            "Chemistry map: oxygen, CO2, nutrients, minerals, toxin, off",
        );
        help_row(ui, &[K("H"), Or, K("F1")], "This help");

        help_section(ui, "Trigger an event", HELP_EVENTS);
        help_row(ui, &[K("Ctrl"), Plus, K("1")], "Plague");
        help_row(ui, &[K("Ctrl"), Plus, K("2")], "Upwelling");
        help_row(ui, &[K("Ctrl"), Plus, K("3")], "Meteor");
        help_row(ui, &[K("Ctrl"), Plus, K("4")], "Rock wall");
    });

    help_section(ui, "Genome", HELP_GENOME);
    ui.label(
        "Each organism's genome is 60 bytes in nine chromosomes. Every byte is one gene; \
         the inspector shows each gene's value and what it does.",
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for c in &evo_core::genome::CHROMOSOMES {
            // Keep each letter with its name when the row wraps.
            ui.horizontal(|ui| {
                key_cap(ui, &c.letter.to_string(), false);
                ui.label(c.name);
            });
            ui.add_space(6.0);
        }
    });
    ui.add_space(4.0);
    help_row_width(
        ui,
        &[K("P")],
        "Holds up to eight body parts (type and symmetry, size, placement). Parts can be \
         copied, lost, or turn into related kinds, so body plans evolve.",
        30.0,
    );
    help_row_width(
        ui,
        &[K("G")],
        "Chemistry: breathing oxygen or fermenting, oxygen- or mineral-based photosynthesis, \
         nutrient uptake, nitrogen fixation, toxins and resistance.",
        30.0,
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label("Genes shown in ");
        ui.label(egui::RichText::new("orange").color(HELP_GENOME).strong());
        ui.label(" mutated in this organism (they differ from its parent).");
    });
}

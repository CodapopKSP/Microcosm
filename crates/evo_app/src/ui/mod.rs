//! egui panels: top bar, organism inspector (chromosomes), species list, events, charts, help.

mod charts;
mod inspector;
mod probe;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use evo_core::World;
use evo_core::stats::EventKind;
use evo_core::traits::{format_game_date, format_game_duration};

use crate::camera::CameraState;
use crate::sim::{SPEEDS, Selection, SimState};

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
    history_filter: [bool; 6],
    pub show_senses: bool,
    pub map_mode: MapMode,
    charts: charts::ChartState,
    fps: f64,
    /// The egui style has been set up (quicker tooltips).
    styled: bool,
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
            history_filter: [true; 6],
            show_senses: true,
            map_mode: MapMode::None,
            charts: charts::ChartState::default(),
            fps: 0.0,
            styled: false,
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
        EventKind::Milestone => egui::Color32::from_rgb(250, 210, 110),
        EventKind::Environment => egui::Color32::from_rgb(120, 190, 240),
        EventKind::Recolonization => egui::Color32::from_rgb(200, 160, 240),
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
    if !ui_state.styled {
        // Tooltips appear almost at once (the default waits half a second).
        ctx.all_styles_mut(|style| {
            style.interaction.tooltip_delay = 0.05;
            style.interaction.show_tooltips_only_when_still = false;
        });
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

    // egui 0.36 panels live inside a root Ui covering the screen.
    let mut root = egui::Ui::new(
        ctx.clone(),
        egui::Id::new("evo_root"),
        egui::UiBuilder::new(),
    );
    top_bar(&mut root, &mut ui_state, &mut sim, &mut cam, &mut selection);

    if ui_state.show_species {
        species_panel(&mut root, &sim.world, &mut selection, &mut cam);
    }

    if let Some(id) = selection.organism {
        egui::Panel::right("inspector")
            .resizable(true)
            .default_size(400.0)
            .show(&mut root, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    inspector::draw(ui, &sim.world, id, &mut selection);
                });
            });
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

    if ui_state.show_events {
        event_ticker(ctx, &sim.world, &mut selection, &mut cam);
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

    if ui_state.show_help {
        let mut open = true;
        egui::Window::new("Help")
            .open(&mut open)
            .collapsible(false)
            .show(ctx, help);
        ui_state.show_help = open;
    }
    Ok(())
}

fn top_bar(
    root: &mut egui::Ui,
    ui_state: &mut UiState,
    sim: &mut SimState,
    cam: &mut CameraState,
    selection: &mut Selection,
) {
    egui::Panel::top("top_bar").show(root, |ui| {
        ui.horizontal(|ui| {
            let world = &sim.world;
            ui.label(egui::RichText::new(clock_text(world)).strong());
            ui.separator();
            let c = &world.env.clock;
            ui.label(format!(
                "Sun {:.0}%",
                c.sun / world.config.light.surface_intensity * 100.0
            ));
            ui.label(format!("Surface {:.1} °C", c.surface_temp));
            let air = &world.env.atmosphere;
            ui.label(format!(
                "Air O₂ {:.0}% CO₂ {:.0}%",
                air.oxygen * 100.0,
                air.co2 * 100.0
            ))
            .on_hover_text(
                "Oxygen and CO₂ in the air, relative to today's Earth. Photosynthesis \
                     releases oxygen and uses CO₂; breathing and rotting do the opposite. \
                     More CO₂ warms the climate.",
            );
            if ui
                .selectable_label(ui_state.map_mode != MapMode::None, "Map")
                .on_hover_text(
                    "Show oxygen, CO₂, nutrients, minerals or toxin in the water (key M cycles)",
                )
                .clicked()
            {
                ui_state.map_mode = ui_state.map_mode.next();
            }
            if ui_state.map_mode != MapMode::None {
                ui.label(egui::RichText::new(ui_state.map_mode.label()).weak());
            }
            ui.separator();
            ui.label(format!("Population {}", world.organisms.len()));
            ui.label(format!("Species {}", world.species.living().count()));
            ui.separator();

            let speed_index = sim.speed_index;
            for (i, s) in SPEEDS.iter().enumerate() {
                let label = match *s {
                    0.0 => "⏸".to_string(),
                    0.5 => "½×".to_string(),
                    s => format!("{s:.0}×"),
                };
                if ui.selectable_label(speed_index == i, label).clicked() {
                    sim.speed_index = i;
                }
            }
            if !sim.paused() && sim.actual_speed < sim.speed() * 0.9 && sim.tps > 0.0 {
                ui.label(
                    egui::RichText::new(format!("(running {:.1}×)", sim.actual_speed))
                        .color(egui::Color32::from_rgb(230, 180, 90)),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("Menu")
                    .on_hover_text("Save, and go back to the main menu")
                    .clicked()
                {
                    ui_state.to_menu = true;
                }
                if ui.button("Help").clicked() {
                    ui_state.show_help = !ui_state.show_help;
                }
                if ui
                    .selectable_label(ui_state.show_history, "History")
                    .clicked()
                {
                    ui_state.show_history = !ui_state.show_history;
                }
                if ui
                    .selectable_label(ui_state.show_charts, "Charts")
                    .clicked()
                {
                    ui_state.show_charts = !ui_state.show_charts;
                }
                if ui
                    .selectable_label(ui_state.show_species, "Species")
                    .clicked()
                {
                    ui_state.show_species = !ui_state.show_species;
                }
                if ui.selectable_label(cam.director, "Auto camera").clicked() {
                    cam.director = !cam.director;
                }
                ui.label(
                    egui::RichText::new(format!(
                        "{:.0} fps · {:.0} ticks/s · sim {:.1} ms",
                        ui_state.fps, sim.tps, sim.sim_ms
                    ))
                    .weak(),
                );
                if let Some((msg, at)) = &sim.last_save_message
                    && at.elapsed().as_secs_f32() < 4.0
                {
                    ui.label(egui::RichText::new(msg).weak());
                }
            });
        });
    });
    // Active events get their own row so they never crowd the controls.
    if !sim.world.happenings.active.is_empty() {
        egui::Panel::top("events_bar").show(root, |ui| {
            ui.horizontal(|ui| {
                let world = &sim.world;
                ui.label(egui::RichText::new("Now:").weak());
                // Active events (click to look).
                let mut goto: Option<Vec2> = None;
                for e in &world.happenings.active {
                    use evo_core::events::WorldEvent;
                    let h = world.config.world.height;
                    let (text, color, target) = match e {
                        WorldEvent::Plague { species, .. } => (
                            format!("Plague: {}", world.species.name(*species)),
                            egui::Color32::from_rgb(255, 120, 60),
                            world
                                .organisms
                                .iter()
                                .find(|o| {
                                    world.species.established_ancestor(o.species) == Some(*species)
                                })
                                .map(|o| Vec2::new(o.pos.x, o.pos.y)),
                        ),
                        WorldEvent::ImpactWinter { .. } => (
                            format!(
                                "Impact winter ({:.0}% sun)",
                                world.happenings.sun_factor(world.time()) * 100.0
                            ),
                            egui::Color32::from_rgb(255, 120, 60),
                            None,
                        ),
                        WorldEvent::Upwelling { x, .. } => (
                            format!("Upwelling at x={x:.0}"),
                            egui::Color32::from_rgb(120, 190, 240),
                            Some(Vec2::new(*x, h - 500.0)),
                        ),
                        WorldEvent::Barrier { x, built, .. } => (
                            format!("Rock wall at x={x:.0}"),
                            egui::Color32::from_rgb(200, 170, 130),
                            Some(Vec2::new(*x, built.min(h) - 300.0)),
                        ),
                    };
                    let response = ui.add(
                        egui::Label::new(egui::RichText::new(text).color(color))
                            .sense(egui::Sense::click()),
                    );
                    if response.clicked() {
                        goto = target;
                    }
                }
                if let Some(p) = goto {
                    cam.director = false;
                    selection.follow = false;
                    cam.focus(p, Some(2.0));
                }
            });
        });
    }
}

fn species_panel(
    root: &mut egui::Ui,
    world: &World,
    selection: &mut Selection,
    cam: &mut CameraState,
) {
    egui::Panel::left("species")
        .resizable(true)
        .default_size(270.0)
        .show(root, |ui| {
            ui.heading("Species");
            let mut living: Vec<_> = world.species.living().collect();
            living.sort_by_key(|s| std::cmp::Reverse(s.population));
            let total = world
                .species
                .species
                .values()
                .filter(|s| s.established)
                .count();
            ui.label(
                egui::RichText::new(format!("{} living · {} ever named", living.len(), total))
                    .weak(),
            );
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for s in living.iter().take(200) {
                    let selected = selection.species == Some(s.id);
                    ui.horizontal(|ui| {
                        swatch(ui, s.color, 12.0);
                        // Main diet of the species (from its founder).
                        let t = evo_core::Traits::decode(&s.founder, &world.config);
                        let (rect, r) =
                            ui.allocate_exact_size(egui::vec2(6.0, 12.0), egui::Sense::hover());
                        ui.painter()
                            .rect_filled(rect, 1.0, diet_color(t.main_diet()));
                        r.on_hover_text(DIET_NAMES[t.main_diet()]);
                        let text = egui::RichText::new(&s.name);
                        let response = ui.selectable_label(selected, text);
                        if response.clicked() {
                            selection.species = if selected { None } else { Some(s.id) };
                        }
                        if response.double_clicked() {
                            // Jump to a member.
                            if let Some(o) = world.organisms.iter().find(|o| o.species == s.id) {
                                selection.organism = Some(o.id);
                                selection.follow = true;
                                cam.focus(
                                    Vec2::new(o.pos.x, o.pos.y),
                                    Some((o.radius / 10.0).clamp(0.2, 3.0)),
                                );
                            }
                        }
                        response.on_hover_ui(|ui| {
                            let age = (world.tick - s.founded) as f32 * world.config.time.dt;
                            ui.label(format!(
                                "Founded {} ago",
                                format_game_duration(age, &world.config.time)
                            ));
                            if let Some(p) = s.parent {
                                ui.label(format!("Descended from {}", world.species.name(p)));
                            }
                            ui.label(format!("Peak population {}", s.peak));
                            ui.label(
                                egui::RichText::new(s.founder.to_string())
                                    .monospace()
                                    .small(),
                            );
                            ui.label(
                                egui::RichText::new("Click to highlight, double-click to find one")
                                    .weak(),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(s.population.to_string());
                        });
                    });
                }
            });
        });
}

fn event_ticker(
    ctx: &egui::Context,
    world: &World,
    selection: &mut Selection,
    cam: &mut CameraState,
) {
    egui::Area::new(egui::Id::new("events"))
        .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(10.0, -10.0))
        .interactable(true)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(egui::Color32::from_black_alpha(160))
                .show(ui, |ui| {
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

const EVENT_KINDS: [(EventKind, &str); 6] = [
    (EventKind::Species, "New species"),
    (EventKind::Extinction, "Extinctions"),
    (EventKind::Milestone, "Milestones"),
    (EventKind::Disaster, "Disasters"),
    (EventKind::Environment, "Environment"),
    (EventKind::Recolonization, "Recolonization"),
];

/// Full event log with filters.
fn history(
    ui: &mut egui::Ui,
    world: &World,
    filter: &mut [bool; 6],
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

fn help(ui: &mut egui::Ui) {
    ui.label("Camera: drag (any mouse button) or WASD/arrows to pan, scroll to zoom, Home for the whole world.");
    ui.label("Click an organism to inspect it and follow it (F toggles follow, Esc clears).");
    ui.label("Right-click a spot to inspect its water: light, temperature, oxygen and the rest.");
    ui.label("Space: pause · 1–7: speed · +/-: faster/slower · F5: quicksave");
    ui.label(
        "Ctrl+1 plague · Ctrl+2 upwelling · Ctrl+3 meteor · Ctrl+4 rock wall (trigger events now)",
    );
    ui.label("Tab: hide the UI (stream view) · C: auto camera · L: species list · G: charts · E: history · T: event ticker · M: chemistry map · H: help");
    ui.separator();
    ui.label("Each organism's genome is 60 bytes in nine chromosomes (B, C, D, G, H, N, P, R, T).");
    ui.label(
        "P holds up to eight body parts (3 bytes each: type and symmetry, size, placement). \
         Parts can be copied, lost, or change into related kinds, so body plans evolve.",
    );
    ui.label(
        "G is chemistry: breathing oxygen or fermenting, oxygen- or mineral-based photosynthesis, \
         nutrient uptake, nitrogen fixation, toxins and resistance to them.",
    );
    ui.label("Every byte is one gene. The inspector shows each gene's value and what it does.");
    ui.label("Genes highlighted in orange mutated in this organism (they differ from its parent).");
}

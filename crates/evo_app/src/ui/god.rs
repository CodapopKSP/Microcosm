//! God powers: a hidden debug panel (Ctrl+Shift+G, not listed in the help). Place organisms
//! (the built-in presets and those saved in the genome editor), smite them, carry them about,
//! heal the selected one or the highlighted species, call off every event, and set up camera
//! moves for recording footage (readied with Ctrl+Shift+P, also with the UI hidden, and started
//! by unpausing the game).

use bevy::input::ButtonInput;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::input::EguiWantsInput;
use bevy_egui::{EguiContexts, egui};
use evo_core::genome::Genome;
use evo_core::traits::Traits;
use evo_core::events::WorldEvent;

use super::hud::{self, ACCENT, MUTED, TEXT, WARN};
use crate::camera::{CameraMove, CameraState, MainCamera};
use crate::sim::{Selection, SimState};

/// What a left click in the world does while the panel is open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    /// The usual: select an organism.
    #[default]
    Select,
    Place,
    Smite,
    Carry,
}

#[derive(Resource)]
pub struct GodMode {
    pub open: bool,
    pub tool: Tool,
    /// What can be placed: the built-in presets, then the player's saved organisms.
    choices: Vec<(String, Genome)>,
    choice: usize,
    count: u32,
    /// The organism being carried (Carry tool).
    carrying: Option<u64>,
    /// Where the left button went down (screen), to tell a click from a drag.
    press: Option<Vec2>,
    /// What the last power did.
    note: Option<String>,
}

impl Default for GodMode {
    fn default() -> Self {
        Self {
            open: false,
            tool: Tool::Select,
            choices: Vec::new(),
            choice: 0,
            count: 1,
            carrying: None,
            press: None,
            note: None,
        }
    }
}

/// Ctrl+Shift+G opens and closes the panel, Ctrl+Shift+P readies the camera move (it pauses the
/// game; unpausing starts the move), and Escape puts the tools down.
pub(super) fn god_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut god: ResMut<GodMode>,
    mut shot: ResMut<CameraMove>,
    mut sim: ResMut<SimState>,
    egui_input: Res<EguiWantsInput>,
) {
    if egui_input.wants_any_keyboard_input() {
        return;
    }
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if ctrl && shift && keys.just_pressed(KeyCode::KeyG) {
        god.open = !god.open;
        if god.open {
            reload_choices(&mut god, &sim.world.config);
        } else {
            god.tool = Tool::Select;
            god.carrying = None;
        }
    }
    if ctrl && shift && keys.just_pressed(KeyCode::KeyP) {
        ready_move(&mut shot, &mut sim);
    }
    if keys.just_pressed(KeyCode::Escape) {
        god.tool = Tool::Select;
        god.carrying = None;
    }
}

/// Ready the camera move: it waits on the start view, with the game paused, and starts when the
/// game is unpaused.
fn ready_move(shot: &mut CameraMove, sim: &mut SimState) {
    shot.ready_up();
    if shot.ready && !sim.paused() {
        sim.toggle_pause();
    }
}

fn reload_choices(god: &mut GodMode, config: &evo_core::Config) {
    let mut choices: Vec<(String, Genome)> = crate::menu::builtin_presets(config)
        .into_iter()
        .map(|(name, _, genome)| (name.to_string(), genome))
        .collect();
    choices.extend(crate::paths::list_organisms().into_iter().map(|o| (o.name, o.genome)));
    // Keep the same pick if it's still there.
    let picked = god.choices.get(god.choice).map(|c| c.0.clone());
    god.choice = picked
        .and_then(|name| choices.iter().position(|c| c.0 == name))
        .unwrap_or(0);
    god.choices = choices;
}

/// The world tools: place, smite or carry with the left button. Runs before the camera, and
/// takes the clicks it uses so they don't also select or pan.
pub(crate) fn god_pointer(
    mut god: ResMut<GodMode>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    egui_input: Res<EguiWantsInput>,
    mut sim: ResMut<SimState>,
    mut selection: ResMut<Selection>,
    cam: Res<CameraState>,
    ui_state: Res<super::UiState>,
    mut gizmos: Gizmos,
) {
    if !god.open || god.tool == Tool::Select || !ui_state.show_ui {
        return;
    }
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
    let pointer_free = !egui_input.wants_any_pointer_input();
    if mouse.just_pressed(MouseButton::Left) && pointer_free {
        god.press = screen;
    }
    let clicked = mouse.just_released(MouseButton::Left)
        && pointer_free
        && matches!((god.press.take(), screen), (Some(a), Some(b)) if a.distance(b) <= 4.0);
    let ring = |gizmos: &mut Gizmos, p: glam::Vec2, r: f32, color: Color| {
        gizmos.circle_2d(Isometry2d::from_translation(Vec2::new(p.x, p.y)), r, color);
    };
    let world = &mut sim.world;
    let slack = 6.0 * cam.zoom;
    let hovered = || world.pick(at, slack).and_then(|id| world.find(id));
    match god.tool {
        Tool::Select => {}
        Tool::Place => {
            let Some((name, genome)) = god.choices.get(god.choice).cloned() else {
                return;
            };
            let r = Traits::decode(&genome, &world.config).adult_radius;
            ring(&mut gizmos, at, r.max(4.0 * cam.zoom), Color::srgba(0.6, 0.95, 0.85, 0.8));
            if clicked {
                mouse.clear_just_released(MouseButton::Left);
                let placed = world.spawn_at(genome, at, god.count).len();
                god.note = Some(match placed {
                    0 => "No room there".to_string(),
                    1 => format!("Placed a {name}"),
                    n => format!("Placed {n} × {name}"),
                });
            }
        }
        Tool::Smite => {
            let target = hovered().map(|o| (o.id, o.pos, o.radius));
            if let Some((_, p, r)) = target {
                ring(&mut gizmos, p, r * 1.3 + 3.0 * cam.zoom, Color::srgba(1.0, 0.35, 0.3, 0.9));
            }
            if clicked {
                mouse.clear_just_released(MouseButton::Left);
                if let Some((id, _, _)) = target {
                    world.smite(&[id]);
                    if selection.organism == Some(id) {
                        selection.organism = None;
                        selection.follow = false;
                    }
                    god.note = Some(format!("Smote #{id}"));
                }
            }
        }
        Tool::Carry => {
            if god.carrying.is_none() {
                let target = hovered().map(|o| (o.id, o.pos, o.radius));
                if let Some((id, p, r)) = target {
                    ring(&mut gizmos, p, r * 1.3 + 3.0 * cam.zoom, Color::srgba(1.0, 0.9, 0.5, 0.8));
                    // Pick it up (and don't pan the view).
                    if mouse.just_pressed(MouseButton::Left) && pointer_free {
                        mouse.clear_just_pressed(MouseButton::Left);
                        god.carrying = Some(id);
                        selection.follow = false;
                    }
                }
            }
            if let Some(id) = god.carrying {
                world.move_organism(id, at);
                if let Some(o) = world.find(id) {
                    ring(&mut gizmos, o.pos, o.radius * 1.3 + 3.0 * cam.zoom, Color::srgba(1.0, 0.9, 0.5, 1.0));
                }
                if !mouse.pressed(MouseButton::Left) {
                    mouse.clear_just_released(MouseButton::Left);
                    god.carrying = None;
                    god.note = Some(format!("Put #{id} down"));
                }
            }
        }
    }
}

/// The panel itself.
pub(super) fn god_panel(
    mut contexts: EguiContexts,
    mut god: ResMut<GodMode>,
    mut sim: ResMut<SimState>,
    mut selection: ResMut<Selection>,
    mut cam: ResMut<CameraState>,
    mut shot: ResMut<CameraMove>,
    ui_state: Res<super::UiState>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) -> Result {
    if !god.open || !ui_state.show_ui {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let god = &mut *god;
    let mut ready_now = false;
    let world = &mut sim.world;
    let mut open = true;
    egui::Window::new("God powers")
        .title_bar(false)
        .frame(hud::card())
        .resizable(false)
        .default_pos(ctx.content_rect().left_top() + egui::vec2(320.0, 120.0))
        .show(ctx, |ui| {
            ui.set_width(300.0);
            ui.style_mut().visuals.override_text_color = Some(TEXT);
            open = header(ui);

            // What a click in the world does.
            section(ui, "Click in the world to");
            hud::tab_bar(
                ui,
                &mut god.tool,
                &[
                    (Tool::Select, "Select"),
                    (Tool::Place, "Place"),
                    (Tool::Smite, "Smite"),
                    (Tool::Carry, "Carry"),
                ],
            );
            ui.add_space(4.0);
            let hint = match god.tool {
                Tool::Select => "Clicks select organisms, as usual.",
                Tool::Place => "Click in the water to place them.",
                Tool::Smite => "Click an organism to kill it.",
                Tool::Carry => "Drag an organism to move it.",
            };
            ui.label(egui::RichText::new(hint).size(12.0).color(MUTED));
            if god.tool == Tool::Place {
                ui.add_space(4.0);
                if god.choices.is_empty() {
                    reload_choices(god, &world.config);
                }
                let name = god.choices.get(god.choice).map(|c| c.0.clone()).unwrap_or_default();
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("god_place")
                        .selected_text(name)
                        .width(190.0)
                        .show_ui(ui, |ui| {
                            let builtin = crate::menu::builtin_presets(&world.config).len();
                            for (i, (name, _)) in god.choices.iter().enumerate() {
                                if i == builtin {
                                    ui.separator();
                                }
                                ui.selectable_value(&mut god.choice, i, name);
                            }
                        });
                    ui.add(
                        egui::DragValue::new(&mut god.count)
                            .range(1..=100)
                            .speed(0.2)
                            .prefix("× "),
                    );
                });
                if god.choices.len() <= crate::menu::builtin_presets(&world.config).len() {
                    ui.label(
                        egui::RichText::new("Organisms you save in the genome editor show up here too.")
                            .size(12.0)
                            .color(MUTED),
                    );
                }
            }

            // The selected organism, and the highlighted species.
            section(ui, "Selected");
            let picked = selection
                .organism
                .and_then(|id| world.find(id))
                .map(|o| (o.id, world.species.name(o.species).to_string()));
            match picked {
                Some((id, name)) => {
                    ui.label(format!("{name}  #{id}"));
                    ui.horizontal(|ui| {
                        if button(ui, "Full health & energy", false).clicked() {
                            world.heal(&[id]);
                            god.note = Some(format!("Healed #{id}"));
                        }
                        if button(ui, "Kill", true).clicked() {
                            world.smite(&[id]);
                            selection.organism = None;
                            selection.follow = false;
                            god.note = Some(format!("Smote #{id}"));
                        }
                    });
                }
                None => {
                    ui.label(egui::RichText::new("Nothing selected").color(MUTED));
                }
            }
            if let Some(s) = selection.species {
                let members: Vec<u64> = world
                    .organisms
                    .iter()
                    .filter(|o| o.species == s)
                    .map(|o| o.id)
                    .collect();
                ui.add_space(4.0);
                ui.label(format!("{} (highlighted): {}", world.species.name(s), members.len()));
                ui.horizontal(|ui| {
                    if button(ui, "Heal them all", false).clicked() {
                        let n = world.heal(&members);
                        god.note = Some(format!("Healed {n}"));
                    }
                    if button(ui, "Kill them all", true).clicked() {
                        let n = world.smite(&members);
                        god.note = Some(format!("Smote {n}"));
                    }
                });
            }

            // The world's events.
            section(ui, "Events");
            let active: Vec<&str> = world
                .happenings
                .active
                .iter()
                .map(|e| match e {
                    WorldEvent::Plague { .. } => "plague",
                    WorldEvent::Upwelling { .. } => "upwelling",
                    WorldEvent::ImpactWinter { .. } => "impact winter",
                    WorldEvent::Barrier { .. } => "rock wall",
                })
                .collect();
            let text = if active.is_empty() {
                "None going on".to_string()
            } else {
                format!("Going on: {}", active.join(", "))
            };
            ui.label(egui::RichText::new(text).color(MUTED));
            ui.add_enabled_ui(!active.is_empty() || !world.happenings.pending.is_empty(), |ui| {
                if button(ui, "Call them all off", false).clicked() {
                    let n = world.call_off_events();
                    god.note = Some(format!("Called off {n}"));
                }
            });

            // A camera move for recording.
            section(ui, "Camera move");
            let view = (cam.center, cam.zoom);
            ui.horizontal(|ui| {
                if button(ui, "Set start", false).on_hover_text("This view").clicked() {
                    shot.start = Some(view);
                }
                if button(ui, "Set end", false).on_hover_text("This view").clicked() {
                    shot.end = Some(view);
                }
                ui.label(
                    egui::RichText::new(match (shot.start.is_some(), shot.end.is_some()) {
                        (true, true) => "both set",
                        (true, false) => "start set",
                        (false, true) => "end set",
                        (false, false) => "",
                    })
                    .size(12.0)
                    .color(MUTED),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Glide");
                ui.add(
                    egui::DragValue::new(&mut shot.duration)
                        .range(0.5..=60.0)
                        .speed(0.05)
                        .suffix(" s"),
                );
                ui.label("after holding");
                ui.add(
                    egui::DragValue::new(&mut shot.lead)
                        .range(0.0..=10.0)
                        .speed(0.05)
                        .suffix(" s"),
                );
            });
            ui.horizontal(|ui| {
                let ready = shot.start.is_some() && shot.end.is_some();
                ui.add_enabled_ui(shot.start.is_some(), |ui| {
                    if button(ui, "Go to start", false).clicked()
                        && let Some((c, z)) = shot.start
                    {
                        cam.set_now(c, z);
                        cam.director = false;
                        selection.follow = false;
                    }
                });
                ui.add_enabled_ui(ready, |ui| {
                    let label = if shot.ready {
                        "Ready: unpause to start"
                    } else if shot.playing.is_some() {
                        "Playing…"
                    } else {
                        "Ready"
                    };
                    if button(ui, label, false).clicked() {
                        ready_now = true;
                    }
                });
            });
            // The window's size: its content, not counting the title bar.
            if let Ok(mut window) = windows.single_mut() {
                let (w, h) = (window.physical_width(), window.physical_height());
                ui.horizontal(|ui| {
                    ui.label(format!("Window {w}×{h}"));
                    for (label, size) in [("1080p", (1920, 1080)), ("1440p", (2560, 1440))] {
                        if button(ui, label, false)
                            .on_hover_text(format!("{}×{} (un-maximize it first)", size.0, size.1))
                            .clicked()
                        {
                            window.resolution.set_physical_resolution(size.0, size.1);
                        }
                    }
                });
            }
            ui.label(
                egui::RichText::new(
                    "Ready (Ctrl+Shift+P, also with the UI hidden) jumps to the start and pauses; \
                     unpausing (Space) starts the move, which pauses with the game. Escape cancels.",
                )
                .size(12.0)
                .color(MUTED),
            );

            if let Some(note) = &god.note {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(note).size(12.0).color(ACCENT));
            }
        });
    if ready_now {
        ready_move(&mut shot, &mut sim);
    }
    if !open {
        god.open = false;
        god.tool = Tool::Select;
        god.carrying = None;
    }
    Ok(())
}

/// Title, a reminder of the key, and a close button. False when closed.
fn header(ui: &mut egui::Ui) -> bool {
    let mut open = true;
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
        hud::icon_bolt(ui.painter(), r.center(), 10.0, WARN);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.label(egui::RichText::new("God powers").size(17.0).strong());
            ui.label(egui::RichText::new("Debug tools · Ctrl+Shift+G").size(12.0).color(MUTED));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if hud::round_button(ui, false, 14.0, hud::icon_close).clicked() {
                open = false;
            }
        });
    });
    open
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(egui::RichText::new(title.to_uppercase()).size(11.0).color(MUTED));
    ui.add_space(2.0);
}

/// A small rounded button; `danger` ones turn red on hover.
fn button(ui: &mut egui::Ui, text: &str, danger: bool) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        egui::FontId::proportional(13.5),
        TEXT,
    );
    let size = egui::vec2(galley.size().x + 22.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    crate::ui_sfx::hover_sound(&response);
    let enabled = ui.is_enabled();
    let hovered = response.hovered() && enabled;
    let tint = if danger { egui::Color32::from_rgb(240, 110, 95) } else { ACCENT };
    let p = ui.painter();
    p.rect(
        rect,
        9.0,
        if hovered {
            egui::Color32::from_rgb(32, 49, 56)
        } else {
            egui::Color32::from_rgb(22, 32, 36)
        },
        egui::Stroke::new(1.0, tint.gamma_multiply(if hovered { 0.7 } else { 0.25 })),
        egui::StrokeKind::Inside,
    );
    let color = match (enabled, hovered) {
        (false, _) => hud::DIM,
        (true, true) => tint,
        (true, false) => TEXT,
    };
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(13.5),
        color,
    );
    response
}

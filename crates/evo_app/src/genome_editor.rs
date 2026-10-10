//! The genome editor: build an organism gene by gene (sliders, typed values, part pickers), see
//! it drawn live by the game's own renderer, and save it as a starting preset for new worlds
//! (in the player's organisms folder; see `paths.rs`). The organism sits alone in a small,
//! paused world, under a camera framed on it.

use std::time::Instant;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use egui::{Color32, vec2};
use evo_core::genome::{CHROMOSOMES, Chromosome, GeneKind};
use evo_core::parts::{self, BodyShape, PartKind, slot_offset};
use evo_core::rng::Rng;
use evo_core::traits::describe_gene;
use evo_core::{Config, Genome, Organism, Traits, World};

use crate::AppState;
use crate::camera::CameraState;
use crate::editor::{clean_name, hint, icon_back, icon_curl, icon_fit, round_icon};
use crate::menu::{ACCENT, ERROR, MUTED, TEXT, caption, card, rgba, style_menu};
use crate::paths::SavedOrganism;
use crate::sim::SimState;

pub struct GenomeEditorPlugin;

impl Plugin for GenomeEditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GenomeEditor>()
            .init_resource::<EditGenome>()
            .add_systems(
                OnEnter(AppState::GenomeEditor),
                enter.before(crate::render::WorldViewSetup),
            )
            .add_systems(
                EguiPrimaryContextPass,
                draw.run_if(in_state(AppState::GenomeEditor))
                    .run_if(resource_exists::<SimState>)
                    .run_if(resource_exists::<CameraState>),
            );
    }
}

/// An organism to open the editor with: (genome, name, description). None keeps what the
/// editor had last.
#[derive(Resource, Default)]
pub struct EditGenome(pub Option<(Genome, String, String)>);

/// Most undo steps kept.
const UNDO_STEPS: usize = 60;
/// Changes closer together than this (seconds) are one undo step (a slider drag).
const UNDO_MERGE: f32 = 0.6;
/// Width of the genes panel and of the about panel.
const GENES_W: f32 = 560.0;
const ABOUT_W: f32 = 340.0;
/// The organism's id in the preview world (changed on every edit).
const FIRST_ID: u64 = 1;

#[derive(Resource)]
pub struct GenomeEditor {
    genome: Genome,
    name: String,
    description: String,
    undo: Vec<Genome>,
    redo: Vec<Genome>,
    last_change: Option<Instant>,
    unsaved: bool,
    /// The whole genome as text (for copy and paste), and what's wrong with it if anything.
    text: String,
    text_error: Option<String>,
    /// Extra zoom from the mouse wheel (1 = framed to fit).
    zoom: f32,
    save_open: bool,
    /// A saved organism waiting for a second click to be deleted.
    confirm_delete: Option<String>,
    saved: Vec<SavedOrganism>,
    message: Option<(String, bool, Instant)>,
    next_id: u64,
    leave: bool,
}

impl Default for GenomeEditor {
    fn default() -> Self {
        let config = crate::paths::load_config(None, None).unwrap_or_default();
        let genome = Genome::parse(&config.population.ancestor).unwrap_or_default();
        GenomeEditor {
            genome,
            name: "My organism".into(),
            description: String::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            last_change: None,
            unsaved: false,
            text: genome.to_string(),
            text_error: None,
            zoom: 1.0,
            save_open: false,
            confirm_delete: None,
            saved: Vec::new(),
            message: None,
            next_id: FIRST_ID,
            leave: false,
        }
    }
}

impl GenomeEditor {
    fn say(&mut self, text: impl Into<String>, error: bool) {
        self.message = Some((text.into(), error, Instant::now()));
    }

    /// Start over with another organism (not an undoable edit).
    fn load(&mut self, genome: Genome, name: &str, description: &str) {
        self.genome = genome;
        self.name = name.to_string();
        self.description = description.to_string();
        self.undo.clear();
        self.redo.clear();
        self.last_change = None;
        self.unsaved = false;
        self.text = genome.to_string();
        self.text_error = None;
        self.zoom = 1.0;
    }

    fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.genome, prev));
            self.after_history();
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.genome, next));
            self.after_history();
        }
    }

    fn after_history(&mut self) {
        self.last_change = None;
        self.unsaved = true;
        self.text = self.genome.to_string();
        self.text_error = None;
    }

    /// Note an edit: `before` is the genome as it was. Quick successive changes (dragging a
    /// slider) make one undo step.
    fn record(&mut self, before: Genome) {
        let merge = self
            .last_change
            .is_some_and(|t| t.elapsed().as_secs_f32() < UNDO_MERGE);
        if !merge {
            self.undo.push(before);
            if self.undo.len() > UNDO_STEPS {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last_change = Some(Instant::now());
        self.unsaved = true;
    }

    fn save(&mut self) -> bool {
        let name = clean_name(&self.name);
        if name.is_empty() {
            self.say("Give it a name first", true);
            return false;
        }
        let o = SavedOrganism {
            name: name.clone(),
            description: self.description.trim().to_string(),
            genome: self.genome,
        };
        match crate::paths::save_organism(&o) {
            Ok(_) => {
                self.name = name.clone();
                self.unsaved = false;
                self.saved = crate::paths::list_organisms();
                self.say(
                    format!("Saved \"{name}\": pick it under Preset on the New world screen"),
                    false,
                );
                true
            }
            Err(e) => {
                self.say(format!("Couldn't save: {e}"), true);
                false
            }
        }
    }
}

/// The preview world: the menu's water, with no events or vents of its own and no life but the
/// organism being edited.
fn preview_world() -> Option<World> {
    let mut config = crate::paths::load_config(None, None)
        .or_else(|_| Config::from_toml(""))
        .ok()?;
    config.events.enabled = false;
    config.vents.count = 0;
    config.population.initial = 0;
    config.population.vent_ancestor = String::new();
    config.validate().ok()?;
    let mut world = World::new(config);
    // A couple of steps so the light (and the water's fields) are worked out: it's paused after.
    for _ in 0..3 {
        world.step();
    }
    Some(world)
}

/// Where the organism floats: in sunlit open water near the surface, mid-ocean (as the menu's
/// backdrop).
fn spot(world: &World) -> Vec2 {
    let c = &world.config.world;
    Vec2::new(c.width * 0.5, c.height - 230.0)
}

/// Put the organism (alone, fully grown, facing right) in the preview world.
fn place(world: &mut World, genome: &Genome, id: u64) {
    let traits = Traits::decode(genome, &world.config);
    let mut o = Organism::new(
        id,
        0,
        0,
        0,
        *genome,
        0,
        &world.config,
        spot(world),
        traits.adult_mass,
        traits.adult_mass,
        Rng::new(id),
        world.tick,
    );
    o.heading = 0.0;
    o.steer_heading = 0.0;
    world.organisms.clear();
    world.organisms.push(o);
}

fn enter(
    mut commands: Commands,
    mut ed: ResMut<GenomeEditor>,
    mut request: ResMut<EditGenome>,
    launch: Res<crate::Launch>,
) {
    if let Some((genome, name, description)) = request.0.take() {
        ed.load(genome, &name, &description);
    }
    ed.saved = crate::paths::list_organisms();
    ed.save_open = false;
    ed.confirm_delete = None;
    ed.leave = false;
    let Some(mut world) = preview_world() else {
        return;
    };
    ed.next_id += 1;
    place(&mut world, &ed.genome, ed.next_id);
    let at = spot(&world);
    let mut state = SimState::new(world, &launch.0).without_autosave();
    state.speed_index = 0; // paused: it stays put, while its parts still move
    commands.insert_resource(state);
    commands.insert_resource(CameraState::fixed(at, 0.1));
}

/// How far the organism reaches from its center, in body radii (body and parts).
fn reach_radii(genome: &Genome) -> f32 {
    let body = BodyShape::from_genome(genome);
    let mut out = Vec::new();
    parts::layout(&parts::decode_parts(genome), &body, &mut out);
    out.iter()
        .map(|i| i.base.length() + i.length)
        .fold(body.half_length.max(body.half_width).max(1.0), f32::max)
}

fn draw(
    mut contexts: EguiContexts,
    mut editor: ResMut<GenomeEditor>,
    mut state: ResMut<SimState>,
    mut cam: ResMut<CameraState>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut next: ResMut<NextState<AppState>>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let ed = &mut *editor;
    if ed.leave {
        next.set(AppState::Menu);
        return Ok(());
    }
    let before = ed.genome;
    let mut history: Option<bool> = None;
    shortcuts(&ctx, ed, &mut history);

    let screen = ctx.content_rect();
    let config = state.world.config.clone();
    let traits = Traits::decode(&ed.genome, &config);

    // Left: the genes.
    egui::Area::new(egui::Id::new("genome_genes"))
        .fixed_pos(screen.left_top() + vec2(14.0, 14.0))
        .show(&ctx, |ui| {
            style_menu(ui);
            ui.set_width(GENES_W);
            card().show(ui, |ui| {
                ui.set_width(GENES_W - 32.0);
                ui.set_max_width(GENES_W - 32.0);
                header(ui, ed, &mut history, &config);
                let height = (screen.height() - 190.0).max(200.0);
                egui::ScrollArea::vertical()
                    .min_scrolled_height(height)
                    .max_height(height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| genes_panel(ui, ed, &traits, &config));
            });
        });

    // Right: what it is and can do (the game's inspector, on the preview organism).
    egui::Area::new(egui::Id::new("genome_about"))
        .fixed_pos(screen.right_top() + vec2(-14.0 - ABOUT_W, 14.0))
        .show(&ctx, |ui| {
            style_menu(ui);
            ui.set_width(ABOUT_W);
            card().show(ui, |ui| {
                ui.set_width(ABOUT_W - 32.0);
                about_panel(ui, &state.world, &traits);
            });
        });

    if ed.save_open {
        save_dialog(&ctx, ed);
    }
    if let Some((text, error, at)) = &ed.message
        && at.elapsed().as_secs_f32() < 5.0
    {
        egui::Area::new(egui::Id::new("genome_message"))
            .fixed_pos(screen.center_bottom() + vec2(0.0, -20.0))
            .pivot(egui::Align2::CENTER_BOTTOM)
            .show(&ctx, |ui| {
                style_menu(ui);
                card().show(ui, |ui| {
                    ui.label(egui::RichText::new(text).color(if *error { ERROR } else { ACCENT }));
                });
            });
    }

    // An edit: an undo step, the text kept in step, and the organism redrawn.
    if ed.genome != before {
        ed.record(before);
        ed.text = ed.genome.to_string();
        ed.text_error = None;
    }
    match history {
        Some(false) => ed.undo(),
        Some(true) => ed.redo(),
        None => {}
    }
    if state.world.organisms.first().map(|o| o.genome) != Some(ed.genome) {
        ed.next_id += 1;
        place(&mut state.world, &ed.genome, ed.next_id);
    }

    // The mouse wheel over the organism zooms.
    if !ctx.is_pointer_over_egui() {
        let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            ed.zoom = (ed.zoom * (-scroll * 0.003).exp()).clamp(0.15, 6.0);
        }
    }
    // Frame it in the space between the panels.
    if let (Ok(window), Some(o)) = (windows.single(), state.world.organisms.first()) {
        let (w, h) = (window.width(), window.height());
        let left = GENES_W + 28.0;
        let right = ABOUT_W + 28.0;
        let free_w = (w - left - right).max(200.0);
        let across = 2.0 * reach_radii(&o.genome) * o.radius;
        let zoom = (across / (0.5 * free_w.min(h))).max(0.01) * ed.zoom;
        let offset = (left + free_w * 0.5) - w * 0.5;
        cam.focus(o.pos - Vec2::new(offset * zoom, 0.0), Some(zoom));
    }
    Ok(())
}

fn shortcuts(ctx: &egui::Context, ed: &mut GenomeEditor, history: &mut Option<bool>) {
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    ctx.input(|i| {
        let cmd = i.modifiers.command;
        if cmd && !i.modifiers.shift && i.key_pressed(egui::Key::Z) {
            *history = Some(false);
        }
        if cmd && (i.key_pressed(egui::Key::Y) || (i.modifiers.shift && i.key_pressed(egui::Key::Z))) {
            *history = Some(true);
        }
        if cmd && i.key_pressed(egui::Key::S) {
            ed.save_open = true;
        }
        if i.key_pressed(egui::Key::Escape) {
            ed.save_open = false;
        }
    });
}

/// Title, file buttons and history.
fn header(ui: &mut egui::Ui, ed: &mut GenomeEditor, history: &mut Option<bool>, config: &Config) {
    ui.horizontal(|ui| {
        if round_icon(ui, true, icon_back).on_hover_text("Back to the menu").clicked() {
            ed.leave = true;
        }
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(egui::RichText::new("Genome editor").size(20.0).color(TEXT));
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&ed.name).size(12.5).color(ACCENT));
                if ed.unsaved {
                    ui.label(egui::RichText::new("· unsaved").size(12.5).color(MUTED));
                }
            });
        });
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if ui
            .add(file_button("Save", false))
            .on_hover_text("Save as a starting organism for new worlds (Ctrl+S)")
            .clicked()
        {
            ed.save_open = true;
        }
        let (r, _) = egui::containers::menu::MenuButton::from_button(file_button("New", true)).ui(ui, |ui| {
            ui.label(egui::RichText::new("Start from").size(12.0).color(MUTED));
            for (name, about, genome) in crate::menu::builtin_presets(config) {
                if ui.button(name).on_hover_text(about).clicked() {
                    ed.load(genome, name, about);
                    ui.close();
                }
            }
        });
        chevron(ui, &r);
        r.on_hover_text("Start over from a built-in organism");
        let (r, _) = egui::containers::menu::MenuButton::from_button(file_button("Open", true))
            .ui(ui, |ui| open_menu(ui, ed));
        chevron(ui, &r);
        r.on_hover_text("Open one of your saved organisms");
        ui.add_space(6.0);
        if round_icon(ui, !ed.undo.is_empty(), |p, c, col| icon_curl(p, c, col, false))
            .on_hover_text("Undo (Ctrl+Z)")
            .clicked()
        {
            *history = Some(false);
        }
        if round_icon(ui, !ed.redo.is_empty(), |p, c, col| icon_curl(p, c, col, true))
            .on_hover_text("Redo (Ctrl+Y)")
            .clicked()
        {
            *history = Some(true);
        }
        if round_icon(ui, true, icon_dice)
            .on_hover_text("Mutate: change it a little, as a birth might")
            .clicked()
        {
            mutate(ed, config);
        }
        if round_icon(ui, true, icon_fit).on_hover_text("Fit it in view").clicked() {
            ed.zoom = 1.0;
        }
    });
    hint(ui, "Scroll over the organism to zoom.");
    ui.add_space(4.0);
}

/// A file button (Save, New, Open): all the same size, those that open a list leaving room
/// for a chevron.
fn file_button(text: &str, list: bool) -> egui::Button<'_> {
    let label = if list { format!("{text}    ") } else { text.to_string() };
    egui::Button::new(egui::RichText::new(label).size(15.0)).min_size(vec2(84.0, 32.0))
}

/// A small down-pointing chevron at the right of a button (it opens a list).
fn chevron(ui: &egui::Ui, r: &egui::Response) {
    let c = r.rect.right_center() + vec2(-15.0, 1.0);
    let color = if r.hovered() { ACCENT } else { MUTED };
    let stroke = egui::Stroke::new(1.6, color);
    ui.painter().line_segment([c + vec2(-4.0, -2.0), c + vec2(0.0, 2.0)], stroke);
    ui.painter().line_segment([c + vec2(0.0, 2.0), c + vec2(4.0, -2.0)], stroke);
}

/// The player's saved organisms, to open or delete.
fn open_menu(ui: &mut egui::Ui, ed: &mut GenomeEditor) {
    if ed.saved.is_empty() {
        ui.label(egui::RichText::new("Nothing saved yet").color(MUTED));
        return;
    }
    let mut open: Option<SavedOrganism> = None;
    let mut delete: Option<String> = None;
    for o in &ed.saved {
        ui.horizontal(|ui| {
            let r = ui.add_sized([200.0, 24.0], egui::Button::new(&o.name));
            if r.clicked() {
                open = Some(o.clone());
            }
            if !o.description.is_empty() {
                r.on_hover_text(&o.description);
            }
            if ed.confirm_delete.as_deref() == Some(o.name.as_str()) {
                if ui.button(egui::RichText::new("Delete").color(ERROR)).clicked() {
                    delete = Some(o.name.clone());
                }
            } else if ui.small_button("✕").on_hover_text("Delete").clicked() {
                ed.confirm_delete = Some(o.name.clone());
            }
        });
    }
    if let Some(o) = open {
        ed.load(o.genome, &o.name, &o.description);
        ed.confirm_delete = None;
        ui.close();
    }
    if let Some(name) = delete {
        match crate::paths::delete_organism(&name) {
            Ok(()) => ed.say(format!("Deleted \"{name}\""), false),
            Err(e) => ed.say(format!("Couldn't delete: {e}"), true),
        }
        ed.saved = crate::paths::list_organisms();
        ed.confirm_delete = None;
    }
}

/// Change the genome as a birth might (trying again until something actually changes).
fn mutate(ed: &mut GenomeEditor, config: &Config) {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    let mut rng = Rng::new(seed);
    let before = ed.genome;
    for _ in 0..50 {
        ed.genome.mutate(&mut rng, &config.mutation);
        if ed.genome != before {
            break;
        }
    }
}

/// A die (mutate).
fn icon_dice(p: &egui::Painter, c: egui::Pos2, color: Color32) {
    let r = egui::Rect::from_center_size(c, vec2(13.0, 13.0));
    p.rect_stroke(r, 3.0, egui::Stroke::new(1.5, color), egui::StrokeKind::Middle);
    for (dx, dy) in [(-3.0, -3.0), (0.0, 0.0), (3.0, 3.0)] {
        p.circle_filled(c + vec2(dx, dy), 1.3, color);
    }
}

/// Every chromosome: plain genes as sliders with typed values, the parts as pickers, and the
/// whole genome as text.
fn genes_panel(ui: &mut egui::Ui, ed: &mut GenomeEditor, traits: &Traits, config: &Config) {
    ui.horizontal(|ui| {
        crate::ui::swatch(ui, traits.color, 16.0);
        let parts = match traits.part_count {
            0 => "no body parts".to_string(),
            1 => "1 body part".to_string(),
            n => format!("{n} body parts"),
        };
        ui.label(
            egui::RichText::new(format!(
                "Adult radius {:.1}  ·  {}  ·  {}",
                traits.adult_radius,
                if traits.is_motile() { "can move" } else { "can't move" },
                parts
            ))
            .size(14.0),
        );
    });
    for c in CHROMOSOMES.iter() {
        let open = matches!(c.letter, 'B' | 'D' | 'P');
        egui::CollapsingHeader::new(
            egui::RichText::new(format!("{}  {}", c.letter, c.name)).size(15.0).color(TEXT),
        )
        .id_salt(("chromosome", c.letter))
        .default_open(open)
        .show(ui, |ui| {
            if c.letter == 'P' {
                parts_section(ui, &mut ed.genome);
            } else {
                gene_rows(ui, c, &mut ed.genome, traits, config);
            }
        });
    }

    caption(ui, "Whole genome");
    hint(ui, "Copy it to share, or paste one in.");
    ui.horizontal(|ui| {
        if ui.button("Copy").clicked() {
            ui.ctx().copy_text(ed.genome.to_string());
            ed.say("Genome copied", false);
        }
    });
    let edit = ui.add(
        egui::TextEdit::multiline(&mut ed.text)
            .font(egui::TextStyle::Monospace)
            .desired_rows(3)
            .desired_width(f32::INFINITY),
    );
    if edit.changed() {
        match Genome::parse(&ed.text) {
            Ok(g) => {
                ed.genome = g;
                ed.text_error = None;
            }
            Err(e) => ed.text_error = Some(e),
        }
    }
    if let Some(e) = &ed.text_error {
        ui.colored_label(ERROR, format!("Can't read this genome: {e}"));
    }
}

/// A chromosome's genes: name, slider, the value in hex (type to set it) and what it means.
fn gene_rows(ui: &mut egui::Ui, c: &Chromosome, genome: &mut Genome, traits: &Traits, config: &Config) {
    egui::Grid::new(("genes", c.letter))
        .num_columns(4)
        .spacing([10.0, 5.0])
        .show(ui, |ui| {
            for (k, info) in c.genes.iter().enumerate() {
                let index = c.start + k;
                ui.label(egui::RichText::new(info.name).size(13.5)).on_hover_text(format!(
                    "Gene {}{}{}",
                    c.letter,
                    k,
                    if info.kind == GeneKind::Signed { " (80 = neutral)" } else { "" }
                ));
                let mut v = genome.0[index];
                ui.spacing_mut().slider_width = 150.0;
                ui.add(egui::Slider::new(&mut v, 0..=255).show_value(false));
                ui.add(
                    egui::DragValue::new(&mut v)
                        .range(0..=255)
                        .hexadecimal(2, false, true)
                        .speed(0.5),
                )
                .on_hover_text("Drag, or click and type a value (hex, 00–FF)");
                genome.0[index] = v;
                ui.label(
                    egui::RichText::new(describe_gene(index, genome, traits, config))
                        .size(12.5)
                        .color(MUTED),
                );
                ui.end_row();
            }
        });
}

const SYMMETRY: [&str; 6] = ["Single", "Pair", "Ring of 3", "Ring of 4", "Ring of 5", "Ring of 6"];

/// Where a part sits: segment 1–6, every segment, or on the previous part.
fn place_name(where_: u8) -> String {
    match where_ {
        0..=5 => format!("Segment {}", where_ + 1),
        6 => "Every segment".into(),
        _ => "On previous part".into(),
    }
}

fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// The eight part slots, each a kind and symmetry, a size, and where and which way it sits.
fn parts_section(ui: &mut egui::Ui, genome: &mut Genome) {
    hint(ui, "Up to eight kinds of part. Angles are measured from the front (0°), clockwise round to the back (180°).");
    for k in 0..parts::SLOTS {
        let o = slot_offset(k);
        let kind_byte = genome.0[o];
        let kind = PartKind::from_code(kind_byte);
        let sym = (kind_byte >> 4).min(5);
        egui::Frame::NONE
            .fill(rgba(150, 240, 210, if kind.is_some() { 0.06 } else { 0.02 }))
            .corner_radius(8)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("P{}", k + 1)).monospace().color(ACCENT));
                    let mut new_kind = kind;
                    egui::ComboBox::from_id_salt(("part_kind", k))
                        .width(150.0)
                        .selected_text(kind.map(|p| capitalized(p.name())).unwrap_or("Empty".into()))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut new_kind, None, "Empty");
                            for p in PartKind::ALL {
                                ui.selectable_value(&mut new_kind, Some(p), capitalized(p.name()))
                                    .on_hover_text(p.role());
                            }
                        });
                    if new_kind != kind {
                        genome.0[o] = match new_kind {
                            Some(p) => sym << 4 | p.code(),
                            None => 0,
                        };
                        // A fresh part gets a middling size so it shows.
                        if kind.is_none() && genome.0[o + 1] == 0 {
                            genome.0[o + 1] = 0x80;
                        }
                    }
                    if let Some(p) = new_kind {
                        let mut s = sym;
                        egui::ComboBox::from_id_salt(("part_sym", k))
                            .width(100.0)
                            .selected_text(SYMMETRY[s as usize])
                            .show_ui(ui, |ui| {
                                for (i, name) in SYMMETRY.iter().enumerate() {
                                    ui.selectable_value(&mut s, i as u8, *name);
                                }
                            });
                        if s != sym {
                            genome.0[o] = s << 4 | p.code();
                        }
                    }
                });
                let Some(p) = PartKind::from_code(genome.0[o]) else {
                    return;
                };
                ui.add(egui::Label::new(egui::RichText::new(capitalized(p.role())).size(12.0).color(MUTED)).wrap());
                egui::Grid::new(("part", k)).num_columns(3).spacing([10.0, 4.0]).show(ui, |ui| {
                    ui.label("Size");
                    let mut size = genome.0[o + 1];
                    ui.spacing_mut().slider_width = 150.0;
                    ui.add(egui::Slider::new(&mut size, 0..=255).show_value(false));
                    ui.add(egui::DragValue::new(&mut size).range(0..=255).hexadecimal(2, false, true).speed(0.5));
                    genome.0[o + 1] = size;
                    ui.end_row();

                    let place = genome.0[o + 2];
                    let (mut where_, mut angle) = (place >> 5, place & 31);
                    ui.label("Place");
                    egui::ComboBox::from_id_salt(("part_where", k))
                        .width(150.0)
                        .selected_text(place_name(where_))
                        .show_ui(ui, |ui| {
                            for w in 0..8u8 {
                                ui.selectable_value(&mut where_, w, place_name(w));
                            }
                        });
                    ui.label("");
                    ui.end_row();

                    ui.label("Angle");
                    ui.add(egui::Slider::new(&mut angle, 0..=31).show_value(false));
                    let deg = angle as i32 * 360 / 32;
                    let deg = if deg > 180 { deg - 360 } else { deg };
                    ui.label(egui::RichText::new(format!("{deg:+}°")).size(12.5).color(MUTED));
                    ui.end_row();
                    genome.0[o + 2] = where_ << 5 | angle;
                });
            });
        ui.add_space(4.0);
    }
}

/// What the organism is and can do: the game's own inspector readout for it.
fn about_panel(ui: &mut egui::Ui, world: &World, traits: &Traits) {
    ui.label(egui::RichText::new("This organism").size(18.0).color(TEXT));
    let diet = crate::ui::DIET_NAMES[traits.main_diet()];
    ui.label(egui::RichText::new(format!("Mainly {}", diet.to_lowercase())).size(13.0).color(MUTED));
    if let Some(o) = world.organisms.first() {
        crate::ui::inspector::body(ui, world, o);
    }
}

/// Save it under a name, with a description for the New world screen.
fn save_dialog(ctx: &egui::Context, ed: &mut GenomeEditor) {
    egui::Area::new(egui::Id::new("genome_save"))
        .fixed_pos(ctx.content_rect().center())
        .pivot(egui::Align2::CENTER_CENTER)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            style_menu(ui);
            card().show(ui, |ui| {
                ui.set_width(380.0);
                ui.label(egui::RichText::new("Save organism").size(20.0).color(TEXT));
                caption(ui, "Name");
                let name = ui.add(egui::TextEdit::singleline(&mut ed.name).desired_width(f32::INFINITY));
                let file_name = clean_name(&ed.name);
                if crate::paths::organism_exists(&file_name) {
                    hint(ui, "You have an organism with this name; it will be replaced.");
                }
                caption(ui, "Description");
                ui.add(
                    egui::TextEdit::multiline(&mut ed.description)
                        .hint_text("Shown with the preset on the New world screen")
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(10.0);
                let enter = name.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| {
                    let ok = !file_name.is_empty();
                    let save = ui
                        .add_enabled(ok, egui::Button::new("Save").min_size(vec2(120.0, 32.0)))
                        .clicked();
                    if (save || (enter && ok)) && ed.save() {
                        ed.save_open = false;
                    }
                    if ui.add_sized([120.0, 32.0], egui::Button::new("Cancel")).clicked() {
                        ed.save_open = false;
                    }
                });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_reach_beyond_the_body() {
        let config = Config::default();
        let mut g = Genome::parse(&config.population.ancestor).unwrap();
        let bare = reach_radii(&g);
        let o = slot_offset(0);
        g.0[o] = PartKind::Antenna.code();
        g.0[o + 1] = 0xFF;
        assert!(reach_radii(&g) > bare + 1.0);
    }
}

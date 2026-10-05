//! The front of the game: a splash screen with the ways in (continue, a new world, load a
//! save), and the new-world setup: map, seed, starting population, where life starts, and the
//! ancestor's genome.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use evo_core::genome::{Genome, gene};
use evo_core::parts::{PartKind, slot_offset};
use evo_core::terrain::Terrain;
use evo_core::traits::{Traits, describe_gene};
use evo_core::{Config, World};
use glam::Vec2;

use crate::sim::SimState;
use crate::{AppState, Launch};

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Menu>()
            .add_systems(
                OnEnter(AppState::Menu),
                (
                    refresh,
                    create_backdrop.before(crate::render::WorldViewSetup),
                ),
            )
            .add_systems(
                Update,
                drift
                    .after(crate::sim::SimStep)
                    .run_if(in_state(AppState::Menu))
                    .run_if(resource_exists::<SimState>)
                    .run_if(resource_exists::<crate::camera::CameraState>),
            )
            .add_systems(
                EguiPrimaryContextPass,
                draw_menu.run_if(in_state(AppState::Menu)),
            );
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Page {
    #[default]
    Splash,
    NewWorld,
    Load,
}

struct SaveFile {
    path: PathBuf,
    name: String,
    modified: Option<SystemTime>,
    bytes: u64,
}

/// What the new world will be.
struct Setup {
    /// Map files available, as (name, description).
    maps: Vec<(String, String)>,
    map: String,
    seed: u64,
    population: u32,
    /// Where life starts: across the ocean (fractions of its width) and between two depths.
    spawn_x: [f32; 2],
    spawn_depth: [f32; 2],
    genome: Genome,
    genome_text: String,
    genome_error: Option<String>,
    preset: usize,
    vent_life: bool,
}

/// A small picture of the map, with what's needed to place things on it.
struct Preview {
    map: String,
    config: Config,
    texture: egui::TextureHandle,
    vents: Vec<Vec2>,
}

#[derive(Resource)]
struct Menu {
    page: Page,
    saves: Vec<SaveFile>,
    error: Option<String>,
    setup: Setup,
    preview: Option<Preview>,
}

impl Default for Menu {
    fn default() -> Self {
        let config = Config::load(Path::new("configs/default.toml")).unwrap_or_default();
        let genome = PRESETS[0].genome(&config);
        Self {
            page: Page::Splash,
            saves: Vec::new(),
            error: None,
            setup: Setup {
                maps: list_maps(),
                map: config.world.map.clone(),
                seed: config.world.seed,
                population: config.population.initial,
                spawn_x: config.population.spawn_x,
                spawn_depth: [
                    config.population.spawn_min_depth,
                    config.population.spawn_depth,
                ],
                genome_text: genome.to_string(),
                genome,
                genome_error: None,
                preset: 0,
                vent_life: false,
            },
            preview: None,
        }
    }
}

/// A starting organism to pick from.
struct Preset {
    name: &'static str,
    about: &'static str,
}

const PRESETS: [Preset; 4] = [
    Preset {
        name: "Drifting plankton",
        about: "A tiny floating photosynthesizer with no parts: it can't move or see. The classic start.",
    },
    Preset {
        name: "Swimming plankton",
        about: "The same, with a flagellum to swim with. It still needs eyes to find the light.",
    },
    Preset {
        name: "Snow eater",
        about: "A floating scavenger living on marine snow instead of sunlight.",
    },
    Preset {
        name: "Vent chemotroph",
        about: "An anchored microbe living on vent minerals, fermenting without oxygen. Start it at a vent.",
    },
];

/// The vent lineage's genome (also the "Vent chemotroph" preset).
const VENT_ANCESTOR: &str = "B:207060300000 C:10C040 D:00FF0000 G:400080000000 H:2030 \
     N:80808080808080808000 P:086010 000000 000000 000000 000000 000000 000000 000000 \
     R:FF00B0 T:FFFF";

impl Preset {
    fn genome(&self, config: &Config) -> Genome {
        let base = Genome::parse(&Config::default().population.ancestor).unwrap_or_default();
        let base = Genome::parse(&config.population.ancestor).unwrap_or(base);
        let mut g = base;
        match self.name {
            "Swimming plankton" => {
                let o = slot_offset(0);
                g.0[o] = PartKind::Flagellum.code();
                g.0[o + 1] = 0x80;
                g.0[o + 2] = 16; // at the back, pushing forward
            }
            "Snow eater" => {
                g.0[gene::PHOTO] = 0;
                g.0[gene::SCAVENGE] = 0xFF;
                g.0[gene::HUE] = 0x20;
            }
            "Vent chemotroph" => {
                g = Genome::parse(VENT_ANCESTOR).unwrap_or(base);
            }
            _ => {}
        }
        g
    }
}

/// Genes with a slider, and what to call them.
const SLIDERS: [(usize, &str); 10] = [
    (gene::SIZE, "Size"),
    (gene::BUOYANCY, "Buoyancy"),
    (gene::HUE, "Colour"),
    (gene::PHOTO, "Photosynthesis"),
    (gene::CHEMO, "Chemosynthesis"),
    (gene::SCAVENGE, "Scavenging"),
    (gene::HUNT, "Hunting"),
    (gene::RESPIRATION, "Breathing"),
    (gene::TEMP_PREF, "Likes warmth"),
    (gene::TEMP_TOL, "Heat tolerance"),
];

fn list_maps() -> Vec<(String, String)> {
    let mut maps: Vec<(String, String)> = std::fs::read_dir("configs/maps")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            (path.extension()? == "toml").then_some(())?;
            let name = path.file_stem()?.to_str()?.to_string();
            let text = std::fs::read_to_string(&path).ok()?;
            let table: toml::Table = toml::from_str(&text).ok()?;
            let about = table
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string();
            Some((name, about))
        })
        .collect();
    maps.sort();
    if maps.is_empty() {
        maps.push(("ocean".into(), "The built-in ocean".into()));
    }
    maps
}

fn list_saves(dir: &Path) -> Vec<SaveFile> {
    let mut saves: Vec<SaveFile> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            (path.extension()? == "bin").then_some(())?;
            let meta = e.metadata().ok()?;
            Some(SaveFile {
                name: path.file_name()?.to_string_lossy().into_owned(),
                path,
                modified: meta.modified().ok(),
                bytes: meta.len(),
            })
        })
        .collect();
    saves.sort_by_key(|s| std::cmp::Reverse(s.modified));
    saves
}

/// "3 hours ago" and the like.
fn ago(when: Option<SystemTime>) -> String {
    let Some(secs) = when.and_then(|t| t.elapsed().ok()).map(|d| d.as_secs()) else {
        return "unknown time".into();
    };
    let (n, unit) = match secs {
        0..60 => return "just now".into(),
        60..3600 => (secs / 60, "minute"),
        3600..86_400 => (secs / 3600, "hour"),
        _ => (secs / 86_400, "day"),
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

/// Today's date and time as `YYYY-MM-DD_HHMM` (UTC), for naming set-aside saves.
fn stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Civil date from days since 1970 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}_{:02}{:02}",
        rest / 3600,
        rest / 60 % 60
    )
}

/// The menu's backdrop: a small live world of founder ancestors in sunlit water, shown by the
/// game's own renderer under a still camera. It's never saved.
fn create_backdrop(mut commands: Commands, launch: Res<Launch>) {
    let path = Path::new("configs/default.toml");
    let Ok(mut config) = Config::load_with_map(path, Some("small")).or_else(|_| Config::load(path))
    else {
        return;
    };
    config.events.enabled = false;
    config.vents.count = 0;
    let (w, h) = (config.world.width, config.world.height);
    let center = bevy::prelude::Vec2::new(w * 0.5, h - 230.0);
    let ocean = config.ocean_width();
    let p = &mut config.population;
    p.initial = 32;
    p.min_population = 0;
    p.max_population = 44; // so they bud only now and then
    p.spawn_x = [(center.x - 160.0) / ocean, (center.x + 160.0) / ocean];
    p.spawn_min_depth = 150.0;
    p.spawn_depth = 330.0;
    if config.validate().is_err() {
        return;
    }
    let world = World::new(config);
    commands.insert_resource(SimState::new(world, &launch.0).without_autosave());
    commands.insert_resource(crate::camera::CameraState::fixed(center, 0.14));
}

/// Founders float up out of the view; bring each back in from below (and from the far side if
/// a current carries it off), out of sight, so there's always a slow rise past the camera.
fn drift(
    mut state: ResMut<SimState>,
    cam: Res<crate::camera::CameraState>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let half = bevy::prelude::Vec2::new(window.width(), window.height()) * 0.5 * cam.zoom;
    let (lo, hi) = (cam.center - half, cam.center + half);
    let tick = state.world.tick;
    for o in &mut state.world.organisms {
        let r = o.radius * 3.0 + 10.0;
        let x01 = (o.id.wrapping_mul(2_654_435_761) ^ tick.wrapping_mul(97)) % 1000;
        let along = lo.x + (x01 as f32 / 1000.0) * (hi.x - lo.x);
        let to = if o.pos.y - r > hi.y || o.pos.y + r < lo.y - 80.0 {
            Some(Vec2::new(along, lo.y - r))
        } else if o.pos.x + r < lo.x {
            Some(Vec2::new(hi.x + r, o.pos.y))
        } else if o.pos.x - r > hi.x {
            Some(Vec2::new(lo.x - r, o.pos.y))
        } else {
            None
        };
        if let Some(p) = to {
            o.pos = p;
            o.prev_pos = p;
        }
    }
}

fn refresh(mut menu: ResMut<Menu>, launch: Res<Launch>) {
    menu.saves = list_saves(&launch.0.save_dir);
    menu.setup.maps = list_maps();
    menu.error = None;
}

#[allow(clippy::too_many_arguments)]
fn draw_menu(
    mut contexts: EguiContexts,
    mut menu: ResMut<Menu>,
    launch: Res<Launch>,
    mut commands: Commands,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
    time: Res<Time<Real>>,
    mut styled: Local<bool>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    if !*styled {
        // Tooltips appear almost at once, as in the game.
        ctx.all_styles_mut(|style| {
            style.interaction.tooltip_delay = 0.05;
            style.interaction.show_tooltips_only_when_still = false;
        });
        *styled = true;
    }
    let mut root = egui::Ui::new(
        ctx.clone(),
        egui::Id::new("menu_root"),
        egui::UiBuilder::new(),
    );
    let t = time.elapsed_secs();
    let mut start: Option<World> = None;
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(40, 16)))
        .show(&mut root, |ui| {
            backdrop(ui, t);
            style_menu(ui);
            match menu.page {
                Page::Splash => splash(ui, &mut menu, &launch, &mut start, &mut exit, t),
                Page::NewWorld => new_world(ui, &ctx, &mut menu, &launch, &mut start),
                Page::Load => load(ui, &mut menu, &mut start),
            }
        });
    if let Some(world) = start {
        commands.insert_resource(SimState::new(world, &launch.0));
        next.set(AppState::Running);
    }
    Ok(())
}

/// A little hash for the backdrop: a fixed "random" 0..1 for item `i`, value `k`.
fn hash(i: u32, k: u32) -> f32 {
    let x = (i.wrapping_mul(2_654_435_761) ^ k.wrapping_mul(40_503)).wrapping_mul(2_246_822_519);
    (x >> 8) as f32 / (1u32 << 24) as f32
}

fn rgba(r: u8, g: u8, b: u8, a: f32) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(r, g, b, (a.clamp(0.0, 1.0) * 255.0) as u8)
}

/// A rectangle shading from `a` (top or left) to `b` (bottom or right).
fn gradient(
    painter: &egui::Painter,
    rect: egui::Rect,
    a: egui::Color32,
    b: egui::Color32,
    vertical: bool,
) {
    let mut mesh = egui::Mesh::default();
    let (ta, tb) = if vertical {
        (
            [rect.left_top(), rect.right_top()],
            [rect.right_bottom(), rect.left_bottom()],
        )
    } else {
        (
            [rect.left_bottom(), rect.left_top()],
            [rect.right_top(), rect.right_bottom()],
        )
    };
    mesh.colored_vertex(ta[0], a);
    mesh.colored_vertex(ta[1], a);
    mesh.colored_vertex(tb[0], b);
    mesh.colored_vertex(tb[1], b);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

/// A living cell: a soft glow, a translucent body with granules, a nucleus, a membrane, and a
/// fringe of beating cilia.
fn draw_cell(
    painter: &egui::Painter,
    center: egui::Pos2,
    r: f32,
    color: egui::Color32,
    t: f32,
    alpha: f32,
) {
    let [cr, cg, cb, _] = color.to_array();
    let c = |a: f32| rgba(cr, cg, cb, a * alpha);
    painter.circle_filled(center, r * 1.5, c(0.04));
    painter.circle_filled(center, r * 1.2, c(0.06));
    painter.circle_filled(center, r, c(0.16));
    // Granules drifting round inside.
    for k in 0..7u32 {
        let a = hash(k, 1) * std::f32::consts::TAU + t * (0.15 + 0.2 * hash(k, 2));
        let d = r * (0.35 + 0.45 * hash(k, 3));
        let p = center + egui::vec2(a.cos(), a.sin()) * d;
        painter.circle_filled(p, r * (0.05 + 0.04 * hash(k, 4)), c(0.45));
    }
    // The nucleus, wandering a little.
    let n = center + egui::vec2((t * 0.4).sin(), (t * 0.33).cos()) * r * 0.12;
    painter.circle_filled(n, r * 0.34, rgba(cr / 2, cg / 2, cb / 2, 0.55 * alpha));
    painter.circle_stroke(n, r * 0.34, egui::Stroke::new((r * 0.05).max(1.0), c(0.7)));
    painter.circle_filled(n + egui::vec2(-r * 0.08, -r * 0.08), r * 0.08, c(0.8));
    // The membrane.
    painter.circle_stroke(center, r, egui::Stroke::new((r * 0.09).max(1.2), c(0.95)));
    // Cilia, beating in a wave around the cell.
    let count = 22;
    for k in 0..count {
        let a = k as f32 / count as f32 * std::f32::consts::TAU + t * 0.25;
        let dir = egui::vec2(a.cos(), a.sin());
        let beat = (t * 5.0 - k as f32 * 0.9).sin() * 0.6;
        let side = egui::vec2(-dir.y, dir.x);
        let base = center + dir * r * 1.05;
        let tip = base + (dir + side * beat).normalized() * r * 0.28;
        painter.line_segment([base, tip], egui::Stroke::new((r * 0.035).max(1.0), c(0.6)));
    }
}

/// Over the live backdrop: a dark edge (vignette), and a deeper shade at the bottom.
fn backdrop(ui: &egui::Ui, _t: f32) {
    let rect = ui.ctx().content_rect();
    let painter = ui.ctx().layer_painter(egui::LayerId::background());
    let (w, h) = (rect.width(), rect.height());
    let edge = w.min(h) * 0.25;
    let dark = rgba(0, 3, 8, 0.7);
    let clear = rgba(0, 3, 8, 0.0);
    gradient(
        &painter,
        egui::Rect::from_min_max(
            rect.left_top(),
            egui::pos2(rect.left() + edge, rect.bottom()),
        ),
        dark,
        clear,
        false,
    );
    gradient(
        &painter,
        egui::Rect::from_min_max(
            egui::pos2(rect.right() - edge, rect.top()),
            rect.right_bottom(),
        ),
        clear,
        dark,
        false,
    );
    gradient(
        &painter,
        egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.top() + h * 0.55),
            rect.right_bottom(),
        ),
        clear,
        rgba(0, 3, 8, 0.8),
        true,
    );
    // A little shade at the top, under the title.
    gradient(
        &painter,
        egui::Rect::from_min_max(
            rect.left_top(),
            egui::pos2(rect.right(), rect.top() + h * 0.45),
        ),
        rgba(0, 6, 14, 0.5),
        rgba(0, 6, 14, 0.0),
        true,
    );
}

/// The title: spaced letters shading from sea-green to leaf-green, softly glowing and gently
/// bobbing, with both O's drawn as living cells.
fn title(ui: &mut egui::Ui, t: f32) {
    let text = "MICROCOSM";
    let size = 92.0;
    let font = egui::FontId::proportional(size);
    let gap = size * 0.16;
    let widths: Vec<f32> = text
        .chars()
        .map(|c| ui.ctx().fonts_mut(|f| f.glyph_width(&font, c)))
        .collect();
    let n = widths.len();
    let total = widths.iter().sum::<f32>() + gap * (n - 1) as f32;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(total, size * 1.25), egui::Sense::hover());
    let painter = ui.painter();
    // A halo of light behind the word.
    for k in 0..6 {
        let f = k as f32 / 6.0;
        painter.circle_filled(
            rect.center(),
            total * (0.25 + 0.35 * f),
            rgba(80, 200, 180, 0.025 * (1.0 - f)),
        );
    }
    let mut x = rect.left();
    for (i, (ch, w)) in text.chars().zip(&widths).enumerate() {
        let f = i as f32 / (n - 1) as f32;
        let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * f) as u8;
        let (r, g, b) = (lerp(100, 185), lerp(225, 245), lerp(220, 130));
        let bob = (t * 1.1 + i as f32 * 0.55).sin() * size * 0.025;
        let center = egui::pos2(x + w * 0.5, rect.center().y + bob);
        if ch == 'O' {
            draw_cell(
                painter,
                center,
                size * 0.3,
                egui::Color32::from_rgb(r, g, b),
                t + i as f32,
                1.0,
            );
        } else {
            for k in 0..8 {
                let a = k as f32 / 8.0 * std::f32::consts::TAU;
                painter.text(
                    center + egui::vec2(a.cos(), a.sin()) * size * 0.04,
                    egui::Align2::CENTER_CENTER,
                    ch,
                    font.clone(),
                    rgba(r, g, b, 0.07),
                );
            }
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                ch,
                font.clone(),
                egui::Color32::from_rgb(r, g, b),
            );
        }
        x += w + gap;
    }
}

/// The menus' look: rounded, translucent sea-glass buttons that light up on hover.
fn style_menu(ui: &mut egui::Ui) {
    let v = &mut ui.style_mut().visuals;
    let round = egui::CornerRadius::same(12);
    let w = &mut v.widgets;
    w.inactive.weak_bg_fill = rgba(18, 70, 80, 0.55);
    // Slider tracks and handles use the plain fill: keep it visible.
    w.inactive.bg_fill = rgba(60, 140, 140, 0.9);
    w.inactive.bg_stroke = egui::Stroke::new(1.0, rgba(130, 230, 210, 0.35));
    w.inactive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(205, 240, 230));
    w.inactive.corner_radius = round;
    w.hovered.weak_bg_fill = rgba(30, 120, 115, 0.75);
    w.hovered.bg_fill = rgba(30, 120, 115, 0.75);
    w.hovered.bg_stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(160, 250, 220));
    w.hovered.fg_stroke = egui::Stroke::new(1.5, egui::Color32::WHITE);
    w.hovered.corner_radius = round;
    w.active.weak_bg_fill = rgba(40, 150, 135, 0.85);
    w.active.bg_fill = rgba(40, 150, 135, 0.85);
    w.active.corner_radius = round;
    v.selection.bg_fill = rgba(40, 150, 135, 0.8);
}

/// A dark, translucent panel that keeps text readable over the backdrop.
fn card() -> egui::Frame {
    egui::Frame::NONE
        .fill(rgba(3, 14, 24, 0.82))
        .stroke(egui::Stroke::new(1.0, rgba(120, 220, 200, 0.18)))
        .corner_radius(14)
        .inner_margin(egui::Margin::same(18))
}

fn big_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add_sized(
        [300.0, 44.0],
        egui::Button::new(egui::RichText::new(text).size(20.0)),
    )
}

fn splash(
    ui: &mut egui::Ui,
    menu: &mut Menu,
    launch: &Launch,
    start: &mut Option<World>,
    exit: &mut MessageWriter<AppExit>,
    t: f32,
) {
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.16).max(20.0));
        title(ui, t);
        ui.label(
            egui::RichText::new("A LIVING OCEAN, GROWN FROM A SINGLE MICROBE")
                .size(15.0)
                .extra_letter_spacing(3.0)
                .color(egui::Color32::from_rgb(150, 200, 200)),
        );
        ui.add_space(52.0);
        let autosave = launch.0.save_dir.join("autosave.bin");
        if let Some(save) = menu.saves.iter().find(|s| s.path == autosave) {
            if big_button(ui, "Continue").clicked() {
                match World::load(&save.path) {
                    Ok(w) => *start = Some(w),
                    Err(e) => menu.error = Some(e),
                }
            }
            ui.label(
                egui::RichText::new(format!("autosave, {}", ago(save.modified)))
                    .size(13.0)
                    .color(egui::Color32::from_rgb(190, 230, 225)),
            );
            ui.add_space(10.0);
        }
        if big_button(ui, "New world").clicked() {
            menu.page = Page::NewWorld;
            menu.error = None;
        }
        ui.add_space(10.0);
        if big_button(ui, "Load a save").clicked() {
            menu.page = Page::Load;
            menu.error = None;
        }
        ui.add_space(10.0);
        if big_button(ui, "Quit").clicked() {
            exit.write(AppExit::Success);
        }
        if let Some(e) = &menu.error {
            ui.add_space(16.0);
            ui.colored_label(egui::Color32::from_rgb(240, 120, 110), e);
        }
    });
}

fn load(ui: &mut egui::Ui, menu: &mut Menu, start: &mut Option<World>) {
    let full = ui.available_rect_before_wrap();
    let width = 640.0f32.min(full.width());
    let column = egui::Rect::from_min_size(
        egui::pos2(full.center().x - width * 0.5, full.top()),
        egui::vec2(width, full.height()),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(column), |ui| {
        card().show(ui, |ui| load_list(ui, menu, start));
    });
}

fn load_list(ui: &mut egui::Ui, menu: &mut Menu, start: &mut Option<World>) {
    ui.horizontal(|ui| {
        if ui.button("‹ Back").clicked() {
            menu.page = Page::Splash;
        }
        ui.heading("Load a save");
    });
    ui.add_space(12.0);
    if let Some(e) = &menu.error {
        ui.colored_label(egui::Color32::from_rgb(240, 120, 110), e);
    }
    if menu.saves.is_empty() {
        ui.label("No saves yet.");
        return;
    }
    let mut chosen = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("saves")
            .num_columns(4)
            .spacing([24.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                for (i, s) in menu.saves.iter().enumerate() {
                    ui.label(egui::RichText::new(&s.name).strong());
                    ui.label(ago(s.modified));
                    ui.label(format!("{:.1} MB", s.bytes as f64 / 1e6));
                    if ui.button("Load").clicked() {
                        chosen = Some(i);
                    }
                    ui.end_row();
                }
            });
    });
    if let Some(i) = chosen {
        match World::load(&menu.saves[i].path) {
            Ok(w) => *start = Some(w),
            Err(e) => menu.error = Some(e),
        }
    }
}

fn new_world(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    menu: &mut Menu,
    launch: &Launch,
    start: &mut Option<World>,
) {
    let config_path = launch
        .0
        .config
        .clone()
        .unwrap_or_else(|| PathBuf::from("configs/default.toml"));
    // The map preview (rebuilt when the map changes).
    if menu
        .preview
        .as_ref()
        .is_none_or(|p| p.map != menu.setup.map)
    {
        match Config::load_with_map(&config_path, Some(&menu.setup.map)) {
            Ok(config) => menu.preview = Some(build_preview(ctx, &menu.setup.map, config)),
            Err(e) => menu.error = Some(e),
        }
    }
    ui.add_space(16.0);
    ui.horizontal(|ui| {
        if ui.button("‹ Back").clicked() {
            menu.page = Page::Splash;
        }
        ui.heading("New world");
    });
    ui.add_space(8.0);
    let Menu {
        setup,
        preview,
        error,
        ..
    } = menu;
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.columns(2, |cols| {
            card().show(&mut cols[0], |ui| world_column(ui, setup, preview.as_ref()));
            card().show(&mut cols[1], |ui| {
                ancestor_column(ui, setup, preview.as_ref())
            });
        });
        ui.add_space(16.0);
        ui.vertical_centered(|ui| {
            if let Some(e) = error.as_ref() {
                ui.colored_label(egui::Color32::from_rgb(240, 120, 110), e);
            }
            let ready = setup.genome_error.is_none();
            let button = ui.add_enabled(
                ready,
                egui::Button::new(egui::RichText::new("Start").size(22.0))
                    .min_size([240.0, 46.0].into()),
            );
            ui.label(
                egui::RichText::new(
                    "Your autosave, if any, is kept in the saves folder under a dated name.",
                )
                .weak(),
            );
            if button.clicked() {
                match create_world(&config_path, setup, &launch.0.save_dir) {
                    Ok(w) => *start = Some(w),
                    Err(e) => *error = Some(e),
                }
            }
        });
        ui.add_space(24.0);
    });
}

fn build_preview(ctx: &egui::Context, map: &str, config: Config) -> Preview {
    let terrain = Terrain::generate(&config);
    let (w, h) = (config.world.width, config.world.height);
    let width = 480usize;
    let height = ((width as f32 * h / w).round() as usize).max(40);
    let mut pixels = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let p = Vec2::new(
                (x as f32 + 0.5) / width as f32 * w,
                h - (y as f32 + 0.5) / height as f32 * h,
            );
            let depth = (h - p.y) / h;
            let [r, g, b] = if terrain.is_solid(p) {
                [92, 80, 66]
            } else {
                let light = (1.0 - depth).powi(3);
                [
                    (6.0 + 30.0 * light) as u8,
                    (20.0 + 120.0 * light) as u8,
                    (45.0 + 130.0 * light) as u8,
                ]
            };
            pixels.extend_from_slice(&[r, g, b, 255]);
        }
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([width, height], &pixels);
    Preview {
        map: map.to_string(),
        texture: ctx.load_texture("map_preview", image, egui::TextureOptions::LINEAR),
        vents: terrain.vent_sites.clone(),
        config,
    }
}

fn world_column(ui: &mut egui::Ui, setup: &mut Setup, preview: Option<&Preview>) {
    ui.heading("The world");
    ui.add_space(4.0);
    let current = setup
        .maps
        .iter()
        .find(|m| m.0 == setup.map)
        .map(|m| m.1.clone())
        .unwrap_or_default();
    egui::ComboBox::from_label("Map")
        .selected_text(&setup.map)
        .show_ui(ui, |ui| {
            for (name, about) in &setup.maps {
                ui.selectable_value(&mut setup.map, name.clone(), name)
                    .on_hover_text(about);
            }
        });
    ui.label(egui::RichText::new(current).weak());

    // The map, with where life will start; click it to move that spot.
    if let Some(p) = preview {
        let size = p.texture.size_vec2();
        let scale = (ui.available_width() / size.x).min(1.4);
        let (rect, response) = ui.allocate_exact_size(size * scale, egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.image(
            p.texture.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        let (w, h) = (p.config.world.width, p.config.world.height);
        let ocean = p.config.ocean_width();
        let to_screen = |x: f32, depth: f32| {
            egui::pos2(
                rect.left() + x / w * rect.width(),
                rect.top() + depth / h * rect.height(),
            )
        };
        for v in &p.vents {
            painter.circle_filled(
                to_screen(v.x, h - v.y),
                3.0,
                egui::Color32::from_rgb(255, 150, 60),
            );
        }
        let area = egui::Rect::from_two_pos(
            to_screen(setup.spawn_x[0] * ocean, setup.spawn_depth[0]),
            to_screen(
                setup.spawn_x[1] * ocean,
                setup.spawn_depth[1].max(setup.spawn_depth[0] + 1.0),
            ),
        );
        painter.rect_filled(
            area,
            2.0,
            egui::Color32::from_rgba_unmultiplied(140, 255, 170, 50),
        );
        painter.rect_stroke(
            area,
            2.0,
            egui::Stroke::new(1.5, egui::Color32::from_rgb(140, 255, 170)),
            egui::StrokeKind::Outside,
        );
        if let Some(pos) = response.interact_pointer_pos()
            && (response.clicked() || response.dragged())
        {
            let x = ((pos.x - rect.left()) / rect.width() * w / ocean).clamp(0.0, 1.0);
            let depth = ((pos.y - rect.top()) / rect.height() * h).clamp(0.0, h);
            let half_x = (setup.spawn_x[1] - setup.spawn_x[0]).abs() * 0.5;
            let half_d = (setup.spawn_depth[1] - setup.spawn_depth[0]).abs() * 0.5;
            let cx = x.clamp(half_x, 1.0 - half_x);
            let cd = depth.clamp(half_d, h - half_d);
            setup.spawn_x = [cx - half_x, cx + half_x];
            setup.spawn_depth = [cd - half_d, cd + half_d];
        }
        ui.label(
            egui::RichText::new(
                "Green: where life starts (click or drag to move it). Orange: permanent vents.",
            )
            .weak(),
        );
    }
    ui.add_space(8.0);
    egui::Grid::new("world_grid")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Seed");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut setup.seed));
                if ui.button("Random").clicked() {
                    setup.seed = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .map(|d| d.as_nanos() as u64 % 1_000_000)
                        .unwrap_or(1);
                }
            });
            ui.end_row();
            ui.label("");
            ui.label(
                egui::RichText::new(
                    "Life, events, random vents and currents. The map is the same for every seed.",
                )
                .weak(),
            );
            ui.end_row();
            ui.label("Starting population");
            ui.add(egui::Slider::new(&mut setup.population, 20..=20_000).logarithmic(true));
            ui.end_row();
            let ocean_depth = preview.map(|p| p.config.world.height).unwrap_or(6000.0);
            ui.label("Across the ocean");
            ui.horizontal(|ui| {
                let mut lo = setup.spawn_x[0] * 100.0;
                let mut hi = setup.spawn_x[1] * 100.0;
                ui.add(
                    egui::Slider::new(&mut lo, 0.0..=100.0)
                        .suffix("%")
                        .text("from"),
                );
                ui.add(
                    egui::Slider::new(&mut hi, 0.0..=100.0)
                        .suffix("%")
                        .text("to"),
                );
                setup.spawn_x = [lo.min(hi) / 100.0, hi.max(lo) / 100.0];
            });
            ui.end_row();
            ui.label("Depth");
            ui.horizontal(|ui| {
                let [mut lo, mut hi] = setup.spawn_depth;
                ui.add(egui::Slider::new(&mut lo, 0.0..=ocean_depth).text("from"));
                ui.add(egui::Slider::new(&mut hi, 0.0..=ocean_depth).text("to"));
                setup.spawn_depth = [lo.min(hi), hi.max(lo + 10.0)];
            });
            ui.end_row();
            ui.label("Quick picks");
            ui.horizontal_wrapped(|ui| {
                let picks: [(&str, [f32; 2], [f32; 2]); 4] = [
                    ("Sunlit surface", [0.0, 1.0], [0.0, 300.0]),
                    ("Shelf shallows", [0.8, 1.0], [0.0, 600.0]),
                    ("Twilight", [0.0, 1.0], [900.0, 2000.0]),
                    ("Deep floor", [0.1, 0.7], [3500.0, 5500.0]),
                ];
                for (name, x, d) in picks {
                    if ui.button(name).clicked() {
                        setup.spawn_x = x;
                        setup.spawn_depth = d;
                    }
                }
                if let Some(p) = preview
                    && let Some(v) = p.vents.first()
                    && ui.button("Shelf vent").clicked()
                {
                    let (ocean, h) = (p.config.ocean_width(), p.config.world.height);
                    let x = v.x / ocean;
                    let depth = h - v.y;
                    setup.spawn_x = [(x - 0.01).max(0.0), (x + 0.01).min(1.0)];
                    setup.spawn_depth = [(depth - 150.0).max(0.0), depth + 50.0];
                }
            });
            ui.end_row();
        });
}

fn ancestor_column(ui: &mut egui::Ui, setup: &mut Setup, preview: Option<&Preview>) {
    ui.heading("The ancestor");
    ui.add_space(4.0);
    let config = preview.map(|p| p.config.clone()).unwrap_or_default();
    let mut preset = setup.preset;
    egui::ComboBox::from_label("Start from")
        .selected_text(PRESETS[preset.min(PRESETS.len() - 1)].name)
        .show_ui(ui, |ui| {
            for (i, p) in PRESETS.iter().enumerate() {
                ui.selectable_value(&mut preset, i, p.name)
                    .on_hover_text(p.about);
            }
        });
    if preset != setup.preset {
        setup.preset = preset;
        setup.genome = PRESETS[preset].genome(&config);
        setup.genome_text = setup.genome.to_string();
        setup.genome_error = None;
    }
    ui.label(egui::RichText::new(PRESETS[setup.preset].about).weak());
    ui.add_space(6.0);

    let traits = Traits::decode(&setup.genome, &config);
    ui.horizontal(|ui| {
        crate::ui::swatch(ui, traits.color, 16.0);
        let parts = if traits.part_count == 0 {
            "no body parts".to_string()
        } else {
            format!("{} body parts", traits.part_count)
        };
        ui.label(format!(
            "Adult radius {:.1} · {} · {}",
            traits.adult_radius,
            if traits.is_motile() {
                "can move"
            } else {
                "can't move"
            },
            parts
        ));
    });
    ui.add_space(4.0);
    let mut changed = false;
    egui::Grid::new("genes")
        .num_columns(3)
        .spacing([10.0, 4.0])
        .show(ui, |ui| {
            for (index, name) in SLIDERS {
                ui.label(name);
                let mut v = setup.genome.0[index];
                if ui
                    .add(egui::Slider::new(&mut v, 0..=255).show_value(false))
                    .changed()
                {
                    setup.genome.0[index] = v;
                    changed = true;
                }
                ui.label(
                    egui::RichText::new(describe_gene(index, &setup.genome, &traits, &config))
                        .weak(),
                );
                ui.end_row();
            }
        });
    if changed {
        setup.genome_text = setup.genome.to_string();
        setup.genome_error = None;
    }
    ui.add_space(6.0);
    ui.label("Genome (edit directly for anything the sliders don't cover)");
    let edit = ui.add(
        egui::TextEdit::multiline(&mut setup.genome_text)
            .font(egui::TextStyle::Monospace)
            .desired_rows(3)
            .desired_width(f32::INFINITY),
    );
    if edit.changed() {
        match Genome::parse(&setup.genome_text) {
            Ok(g) => {
                setup.genome = g;
                setup.genome_error = None;
            }
            Err(e) => setup.genome_error = Some(e),
        }
    }
    if let Some(e) = &setup.genome_error {
        ui.colored_label(egui::Color32::from_rgb(240, 120, 110), e);
    }
    ui.add_space(6.0);
    ui.checkbox(
        &mut setup.vent_life,
        "Also seed vent life: anchored chemotrophs at each vent",
    );
}

/// Build the world from the setup. An existing autosave is set aside under a dated name first.
fn create_world(config_path: &Path, setup: &Setup, save_dir: &Path) -> Result<World, String> {
    let mut config = Config::load_with_map(config_path, Some(&setup.map))?;
    config.world.seed = setup.seed;
    let p = &mut config.population;
    p.initial = setup.population;
    p.spawn_x = setup.spawn_x;
    p.spawn_min_depth = setup.spawn_depth[0];
    p.spawn_depth = setup.spawn_depth[1];
    p.ancestor = setup.genome.to_string();
    p.vent_ancestor = if setup.vent_life {
        VENT_ANCESTOR.to_string()
    } else {
        String::new()
    };
    config.validate()?;
    let autosave = save_dir.join("autosave.bin");
    if autosave.exists() {
        let aside = save_dir.join(format!("world-{}.bin", stamp()));
        std::fs::rename(&autosave, &aside).map_err(|e| e.to_string())?;
    }
    Ok(World::new(config))
}

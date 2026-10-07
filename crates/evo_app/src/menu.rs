//! The front of the game: a splash screen with the ways in (continue, a new world, load a
//! save), and the new-world setup: map, seed, starting population, where life starts, and the
//! ancestor's genome.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use evo_core::genome::{CHROMOSOMES, Genome, gene};
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
    /// Any text: a number is used as is, other text is hashed into one.
    seed_text: String,
    population: u32,
    /// Where life starts: across the ocean (fractions of its width) and between two depths.
    spawn_x: [f32; 2],
    spawn_depth: [f32; 2],
    genome: Genome,
    /// The genome as text, whole (for copy and paste) and chromosome by chromosome.
    genome_text: String,
    chromosome_text: Vec<String>,
    /// What's wrong with the genome text, if anything (Start is disabled).
    genome_error: Option<String>,
    preset: usize,
    vent_life: bool,
    /// Where a drag on the map started (fraction across the ocean, depth).
    drag_from: Option<(f32, f32)>,
}

impl Setup {
    /// Refresh the genome texts after the genome changed.
    fn sync_text(&mut self) {
        self.genome_text = self.genome.to_string();
        self.chromosome_text = CHROMOSOMES
            .iter()
            .map(|c| chromosome_hex(&self.genome, c))
            .collect();
        self.genome_error = None;
    }

    fn seed(&self) -> u64 {
        seed_from(&self.seed_text)
    }
}

/// A seed from any text: a number is used as is, anything else is hashed.
fn seed_from(text: &str) -> u64 {
    let t = text.trim();
    t.parse::<u64>().unwrap_or_else(|_| {
        t.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
            (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3)
        })
    })
}

/// A chromosome's genes in hex, spaced so each gene (or part slot, for P) reads on its own.
fn chromosome_hex(genome: &Genome, c: &evo_core::genome::Chromosome) -> String {
    let bytes = &genome.0[c.range()];
    let group = if c.letter == 'P' { 3 } else { 1 };
    bytes
        .chunks(group)
        .map(|g| g.iter().map(|b| format!("{b:02X}")).collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
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
    /// Height of the taller New-world card last frame (both are drawn this tall).
    card_height: f32,
    /// The Map editor button was pressed.
    open_editor: bool,
    /// The map the last picture was of (so coming back from the editor doesn't reset the
    /// chosen start area).
    preview_map: Option<String>,
}

impl Default for Menu {
    fn default() -> Self {
        let config = Config::load(Path::new("configs/default.toml")).unwrap_or_default();
        let genome = PRESETS[0].genome(&config);
        let mut setup = Setup {
            maps: list_maps(),
            map: config.world.map.clone(),
            seed_text: config.world.seed.to_string(),
            population: config.population.initial,
            spawn_x: config.population.spawn_x,
            spawn_depth: [
                config.population.spawn_min_depth,
                config.population.spawn_depth,
            ],
            genome,
            genome_text: String::new(),
            chromosome_text: Vec::new(),
            genome_error: None,
            preset: 0,
            vent_life: false,
            drag_from: None,
        };
        setup.sync_text();
        Self {
            page: Page::Splash,
            saves: Vec::new(),
            error: None,
            setup,
            preview: None,
            card_height: 0.0,
            open_editor: false,
            preview_map: None,
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
        about: "A tiny floating photosynthesizer. It can't move or see.",
    },
    Preset {
        name: "Swimming plankton",
        about: "Same as Drifting Plankton but with a small flagellum.",
    },
    Preset {
        name: "Snow eater",
        about: "A floating scavenger that eats marine snow.",
    },
    Preset {
        name: "Vent chemotroph",
        about: "Lives on vent minerals without oxygen. Start it near a vent.",
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
            let name = path.file_stem()?.to_str()?.to_string();
            // Maps painted in the editor.
            if path.extension()? == "map" {
                let map = evo_core::map::PaintedMap::load(&path).ok()?;
                let about = if map.description.is_empty() {
                    "Painted in the map editor".to_string()
                } else {
                    map.description
                };
                return Some((name, about));
            }
            (path.extension()? == "toml").then_some(())?;
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
        maps.push(("Deep Ocean".into(), String::new()));
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
    let Ok(mut config) = Config::load(path).or_else(|_| Config::from_toml(""))
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
    // A map may have been saved (or changed) in the editor: redraw its picture.
    menu.preview = None;
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
            if menu.page != Page::Splash {
                ui.ctx()
                    .layer_painter(egui::LayerId::background())
                    .rect_filled(ui.ctx().content_rect(), 0.0, rgba(0, 4, 10, 0.55));
            }
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
    if std::mem::take(&mut menu.open_editor) {
        next.set(AppState::Editor);
    }
    Ok(())
}

/// A little hash for the backdrop: a fixed "random" 0..1 for item `i`, value `k`.
fn hash(i: u32, k: u32) -> f32 {
    let x = (i.wrapping_mul(2_654_435_761) ^ k.wrapping_mul(40_503)).wrapping_mul(2_246_822_519);
    (x >> 8) as f32 / (1u32 << 24) as f32
}

pub(crate) fn rgba(r: u8, g: u8, b: u8, a: f32) -> egui::Color32 {
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
pub(crate) fn style_menu(ui: &mut egui::Ui) {
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
    v.override_text_color = Some(TEXT);
    v.weak_text_color = Some(MUTED);
    v.extreme_bg_color = rgba(0, 6, 12, 0.9); // text fields
}

/// A section heading inside a card.
fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(20.0).color(ACCENT));
    ui.add_space(6.0);
}

/// A small label above a group of controls.
fn caption(ui: &mut egui::Ui, text: &str) {
    ui.add_space(8.0);
    ui.label(egui::RichText::new(text).size(13.0).strong().color(MUTED));
}

/// Text colours of the menus: main, secondary (still readable), accent and error.
pub(crate) const TEXT: egui::Color32 = egui::Color32::from_rgb(222, 238, 236);
pub(crate) const MUTED: egui::Color32 = egui::Color32::from_rgb(160, 196, 198);
pub(crate) const ACCENT: egui::Color32 = egui::Color32::from_rgb(150, 240, 210);
pub(crate) const ERROR: egui::Color32 = egui::Color32::from_rgb(250, 140, 125);

/// A dark panel that keeps text readable over the backdrop (opaque enough that the bright
/// organisms behind don't shine through).
pub(crate) fn card() -> egui::Frame {
    egui::Frame::NONE
        .fill(rgba(4, 16, 26, 0.96))
        .stroke(egui::Stroke::new(1.0, rgba(120, 220, 200, 0.18)))
        .corner_radius(14)
        .inner_margin(egui::Margin::same(18))
}

/// The Back button at the top left of a page.
fn back_button(ui: &mut egui::Ui) -> bool {
    ui.add_sized(
        [92.0, 36.0],
        egui::Button::new(egui::RichText::new("Back").size(17.0)),
    )
    .clicked()
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
            egui::RichText::new("EVOLUTION SIMULATOR")
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
        if big_button(ui, "Load").clicked() {
            menu.page = Page::Load;
            menu.error = None;
        }
        ui.add_space(10.0);
        if big_button(ui, "Map editor").clicked() {
            menu.open_editor = true;
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
        if back_button(ui) {
            menu.page = Page::Splash;
        }
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Load").size(26.0).color(TEXT));
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
    // The map picture (rebuilt when the map changes). A newly picked map starts life in its
    // first start area.
    if menu
        .preview
        .as_ref()
        .is_none_or(|p| p.map != menu.setup.map)
    {
        let changed = menu.preview.as_ref().is_some_and(|p| p.map != menu.setup.map)
            || menu.preview_map.as_deref() != Some(menu.setup.map.as_str());
        match Config::load_with_map(&config_path, Some(&menu.setup.map)) {
            Ok(config) => {
                let preview = build_preview(ctx, &menu.setup.map, config);
                if changed && let Some(area) = start_areas(&preview).into_iter().next() {
                    menu.setup.spawn_x = area.1;
                    menu.setup.spawn_depth = area.2;
                }
                menu.preview_map = Some(menu.setup.map.clone());
                menu.preview = Some(preview);
            }
            Err(e) => menu.error = Some(e),
        }
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if back_button(ui) {
            menu.page = Page::Splash;
        }
        ui.add_space(10.0);
        ui.label(egui::RichText::new("New world").size(30.0).color(TEXT));
    });
    ui.add_space(12.0);
    let Menu {
        setup,
        preview,
        error,
        card_height,
        ..
    } = menu;
    // Both cards are drawn as tall as the taller one was last frame.
    let target = *card_height;
    let mut tallest = 0.0f32;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.columns(2, |cols| {
                card().show(&mut cols[0], |ui| {
                    let top = ui.cursor().min.y;
                    world_column(ui, setup, preview.as_ref());
                    // Pad up to the taller card's height (measured last frame).
                    let used = ui.cursor().min.y - top;
                    tallest = tallest.max(used);
                    if target > used {
                        ui.add_space(target - used);
                    }
                });
                card().show(&mut cols[1], |ui| {
                    let top = ui.cursor().min.y;
                    ancestor_column(ui, setup, preview.as_ref());
                    // Pad up to the taller card's height (measured last frame).
                    let used = ui.cursor().min.y - top;
                    tallest = tallest.max(used);
                    if target > used {
                        ui.add_space(target - used);
                    }
                });
            });
            ui.add_space(18.0);
            ui.vertical_centered(|ui| {
                if let Some(e) = error.as_ref() {
                    ui.colored_label(ERROR, e);
                }
                if let Some(e) = &setup.genome_error {
                    ui.colored_label(ERROR, format!("Genome: {e}"));
                }
                let ready = setup.genome_error.is_none();
                let button = ui.add_enabled(
                    ready,
                    egui::Button::new(egui::RichText::new("Start").size(22.0))
                        .min_size([260.0, 48.0].into()),
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
    *card_height = tallest;
}

/// A map's start areas: name, span across the ocean (fractions) and depths.
fn start_areas(p: &Preview) -> Vec<(String, [f32; 2], [f32; 2])> {
    let Some(map) = &p.config.terrain.painted else {
        return Vec::new();
    };
    let ocean = p.config.ocean_width();
    map.spawn_areas
        .iter()
        .map(|a| {
            let (x, d) = a.in_water(ocean, map.water_level);
            (a.name.clone(), x, [d[0], d[1].max(d[0] + 10.0)])
        })
        .collect()
}

/// Size of the map picture's texture.
const PREVIEW_SIZE: [usize; 2] = [720, 260];

/// The depth shown a fraction `f` of the way down the map picture: stretched near the surface,
/// where life lives, squeezed in the deep.
fn depth_at(f: f32, height: f32) -> f32 {
    f.clamp(0.0, 1.0).powi(2) * height
}

/// Where depth `depth` sits down the map picture (0 = surface, 1 = bottom).
fn fraction_at(depth: f32, height: f32) -> f32 {
    (depth / height.max(1.0)).clamp(0.0, 1.0).sqrt()
}

fn build_preview(ctx: &egui::Context, map: &str, config: Config) -> Preview {
    let terrain = Terrain::generate(&config);
    let (w, h) = (config.world.width, config.world.height);
    let ocean = config.ocean_width();
    let half = config.light.half_depth.max(1.0);
    let [width, height] = PREVIEW_SIZE;
    let mut pixels = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let depth = depth_at((y as f32 + 0.5) / height as f32, h);
        let light = (-depth / half * std::f32::consts::LN_2).exp();
        for x in 0..width {
            let px = (x as f32 + 0.5) / width as f32 * w;
            let p = Vec2::new(px, h - depth);
            let [r, g, b] = if terrain.is_solid(p) {
                let shade = 0.75 + 0.25 * light;
                [
                    (96.0 * shade) as u8,
                    (82.0 * shade) as u8,
                    (66.0 * shade) as u8,
                ]
            } else {
                let l = light.sqrt();
                let cut_off = if px > ocean { 0.55 } else { 1.0 };
                [
                    ((8.0 + 40.0 * l) * cut_off) as u8,
                    ((24.0 + 130.0 * l) * cut_off) as u8,
                    ((48.0 + 140.0 * l) * cut_off) as u8,
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

/// The map, with where life starts: drag to draw that area, click to move it.
fn map_view(ui: &mut egui::Ui, setup: &mut Setup, p: &Preview) {
    let size = egui::vec2(ui.available_width(), 230.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.image(
        p.texture.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
    let (w, h) = (p.config.world.width, p.config.world.height);
    let ocean = p.config.ocean_width();
    let x_of = |x: f32| rect.left() + x / w * rect.width();
    let y_of = |depth: f32| rect.top() + fraction_at(depth, h) * rect.height();
    // A depth scale.
    for depth in [100.0, 300.0, 1000.0, 2500.0] {
        if depth >= h {
            continue;
        }
        let y = y_of(depth);
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0, rgba(220, 240, 240, 0.12)),
        );
        painter.text(
            egui::pos2(rect.left() + 4.0, y - 1.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{depth:.0}"),
            egui::FontId::proportional(11.0),
            rgba(220, 240, 240, 0.65),
        );
    }
    if ocean < w - 1.0 {
        painter.text(
            egui::pos2((x_of(ocean) + rect.right()) * 0.5, rect.top() + 14.0),
            egui::Align2::CENTER_CENTER,
            "cut off",
            egui::FontId::proportional(11.0),
            rgba(220, 240, 240, 0.6),
        );
    }
    for v in &p.vents {
        let c = egui::pos2(x_of(v.x), y_of(h - v.y));
        painter.circle_filled(c, 4.0, egui::Color32::from_rgb(255, 150, 60));
        painter.circle_stroke(c, 6.0, egui::Stroke::new(1.0, rgba(255, 180, 90, 0.6)));
    }
    // The map's start areas, faintly, with their names.
    for (name, x, d) in start_areas(p) {
        let r = egui::Rect::from_min_max(
            egui::pos2(x_of(x[0] * ocean), y_of(d[0])),
            egui::pos2(x_of(x[1] * ocean), y_of(d[1])),
        );
        painter.rect_stroke(
            r,
            2.0,
            egui::Stroke::new(1.0, rgba(150, 255, 180, 0.35)),
            egui::StrokeKind::Inside,
        );
        painter.text(
            r.left_top() + egui::vec2(4.0, 2.0),
            egui::Align2::LEFT_TOP,
            name,
            egui::FontId::proportional(10.5),
            rgba(190, 255, 210, 0.7),
        );
    }
    let area = egui::Rect::from_min_max(
        egui::pos2(x_of(setup.spawn_x[0] * ocean), y_of(setup.spawn_depth[0])),
        egui::pos2(x_of(setup.spawn_x[1] * ocean), y_of(setup.spawn_depth[1])),
    );
    painter.rect_filled(area, 3.0, rgba(140, 255, 170, 0.22));
    painter.rect_stroke(
        area,
        3.0,
        egui::Stroke::new(2.0, egui::Color32::from_rgb(150, 255, 180)),
        egui::StrokeKind::Outside,
    );

    // Pointer to the world: (fraction across the ocean, depth).
    let to_world = |pos: egui::Pos2| {
        let x = ((pos.x - rect.left()) / rect.width() * w / ocean).clamp(0.0, 1.0);
        let depth = depth_at((pos.y - rect.top()) / rect.height(), h);
        (x, depth)
    };
    if response.drag_started()
        && let Some(pos) = response.interact_pointer_pos()
    {
        setup.drag_from = Some(to_world(pos));
    }
    if response.dragged()
        && let (Some(a), Some(pos)) = (setup.drag_from, response.interact_pointer_pos())
    {
        let b = to_world(pos);
        let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
        let (d0, d1) = (a.1.min(b.1), a.1.max(b.1));
        setup.spawn_x = [x0, x1.max(x0 + 0.005)];
        setup.spawn_depth = [d0, d1.max(d0 + 10.0)];
    }
    if response.drag_stopped() {
        setup.drag_from = None;
    }
    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let (x, depth) = to_world(pos);
        let half_x = (setup.spawn_x[1] - setup.spawn_x[0]) * 0.5;
        let half_d = (setup.spawn_depth[1] - setup.spawn_depth[0]) * 0.5;
        let cx = x.clamp(half_x, 1.0 - half_x);
        let cd = depth.clamp(half_d, h - half_d);
        setup.spawn_x = [cx - half_x, cx + half_x];
        setup.spawn_depth = [cd - half_d, cd + half_d];
    }
    if let Some(pos) = response.hover_pos() {
        let (x, depth) = to_world(pos);
        response.on_hover_text_at_pointer(format!("{:.0}% across · depth {depth:.0}", x * 100.0));
    }
    ui.label(
        egui::RichText::new(
            "Drag to set where life starts, or click to move it. Orange dots are vents.",
        )
        .size(12.0)
        .color(MUTED),
    );
}

fn world_column(ui: &mut egui::Ui, setup: &mut Setup, preview: Option<&Preview>) {
    heading(ui, "The world");
    caption(ui, "Map");
    let about = setup
        .maps
        .iter()
        .find(|m| m.0 == setup.map)
        .map(|m| m.1.clone())
        .unwrap_or_default();
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("map")
            .width(150.0)
            .selected_text(&setup.map)
            .show_ui(ui, |ui| {
                for (name, about) in &setup.maps {
                    ui.selectable_value(&mut setup.map, name.clone(), name)
                        .on_hover_text(about);
                }
            });
        ui.label(egui::RichText::new(about).color(MUTED));
    });
    ui.add_space(6.0);
    if let Some(p) = preview {
        map_view(ui, setup, p);
    }

    caption(ui, "Where life starts");
    let depth_max = preview.map(|p| p.config.world.height).unwrap_or(6000.0);
    egui::Grid::new("where")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Across the ocean");
            ui.horizontal(|ui| {
                let mut lo = setup.spawn_x[0] * 100.0;
                let mut hi = setup.spawn_x[1] * 100.0;
                ui.add(
                    egui::DragValue::new(&mut lo)
                        .range(0.0..=100.0)
                        .suffix(" %")
                        .speed(0.5),
                );
                ui.label("to");
                ui.add(
                    egui::DragValue::new(&mut hi)
                        .range(0.0..=100.0)
                        .suffix(" %")
                        .speed(0.5),
                );
                setup.spawn_x = [lo.min(hi) / 100.0, hi.max(lo) / 100.0];
            });
            ui.end_row();
            ui.label("Depth");
            ui.horizontal(|ui| {
                let [mut lo, mut hi] = setup.spawn_depth;
                ui.add(
                    egui::DragValue::new(&mut lo)
                        .range(0.0..=depth_max)
                        .speed(2.0),
                );
                ui.label("to");
                ui.add(
                    egui::DragValue::new(&mut hi)
                        .range(0.0..=depth_max)
                        .speed(2.0),
                );
                setup.spawn_depth = [lo.min(hi), hi.max(lo + 10.0)];
            });
            ui.end_row();
        });
    // The map's own start areas (drawn in the map editor).
    if let Some(p) = preview {
        let areas = start_areas(p);
        if areas.is_empty() {
            ui.label(
                egui::RichText::new("This map has no start areas: drag one on the map above.")
                    .size(12.0)
                    .color(MUTED),
            );
        }
        ui.horizontal_wrapped(|ui| {
            for (name, x, d) in areas {
                let on = setup.spawn_x == x && setup.spawn_depth == d;
                if ui.selectable_label(on, name).clicked() {
                    setup.spawn_x = x;
                    setup.spawn_depth = d;
                }
            }
        });
    }

    caption(ui, "Seed");
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut setup.seed_text)
                .desired_width(320.0)
                .hint_text("any number or text"),
        );
        if ui.button("Random").clicked() {
            setup.seed_text = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| (d.as_nanos() as u64 % 1_000_000_000).to_string())
                .unwrap_or_else(|_| "1".into());
        }
    });
    // Text seeds are turned into a number: show which.
    if setup.seed_text.trim().parse::<u64>().is_err() {
        ui.label(
            egui::RichText::new(format!("Uses seed {}.", setup.seed()))
                .size(12.0)
                .color(MUTED),
        );
    }

    caption(ui, "Starting population");
    ui.add(egui::Slider::new(&mut setup.population, 20..=20_000).logarithmic(true));
}

fn ancestor_column(ui: &mut egui::Ui, setup: &mut Setup, preview: Option<&Preview>) {
    heading(ui, "The ancestor");
    let config = preview.map(|p| p.config.clone()).unwrap_or_default();
    caption(ui, "Preset");
    let mut preset = setup.preset;
    egui::ComboBox::from_id_salt("preset")
        .width(220.0)
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
        setup.sync_text();
    }
    ui.label(egui::RichText::new(PRESETS[setup.preset].about).color(MUTED));

    let traits = Traits::decode(&setup.genome, &config);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        crate::ui::swatch(ui, traits.color, 18.0);
        let parts = match traits.part_count {
            0 => "no body parts".to_string(),
            1 => "1 body part".to_string(),
            n => format!("{n} body parts"),
        };
        ui.label(
            egui::RichText::new(format!(
                "Adult radius {:.1}  ·  {}  ·  {}",
                traits.adult_radius,
                if traits.is_motile() {
                    "can move"
                } else {
                    "can't move"
                },
                parts
            ))
            .size(15.0),
        );
    });

    caption(ui, "Genes");
    ui.spacing_mut().slider_width = 170.0;
    let mut changed = false;
    egui::Grid::new("genes")
        .num_columns(3)
        .spacing([12.0, 5.0])
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
                        .color(MUTED),
                );
                ui.end_row();
            }
        });
    if changed {
        setup.sync_text();
    }

    caption(ui, "Genome");
    let mut errors: Vec<String> = Vec::new();
    let mut edited = false;
    egui::Grid::new("chromosomes")
        .num_columns(3)
        .spacing([10.0, 4.0])
        .show(ui, |ui| {
            for (k, c) in CHROMOSOMES.iter().enumerate() {
                egui::Frame::NONE
                    .fill(rgba(30, 110, 110, 0.9))
                    .corner_radius(5)
                    .inner_margin(egui::Margin::symmetric(7, 1))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(c.letter.to_string())
                                .monospace()
                                .strong()
                                .color(egui::Color32::WHITE),
                        );
                    });
                ui.label(egui::RichText::new(c.name).size(12.0).color(MUTED));
                let Some(text) = setup.chromosome_text.get_mut(k) else {
                    continue;
                };
                let edit = ui.add(
                    egui::TextEdit::singleline(text)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY),
                );
                let digits: String = text.chars().filter(|ch| !ch.is_whitespace()).collect();
                let want = c.genes.len() * 2;
                if digits.len() != want || !digits.chars().all(|ch| ch.is_ascii_hexdigit()) {
                    errors.push(format!(
                        "chromosome {} needs {} hex digits ({} genes), has {}",
                        c.letter,
                        want,
                        c.genes.len(),
                        digits.len()
                    ));
                } else if edit.changed() {
                    for (i, pair) in digits.as_bytes().chunks(2).enumerate() {
                        let hex = std::str::from_utf8(pair).unwrap_or("00");
                        setup.genome.0[c.start + i] = u8::from_str_radix(hex, 16).unwrap_or(0);
                    }
                    edited = true;
                }
                ui.end_row();
            }
        });
    if edited {
        setup.genome_text = setup.genome.to_string();
    }

    ui.add_space(4.0);
    egui::CollapsingHeader::new("Copy / paste")
        .id_salt("whole_genome")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Copy").clicked() {
                    ui.ctx().copy_text(setup.genome.to_string());
                }
            });
            let edit = ui.add(
                egui::TextEdit::multiline(&mut setup.genome_text)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            match Genome::parse(&setup.genome_text) {
                Ok(g) if edit.changed() => {
                    setup.genome = g;
                    let text = std::mem::take(&mut setup.genome_text);
                    setup.sync_text();
                    setup.genome_text = text;
                    errors.clear();
                }
                Ok(_) => {}
                Err(e) => {
                    ui.colored_label(ERROR, format!("Can't read this genome: {e}"));
                }
            }
        });
    setup.genome_error = errors.into_iter().next();

    ui.add_space(6.0);
    ui.checkbox(&mut setup.vent_life, "Add vent chemotrophs at each vent");
}

/// Build the world from the setup. An existing autosave is set aside under a dated name first.
fn create_world(config_path: &Path, setup: &Setup, save_dir: &Path) -> Result<World, String> {
    let mut config = Config::load_with_map(config_path, Some(&setup.map))?;
    config.world.seed = setup.seed();
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

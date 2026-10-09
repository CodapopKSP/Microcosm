//! Viewer for the evolution simulator: pan/zoom around the ocean, inspect organisms and species.
// Windows: no console window beside the game in builds for players.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod camera;
mod editor;
mod gallery;
mod menu;
mod music;
mod paths;
mod render;
mod sfx;
mod sim;
mod ui;

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::window::{MonitorSelection, PresentMode, WindowMode, WindowResolution};
use bevy_egui::EguiPlugin;
use clap::Parser;
use evo_core::{Config, World};

#[derive(Parser, Debug, Clone)]
#[command(about = "Microcosm: a living ocean, grown from a single microbe")]
pub struct Args {
    /// Config file for a new world (implies `--new`), or, with `--load`, a config to apply to the
    /// loaded world. Default: the built-in settings.
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// Start a new world even if an autosave exists.
    #[arg(long)]
    pub new: bool,
    /// Override the world seed.
    #[arg(long)]
    pub seed: Option<u64>,
    /// Use this map (one of yours, or a built-in one) instead of the config's `world.map`
    /// (implies `--new`).
    #[arg(long)]
    pub world_map: Option<String>,
    /// Resume a saved world (a .bin file).
    #[arg(long)]
    pub load: Option<PathBuf>,
    /// Where saves go. Default: the game's folder in your user data folder (see `paths.rs`).
    #[arg(long = "save-dir", value_name = "DIR")]
    pub save_dir_arg: Option<PathBuf>,
    /// The folder saves go in (`--save-dir`, else the default).
    #[arg(skip)]
    pub save_dir: PathBuf,
    /// Minutes between autosaves (0 = off).
    #[arg(long, default_value_t = 10.0)]
    pub autosave_minutes: f64,
    /// Start in fullscreen stream mode (UI hidden, auto camera).
    #[arg(long)]
    pub stream: bool,
    /// Disable vsync (for benchmarking).
    #[arg(long)]
    pub no_vsync: bool,
    /// Save a screenshot to this path after `--screenshot-delay` seconds, then exit.
    #[arg(long)]
    pub screenshot: Option<PathBuf>,
    #[arg(long, default_value_t = 4.0)]
    pub screenshot_delay: f32,
    /// Initial view as `x,y,zoom` (zoom = world units per pixel).
    #[arg(long, value_delimiter = ',')]
    pub view: Option<Vec<f32>>,
    /// Open the water inspector at world position `x,y` (as a right-click there does).
    #[arg(long, value_delimiter = ',')]
    pub probe: Option<Vec<f32>>,
    /// Select the organism nearest the initial view center.
    #[arg(long)]
    pub select: bool,
    /// Open the map editor.
    #[arg(long)]
    pub editor: bool,
    /// Open the charts window at startup.
    #[arg(long)]
    pub charts: bool,
    /// Open the charts window on the family tree.
    #[arg(long)]
    pub tree: bool,
    /// Follow a member of the first living species whose name contains this text.
    #[arg(long)]
    pub follow: Option<String>,
    /// Zoom (world units per pixel) to use with `--follow`.
    #[arg(long, default_value_t = 0.15)]
    pub follow_zoom: f32,
    /// Trigger an event at the start (plague, upwelling, impact, barrier). Repeatable.
    #[arg(long)]
    pub trigger: Vec<evo_core::events::Trigger>,
    /// Art test: a paused world with rows of organisms, each row sweeping one gene 00→FF.
    #[arg(long)]
    pub gallery: bool,
    /// Start with a chemistry map shown: oxygen, co2, nutrients, minerals or toxin (key M cycles).
    #[arg(long)]
    pub map: Option<String>,
}

/// Where the app is: the menus (splash, new world, load) or a running world.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    Menu,
    Running,
    /// The map editor.
    Editor,
}

/// The command line, for starting worlds later (from the menu).
#[derive(Resource, Clone)]
pub struct Launch(pub Args);

/// Viewer-wide options from the command line.
#[derive(Resource, Clone)]
pub struct ViewerOptions {
    pub stream: bool,
    pub screenshot: Option<PathBuf>,
    pub screenshot_delay: f32,
    pub view: Option<(f32, f32, f32)>,
    pub select: bool,
    pub charts: bool,
    pub tree: bool,
    pub follow: Option<String>,
    pub follow_zoom: f32,
    pub map: Option<String>,
    pub probe: Option<(f32, f32)>,
}

fn main() {
    let mut args = Args::parse();
    args.save_dir = args.save_dir_arg.clone().unwrap_or_else(paths::saves_dir);
    // Any flag that names or shapes a world goes straight in; otherwise the menu comes first.
    let direct = args.load.is_some()
        || args.new
        || args.config.is_some()
        || args.seed.is_some()
        || args.world_map.is_some()
        || args.gallery
        || args.stream
        || !args.trigger.is_empty()
        || args.view.is_some()
        || args.follow.is_some()
        || args.select
        || args.probe.is_some()
        || args.charts
        || args.tree;
    let world = direct.then(|| direct_world(&args));
    let start = if world.is_some() {
        AppState::Running
    } else if args.editor {
        AppState::Editor
    } else {
        AppState::Menu
    };

    let mut app = App::new();
    // A world given on the command line must exist before the first state transition.
    if let Some(world) = world {
        let mut state = sim::SimState::new(world, &args);
        if args.gallery {
            state.speed_index = 0;
        }
        app.insert_resource(state);
    }
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Microcosm".into(),
            resolution: WindowResolution::new(1600, 900),
            // Strict vsync: `AutoVsync` picks relaxed vsync where it can, which shows a late
            // frame at once and tears (on X11 a maximized window isn't composited, so it shows).
            present_mode: if args.no_vsync {
                PresentMode::AutoNoVsync
            } else {
                PresentMode::Fifo
            },
            ..default()
        }),
        ..default()
    }))
    .add_plugins(EguiPlugin::default())
    .insert_resource(ClearColor(Color::srgb(0.01, 0.02, 0.05)))
    .insert_resource(Launch(args.clone()))
    .insert_state(start)
    .insert_resource(ViewerOptions {
        stream: args.stream,
        screenshot: args.screenshot.clone(),
        screenshot_delay: args.screenshot_delay,
        view: args
            .view
            .as_ref()
            .filter(|v| v.len() == 3)
            .map(|v| (v[0], v[1], v[2]))
            .or(args.gallery.then(|| gallery::view_for(&Config::default()))),
        select: args.select,
        charts: args.charts || args.tree,
        tree: args.tree,
        follow: args.follow.clone(),
        follow_zoom: args.follow_zoom,
        map: args.map.clone(),
        probe: args
            .probe
            .as_ref()
            .filter(|v| v.len() == 2)
            .map(|v| (v[0], v[1])),
    })
    .add_plugins((
        sim::SimPlugin,
        camera::CameraPlugin,
        render::RenderPlugin,
        ui::UiPlugin,
        menu::MenuPlugin,
        editor::EditorPlugin,
        music::MusicPlugin,
        sfx::SfxPlugin,
    ))
    .add_systems(Update, (screenshot_and_exit, toggle_fullscreen))
    .run();
}

/// The world named by the command line: a save (`--load`, else the autosave unless `--new` or
/// another new-world flag), the gallery, or a new world from the config.
fn direct_world(args: &Args) -> World {
    // By default, pick up where the last session left off.
    let autosave = args.save_dir.join("autosave.bin");
    let resume = args.load.clone().or_else(|| {
        let fresh = args.new
            || args.config.is_some()
            || args.seed.is_some()
            || args.world_map.is_some()
            || args.screenshot.is_some();
        (!fresh && autosave.exists()).then(|| autosave.clone())
    });
    let mut world = if args.gallery {
        gallery::build()
    } else {
        let loaded = resume.as_ref().and_then(|path| {
            eprintln!("resuming {} (use --new for a fresh world)", path.display());
            match World::load(path) {
                Ok(w) => Some(w),
                // An autosave picked up implicitly that this version can't read is set aside.
                Err(e) if args.load.is_none() => {
                    let aside = path.with_extension("old.bin");
                    if let Err(err) = std::fs::rename(path, &aside) {
                        fail(&format!("failed to load {}: {e} ({err})", path.display()));
                    }
                    eprintln!(
                        "{e}\nmoved it to {} and starting a new world",
                        aside.display()
                    );
                    None
                }
                Err(e) => fail(&format!("failed to load {}: {e}", path.display())),
            }
        });
        match loaded {
            Some(mut w) => {
                if let Some(cfg_path) = &args.config {
                    let cfg = paths::load_config(Some(cfg_path), None).unwrap_or_else(|e| fail(&e));
                    w.replace_config(cfg).unwrap_or_else(|e| fail(&e));
                    eprintln!("applied {} to the loaded world", cfg_path.display());
                }
                w
            }
            None => {
                let mut config =
                    paths::load_config(args.config.as_deref(), args.world_map.as_deref())
                        .unwrap_or_else(|e| fail(&e));
                if let Some(seed) = args.seed {
                    config.world.seed = seed;
                }
                World::new(config)
            }
        }
    };
    for t in &args.trigger {
        world.trigger(*t);
    }
    world
}

/// F11: fullscreen on the window's current monitor, or back to a window.
fn toggle_fullscreen(
    keys: Res<ButtonInput<KeyCode>>,
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
) {
    if !keys.just_pressed(KeyCode::F11) {
        return;
    }
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    window.mode = match window.mode {
        WindowMode::Windowed => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
        _ => WindowMode::Windowed,
    };
}

/// `--screenshot`: capture the window once after a delay, then quit.
fn screenshot_and_exit(
    mut commands: Commands,
    options: Res<ViewerOptions>,
    time: Res<Time<Real>>,
    mut taken: Local<Option<f32>>,
    mut exit: MessageWriter<AppExit>,
) {
    use bevy::render::view::screenshot::{Screenshot, save_to_disk};
    let Some(path) = &options.screenshot else {
        return;
    };
    let now = time.elapsed_secs();
    match *taken {
        None if now >= options.screenshot_delay => {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path.clone()));
            *taken = Some(now);
        }
        Some(at) if now - at > 1.5 => {
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1);
}

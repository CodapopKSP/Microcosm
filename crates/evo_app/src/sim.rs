//! Runs the simulation inside the frame loop: fixed ticks at the chosen speed, within a time budget.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use evo_core::World;

use crate::Args;

/// Playback speeds (multiples of real time). 0 = paused.
pub const SPEEDS: [f64; 8] = [0.0, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 64.0];

/// Most of a frame the simulation may use before the frame is allowed to render.
const FRAME_BUDGET: Duration = Duration::from_millis(13);

#[derive(Resource)]
pub struct SimState {
    pub world: World,
    /// Index into [`SPEEDS`].
    pub speed_index: usize,
    /// Speed to go back to when unpausing.
    resume_speed: usize,
    /// Real seconds of sim time owed (scaled by speed).
    accumulator: f64,
    /// Interpolation factor between the previous and current tick positions.
    pub alpha: f32,
    /// Measured ticks per real second.
    pub tps: f64,
    /// Measured speed as a multiple of real time (may be below the target when the sim can't keep up).
    pub actual_speed: f64,
    tps_ticks: u64,
    tps_since: Instant,
    pub save_dir: PathBuf,
    autosave_every: Option<Duration>,
    last_save: Instant,
    pub last_save_message: Option<(String, Instant)>,
    /// Mean milliseconds of sim work per frame (for the debug readout).
    pub sim_ms: f64,
}

impl SimState {
    pub fn new(world: World, args: &Args) -> Self {
        Self {
            world,
            speed_index: 2,
            resume_speed: 2,
            accumulator: 0.0,
            alpha: 1.0,
            tps: 0.0,
            actual_speed: 0.0,
            tps_ticks: 0,
            tps_since: Instant::now(),
            save_dir: args.save_dir.clone(),
            autosave_every: (args.autosave_minutes > 0.0)
                .then(|| Duration::from_secs_f64(args.autosave_minutes * 60.0)),
            last_save: Instant::now(),
            last_save_message: None,
            sim_ms: 0.0,
        }
    }

    /// Never saves (the menu's backdrop world).
    pub fn without_autosave(mut self) -> Self {
        self.autosave_every = None;
        self
    }

    pub fn speed(&self) -> f64 {
        SPEEDS[self.speed_index]
    }

    pub fn paused(&self) -> bool {
        self.speed() == 0.0
    }

    /// Pause, or resume at the speed it had before.
    pub fn toggle_pause(&mut self) {
        if self.paused() {
            self.speed_index = self.resume_speed.max(1);
        } else {
            self.resume_speed = self.speed_index;
            self.speed_index = 0;
        }
    }

    /// Save to `save_dir/name`, keeping the previous file as `*.prev.bin`.
    pub fn save(&mut self, name: &str) {
        let path = self.save_dir.join(name);
        let result = std::fs::create_dir_all(&self.save_dir)
            .map_err(|e| e.to_string())
            .and_then(|_| {
                if path.exists() {
                    std::fs::rename(&path, path.with_extension("prev.bin"))
                        .map_err(|e| e.to_string())?;
                }
                self.world.save(&path)
            });
        let msg = match result {
            Ok(()) => format!("Saved {}", path.display()),
            Err(e) => format!("Save failed: {e}"),
        };
        info!("{msg}");
        self.last_save_message = Some((msg, Instant::now()));
        self.last_save = Instant::now();
    }
}

/// Organism and species the viewer is focused on.
#[derive(Resource, Default)]
pub struct Selection {
    pub organism: Option<u64>,
    /// Highlight every member of this species.
    pub species: Option<u32>,
    /// Keep the camera on the selected organism.
    pub follow: bool,
    /// A spot whose water is being inspected (right-click).
    pub probe: Option<glam::Vec2>,
}

/// The simulation step for this frame; camera, rendering and UI run after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimStep;

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Selection>()
            .add_systems(
                Update,
                (
                    speed_keys.run_if(in_state(crate::AppState::Running)),
                    run_simulation.run_if(resource_exists::<SimState>),
                )
                    .chain()
                    .in_set(SimStep),
            )
            .add_systems(Last, save_on_exit)
            .add_systems(
                OnExit(crate::AppState::Running),
                |mut commands: Commands| {
                    commands.remove_resource::<SimState>();
                },
            );
    }
}

fn run_simulation(mut state: ResMut<SimState>, time: Res<Time<Real>>) {
    let frame_start = Instant::now();
    let state = &mut *state;
    let tick_dt = state.world.config.time.dt as f64;
    if !state.paused() {
        state.accumulator += time.delta_secs_f64().min(0.25) * state.speed();
        while state.accumulator >= tick_dt {
            state.world.step();
            state.accumulator -= tick_dt;
            state.tps_ticks += 1;
            if frame_start.elapsed() > FRAME_BUDGET {
                // Can't keep up: drop the backlog instead of spiraling.
                state.accumulator = state.accumulator.min(tick_dt * 0.999);
                break;
            }
        }
        state.alpha = (state.accumulator / tick_dt).clamp(0.0, 1.0) as f32;
    } else {
        state.alpha = 1.0;
    }
    let ms = frame_start.elapsed().as_secs_f64() * 1000.0;
    state.sim_ms = state.sim_ms * 0.95 + ms * 0.05;

    let since = state.tps_since.elapsed().as_secs_f64();
    if since >= 1.0 {
        state.tps = state.tps_ticks as f64 / since;
        state.actual_speed = state.tps * tick_dt;
        state.tps_ticks = 0;
        state.tps_since = Instant::now();
    }

    if let Some(every) = state.autosave_every
        && state.last_save.elapsed() >= every
    {
        state.save("autosave.bin");
    }
}

fn speed_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<SimState>,
    egui_input: Res<bevy_egui::input::EguiWantsInput>,
) {
    if egui_input.wants_any_keyboard_input() {
        return;
    }
    // (Ctrl + digits pick up Acts of God, not speeds.)
    if keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight) {
        return;
    }
    if keys.just_pressed(KeyCode::Space) {
        state.toggle_pause();
    }
    let digits = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
    ];
    for (i, key) in digits.iter().enumerate() {
        if keys.just_pressed(*key) {
            state.speed_index = i + 1;
        }
    }
    if keys.just_pressed(KeyCode::Equal) || keys.just_pressed(KeyCode::NumpadAdd) {
        state.speed_index = (state.speed_index + 1).min(SPEEDS.len() - 1);
    }
    if keys.just_pressed(KeyCode::Minus) || keys.just_pressed(KeyCode::NumpadSubtract) {
        state.speed_index = state.speed_index.saturating_sub(1);
    }
    if keys.just_pressed(KeyCode::F5) {
        state.save("quicksave.bin");
    }
}

/// Save when the app is closing (window closed or quit), unless autosave is off.
fn save_on_exit(
    mut exits: MessageReader<AppExit>,
    state: Option<ResMut<SimState>>,
    options: Res<crate::ViewerOptions>,
    app_state: Res<State<crate::AppState>>,
) {
    let Some(mut state) = state else { return };
    if *app_state.get() != crate::AppState::Running {
        return;
    }
    if exits.read().next().is_some()
        && state.autosave_every.is_some()
        && options.screenshot.is_none()
    {
        state.save("autosave.bin");
    }
}

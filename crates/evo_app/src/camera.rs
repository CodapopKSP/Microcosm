//! Camera: pan, zoom, follow, click-to-select, and the stream-mode director.

use bevy::camera::Hdr;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::input::EguiWantsInput;

use crate::sim::{Selection, SimState};

#[derive(Component)]
pub struct MainCamera;

/// Camera state kept in world units (the projection scale is derived from `zoom`).
#[derive(Resource)]
pub struct CameraState {
    pub center: Vec2,
    /// World units per screen pixel.
    pub zoom: f32,
    /// Smoothed targets.
    target_center: Vec2,
    target_zoom: f32,
    /// Stream mode: the camera wanders between interesting organisms on its own.
    pub director: bool,
    director_timer: f32,
    drag_origin: Option<Vec2>,
    /// Where a right-button press started (a release close by is a click, not a pan).
    right_origin: Option<Vec2>,
    /// Events already seen by the director (so it only reacts to new ones).
    seen_events: u64,
}

impl CameraState {
    /// A still camera on `center` at `zoom` (the menu's backdrop).
    pub fn fixed(center: Vec2, zoom: f32) -> Self {
        Self {
            center,
            zoom,
            target_center: center,
            target_zoom: zoom,
            director: false,
            director_timer: 0.0,
            drag_origin: None,
            right_origin: None,
            seen_events: 0,
        }
    }

    /// Jump straight to a view (no easing).
    pub fn set_now(&mut self, center: Vec2, zoom: f32) {
        self.center = center;
        self.target_center = center;
        self.zoom = zoom;
        self.target_zoom = zoom;
    }

    pub fn focus(&mut self, center: Vec2, zoom: Option<f32>) {
        self.target_center = center;
        if let Some(z) = zoom {
            self.target_zoom = z;
        }
    }
}

/// A scripted camera move, for recording footage (set up in the hidden god panel): readied, it
/// waits on the start view with the game paused; when the game is unpaused it holds there for
/// `lead` seconds, then glides to the end view, the same every take. It runs with the game:
/// pausing pauses it too.
#[derive(Resource)]
pub struct CameraMove {
    /// Views as (center, zoom).
    pub start: Option<(Vec2, f32)>,
    pub end: Option<(Vec2, f32)>,
    /// Seconds of gliding.
    pub duration: f32,
    /// Seconds held on the start view first (time to get the recording going).
    pub lead: f32,
    /// Seconds it has run (while the game was running); `None` when it isn't playing.
    pub playing: Option<f32>,
    /// Waiting on the start view for the game to be unpaused.
    pub ready: bool,
}

impl Default for CameraMove {
    fn default() -> Self {
        Self {
            start: None,
            end: None,
            duration: 6.0,
            lead: 1.0,
            playing: None,
            ready: false,
        }
    }
}

impl CameraMove {
    /// Get ready to play, if both views are set: it starts when the game next runs.
    pub fn ready_up(&mut self) {
        if self.start.is_some() && self.end.is_some() {
            self.ready = true;
            self.playing = None;
        }
    }

    /// The view `elapsed` seconds in, and whether the move is over. Eases in and out; zoom
    /// changes evenly in scale, and the center moves with it so one point stays put on screen
    /// (a zoom out reveals around where it started rather than sliding off).
    pub fn view_at(&self, elapsed: f32) -> Option<(Vec2, f32, bool)> {
        let ((c0, z0), (c1, z1)) = (self.start?, self.end?);
        let p = ((elapsed - self.lead) / self.duration.max(0.01)).clamp(0.0, 1.0);
        let e = p * p * p * (p * (p * 6.0 - 15.0) + 10.0);
        let zoom = z0 * (z1 / z0).powf(e);
        let w = if (z1 - z0).abs() > 1e-6 * z0 {
            (zoom - z0) / (z1 - z0)
        } else {
            e
        };
        Some((c0 + (c1 - c0) * w, zoom, p >= 1.0))
    }
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraMove>()
            .add_systems(Startup, spawn_camera)
            .add_systems(
                OnEnter(crate::AppState::Running),
                (start_camera, open_probe).chain(),
            )
            .add_systems(
                Update,
                (camera_input, director, follow_selection, play_move, apply_camera)
                    .chain()
                    .after(crate::sim::SimStep)
                    .run_if(in_state(crate::AppState::Running)),
            )
            .add_systems(
                Update,
                apply_camera
                    .after(crate::sim::SimStep)
                    .run_if(
                        in_state(crate::AppState::Menu)
                            .or_else(in_state(crate::AppState::GenomeEditor)),
                    )
                    .run_if(resource_exists::<CameraState>)
                    .run_if(resource_exists::<SimState>),
            );
    }
}

/// Play the camera move: wait on its start view while it's ready, start when the game runs,
/// and advance only while the game runs (Escape cancels it). It overrides the other camera
/// controls meanwhile.
fn play_move(
    mut cam: ResMut<CameraState>,
    mut shot: ResMut<CameraMove>,
    mut selection: ResMut<Selection>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Real>>,
    state: Res<SimState>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        shot.ready = false;
        shot.playing = None;
        return;
    }
    if shot.ready {
        if !state.paused() {
            shot.ready = false;
            shot.playing = Some(0.0);
        } else if let Some((center, zoom)) = shot.start {
            cam.set_now(center, zoom);
            cam.director = false;
            selection.follow = false;
            return;
        }
    }
    let Some(elapsed) = shot.playing else { return };
    let running = if state.paused() { 0.0 } else { time.delta_secs() };
    let elapsed = elapsed + running;
    let Some((center, zoom, done)) = shot.view_at(elapsed) else {
        shot.playing = None;
        return;
    };
    cam.set_now(center, zoom);
    cam.director = false;
    selection.follow = false;
    shot.playing = (!done).then_some(elapsed);
}

/// `--probe x,y`: open the water inspector there at startup.
fn open_probe(options: Res<crate::ViewerOptions>, mut selection: ResMut<Selection>) {
    if let Some((x, y)) = options.probe {
        selection.probe = Some(glam::Vec2::new(x, y));
    }
}

/// The one camera, for the whole app (the menus draw on it too).
fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        MainCamera,
        Hdr,
        Bloom {
            intensity: 0.18,
            ..Bloom::NATURAL
        },
        Transform::from_xyz(0.0, 0.0, 100.0),
    ));
}

/// Point the camera at a world that has just started.
fn start_camera(
    mut commands: Commands,
    state: Res<SimState>,
    options: Res<crate::ViewerOptions>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
) {
    commands.insert_resource(Selection::default());
    let (w, h) = (
        state.world.config.world.width,
        state.world.config.world.height,
    );
    // Start looking at the sunlit shallows in the middle of the world.
    let (center, zoom) = match options.view {
        Some((x, y, z)) => (Vec2::new(x, y), z),
        None => (Vec2::new(w * 0.5, h - 450.0), 1.6),
    };
    let mut center = center;
    let mut zoom = zoom;
    if options.follow.as_deref() == Some("carcass") {
        // Dev helper: look at the largest carcass.
        if let Some(c) = state
            .world
            .carcasses
            .iter()
            .max_by(|a, b| a.radius.partial_cmp(&b.radius).unwrap())
        {
            center = Vec2::new(c.pos.x, c.pos.y);
            zoom = options.follow_zoom;
        }
    } else if let Some(name) = &options.follow {
        let needle = name.to_lowercase();
        let species = state
            .world
            .species
            .living()
            .find(|s| s.name.to_lowercase().contains(&needle))
            .map(|s| s.id);
        let member = species.and_then(|id| state.world.organisms.iter().find(|o| o.species == id));
        if let Some(o) = member {
            center = Vec2::new(o.pos.x, o.pos.y);
            zoom = options.follow_zoom;
            commands.insert_resource(crate::sim::Selection {
                organism: Some(o.id),
                species,
                follow: true,
                probe: None,
            });
        } else {
            eprintln!("--follow: no living species matching '{name}'");
        }
    }
    if options.select {
        let target = glam::Vec2::new(center.x, center.y);
        let nearest = state.world.organisms.iter().min_by(|a, b| {
            a.pos
                .distance(target)
                .partial_cmp(&b.pos.distance(target))
                .unwrap()
        });
        if let Some(o) = nearest {
            commands.insert_resource(crate::sim::Selection {
                organism: Some(o.id),
                species: None,
                follow: true,
                probe: None,
            });
        }
    }
    commands.insert_resource(CameraState {
        center,
        zoom,
        target_center: center,
        target_zoom: zoom,
        director: options.stream,
        director_timer: 0.0,
        drag_origin: None,
        right_origin: None,
        seen_events: state.world.events.count,
    });
    for mut t in &mut cameras {
        t.translation = Vec3::new(center.x, center.y, 100.0);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn camera_input(
    mut cam: ResMut<CameraState>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    motion: Res<AccumulatedMouseMotion>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    egui_input: Res<EguiWantsInput>,
    mut selection: ResMut<Selection>,
    state: Res<SimState>,
    time: Res<Time<Real>>,
    mut sound: ResMut<crate::ui_sfx::UiSoundRequest>,
) {
    let Ok(window) = windows.single() else { return };
    let Ok((camera, cam_transform)) = cameras.single() else {
        return;
    };
    let dt = time.delta_secs().min(0.1);
    let world = &state.world;
    let pointer_free = !egui_input.wants_any_pointer_input();
    let keys_free = !egui_input.wants_any_keyboard_input();
    let cursor_world = window
        .cursor_position()
        .and_then(|c| camera.viewport_to_world_2d(cam_transform, c).ok());

    // Keyboard pan.
    if keys_free {
        let mut dir = Vec2::ZERO;
        if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
            dir.x -= 1.0;
        }
        if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
            dir.x += 1.0;
        }
        if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
            dir.y += 1.0;
        }
        if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
            dir.y -= 1.0;
        }
        if dir != Vec2::ZERO {
            let step = dir.normalize() * 900.0 * cam.zoom * dt;
            cam.target_center += step;
            selection.follow = false;
            cam.director = false;
        }
        if keys.just_pressed(KeyCode::KeyF) {
            selection.follow = !selection.follow && selection.organism.is_some();
        }
        if keys.just_pressed(KeyCode::KeyC) {
            cam.director = !cam.director;
        }
        if keys.just_pressed(KeyCode::Home) {
            // As much of the world as fits.
            let bounds = view_bounds(world);
            let zoom = max_zoom(bounds, window);
            cam.focus(bounds.center(), Some(zoom));
            selection.follow = false;
            cam.director = false;
        }
        if keys.just_pressed(KeyCode::Escape) {
            selection.organism = None;
            selection.species = None;
            selection.follow = false;
            selection.probe = None;
        }
    }

    // Zoom toward the cursor.
    if pointer_free && scroll.delta.y != 0.0 {
        let lines = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y,
            MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
        };
        let factor = 0.85f32.powf(lines);
        let old_zoom = cam.target_zoom;
        let new_zoom = (old_zoom * factor).clamp(MIN_ZOOM, max_zoom(view_bounds(world), window));
        if let Some(cursor) = cursor_world {
            // Keep the point under the cursor fixed.
            let offset = cursor - cam.target_center;
            cam.target_center = cursor - offset * (new_zoom / old_zoom);
        }
        cam.target_zoom = new_zoom;
        cam.director = false;
    }

    // Drag to pan (left, right, or middle button).
    let dragging = mouse.pressed(MouseButton::Right)
        || mouse.pressed(MouseButton::Middle)
        || (mouse.pressed(MouseButton::Left) && cam.drag_origin.is_some());
    if mouse.just_pressed(MouseButton::Left) && pointer_free {
        cam.drag_origin = window.cursor_position();
    }
    if dragging && pointer_free && motion.delta != Vec2::ZERO {
        let moved_far = match (cam.drag_origin, window.cursor_position()) {
            (Some(a), Some(b)) => a.distance(b) > 4.0,
            _ => true,
        };
        if moved_far || !mouse.pressed(MouseButton::Left) {
            let delta = Vec2::new(-motion.delta.x, motion.delta.y) * cam.zoom;
            cam.target_center += delta;
            cam.center += delta;
            selection.follow = false;
            cam.director = false;
        }
    }

    // Right-click (without dragging) inspects the water there.
    if mouse.just_pressed(MouseButton::Right) && pointer_free {
        cam.right_origin = window.cursor_position();
    }
    if mouse.just_released(MouseButton::Right) {
        let was_click = match (cam.right_origin.take(), window.cursor_position()) {
            (Some(a), Some(b)) => a.distance(b) <= 4.0,
            _ => false,
        };
        if was_click
            && pointer_free
            && let Some(p) = cursor_world
        {
            selection.probe = Some(glam::Vec2::new(p.x, p.y));
        }
    }

    // Click (without dragging) selects.
    if mouse.just_released(MouseButton::Left) {
        let was_click = match (cam.drag_origin, window.cursor_position()) {
            (Some(a), Some(b)) => a.distance(b) <= 4.0,
            _ => false,
        };
        cam.drag_origin = None;
        if was_click
            && pointer_free
            && let Some(p) = cursor_world
        {
            let slack = 6.0 * cam.zoom;
            selection.organism = world.pick(glam::Vec2::new(p.x, p.y), slack);
            if selection.organism.is_some() {
                selection.follow = true;
                sound.0 = Some(crate::ui_sfx::UiSound::OrganismSelect);
            }
        }
    }
}

/// Stream mode: periodically pick something interesting and glide to it.
fn director(
    mut cam: ResMut<CameraState>,
    mut selection: ResMut<Selection>,
    state: Res<SimState>,
    time: Res<Time<Real>>,
) {
    if !cam.director {
        cam.seen_events = state.world.events.count;
        return;
    }
    cam.director_timer -= time.delta_secs();
    // New events with a place (meteor, rock wall, upwelling, vent) take priority.
    let new_events = state.world.events.count.saturating_sub(cam.seen_events) as usize;
    cam.seen_events = state.world.events.count;
    let located = state
        .world
        .events
        .entries
        .iter()
        .rev()
        .take(new_events)
        .find_map(|e| e.pos);
    if let Some(p) = located {
        selection.organism = None;
        selection.follow = false;
        cam.focus(Vec2::new(p.x, p.y), Some(2.5));
        cam.director_timer = 30.0;
        return;
    }
    let current_alive = selection
        .organism
        .and_then(|id| state.world.find(id))
        .is_some();
    if cam.director_timer > 0.0 && (current_alive || selection.organism.is_none()) {
        return;
    }
    cam.director_timer = 25.0;
    let world = &state.world;
    if world.organisms.is_empty() {
        return;
    }
    // Prefer rare and large organisms, hunters, and members of young species.
    let mut best: Option<(f32, u64)> = None;
    let seed = (time.elapsed_secs_f64() * 1000.0) as u64;
    let mut rng = evo_core::rng::Rng::new(seed);
    for _ in 0..400 {
        let o = &world.organisms[rng.below(world.organisms.len() as u32) as usize];
        let species = world.species.get(o.species);
        let pop = species.map(|s| s.population).unwrap_or(1).max(1) as f32;
        let young = species
            .map(|s| 1.0 / (1.0 + (world.tick - s.founded) as f32 / 20_000.0))
            .unwrap_or(0.0);
        let score = o.radius.ln_1p() * 2.0
            + o.traits.hunt * 2.0
            + o.traits.is_motile() as u32 as f32
            + (o.traits.part_count as f32).min(12.0) * 0.25
            + (o.traits.lure > 0.0) as u32 as f32
            + 3.0 / pop.sqrt()
            + young * 2.0
            + rng.f32();
        if best.is_none_or(|(s, _)| score > s) {
            best = Some((score, o.id));
        }
    }
    if let Some((_, id)) = best {
        selection.organism = Some(id);
        selection.follow = true;
        if let Some(o) = world.find(id) {
            let zoom = (o.radius / 12.0).clamp(0.15, 3.0);
            cam.focus(Vec2::new(o.pos.x, o.pos.y), Some(zoom));
        }
    }
}

fn follow_selection(mut cam: ResMut<CameraState>, selection: Res<Selection>, state: Res<SimState>) {
    if !selection.follow {
        return;
    }
    if let Some(o) = selection.organism.and_then(|id| state.world.find(id)) {
        let p = o.prev_pos.lerp(o.pos, state.alpha);
        cam.target_center = Vec2::new(p.x, p.y);
    }
}

/// How much sky may show above the water surface, in world units.
const SKY: f32 = 1600.0;

/// The part of the world the camera may show: the ocean and a strip of sky.
/// The closest zoom in (world units per screen pixel).
pub(crate) const MIN_ZOOM: f32 = 0.05;

pub(crate) fn view_bounds(world: &evo_core::World) -> Rect {
    let (w, h) = (world.config.world.width, world.config.world.height);
    // Room for the tallest land on a painted map, too.
    let land = world.terrain.land.iter().cloned().fold(0.0, f32::max);
    Rect::new(0.0, 0.0, w, h + SKY.max(land + 300.0))
}

/// The furthest zoom out (world units per pixel) at which the window still sees only `bounds`.
/// UI panels float over the world and don't change it, so the world's edges stay put when
/// they open and close.
pub(crate) fn max_zoom(bounds: Rect, window: &Window) -> f32 {
    (bounds.width() / window.width().max(1.0)).min(bounds.height() / window.height().max(1.0))
}

/// Keep a view of `zoom` centered on `center` inside `bounds`.
fn clamp_center(center: Vec2, zoom: f32, bounds: Rect, window: &Window) -> Vec2 {
    let half = Vec2::new(window.width(), window.height()) * 0.5 * zoom;
    let lo = bounds.min + half;
    let hi = bounds.max - half;
    let fit = |c: f32, lo: f32, hi: f32| {
        if hi < lo {
            (lo + hi) * 0.5
        } else {
            c.clamp(lo, hi)
        }
    };
    Vec2::new(fit(center.x, lo.x, hi.x), fit(center.y, lo.y, hi.y))
}

fn apply_camera(
    mut cam: ResMut<CameraState>,
    mut cameras: Query<(&mut Transform, &mut Projection), With<MainCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    state: Res<SimState>,
    time: Res<Time<Real>>,
) {
    let dt = time.delta_secs().min(0.1);
    let bounds = view_bounds(&state.world);
    let k = 1.0 - (-dt * 10.0).exp();
    if let Ok(window) = windows.single() {
        // Never zoom out past the edges of the world, and never look past them.
        let most = max_zoom(bounds, window);
        cam.target_zoom = cam.target_zoom.min(most);
        cam.zoom += (cam.target_zoom - cam.zoom) * k;
        cam.zoom = cam.zoom.min(most);
        cam.target_center = clamp_center(cam.target_center, cam.target_zoom, bounds, window);
        let center = cam.center + (cam.target_center - cam.center) * k;
        cam.center = clamp_center(center, cam.zoom, bounds, window);
    } else {
        cam.zoom += (cam.target_zoom - cam.zoom) * k;
        let target = cam.target_center.clamp(bounds.min, bounds.max);
        cam.target_center = target;
        let center = cam.center;
        cam.center = center + (target - center) * k;
    }
    let Ok((mut transform, mut projection)) = cameras.single_mut() else {
        return;
    };
    transform.translation.x = cam.center.x;
    transform.translation.y = cam.center.y;
    if let Projection::Orthographic(ortho) = &mut *projection {
        ortho.scale = cam.zoom;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_camera_move_holds_then_glides_end_to_end() {
        let shot = CameraMove {
            start: Some((Vec2::new(100.0, 50.0), 0.5)),
            end: Some((Vec2::new(2000.0, 900.0), 8.0)),
            duration: 4.0,
            lead: 1.0,
            playing: None,
            ready: false,
        };
        let at = |t: f32| shot.view_at(t).unwrap();
        assert_eq!(at(0.5), (Vec2::new(100.0, 50.0), 0.5, false), "held on the start");
        let (c, z, done) = at(5.0);
        assert!(done && (z - 8.0).abs() < 1e-4 && c.distance(Vec2::new(2000.0, 900.0)) < 1e-2);
        // Zoom grows steadily, and one world point stays put on screen throughout.
        let (c0, c1) = (Vec2::new(100.0, 50.0), Vec2::new(2000.0, 900.0));
        let fixed = c0 + (c0 - c1) * 0.5 / (8.0 - 0.5);
        let screen = |(c, z, _): (Vec2, f32, bool)| (fixed - c) / z;
        let mut last = 0.5;
        for k in 1..40 {
            let v = at(1.0 + k as f32 * 0.1);
            assert!(v.1 >= last, "zoom only grows");
            last = v.1;
            assert!(screen(v).distance(screen(at(0.0))) < 1e-2, "anchor drifted at {k}");
        }
    }
}

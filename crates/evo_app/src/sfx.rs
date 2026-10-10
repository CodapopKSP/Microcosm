//! Sound effects: a splash when an organism on screen makes a sudden stroke (its speed jumps
//! by more than [`SPLASH_KICK`] in a frame). Heard only when zoomed in close: loudest at the
//! closest zoom, fading fast, silent from a zoom of [`AUDIBLE_ZOOM`] out.
//! Panned left or right by where it is on screen: each sound comes pre-mixed at [`PANS`]
//! positions from hard left to hard right, and the nearest one plays.
//!
//! When a hunter on screen swallows its prey whole, one of the eat sounds plays (panned and
//! faded with zoom like the splashes).
//!
//! Splashes and swallows are pitched by body size: as recorded at radius 15, up to two whole
//! steps higher for the smallest bodies and two lower for the biggest (see [`pitch`]).
//!
//! Under them, a quiet loop of running water, and the rumble of any vents nearby (louder the
//! stronger and nearer the middle of the screen they are, panned by where they are), heard at
//! the same zooms.

use std::collections::{HashMap, VecDeque};

use bevy::audio::{AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, PlaybackSettings, Volume};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use evo_core::rng::Rng;

use crate::AppState;
use crate::camera::{CameraState, MIN_ZOOM};
use crate::sim::SimState;

// ---- Tuning ----

/// Speed gained in one frame (units/s) that counts as a stroke worth a splash. Most strokes
/// gain under 1; the sharpest few in a hundred gain 2 or more.
pub const SPLASH_KICK: f32 = 0.8;
/// Kicks this many times [`SPLASH_KICK`] or more play at full loudness (gentler ones softer).
const HARD_KICK: f32 = 4.0;
/// The zoom (world units per screen pixel) from which sounds are silent; the closest zoom is
/// [`MIN_ZOOM`].
pub const AUDIBLE_ZOOM: f32 = 1.2;
/// How much quieter sounds are (decibels) by the far end of that range: they fall steadily,
/// the same number of decibels per zoom step (see [`splash_volume`]).
const FALLOFF_DB: f32 = 15.0;
/// Loudness at the closest zoom (1 = as recorded).
const MAX_VOLUME: f32 = 1.0;
/// At most this many splashes start each second, and this many play at once.
const MAX_PER_SECOND: usize = 14;
const MAX_AT_ONCE: usize = 10;
/// The same organism splashes at most this often (seconds).
const ORGANISM_COOLDOWN: f32 = 0.5;

/// Pan positions the sounds are mixed at, from hard left to hard right (each file's suffix
/// `_p0`..`_p4`); the left and right gains are min(1, 1 ∓ pan).
pub const PANS: [f32; 5] = [-0.9, -0.45, 0.0, 0.45, 0.9];

/// A sound at each pan position.
macro_rules! panned {
    ($name:literal) => {
        [
            include_bytes!(concat!("../assets/audio/", $name, "_p0.ogg")).as_slice(),
            include_bytes!(concat!("../assets/audio/", $name, "_p1.ogg")).as_slice(),
            include_bytes!(concat!("../assets/audio/", $name, "_p2.ogg")).as_slice(),
            include_bytes!(concat!("../assets/audio/", $name, "_p3.ogg")).as_slice(),
            include_bytes!(concat!("../assets/audio/", $name, "_p4.ogg")).as_slice(),
        ]
    };
}

/// The splashes, one picked at random each time.
const SPLASHES: [[&[u8]; 5]; 12] = [
    panned!("splash1"),
    panned!("splash2"),
    panned!("splash3"),
    panned!("splash4"),
    panned!("splash5"),
    panned!("splash6"),
    panned!("splash7"),
    panned!("splash8"),
    panned!("splash9"),
    panned!("splash10"),
    panned!("splash11"),
    panned!("splash12"),
];

/// The eat sounds (prey swallowed whole), one picked at random each time.
const EATS: [[&[u8]; 5]; 7] = [
    panned!("eat1"),
    panned!("eat2"),
    panned!("eat3"),
    panned!("eat4"),
    panned!("eat5"),
    panned!("eat6"),
    panned!("eat7"),
];
/// Loudness of a swallow at the closest zoom (bigger hunters are louder, up to this): 4 dB
/// below the recording.
pub const EAT_VOLUME: f32 = 0.63;
/// Hunters this big (radius) or bigger make the full sound; smaller ones are quieter (see
/// [`eat_size`]).
const EAT_FULL_RADIUS: f32 = 30.0;
/// Sounds play at their recorded pitch for a body this big (radius)…
const PITCH_BASE_RADIUS: f32 = 15.0;
/// …up to this many semitones higher for the smallest bodies (radius 1)…
const PITCH_SMALL_RADIUS: f32 = 1.0;
/// …and as many lower from this radius up.
const PITCH_LARGE_RADIUS: f32 = 30.0;
/// Two whole steps.
const PITCH_SEMITONES: f32 = 4.0;
/// Random pitch variation between sounds, ± this fraction (about half a semitone).
const PITCH_JITTER: f32 = 0.03;

/// At most this many eat sounds start each second.
const MAX_EATS_PER_SECOND: usize = 8;

/// Loudness of a full-strength vent in the middle of the screen at the closest zoom.
pub const VENT_VOLUME: f32 = 3.0;
/// How far from the middle of the screen a vent is still heard, in half-screens (1 = at the
/// screen's edge); it fades toward there.
const VENT_REACH: f32 = 2.6;
/// Where a vent's sound comes from: this far above the vent's point, up at its mouths (the
/// chimneys' outlets sit 12–58 above it) rather than at its base.
const VENT_SOUND_HEIGHT: f32 = 45.0;
/// The vent rumble at each pan position (a seamless loop).
const VENT: [&[u8]; 5] = panned!("vent");

/// Loudness of the running-water loop at the closest zoom (it follows the same zoom curve as
/// the splashes).
pub const WATER_VOLUME: f32 = 0.35;
/// How quickly the water's loudness follows the zoom (per second; higher = quicker).
const WATER_EASE: f32 = 6.0;
const WATER: &[u8] = include_bytes!("../assets/audio/running_water.ogg");

pub struct SfxPlugin;

impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            splashes
                .after(crate::sim::SimStep)
                .run_if(in_state(AppState::Running))
                .run_if(resource_exists::<SimState>),
        )
        .add_systems(
            Update,
            eats.after(crate::sim::SimStep)
                .run_if(in_state(AppState::Running))
                .run_if(resource_exists::<SimState>),
        )
        .add_systems(OnEnter(AppState::Running), (start_water, start_vents))
        .add_systems(
            Update,
            (water_volume, vent_volume)
                .run_if(in_state(AppState::Running))
                .run_if(resource_exists::<SimState>),
        )
        .add_systems(OnExit(AppState::Running), clear);
    }
}

/// A splash playing.
#[derive(Component)]
struct Splash;

/// The running-water loop.
#[derive(Component)]
struct Water;

/// The vent loop at one pan position (an index into [`PANS`]).
#[derive(Component)]
struct VentLoop(usize);

/// Start the vent loop at every pan position, all silent: each is turned up by the vents on
/// its side of the screen.
fn start_vents(mut commands: Commands, mut sources: ResMut<Assets<AudioSource>>) {
    for (i, bytes) in VENT.iter().enumerate() {
        let handle = sources.add(AudioSource {
            bytes: (*bytes).into(),
        });
        commands.spawn((
            VentLoop(i),
            AudioPlayer(handle),
            PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)),
        ));
    }
}

/// How loud the vents should be at each pan position: each vent adds its share (by strength
/// and nearness to the middle of the screen, and the zoom) to the two positions either side of
/// it, so a vent pans smoothly as the camera moves.
pub fn vent_mix(vents: &[(Vec2, f32)], center: Vec2, half: Vec2, zoom_volume: f32) -> [f32; 5] {
    let mut mix = [0.0f32; 5];
    for &(pos, strength) in vents {
        let rel = (pos - center) / half;
        let near = (1.0 - rel.length() / VENT_REACH).max(0.0);
        // A gentle fade across that reach (most of it stays well audible).
        let gain = VENT_VOLUME * zoom_volume * strength.clamp(0.0, 1.5) * near.powf(1.5);
        if gain <= 0.0 {
            continue;
        }
        // Between which two pan positions, and how far along.
        let pan = rel.x.clamp(PANS[0], PANS[PANS.len() - 1]);
        let i = PANS.iter().rposition(|p| *p <= pan).unwrap_or(0).min(PANS.len() - 2);
        let t = (pan - PANS[i]) / (PANS[i + 1] - PANS[i]);
        mix[i] += gain * (1.0 - t);
        mix[i + 1] += gain * t;
    }
    // Several loud vents together shouldn't blow out the speakers.
    mix.map(|v| v.min(VENT_VOLUME * 1.5))
}

fn vent_volume(
    state: Res<SimState>,
    cam: Res<CameraState>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut sinks: Query<(&VentLoop, &mut AudioSink)>,
    time: Res<Time<Real>>,
) {
    let Ok(window) = windows.single() else { return };
    let world = &state.world;
    let zoom_volume = splash_volume(cam.zoom) / MAX_VOLUME;
    let half = Vec2::new(window.width(), window.height()) * 0.5 * cam.zoom;
    let vents: Vec<(Vec2, f32)> = world
        .env
        .vents
        .iter()
        .map(|v| (Vec2::new(v.pos.x, v.pos.y + VENT_SOUND_HEIGHT), v.strength))
        .collect();
    let mix = vent_mix(&vents, cam.center, half, zoom_volume);
    let k = 1.0 - (-WATER_EASE * time.delta_secs()).exp();
    for (VentLoop(i), mut sink) in &mut sinks {
        let now = sink.volume().to_linear();
        let next = now + (mix[*i] - now) * k;
        sink.set_volume(Volume::Linear(if next < 1e-4 { 0.0 } else { next }));
    }
}

/// Start the water loop, silent until the camera is close enough to hear it.
fn start_water(mut commands: Commands, mut sources: ResMut<Assets<AudioSource>>) {
    let handle = sources.add(AudioSource {
        bytes: WATER.into(),
    });
    commands.spawn((
        Water,
        AudioPlayer(handle),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)),
    ));
}

/// Keep the water as loud as the zoom says, easing toward it so zooming doesn't jump.
fn water_volume(
    cam: Res<CameraState>,
    mut sinks: Query<&mut AudioSink, With<Water>>,
    time: Res<Time<Real>>,
) {
    let target = WATER_VOLUME * splash_volume(cam.zoom) / MAX_VOLUME;
    let k = 1.0 - (-WATER_EASE * time.delta_secs()).exp();
    for mut sink in &mut sinks {
        let now = sink.volume().to_linear();
        let next = now + (target - now) * k;
        sink.set_volume(Volume::Linear(if next < 1e-4 { 0.0 } else { next }));
    }
}

/// Each splash at each pan position.
#[derive(Default)]
struct Sounds {
    splashes: Vec<Vec<Handle<AudioSource>>>,
}

/// The pan position nearest to `pan` (−1 = left edge of the screen, 1 = right edge).
pub fn pan_index(pan: f32) -> usize {
    let mut best = 0;
    for (i, p) in PANS.iter().enumerate() {
        if (p - pan).abs() < (PANS[best] - pan).abs() {
            best = i;
        }
    }
    best
}

/// Each visible organism's speed last frame, and when it last splashed.
#[derive(Default)]
struct Tracker {
    speed: HashMap<u64, f32>,
    last_splash: HashMap<u64, f64>,
    /// When recent splashes started (for the per-second limit).
    recent: VecDeque<f64>,
}

/// How loud sounds are at `zoom` (relative to [`MAX_VOLUME`]): full at the closest zoom, then
/// falling steadily (linearly in decibels, as loudness is heard: −[`FALLOFF_DB`]·x, with x how
/// far from the closest zoom toward [`AUDIBLE_ZOOM`] on a log scale, as zooming feels), and
/// fading out to silence at that zoom.
pub fn splash_volume(zoom: f32) -> f32 {
    let x = (zoom / MIN_ZOOM).max(1.0).ln() / (AUDIBLE_ZOOM / MIN_ZOOM).ln();
    if x >= 1.0 {
        return 0.0;
    }
    let db = -FALLOFF_DB * x;
    // The last stretch fades to nothing rather than stopping.
    let edge = 1.0 - ((x - 0.85) / 0.15).clamp(0.0, 1.0);
    MAX_VOLUME * 10f32.powf(db / 20.0) * edge
}

#[allow(clippy::too_many_arguments)]
fn splashes(
    mut commands: Commands,
    state: Res<SimState>,
    cam: Res<CameraState>,
    windows: Query<&Window, With<PrimaryWindow>>,
    playing: Query<(), With<Splash>>,
    mut sources: ResMut<Assets<AudioSource>>,
    time: Res<Time<Real>>,
    mut sounds: Local<Option<Sounds>>,
    mut tracker: Local<Tracker>,
    mut rng: Local<Option<Rng>>,
) {
    let Ok(window) = windows.single() else { return };
    let world = &state.world;
    let now = time.elapsed_secs_f64();
    let volume = splash_volume(cam.zoom);
    // Out of earshot or paused: just forget the speeds, so zooming in doesn't set off a burst.
    if volume <= 0.0 || state.paused() {
        tracker.speed.clear();
        return;
    }
    let sounds = sounds.get_or_insert_with(|| Sounds {
        splashes: SPLASHES
            .iter()
            .map(|pans| {
                pans.iter()
                    .map(|b| sources.add(AudioSource { bytes: (*b).into() }))
                    .collect()
            })
            .collect(),
    });
    let rng = rng.get_or_insert_with(|| Rng::new(now.to_bits()));
    while tracker.recent.front().is_some_and(|t| now - t > 1.0) {
        tracker.recent.pop_front();
    }

    // What's on screen.
    let half = Vec2::new(window.width(), window.height()) * 0.5 * cam.zoom;
    let (lo, hi) = (cam.center - half, cam.center + half);
    let mut at_once = playing.iter().count();
    let mut seen = HashMap::with_capacity(tracker.speed.len());
    for o in &world.organisms {
        let p = Vec2::new(o.pos.x, o.pos.y);
        if p.x < lo.x || p.x > hi.x || p.y < lo.y || p.y > hi.y {
            continue;
        }
        let speed = o.vel.length();
        seen.insert(o.id, speed);
        let Some(&before) = tracker.speed.get(&o.id) else {
            continue;
        };
        let kick = speed - before;
        if kick < SPLASH_KICK
            || at_once >= MAX_AT_ONCE
            || tracker.recent.len() >= MAX_PER_SECOND
            || tracker
                .last_splash
                .get(&o.id)
                .is_some_and(|t| now - t < ORGANISM_COOLDOWN as f64)
        {
            continue;
        }
        // Left or right by where it is on screen.
        let pan = pan_index(((p.x - cam.center.x) / half.x).clamp(-1.0, 1.0));
        let sound = sounds.splashes[rng.below(sounds.splashes.len() as u32) as usize][pan].clone();
        // Harder strokes are louder; smaller bodies higher pitched, each a little different.
        let strength = (0.5 + 0.5 * (kick / (SPLASH_KICK * HARD_KICK)).min(1.0)).min(1.0);
        let mut settings = PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume * strength));
        settings.speed = pitch(o.radius) * rng.range(1.0 - PITCH_JITTER, 1.0 + PITCH_JITTER);
        commands.spawn((Splash, AudioPlayer(sound), settings));
        tracker.last_splash.insert(o.id, now);
        tracker.recent.push_back(now);
        at_once += 1;
    }
    tracker.speed = seen;
    let stale = now - 5.0;
    tracker.last_splash.retain(|_, t| *t > stale);
}

/// A hunter swallowing its prey whole (the game's engulf effect) plays an eat sound, if it's
/// on screen and close enough to hear.
#[allow(clippy::too_many_arguments)]
fn eats(
    mut commands: Commands,
    state: Res<SimState>,
    cam: Res<CameraState>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut sources: ResMut<Assets<AudioSource>>,
    time: Res<Time<Real>>,
    mut sounds: Local<Vec<Vec<Handle<AudioSource>>>>,
    mut heard_up_to: Local<Option<u64>>,
    mut recent: Local<VecDeque<f64>>,
    mut rng: Local<Option<Rng>>,
) {
    let Ok(window) = windows.single() else { return };
    let world = &state.world;
    // Swallows already in the effects list when the game starts aren't new.
    let after = heard_up_to.unwrap_or(world.tick);
    *heard_up_to = Some(world.tick);
    let volume = EAT_VOLUME * splash_volume(cam.zoom) / MAX_VOLUME;
    if volume <= 0.0 {
        return;
    }
    if sounds.is_empty() {
        *sounds = EATS
            .iter()
            .map(|pans| {
                pans.iter()
                    .map(|b| sources.add(AudioSource { bytes: (*b).into() }))
                    .collect()
            })
            .collect();
    }
    let now = time.elapsed_secs_f64();
    while recent.front().is_some_and(|t| now - t > 1.0) {
        recent.pop_front();
    }
    let rng = rng.get_or_insert_with(|| Rng::new(now.to_bits() ^ 0xEA7));
    let half = Vec2::new(window.width(), window.height()) * 0.5 * cam.zoom;
    for fx in &world.fx {
        if fx.kind != evo_core::world::FxKind::Engulf || fx.tick <= after {
            continue;
        }
        let rel = (Vec2::new(fx.pos.x, fx.pos.y) - cam.center) / half;
        if rel.x.abs() > 1.0 || rel.y.abs() > 1.0 || recent.len() >= MAX_EATS_PER_SECOND {
            continue;
        }
        let pan = pan_index(rel.x);
        let sound = sounds[rng.below(sounds.len() as u32) as usize][pan].clone();
        let hunter = fx.body.map_or(0.0, |b| b.hunter_radius);
        let size = eat_size(hunter);
        if size <= 0.0 {
            continue;
        }
        let mut settings = PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume * size));
        settings.speed = pitch(hunter) * rng.range(1.0 - PITCH_JITTER, 1.0 + PITCH_JITTER);
        commands.spawn((Splash, AudioPlayer(sound), settings));
        recent.push_back(now);
    }
}

/// How loud a swallow is for its hunter's size, 0..1: full from `EAT_FULL_RADIUS` up, and
/// gently quieter below it (the square root of its share of that radius: half the radius is
/// about -3 dB, a tenth about -10 dB).
fn eat_size(hunter_radius: f32) -> f32 {
    (hunter_radius / EAT_FULL_RADIUS).clamp(0.0, 1.0).sqrt()
}

/// Playback speed (pitch) for a body of this radius: recorded pitch at `PITCH_BASE_RADIUS`,
/// rising steadily to two whole steps up at radius 1 and falling to two whole steps down at 30.
fn pitch(radius: f32) -> f32 {
    let semitones = if radius < PITCH_BASE_RADIUS {
        let t = (PITCH_BASE_RADIUS - radius) / (PITCH_BASE_RADIUS - PITCH_SMALL_RADIUS);
        PITCH_SEMITONES * t.min(1.0)
    } else {
        let t = (radius - PITCH_BASE_RADIUS) / (PITCH_LARGE_RADIUS - PITCH_BASE_RADIUS);
        -PITCH_SEMITONES * t.min(1.0)
    };
    2f32.powf(semitones / 12.0)
}

/// Leaving the game: stop the splashes still playing, the water and the vents.
fn clear(
    mut commands: Commands,
    playing: Query<Entity, Or<(With<Splash>, With<Water>, With<VentLoop>)>>,
) {
    for e in &playing {
        commands.entity(e).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vents_are_heard_by_where_they_are() {
        let half = Vec2::new(100.0, 60.0);
        let center = Vec2::ZERO;
        // In the middle: centre position, full.
        let mix = vent_mix(&[(Vec2::ZERO, 1.0)], center, half, 1.0);
        assert!((mix[2] - VENT_VOLUME).abs() < 1e-5 && mix[0] == 0.0 && mix[4] == 0.0);
        // Off to the right: right positions only, quieter.
        let mix = vent_mix(&[(Vec2::new(70.0, 0.0), 1.0)], center, half, 1.0);
        assert!(mix[0] == 0.0 && mix[1] == 0.0 && mix[3] + mix[4] > 0.0);
        assert!(mix[3] + mix[4] < VENT_VOLUME);
        // Far away, or zoomed out: silent.
        let far = vent_mix(&[(Vec2::new(500.0, 0.0), 1.0)], center, half, 1.0);
        assert!(far.iter().all(|v| *v == 0.0));
        let out = vent_mix(&[(Vec2::ZERO, 1.0)], center, half, 0.0);
        assert!(out.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn smaller_bodies_sound_higher() {
        let semis = |r: f32| 12.0 * pitch(r).log2();
        assert!(semis(15.0).abs() < 1e-4, "recorded pitch at the base radius");
        assert!((semis(1.0) - 4.0).abs() < 1e-4, "two whole steps up");
        assert!((semis(30.0) + 4.0).abs() < 1e-4, "two whole steps down");
        assert!((semis(0.5) - 4.0).abs() < 1e-4 && (semis(200.0) + 4.0).abs() < 1e-4);
        assert!(semis(5.0) > semis(10.0) && semis(20.0) > semis(25.0));
    }

    #[test]
    fn bigger_hunters_swallow_louder() {
        assert_eq!(eat_size(EAT_FULL_RADIUS), 1.0);
        assert_eq!(eat_size(2.0 * EAT_FULL_RADIUS), 1.0, "full from 30 up");
        assert!((eat_size(15.0) - 0.5f32.sqrt()).abs() < 1e-6, "half as big, about -3 dB");
        assert!(eat_size(3.0) > 0.3, "small hunters still heard");
        assert_eq!(eat_size(0.0), 0.0);
    }

    #[test]
    fn splashes_pan_by_where_they_are() {
        assert_eq!(pan_index(-1.0), 0);
        assert_eq!(pan_index(-0.5), 1);
        assert_eq!(pan_index(0.1), 2);
        assert_eq!(pan_index(0.5), 3);
        assert_eq!(pan_index(1.0), 4);
    }

    #[test]
    fn splashes_are_loudest_close_up_and_silent_further_out() {
        assert!((splash_volume(MIN_ZOOM) - MAX_VOLUME).abs() < 1e-6);
        // At the cut-off (log scale): silent, and further out too.
        assert_eq!(splash_volume(AUDIBLE_ZOOM), 0.0);
        assert_eq!(splash_volume(AUDIBLE_ZOOM * 1.5), 0.0);
        assert!(splash_volume(AUDIBLE_ZOOM * 0.8) > 0.0);
        // A steady fall: the same decibels per zoom step close in as further out.
        let at = |x: f32| splash_volume(MIN_ZOOM * (AUDIBLE_ZOOM / MIN_ZOOM).powf(x));
        let db = |v: f32| 20.0 * (v / MAX_VOLUME).log10();
        let early = (db(at(0.0)) - db(at(0.2))) / 0.2;
        let late = (db(at(0.4)) - db(at(0.8))) / 0.4;
        assert!((early - late).abs() < 0.01, "{early} dB/step early, {late} late");
        // Never louder further out.
        let mut last = f32::MAX;
        for k in 0..=100 {
            let v = at(k as f32 / 100.0);
            assert!(v <= last + 1e-6);
            last = v;
        }
    }
}

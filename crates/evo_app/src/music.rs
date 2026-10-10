//! Music: the menu track loops on the menus, and fades out when a world starts or an editor
//! opens (they're quiet). It's built into the program (`assets/audio/menu.ogg`).

use bevy::audio::{AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, PlaybackSettings, Volume};
use bevy::prelude::*;

use crate::AppState;

const MENU_TRACK: &[u8] = include_bytes!("../assets/audio/menu.ogg");
/// Loudness of the menu track (1 = as recorded).
const VOLUME: f32 = 0.7;
/// Seconds to fade out when a world starts.
const FADE_OUT: f32 = 1.5;
/// The track's length: it's six sections of equal length, then repeats.
pub(crate) const TRACK_LENGTH: f32 = 48.0;
pub(crate) const SECTIONS: u32 = 6;

pub struct MusicPlugin;

impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Menu), start_menu_music)
            .add_systems(OnEnter(AppState::Editor), fade_out_menu_music)
            .add_systems(OnEnter(AppState::GenomeEditor), fade_out_menu_music)
            .add_systems(OnEnter(AppState::Running), fade_out_menu_music)
            .init_resource::<MusicClock>()
            .add_systems(Update, (fade, clock, follow_volume));
    }
}

/// Where the menu track is: its section (0..SECTIONS) while it's playing, else None. (The
/// buttons' hover sounds follow it.)
#[derive(Resource, Default)]
pub struct MusicClock {
    pub section: Option<u32>,
    /// The track playing, and when (real seconds) it started.
    track: Option<(Entity, f64)>,
}

/// The menu track playing.
#[derive(Component)]
struct MenuMusic;

/// A track on its way out.
#[derive(Component)]
struct FadingOut;

/// The menu track, decoded from the built-in file once.
#[derive(Resource)]
struct MenuTrack(Handle<AudioSource>);

/// Start the menu track, unless it's already playing.
fn start_menu_music(
    mut commands: Commands,
    playing: Query<(), (With<MenuMusic>, Without<FadingOut>)>,
    fading: Query<Entity, With<FadingOut>>,
    track: Option<Res<MenuTrack>>,
    mut sources: ResMut<Assets<AudioSource>>,
    settings: Res<crate::settings::Settings>,
) {
    if !playing.is_empty() {
        return;
    }
    // Back to the menu while the last one was still fading: start over.
    for e in &fading {
        commands.entity(e).despawn();
    }
    let handle = match track {
        Some(t) => t.0.clone(),
        None => {
            let handle = sources.add(AudioSource {
                bytes: MENU_TRACK.into(),
            });
            commands.insert_resource(MenuTrack(handle.clone()));
            handle
        }
    };
    commands.spawn((
        MenuMusic,
        AudioPlayer(handle),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(VOLUME * settings.level())),
    ));
}

/// Keep the menu track at the master volume (it changes while it plays).
fn follow_volume(
    settings: Res<crate::settings::Settings>,
    mut sinks: Query<&mut AudioSink, (With<MenuMusic>, Without<FadingOut>)>,
) {
    if !settings.is_changed() {
        return;
    }
    for mut sink in &mut sinks {
        sink.set_volume(Volume::Linear(VOLUME * settings.level()));
    }
}

fn fade_out_menu_music(mut commands: Commands, music: Query<Entity, With<MenuMusic>>) {
    for e in &music {
        commands.entity(e).insert(FadingOut);
    }
}

/// Keep the clock on the track. Its start is taken from the player's position once it's
/// playing; from then on the clock counts real time (the player's position isn't reliable
/// across loops: it jumps back early).
fn clock(
    mut clock: ResMut<MusicClock>,
    playing: Query<(Entity, &AudioSink), (With<MenuMusic>, Without<FadingOut>)>,
    time: Res<Time<Real>>,
) {
    let now = time.elapsed_secs_f64();
    let Some((entity, sink)) = playing.iter().next() else {
        clock.track = None;
        clock.section = None;
        return;
    };
    if clock.track.is_none_or(|(e, _)| e != entity) {
        let pos = sink.position().as_secs_f64();
        // Not started yet (still loading): no section until it is.
        if pos <= 0.0 {
            clock.section = None;
            return;
        }
        clock.track = Some((entity, now - pos));
    }
    let start = clock.track.map_or(now, |(_, t)| t);
    clock.section = Some(section_at((now - start) as f32));
}

/// The track's section `t` seconds after it started (it repeats).
fn section_at(t: f32) -> u32 {
    let t = t.rem_euclid(TRACK_LENGTH);
    ((t / (TRACK_LENGTH / SECTIONS as f32)) as u32).min(SECTIONS - 1)
}

/// Turn fading tracks down, and stop them when they're silent.
fn fade(
    mut commands: Commands,
    mut sinks: Query<(Entity, &mut AudioSink), With<FadingOut>>,
    time: Res<Time<Real>>,
) {
    let step = VOLUME * time.delta_secs() / FADE_OUT;
    for (e, mut sink) in &mut sinks {
        let v = sink.volume().to_linear() - step;
        if v <= 0.0 {
            commands.entity(e).despawn();
        } else {
            sink.set_volume(Volume::Linear(v));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_follow_the_track_and_repeat() {
        let len = TRACK_LENGTH / SECTIONS as f32;
        assert_eq!(section_at(0.0), 0);
        assert_eq!(section_at(len - 0.01), 0);
        assert_eq!(section_at(len + 0.01), 1);
        assert_eq!(section_at(TRACK_LENGTH - 0.01), SECTIONS - 1);
        assert_eq!(section_at(TRACK_LENGTH + 0.01), 0, "it loops");
        assert_eq!(section_at(3.0 * TRACK_LENGTH + 2.5 * len), 2);
    }
}

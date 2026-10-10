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

pub struct MusicPlugin;

impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Menu), start_menu_music)
            .add_systems(OnEnter(AppState::Editor), fade_out_menu_music)
            .add_systems(OnEnter(AppState::GenomeEditor), fade_out_menu_music)
            .add_systems(OnEnter(AppState::Running), fade_out_menu_music)
            .add_systems(Update, fade);
    }
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
        PlaybackSettings::LOOP.with_volume(Volume::Linear(VOLUME)),
    ));
}

fn fade_out_menu_music(mut commands: Commands, music: Query<Entity, With<MenuMusic>>) {
    for e in &music {
        commands.entity(e).insert(FadingOut);
    }
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

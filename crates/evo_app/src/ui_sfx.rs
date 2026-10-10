//! Button sounds: a click when a button is pressed, and a soft note when the pointer moves onto
//! one of the main buttons. Any egui widget that's pressed but not dragged clicks (buttons, menu
//! items, checkboxes, tabs, section headers), not sliders, number fields or text boxes. Only the
//! main buttons, which light up under the pointer, play the note: they opt in with
//! [`hover_sound`] (the menu's big buttons, Back and Start, the round toolbar buttons, the
//! editors' tools and file buttons). The note
//! follows the menu music: it alternates section by section between two notes written to fit
//! it (the first in even sections, the second in odd ones); without the menu music (in a
//! world, in the editors) it's always the first.
//!
//! These sounds are played exactly as made (uncompressed, sample for sample) through an audio
//! output of their own, opened at their format (44.1 kHz stereo) with a small buffer, each
//! added straight to its mixer the moment it's asked for. (Bevy's own playback queues each sound
//! in a fresh queue that misreads the first 512 samples of a stereo 44.1 kHz sound as mono
//! 48 kHz, playing them at about half speed: a brief lower extra note before the real one; and
//! it buffers ~50 ms.) The hover note in a world is the exception: it goes through Bevy, extra
//! note and all, as that's kept on purpose. If the own output can't be opened, everything goes
//! through Bevy.
//!
//! A button that does something with its own sound (recording in the field notebook) asks for
//! it with [`UiSoundRequest`], and it plays instead of the click; so does clicking an organism
//! in the world (not a button, but it has a sound of its own). Clickable things that aren't
//! buttons (expand/fold controls, list rows, charts, colour swatches, panel backgrounds) make no
//! sound: they're marked [`silent`], and panels are recognised as such.

use std::num::NonZero;

use bevy::audio::{AudioPlayer, AudioSource, Decodable, PlaybackSettings, Volume};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use rodio::Source as _;

use crate::music::MusicClock;

/// The hover note in a world (through Bevy, see above).
const HOVER_1: &[u8] = include_bytes!("../assets/audio/ui_hover1.ogg");
// The rest: the original files, untouched.
const MENU_HOVER: [&[u8]; 2] = [
    include_bytes!("../assets/audio/ui_hover1_menu.wav"),
    include_bytes!("../assets/audio/ui_hover2_menu.wav"),
];
const CLICK: &[u8] = include_bytes!("../assets/audio/ui_click.wav");
const NOTEBOOK_LOG: &[u8] = include_bytes!("../assets/audio/ui_notebook_log.wav");
const ORGANISM_SELECT: &[u8] = include_bytes!("../assets/audio/ui_organism_select.wav");
/// The own output's format (the sound files' own) and buffer (512 frames, ~12 ms).
const OUTPUT_CHANNELS: u16 = 2;
const OUTPUT_RATE: u32 = 44_100;
const OUTPUT_BUFFER: u32 = 512;
/// Loudness (1 = as recorded).
const VOLUME: f32 = 0.133;
/// Where the button under the pointer is noted for the frame (egui's per-frame memory).
const HOVERED: &str = "ui_sfx_hovered";
/// Where the widgets that make no sound are listed for the frame.
const SILENT: &str = "ui_sfx_silent";
/// Where the buttons with the hover note are listed for the frame, with where they are (to
/// catch the pointer passing right over one between two frames).
const BUTTONS: &str = "ui_sfx_buttons";

/// No sound for this widget, not even a click (expand/fold controls). Call it each frame it's
/// drawn, or in the frame it's clicked.
pub fn silent(r: &egui::Response) {
    r.ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Vec<egui::Id>>(egui::Id::new(SILENT))
            .push(r.id)
    });
}

/// `response.on_hover_sound()`: [`hover_sound`] in a chain of response calls.
pub trait HoverSound {
    fn on_hover_sound(self) -> Self;
}

impl HoverSound for egui::Response {
    fn on_hover_sound(self) -> Self {
        hover_sound(&self);
        self
    }
}

/// Give a button the hover note: call it with the button's response each frame it's drawn.
pub fn hover_sound(r: &egui::Response) {
    if !(r.enabled() && r.sense.senses_click() && !r.sense.senses_drag()) {
        return;
    }
    r.ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Vec<(egui::Id, egui::Rect)>>(egui::Id::new(BUTTONS))
            .push((r.id, r.interact_rect));
        if r.hovered() {
            d.insert_temp(egui::Id::new(HOVERED), Some(r.id));
        }
    });
}

/// Whether the pointer's path from `from` to `to` passes over `rect` (checked every couple of
/// pixels along it).
fn crosses(rect: egui::Rect, from: egui::Pos2, to: egui::Pos2) -> bool {
    let steps = ((to - from).length() / 2.0).ceil().max(1.0) as usize;
    (0..=steps).any(|k| rect.contains(from + (to - from) * (k as f32 / steps as f32)))
}

pub struct UiSfxPlugin;

impl Plugin for UiSfxPlugin {
    fn build(&self, app: &mut App) {
        // Right after every UI has drawn (and asked for any sound of its own) this frame, and
        // before Bevy starts queued sounds (after transforms), so a sound starts this frame.
        app.add_systems(Startup, open_output);
        app.init_resource::<UiSoundRequest>().add_systems(
            PostUpdate,
            button_sounds
                .after(bevy_egui::EguiPostUpdateSet::EndPass)
                .before(bevy::transform::TransformSystems::Propagate),
        );
    }
}

/// A sound a button asks for in place of the click.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UiSound {
    /// An organism recorded in the field notebook.
    NotebookLog,
    /// An organism clicked (selected) in the world.
    OrganismSelect,
}

/// The sound a button pressed this frame asks for (played instead of the click).
#[derive(Resource, Default)]
pub struct UiSoundRequest(pub Option<UiSound>);

/// The button sounds' own output's mixer (the stream itself is kept open by [`UiStream`]).
/// Missing if it couldn't be opened: then the sounds go through Bevy.
#[derive(Resource, Clone)]
struct UiMixer(rodio::mixer::Mixer);

/// Keeps the button sounds' output stream open (it can't leave its thread on every system).
struct UiStream(#[allow(dead_code)] rodio::MixerDeviceSink);

/// Open the button sounds' own output: at their format with a small buffer if the device
/// takes it, else as the device likes (the mixer then converts).
fn open_output(world: &mut World) {
    let channels = NonZero::new(OUTPUT_CHANNELS).expect("channels");
    let rate = NonZero::new(OUTPUT_RATE).expect("rate");
    let open = |buffer: Option<u32>| -> Result<rodio::MixerDeviceSink, rodio::DeviceSinkError> {
        let mut builder = rodio::DeviceSinkBuilder::from_default_device()?
            .with_channels(channels)
            .with_sample_rate(rate);
        if let Some(frames) = buffer {
            builder = builder.with_buffer_size(rodio::cpal::BufferSize::Fixed(frames));
        }
        builder.open_stream()
    };
    match open(Some(OUTPUT_BUFFER))
        .or_else(|_| open(None))
        .or_else(|_| rodio::DeviceSinkBuilder::open_default_sink())
    {
        Ok(mut stream) => {
            stream.log_on_drop(false);
            world.insert_resource(UiMixer(stream.mixer().clone()));
            world.insert_non_send(UiStream(stream));
        }
        Err(e) => warn!("button sounds: no output of their own ({e}); playing them through Bevy"),
    }
}

/// A sound decoded once, to add to the mixer as is.
#[derive(Clone, Copy)]
struct Clip {
    channels: rodio::ChannelCount,
    rate: rodio::SampleRate,
    samples: &'static [f32],
    bytes: &'static [u8],
}

impl Clip {
    fn decode(bytes: &'static [u8]) -> Clip {
        let decoder = AudioSource { bytes: bytes.into() }.decoder();
        let (channels, rate) = (decoder.channels(), decoder.sample_rate());
        let samples: Vec<f32> = decoder.collect();
        Clip { channels, rate, samples: Box::leak(samples.into_boxed_slice()), bytes }
    }

    fn source(self) -> rodio::static_buffer::StaticSamplesBuffer {
        rodio::static_buffer::StaticSamplesBuffer::new(self.channels, self.rate, self.samples)
    }
}

struct Sounds {
    /// In a world (through Bevy).
    hover: Handle<AudioSource>,
    /// In the menus, by the music's section.
    menu_hover: [Clip; 2],
    click: Clip,
    notebook_log: Clip,
    organism_select: Clip,
}

/// Whether a widget is a button: it can be pressed, can't be dragged, and is enabled.
fn is_button(ctx: &egui::Context, id: egui::Id) -> bool {
    ctx.read_response(id).is_some_and(|r| {
        r.enabled() && r.sense.senses_click() && !r.sense.senses_drag()
    })
}

/// Whether a click landed on a panel's background rather than a widget in it: egui lets a
/// clicked area (window, card) come to the front, so its background takes clicks too.
fn is_panel(ctx: &egui::Context, id: egui::Id) -> bool {
    ctx.memory(|m| m.layer_ids().any(|layer| layer.id.with("move") == id))
}

/// Whether the pointer is moving (moved this frame, or is still gliding).
fn pointer_moved(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.pointer.delta() != egui::Vec2::ZERO || i.pointer.is_moving())
}

/// The main button the pointer is over (one that opted in to the hover note), and the button
/// just clicked, if any. Reading the hovered one clears it for the next frame.
fn buttons(ctx: &egui::Context) -> (Option<egui::Id>, Option<egui::Id>) {
    let hovered = ctx
        .data_mut(|d| d.remove_temp::<Option<egui::Id>>(egui::Id::new(HOVERED)))
        .flatten();
    let silent = ctx
        .data_mut(|d| d.remove_temp::<Vec<egui::Id>>(egui::Id::new(SILENT)))
        .unwrap_or_default();
    let clicked = ctx.interaction_snapshot(|s| s.clicked);
    (
        hovered,
        clicked.filter(|&id| is_button(ctx, id) && !silent.contains(&id) && !is_panel(ctx, id)),
    )
}

#[allow(clippy::too_many_arguments)]
fn button_sounds(
    mut commands: Commands,
    mut contexts: EguiContexts,
    clock: Res<MusicClock>,
    app_state: Res<State<crate::AppState>>,
    mut request: ResMut<UiSoundRequest>,
    mut sources: ResMut<Assets<AudioSource>>,
    mixer: Option<Res<UiMixer>>,
    settings: Res<crate::settings::Settings>,
    mut sounds: Local<Option<Sounds>>,
    mut last_hovered: Local<Option<egui::Id>>,
    mut last_pointer: Local<Option<egui::Pos2>>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let (button, clicked) = buttons(&ctx);
    let listed = ctx
        .data_mut(|d| d.remove_temp::<Vec<(egui::Id, egui::Rect)>>(egui::Id::new(BUTTONS)))
        .unwrap_or_default();
    let pointer = ctx.input(|i| i.pointer.latest_pos());
    // Only when the pointer moved onto it: a button sliding under a still pointer (as a screen
    // lays itself out when it opens) makes no sound.
    let moved = pointer_moved(&ctx);
    let entered = button.is_some() && button != *last_hovered;
    // A quick swipe can pass right over a button between two frames without it ever being
    // under the pointer when the frame is drawn: that counts too.
    let swept = match (*last_pointer, pointer) {
        (Some(from), Some(to)) if from != to => listed
            .iter()
            .any(|&(id, rect)| Some(id) != button && Some(id) != *last_hovered && crosses(rect, from, to)),
        _ => false,
    };
    let new_hover = moved && (entered || swept);
    *last_hovered = button;
    *last_pointer = pointer;
    let special = request.0.take();
    if !new_hover && clicked.is_none() && special.is_none() {
        return Ok(());
    }
    let sounds = sounds.get_or_insert_with(|| Sounds {
        hover: sources.add(AudioSource { bytes: HOVER_1.into() }),
        menu_hover: MENU_HOVER.map(Clip::decode),
        click: Clip::decode(CLICK),
        notebook_log: Clip::decode(NOTEBOOK_LOG),
        organism_select: Clip::decode(ORGANISM_SELECT),
    });
    let volume = VOLUME * settings.level();
    // Straight into the own output's mixer; or through Bevy without one.
    let mut play = |clip: Clip| match &mixer {
        Some(mixer) => mixer.0.add(clip.source().amplify(volume)),
        None => {
            let handle = sources.add(AudioSource { bytes: clip.bytes.into() });
            commands.spawn((
                AudioPlayer(handle),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)),
            ));
        }
    };
    // Every hover gets its note, and a click its click (both, if they come together).
    if let Some(special) = special {
        play(match special {
            UiSound::NotebookLog => sounds.notebook_log,
            UiSound::OrganismSelect => sounds.organism_select,
        });
    } else if clicked.is_some() {
        play(sounds.click);
    }
    if new_hover {
        if *app_state.get() == crate::AppState::Running {
            commands.spawn((
                AudioPlayer(sounds.hover.clone()),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)),
            ));
        } else {
            play(sounds.menu_hover[clock.section.map_or(0, |s| (s % 2) as usize)]);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One egui pass with these input events: a button at the top, a slider below it. Returns
    /// (button id, slider id, hovered button, clicked button) as seen during that pass.
    fn pass(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> (egui::Id, egui::Id, Option<egui::Id>, Option<egui::Id>) {
        pass_moved(ctx, events).0
    }

    /// As [`pass`], and whether the pointer counted as moving.
    fn pass_moved(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> ((egui::Id, egui::Id, Option<egui::Id>, Option<egui::Id>), bool) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))),
            events,
            ..Default::default()
        };
        let mut out = None;
        let mut output = ctx.run_ui(input, |ui| {
            let b = ui.add_sized([200.0, 40.0], egui::Button::new("Go"));
            hover_sound(&b);
            let b = b.id;
            let mut v = 0.5f32;
            let s = ui.add_sized([200.0, 40.0], egui::Slider::new(&mut v, 0.0..=1.0));
            hover_sound(&s); // a slider never makes the note, even if asked
            let s = s.id;
            // A fold header, below them: marked silent.
            let fold = egui::CollapsingHeader::new("More").show(ui, |ui| ui.label("…"));
            silent(&fold.header_response);
            let (hovered, clicked) = buttons(ui.ctx());
            out = Some(((b, s, hovered, clicked), pointer_moved(ui.ctx())));
        });
        output.textures_delta.clear(); // no renderer here
        out.unwrap()
    }

    fn at(x: f32, y: f32) -> egui::Event {
        egui::Event::PointerMoved(egui::pos2(x, y))
    }

    fn press(x: f32, y: f32, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: egui::pos2(x, y),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }

    #[test]
    fn buttons_are_heard_but_sliders_are_not() {
        // (The note only for buttons that opt in; sliders neither note nor click.)
        let ctx = egui::Context::default();
        let (button, _, hovered, _) = pass(&ctx, vec![]);
        assert_eq!(hovered, None, "nothing under the pointer yet");
        // Over the button (top of the panel).
        let (_, _, hovered, clicked) = pass(&ctx, vec![at(60.0, 25.0)]);
        assert_eq!(hovered, Some(button));
        assert_eq!(clicked, None);
        let (_, _, _, clicked) = pass(&ctx, vec![press(60.0, 25.0, true)]);
        assert_eq!(clicked, None, "a click is a press and a release");
        let (_, _, _, clicked) = pass(&ctx, vec![press(60.0, 25.0, false)]);
        assert_eq!(clicked, Some(button));
        // Over the slider below it: neither hovering nor clicking it counts.
        let (_, slider, hovered, _) = pass(&ctx, vec![at(60.0, 70.0)]);
        assert_ne!(hovered, Some(slider));
        assert_eq!(hovered, None);
        pass(&ctx, vec![press(60.0, 70.0, true)]);
        let (_, _, _, clicked) = pass(&ctx, vec![press(60.0, 70.0, false)]);
        assert_eq!(clicked, None);
        // The fold header (third row): no click.
        pass(&ctx, vec![at(30.0, 100.0), press(30.0, 100.0, true)]);
        let (_, _, _, clicked) = pass(&ctx, vec![press(30.0, 100.0, false)]);
        assert_eq!(clicked, None, "folding is silent");
    }

    /// A panel (an area, like the organism card) with a button in it: clicking the panel's
    /// background isn't a button click; clicking the button is.
    #[test]
    fn panel_backgrounds_dont_click() {
        let ctx = egui::Context::default();
        let frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))),
                events,
                ..Default::default()
            };
            let mut out = None;
            let mut output = ctx.run_ui(input, |ui| {
                egui::Area::new(egui::Id::new("card"))
                    .fixed_pos(egui::pos2(50.0, 50.0))
                    .show(ui.ctx(), |ui| {
                        ui.set_min_size(egui::vec2(200.0, 150.0));
                        ui.add_sized([100.0, 30.0], egui::Button::new("Follow"));
                    });
                out = Some(buttons(ui.ctx()).1);
            });
            output.textures_delta.clear();
            out.unwrap()
        };
        // (A new area settles over a few frames before it takes clicks.)
        for _ in 0..3 {
            frame(vec![]);
        }
        // The background, well away from the button.
        frame(vec![at(200.0, 170.0), press(200.0, 170.0, true)]);
        assert_eq!(frame(vec![press(200.0, 170.0, false)]), None, "background: no click");
        // The button.
        frame(vec![at(90.0, 64.0), press(90.0, 64.0, true)]);
        assert!(frame(vec![press(90.0, 64.0, false)]).is_some(), "button: click");
    }

    #[test]
    fn a_quick_swipe_over_a_button_counts() {
        let button = egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(32.0, 32.0));
        // Straight over it between two frames, never landing on it.
        assert!(crosses(button, egui::pos2(116.0, 60.0), egui::pos2(116.0, 170.0)));
        assert!(crosses(button, egui::pos2(60.0, 60.0), egui::pos2(170.0, 170.0)));
        // Past it.
        assert!(!crosses(button, egui::pos2(60.0, 60.0), egui::pos2(60.0, 170.0)));
        assert!(!crosses(button, egui::pos2(60.0, 60.0), egui::pos2(60.0, 60.0)));
    }

    #[test]
    fn a_still_pointer_is_not_moving() {
        let ctx = egui::Context::default();
        pass_moved(&ctx, vec![at(300.0, 250.0)]);
        let (_, moved) = pass_moved(&ctx, vec![at(60.0, 25.0)]);
        assert!(moved, "it just moved onto the button");
        // Many frames later with no new pointer events: still.
        let mut moved = true;
        for _ in 0..60 {
            moved = pass_moved(&ctx, vec![]).1;
        }
        assert!(!moved, "a pointer left alone isn't moving");
    }
}

#[cfg(test)]
mod sound_files {
    use bevy::audio::{Decodable, Source};

    /// The menu's notes are uncompressed, at the game's rate, and fade cleanly.
    #[test]
    fn menu_notes_are_clean() {
        for (bytes, file) in super::MENU_HOVER.iter().zip(["hover-1.wav", "hover-2.wav"]) {
            let dec = bevy::audio::AudioSource { bytes: (*bytes).into() }.decoder();
            assert_eq!(dec.sample_rate().get(), 44_100);
            let ch = dec.channels().get() as usize;
            let played: Vec<f32> = dec.collect();
            assert!(played.len() > 44_100 / 10 * ch, "{file}: too short");
            // It fades out within its length.
            let peak = played.iter().fold(0f32, |m, s| m.max(s.abs()));
            let tail = played[played.len() * 3 / 4..].iter().fold(0f32, |m, s| m.max(s.abs()));
            assert!(peak > 0.3 && tail < peak * 0.05, "{file}: peak {peak} tail {tail}");
        }
    }
}

#[cfg(test)]
mod playback_path {
    use super::*;

    /// The sounds through the own output: each added to a mixer of the output's format comes
    /// out sample for sample as the file (at the volume), from the very first sample.
    #[test]
    fn sounds_play_exactly_as_made_and_at_once() {
        for bytes in [MENU_HOVER[0], MENU_HOVER[1], CLICK, NOTEBOOK_LOG, ORGANISM_SELECT] {
            let clip = Clip::decode(bytes);
            assert_eq!(
                (clip.channels.get(), clip.rate.get()),
                (OUTPUT_CHANNELS, OUTPUT_RATE),
                "in the output's format, so the mixer doesn't convert it"
            );
            let (mixer, out) = rodio::mixer::mixer(clip.channels, clip.rate);
            mixer.add(clip.source().amplify(0.2));
            let played: Vec<f32> = out.take(clip.samples.len()).collect();
            let worst = played
                .iter()
                .zip(clip.samples)
                .map(|(p, s)| (p / 0.2 - s).abs())
                .fold(0f32, f32::max);
            assert!(worst < 1e-5, "differs by {worst}");
        }
    }
}

# Trailer

`make_assets.py` writes everything to `out/` (git-ignored): run `python3 trailer/make_assets.py`
(`--height 1080` or `--height 2160` for other sizes). It needs Python 3 with Pillow and NumPy, and ffmpeg.

| File | What |
|---|---|
| `music.wav` | The full 60 s soundtrack: 2 s silence, the menu song (48 s), its first section as the outro, silence to the end |
| `01_…06_*.mov` | The title cards, ProRes 4444 with a transparent background, 2560×1440, 60 fps, animated in and out |
| `01_…06_*.png` | The same cards, still, to animate yourself instead |
| `markers.edl` | Section markers for Resolve |

## Timeline (60 s)

The song is six 8-second sections; the outro repeats the first. Cut on the section changes.

| Time | Section | Footage | Text (clip) |
|---|---|---|---|
| 0:00 | pad | fade in from black, no music | |
| 0:02 | S1 | plankton floating, reproducing | |
| 0:10 | S2 | lots of more advanced plankton flowing | |
| 0:18 | S3 | predators chasing plankton | `01_random_chance` |
| 0:26 | S4 | scavengers eating a whale fall | `02_natural_selection` |
| 0:34 | S5 | tentacle fractal plankton | `03_every_organism` |
| 0:42 | S6 | vent life | `04_common_ancestor` |
| 0:50 | outro | a dense pack, zooming out to the whole world | `05_title` at ~0:52.5, `06_subtitle` ~1.5 s later |
| 0:57.7 | | the music has ended | |
| 0:58 | pad | fade to black | |

Each line card is 7 s long: its words rise in over the first ~1.5 s and it fades out over the last
second. Start one at its section's change (or a beat after) and it ends just before the next.
The title is 7.5 s and the subtitle 6 s; with the subtitle 1.5 s after the title, both fade out
together at the end. The narration lines are centred in the frame (for a black background), and
the subtitle sits under the title; like any clip you can move, scale or retime them.

## In DaVinci Resolve

- Set the project to 2560×1440 at 60 fps (Project Settings > Master Settings) before importing.
- Drag `music.wav` onto an audio track at the timeline's start, and the cards onto a video track
  above your footage.
- Markers: right-click in the timeline > Timelines > Import > Timeline Markers from EDL, and pick
  `markers.edl`.
- If a card's edges look dark or haloed, select it in the Media Pool > Clip Attributes > Alpha
  mode: Straight.
- The free Linux version can't read H.264/AAC (what OBS records by default). Either record in
  OBS with a format Resolve reads (Settings > Output > Recording: Recording Format "mov",
  Encoder "Apple ProRes 422" if listed, else any), or convert each recording first:

      ffmpeg -i in.mp4 -c:v dnxhd -profile:v dnxhr_hq -pix_fmt yuv422p -c:a pcm_s16le out.mov

## Recording footage

- Record the game's window at 2560×1440 and 60 fps: the **1440p** button in the God powers
  panel's Camera move section sets the window to that size (un-maximize it first). The size is
  the window's content; the title bar comes on top of it, so capture the window (OBS "Window
  Capture"), not the screen. (Full screen would be your monitor's 3840×1600.) Hide the cursor in OBS (the source's
  "Capture Cursor" setting).
- Tab hides all of the game's UI, leaving only the world. Press Escape to deselect first, so
  no selection ring shows.
- Camera moves: open the hidden God powers panel (Ctrl+Shift+G) and find the Camera move section.
  Frame the start and press **Set start**, frame the end and press **Set end**, and choose
  how long it glides (and how long it holds on the start first). Then hide the UI with Tab and
  press **Ctrl+Shift+P**: the camera jumps to the start and the game pauses. Start recording,
  then press **Space** (or the play button): the game and the camera start together. The camera
  holds, then glides to the end, the same every take, and pausing the game pauses it too. Escape
  cancels it. A zoom out keeps the spot where it started still on screen, so a
  pack of organisms opens out to the world around it.
- The same panel can place organisms (including ones saved in the genome editor), so you can
  stage scenes: a predator next to a swarm, a big body for the whale fall, and so on.

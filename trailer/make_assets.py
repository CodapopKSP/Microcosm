#!/usr/bin/env python3
"""Microcosm trailer assets, written to trailer/out/:

- one clip per title card, with a transparent background (ProRes 4444 .mov, 60 fps), and a
  still of each (.png) to animate in the editor instead if you like;
- the music track (music.wav): 2 s of silence, the 48 s menu song, its first section again as
  the outro, then silence to 60 s;
- section markers for DaVinci Resolve (markers.edl).

    python3 trailer/make_assets.py                 # everything, at 2560x1440
    python3 trailer/make_assets.py --height 1080   # at 1920x1080 (or 2160 for 4K)
    python3 trailer/make_assets.py --only title    # just the clips whose name contains "title"

Edit CARDS below to change the words; each card is its own clip, so re-running only touches
the files you ask for.
"""

import argparse
import math
import subprocess
from pathlib import Path

import numpy as np
from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "trailer" / "out"
SONG = ROOT / "crates/evo_app/assets/audio/menu.ogg"
# The game's interface face (egui's default) and the field notebook's handwriting.
UI_FONT = "/usr/share/fonts/truetype/ubuntu/Ubuntu-L.ttf"
HAND_FONT = str(ROOT / "crates/evo_app/assets/fonts/Kalam-Regular.ttf")
FPS = 60

# The timeline, in seconds (see README.md).
PAD = 2.0
SECTION = 8.0
SONG_LENGTH = 48.0
# Where the outro (the song's first section again) stops: the music has died away there, just
# before the next section's pickup comes in.
OUTRO_STOP = 7.72
TOTAL = 60.0

# The title cards: (file name, kind, text, length in seconds).
CARDS = [
    ("01_random_chance", "line", "A world formed through random chance", 7.0),
    ("02_natural_selection", "line", "By mutation and natural selection", 7.0),
    ("03_every_organism", "line", "Every organism you see", 7.0),
    ("04_common_ancestor", "line", "Evolved from a single common ancestor", 7.0),
    ("05_title", "title", "MICROCOSM", 7.5),
    ("06_subtitle", "subtitle", "Evolution Simulator", 6.0),
]

# Colours (the game's sea-glass palette).
LINE_COLOR = (238, 250, 246)
TITLE_FROM, TITLE_TO = (100, 225, 220), (185, 245, 130)
SUBTITLE_COLOR = (205, 240, 230)
HALO = (80, 200, 180)

S = 1.0  # resolution scale, relative to 1080p (set from --height)
W, H = 1920, 1080


# --- Easing and small helpers -----------------------------------------------------------------


def clamp01(x):
    return max(0.0, min(1.0, x))


def ease_out(x):
    x = clamp01(x)
    return 1.0 - (1.0 - x) ** 3


def smooth(x):
    x = clamp01(x)
    return x * x * (3.0 - 2.0 * x)


def ease_out_back(x):
    x = clamp01(x)
    c = 1.4
    return 1.0 + (c + 1.0) * (x - 1.0) ** 3 + c * (x - 1.0) ** 2


def hash01(k, n):
    v = math.sin(k * 12.9898 + n * 78.233) * 43758.5453
    return v - math.floor(v)


def colored(mask, color, alpha=1.0):
    """A layer of solid `color` showing through `mask` (straight alpha, clean edges)."""
    layer = Image.new("RGBA", mask.size, color + (0,))
    a = mask if alpha >= 1.0 else mask.point(lambda v: int(v * alpha + 0.5))
    layer.putalpha(a)
    return layer


def faded(mask, alpha):
    if alpha >= 0.999:
        return mask
    return mask.point(lambda v: int(v * alpha + 0.5))


def shifted(img, dx, dy):
    """`img` moved by a fraction of a pixel (smooth motion at 60 fps)."""
    if abs(dx) < 1e-3 and abs(dy) < 1e-3:
        return img
    return img.transform(img.size, Image.AFFINE, (1, 0, -dx, 0, 1, -dy), resample=Image.BICUBIC)


def text_mask(text, font, tracking=0.0, pad=0):
    """White-on-black mask of `text`, letter-spaced by `tracking`, with `pad` all round.
    Returns (mask, ink width)."""
    widths = [font.getlength(c) for c in text]
    width = sum(widths) + tracking * (len(text) - 1)
    ascent, descent = font.getmetrics()
    mask = Image.new("L", (int(math.ceil(width)) + 2 * pad, ascent + descent + 2 * pad), 0)
    draw = ImageDraw.Draw(mask)
    x = pad
    for c, w in zip(text, widths):
        draw.text((x, pad), c, font=font, fill=255)
        x += w + tracking
    return mask, width


# --- Cards ------------------------------------------------------------------------------------


def line_card(text, length):
    """A line of narration: the words rise softly into focus one after another, hold, and
    drift up as they fade."""
    font = ImageFont.truetype(UI_FONT, round(62 * S))
    tracking = 1.5 * S
    pad = round(24 * S)
    words = text.split(" ")
    space = font.getlength(" ") + tracking
    masks = [text_mask(w, font, tracking, pad) for w in words]
    total = sum(w for _, w in masks) + space * (len(words) - 1)
    x = W / 2 - total / 2
    cy = H * 0.5
    places = []
    for mask, width in masks:
        places.append((x - pad, cy - mask.size[1] / 2))
        x += width + space
    # Work in a box around the line; the rest of the frame stays empty.
    box = (
        int(W / 2 - total / 2 - 80 * S),
        int(cy - 90 * S),
        int(W / 2 + total / 2 + 80 * S),
        int(cy + 90 * S),
    )
    out_start = length - 1.15

    def frame(t):
        canvas = Image.new("L", (box[2] - box[0], box[3] - box[1]), 0)
        leave = clamp01((t - out_start) / 0.95)
        for i, ((mask, _), (px, py)) in enumerate(zip(masks, places)):
            k = ease_out((t - 0.3 - i * 0.11) / 0.85)
            alpha = k * (1.0 - smooth(leave))
            if alpha <= 0.002:
                continue
            blur = (1.0 - k) * 6 * S + leave * 4 * S
            rise = (1.0 - k) * 16 * S - smooth(leave) * 10 * S
            m = mask.filter(ImageFilter.GaussianBlur(blur)) if blur > 0.25 else mask
            fx, fy = px - box[0], py - box[1] + rise
            m = shifted(m, fx - int(fx), fy - int(fy))
            word = Image.new("L", canvas.size, 0)
            word.paste(faded(m, alpha), (int(fx), int(fy)))
            canvas = ImageChops.lighter(canvas, word)
        return compose(canvas, LINE_COLOR, box, shadow=0.55)

    return frame


def title_card(text, length):
    """The title as on the main menu: spaced letters shading from sea-green to leaf-green,
    softly glowing and gently bobbing, with the O's drawn as living cells. The letters rise
    in from left to right."""
    size = 170 * S
    font = ImageFont.truetype(UI_FONT, round(size))
    gap = size * 0.16
    pad = round(size * 0.25)
    n = len(text)
    widths = [font.getlength(c) for c in text]
    total = sum(widths) + gap * (n - 1)
    cy = H * 0.44
    letters = []
    x = W / 2 - total / 2
    for i, (c, w) in enumerate(zip(text, widths)):
        f = i / (n - 1)
        color = tuple(round(a + (b - a) * f) for a, b in zip(TITLE_FROM, TITLE_TO))
        if c == "O":
            letters.append((c, x + w / 2, color, None, None))
        else:
            mask = Image.new("L", (round(w) + 2 * pad, round(size * 1.4) + 2 * pad), 0)
            ImageDraw.Draw(mask).text(
                (mask.size[0] / 2, mask.size[1] / 2), c, font=font, fill=255, anchor="mm"
            )
            glow = mask.filter(ImageFilter.GaussianBlur(size * 0.05))
            letters.append((c, x + w / 2, color, mask, glow))
        x += w + gap
    box = (int(W / 2 - total / 2 - size), int(cy - size * 1.1), int(W / 2 + total / 2 + size), int(cy + size * 1.1))
    bw, bh = box[2] - box[0], box[3] - box[1]
    # A soft halo of light behind the word.
    halo = Image.new("L", (bw, bh), 0)
    ImageDraw.Draw(halo).ellipse(
        (bw / 2 - total * 0.55, bh / 2 - size * 0.55, bw / 2 + total * 0.55, bh / 2 + size * 0.55),
        fill=255,
    )
    halo = halo.filter(ImageFilter.GaussianBlur(size * 0.45))
    out_start = length - 1.1

    def frame(t):
        img = Image.new("RGBA", (bw, bh), (0, 0, 0, 0))
        leave = 1.0 - smooth((t - out_start) / 1.0)
        img = Image.alpha_composite(img, colored(halo, HALO, 0.16 * smooth(t / 1.6) * leave))
        for i, (c, lx, color, mask, glow) in enumerate(letters):
            k = (t - 0.25 - i * 0.085) / 0.9
            alpha = ease_out(k) * leave
            if alpha <= 0.002:
                continue
            bob = math.sin(t * 1.1 + i * 0.55) * size * 0.025
            rise = (1.0 - ease_out(k)) * 26 * S
            cx, cyy = lx - box[0], cy - box[1] + bob + rise
            if mask is None:
                cell = cell_image(size * 0.3 * (0.5 + 0.5 * ease_out_back(k)), color, t + i, alpha)
                fx, fy = cx - cell.size[0] / 2, cyy - cell.size[1] / 2
                cell = shifted(cell, fx - int(fx), fy - int(fy))
                img.alpha_composite(cell, (int(fx), int(fy)))
            else:
                fx, fy = cx - mask.size[0] / 2, cyy - mask.size[1] / 2
                for m, a in ((glow, 0.55), (mask, 1.0)):
                    m = shifted(m, fx - int(fx), fy - int(fy))
                    img.alpha_composite(colored(m, color, a * alpha), (int(fx), int(fy)))
        return place(img, box)

    return frame


def cell_image(r, color, t, alpha):
    """A cell, as the menu draws the title's O's: a translucent body with drifting granules, a
    nucleus, a membrane and beating cilia. Drawn large and scaled down for smooth edges."""
    ss = 3
    R = r * ss
    side = int(math.ceil(R * 3.3))
    c = side / 2
    layers = []

    def disc(radius, a, center=(c, c), fill=True, width=0, col=color):
        m = Image.new("L", (side, side), 0)
        d = ImageDraw.Draw(m)
        bb = (center[0] - radius, center[1] - radius, center[0] + radius, center[1] + radius)
        if fill:
            d.ellipse(bb, fill=255)
        else:
            d.ellipse(bb, outline=255, width=max(1, round(width)))
        layers.append((m, col, a))

    disc(R * 1.5, 0.04)
    disc(R * 1.2, 0.06)
    disc(R, 0.16)
    # Granules drifting round inside.
    m = Image.new("L", (side, side), 0)
    d = ImageDraw.Draw(m)
    for k in range(7):
        a = hash01(k, 1) * math.tau + t * (0.15 + 0.2 * hash01(k, 2))
        dist = R * (0.35 + 0.45 * hash01(k, 3))
        gr = R * (0.05 + 0.04 * hash01(k, 4))
        px, py = c + math.cos(a) * dist, c + math.sin(a) * dist
        d.ellipse((px - gr, py - gr, px + gr, py + gr), fill=255)
    layers.append((m, color, 0.45))
    # The nucleus, wandering a little.
    nx, ny = c + math.sin(t * 0.4) * R * 0.12, c + math.cos(t * 0.33) * R * 0.12
    half = tuple(v // 2 for v in color)
    disc(R * 0.34, 0.55, (nx, ny), col=half)
    disc(R * 0.34, 0.7, (nx, ny), fill=False, width=max(R * 0.05, ss))
    disc(R * 0.08, 0.8, (nx - R * 0.08, ny - R * 0.08))
    # The membrane.
    disc(R, 0.95, fill=False, width=max(R * 0.09, 1.2 * ss))
    # Cilia, beating in a wave around the cell.
    m = Image.new("L", (side, side), 0)
    d = ImageDraw.Draw(m)
    count = 22
    for k in range(count):
        a = k / count * math.tau + t * 0.25
        dx, dy = math.cos(a), math.sin(a)
        beat = math.sin(t * 5.0 - k * 0.9) * 0.6
        sx, sy = -dy, dx
        bx, by = c + dx * R * 1.05, c + dy * R * 1.05
        vx, vy = dx + sx * beat, dy + sy * beat
        norm = math.hypot(vx, vy)
        tx, ty = bx + vx / norm * R * 0.28, by + vy / norm * R * 0.28
        d.line((bx, by, tx, ty), fill=255, width=max(1, round(max(R * 0.035, ss))))
    layers.append((m, color, 0.6))

    img = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    for m, col, a in layers:
        img.alpha_composite(colored(m, col, a * alpha))
    out = int(math.ceil(side / ss))
    return img.resize((out, out), Image.LANCZOS)


def subtitle_card(text, length):
    """The subtitle in the field notebook's handwriting, written on from left to right."""
    font = ImageFont.truetype(HAND_FONT, round(60 * S))
    tracking = 1.0 * S
    pad = round(30 * S)
    mask, width = text_mask(text, font, tracking, pad)
    cy = H * 0.44 + 150 * S
    left = W / 2 - width / 2
    box = (int(left - pad - 40 * S), int(cy - mask.size[1] / 2 - 40 * S), 0, 0)
    box = (box[0], box[1], box[0] + mask.size[0] + int(80 * S), box[1] + mask.size[1] + int(80 * S))
    origin = (int(left - pad) - box[0], int(cy - mask.size[1] / 2) - box[1])
    feather = 46 * S
    xs = np.arange(mask.size[0], dtype=np.float32)
    ink = np.asarray(mask, dtype=np.float32)
    out_start = length - 1.0

    def frame(t):
        # A soft edge sweeps across, as if the pen were writing it.
        edge = smooth((t - 0.15) / 1.6) * (width + feather) + pad
        reveal = np.clip((edge - xs) / feather, 0.0, 1.0)
        alpha = 1.0 - smooth((t - out_start) / 0.9)
        shown = Image.fromarray((ink * reveal[None, :] * alpha).astype(np.uint8))
        canvas = Image.new("L", (box[2] - box[0], box[3] - box[1]), 0)
        canvas.paste(shown, origin)
        return compose(canvas, SUBTITLE_COLOR, box, shadow=0.5)

    return frame


def compose(mask, color, box, shadow):
    """Text in `color` over a soft dark shadow (to read over bright water), placed in a frame."""
    blur = mask.filter(ImageFilter.GaussianBlur(10 * S))
    img = Image.new("RGBA", mask.size, (0, 0, 0, 0))
    img.alpha_composite(colored(shifted(blur, 0, 3 * S), (0, 8, 14), shadow))
    img.alpha_composite(colored(mask, color))
    return place(img, box)


def place(img, box):
    frame = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    frame.alpha_composite(img, (box[0], box[1]))
    return frame


# --- Output -----------------------------------------------------------------------------------


def render(name, frame, length):
    mov = OUT / f"{name}.mov"
    frames = round(length * FPS)
    ffmpeg = subprocess.Popen(
        [
            "ffmpeg", "-v", "error", "-y",
            "-f", "rawvideo", "-pix_fmt", "rgba", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-",
            "-c:v", "prores_ks", "-profile:v", "4444", "-pix_fmt", "yuva444p10le",
            "-alpha_bits", "16", "-vendor", "apl0", str(mov),
        ],
        stdin=subprocess.PIPE,
    )
    still_at = round(min(length * 0.5, length - 1.5) * FPS)
    for f in range(frames):
        img = frame(f / FPS)
        if f == still_at:
            img.save(OUT / f"{name}.png")
        ffmpeg.stdin.write(img.tobytes())
    ffmpeg.stdin.close()
    if ffmpeg.wait() != 0:
        raise SystemExit(f"ffmpeg failed on {name}")
    print(f"  {mov.relative_to(ROOT)}  ({length:.1f} s)")


def music():
    """The trailer's music: silence, the song, its first section as the outro, silence."""
    rate = 44100
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", str(SONG), "-f", "f32le", "-ac", "2", "-ar", str(rate), "-"],
        check=True,
        capture_output=True,
    ).stdout
    song = np.frombuffer(raw, dtype=np.float32).reshape(-1, 2)
    outro = song[: round(OUTRO_STOP * rate)].copy()
    tail = round(0.06 * rate)  # a short fade so the last sample doesn't click
    outro[-tail:] *= np.linspace(1.0, 0.0, tail, dtype=np.float32)[:, None]
    track = np.zeros((round(TOTAL * rate), 2), dtype=np.float32)
    start = round(PAD * rate)
    track[start : start + len(song)] = song
    track[start + len(song) : start + len(song) + len(outro)] = outro
    wav = OUT / "music.wav"
    subprocess.run(
        [
            "ffmpeg", "-v", "error", "-y", "-f", "f32le", "-ac", "2", "-ar", str(rate), "-i", "-",
            "-c:a", "pcm_s24le", str(wav),
        ],
        input=track.tobytes(),
        check=True,
    )
    print(f"  {wav.relative_to(ROOT)}  ({TOTAL:.0f} s)")


MARKERS = [
    (0.0, "Pad: fade in", "ResolveColorBlue"),
    (2.0, "S1: plankton floating, reproducing", "ResolveColorGreen"),
    (10.0, "S2: advanced plankton flowing", "ResolveColorGreen"),
    (18.0, "S3: predators chasing / random chance", "ResolveColorYellow"),
    (26.0, "S4: whale fall / natural selection", "ResolveColorYellow"),
    (34.0, "S5: tentacle plankton / every organism", "ResolveColorPink"),
    (42.0, "S6: vent life / common ancestor", "ResolveColorPink"),
    (50.0, "Outro: zoom out, title", "ResolveColorPurple"),
    (50.0 + OUTRO_STOP, "Music ends", "ResolveColorRed"),
    (58.0, "Pad: fade out", "ResolveColorBlue"),
]


def markers():
    """Timeline markers as an EDL (Resolve: right-click the timeline > Timelines > Import >
    Timeline Markers from EDL). Timecodes start at 01:00:00:00, Resolve's default."""

    def tc(seconds):
        frames = round(seconds * FPS)
        f, s = frames % FPS, frames // FPS
        return f"01:{s // 60:02d}:{s % 60:02d}:{f:02d}"

    lines = ["TITLE: Microcosm trailer markers", "FCM: NON-DROP FRAME", ""]
    for i, (at, name, color) in enumerate(MARKERS, 1):
        a, b = tc(at), tc(at + 1 / FPS)
        lines.append(f"{i:03d}  001      V     C        {a} {b} {a} {b}  ")
        lines.append(f" |C:{color} |M:{name} |D:1")
        lines.append("")
    path = OUT / "markers.edl"
    path.write_text("\n".join(lines))
    print(f"  {path.relative_to(ROOT)}")


def main():
    global S, W, H
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--height", type=int, default=1440, help="frame height: 1080, 1440 or 2160 (16:9)")
    parser.add_argument("--only", help="only the clips whose name contains this (no music)")
    args = parser.parse_args()
    S = args.height / 1080
    W, H = round(args.height * 16 / 9), args.height
    OUT.mkdir(parents=True, exist_ok=True)
    print(f"Writing to {OUT.relative_to(ROOT)}/ at {W}x{H}, {FPS} fps:")
    kinds = {"line": line_card, "title": title_card, "subtitle": subtitle_card}
    for name, kind, text, length in CARDS:
        if args.only and args.only not in name:
            continue
        render(name, kinds[kind](text, length), length)
    if not args.only:
        music()
        markers()


if __name__ == "__main__":
    main()

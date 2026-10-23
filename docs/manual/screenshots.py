"""Screenshots for the manual, captured from the game.

    python3 docs/manual/screenshots.py gallery     # capture the art gallery (--gallery)
    python3 docs/manual/screenshots.py strips      # crop the captures into figures/strip-*.png
    python3 docs/manual/screenshots.py shot OUT.png [--hide-ui] -- <game arguments>

`gallery` and `strips` rebuild the body-part and animation figures from the game's own art;
rerun them after the art changes. World scenes (the cover, the whole ocean, the chemistry map,
the inspector, the family tree) come from a running world, so they are taken by hand with `shot`
(see the README in this folder for the views used).

Needs a release build (`cargo build --release`), an X11 display, xdotool and Pillow.
"""

import os
import subprocess
import sys
import time
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
APP = ROOT / "target" / "release" / "evo_app"
CAPTURES = HERE / "captures"
FIGURES = HERE / "figures"

# The gallery's layout (crates/evo_app/src/gallery.rs): rows of nine organisms 48 units apart,
# the first row 180 units below the surface, the first column at x = 1000.
SPACING, TOP, LEFT, HEIGHT = 48.0, 180.0, 1000.0, 6000.0
ZOOM = 0.08  # world units per pixel: 12.5 pixels per unit
SHOT_COLUMNS = (1, 4, 7)  # each capture shows its column and the two beside it
GALLERY_ROWS = 42
# Rows of parts partway to a related kind (gallery.rs `form_rows`), after the animation rows.
FORM_ROWS = range(29, 42)


def capture(out, game_args, delay=5.5, hide_ui=False):
    """Run the game, optionally hide its interface (Tab), and save a screenshot."""
    out = Path(out).resolve()
    out.parent.mkdir(parents=True, exist_ok=True)
    if out.exists():
        out.unlink()
    proc = subprocess.Popen(
        [str(APP), "--autosave-minutes", "0", "--screenshot", str(out),
         "--screenshot-delay", str(delay), *game_args],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, cwd=ROOT)
    if hide_ui:
        time.sleep(max(delay - 2.5, 1.0))
        window = ""
        for _ in range(100):
            found = subprocess.run(["xdotool", "search", "--name", "^Microcosm$"],
                                   capture_output=True, text=True).stdout.split()
            if found:
                window = found[-1]
                break
            time.sleep(0.1)
        subprocess.run(["xdotool", "windowactivate", "--sync", window], capture_output=True)
        subprocess.run(["xdotool", "key", "--window", window, "Tab"])
    proc.wait(timeout=delay + 60)
    return out.exists()


def gallery_path(row, col):
    return CAPTURES / f"gallery-r{row:02d}-c{col}.png"


def capture_gallery(rows=range(GALLERY_ROWS), columns=None):
    """Capture gallery rows. By default each capture covers three columns (it needs the full
    1600-pixel window); pass `columns` to capture single columns centred in the window."""
    for row in rows:
        y = HEIGHT - TOP - row * SPACING
        for col in columns or SHOT_COLUMNS:
            x = LEFT + col * SPACING
            ok = capture(gallery_path(row, col), ["--gallery", "--view", f"{x},{y},{ZOOM}"],
                         hide_ui=True)
            print(f"row {row} column {col}: {'ok' if ok else 'FAILED'}", flush=True)


def tile(row, col, size, dx=0.0):
    """Crop one gallery organism (row, column 0-8), centred `dx` units right of its spot.
    Uses the capture centred on that column if there is one, else the nearest capture."""
    shot = col if gallery_path(row, col).exists() else min(SHOT_COLUMNS, key=lambda c: abs(c - col))
    image = Image.open(gallery_path(row, shot)).convert("RGB")
    cx = image.width / 2 + ((col - shot) * SPACING + dx) / ZOOM
    cy = image.height / 2
    w, h = size
    return image.crop((int(cx - w / 2), int(cy - h / 2), int(cx + w / 2), int(cy + h / 2)))


FONT = "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf"
BACKGROUND = (8, 17, 33)
LABEL = (196, 214, 228)


def font(size):
    try:
        return ImageFont.truetype(FONT, size)
    except OSError:
        return ImageFont.load_default()


def grid(tiles, columns, label_height=56, gap=0):
    """Lay out (image, label) tiles in a grid on the dark background."""
    w, h = tiles[0][0].size
    rows = (len(tiles) + columns - 1) // columns
    out = Image.new("RGB", (columns * w + (columns - 1) * gap,
                            rows * (h + label_height) + (rows - 1) * gap), BACKGROUND)
    draw = ImageDraw.Draw(out)
    f = font(30)
    for i, (image, label) in enumerate(tiles):
        x = (i % columns) * (w + gap)
        y = (i // columns) * (h + label_height + gap)
        out.paste(image, (x, y))
        if label:
            tw = draw.textlength(label, font=f)
            draw.text((x + (w - tw) / 2, y + h + 6), label, font=f, fill=LABEL)
    return out


def save(image, name, width=2000):
    if image.width > width:
        image = image.resize((width, round(image.height * width / image.width)), Image.LANCZOS)
    image.save(FIGURES / name, optimize=True)
    print("wrote", (FIGURES / name).relative_to(ROOT))


def strips():
    # Body parts at full size (column 8 = FF), as the gallery arranges them.
    parts = [(0, "Flagellum"), (1, "Cilia bands"), (2, "Tentacles"), (3, "Fronds"),
             (4, "Fins"), (5, "Tail fluke"), (6, "Legs"), (7, "Holdfast"),
             (8, "Eye on a stalk"), (10, "Eyes"), (11, "Antennae"), (12, "Lure"),
             (13, "Spines"), (14, "Jaws"), (15, "Shell plates")]
    save(grid([(tile(r, 8, (420, 420)), name) for r, name in parts], 5), "strip-parts.png")

    # One part through its sizes: eyes from 00 to FF.
    save(grid([(tile(10, c, (240, 200)), f"{c * 255 // 8:02X}") for c in range(9)], 9),
         "strip-eye-sizes.png")

    # Body shape: elongation 00..FF, then 1..6 segments.
    elong = [(tile(16, c, (330, 200)), f"{c * 255 // 8:02X}") for c in (0, 2, 4, 6, 8)]
    segs = [(tile(17, c, (520, 200)), f"{1 + (c * 255 // 8) * 6 // 256}") for c in (0, 2, 5, 7)]
    a, b = grid(elong, 5), grid(segs, 4)
    shape = Image.new("RGB", (max(a.width, b.width), a.height + b.height + 24), BACKGROUND)
    shape.paste(a, ((shape.width - a.width) // 2, 0))
    shape.paste(b, ((shape.width - b.width) // 2, a.height + 24))
    save(shape, "strip-shape.png")

    # Budding (small offspring, then splitting in two), nine stages each. The parent sits
    # 6 units left of the column; the child grows out to its right.
    bud = [(tile(23, c, (230, 170), dx=-2.5), "") for c in range(9)]
    fission = [(tile(24, c, (230, 170), dx=-2.5), "") for c in range(9)]
    save(grid(bud + fission, 9, label_height=0), "strip-budding.png")

    # Dying: the colours drain to grey.
    save(grid([(tile(25, c, (180, 150)), "") for c in range(9)], 9, label_height=0),
         "strip-dying.png")

    # Swallowed whole: the hunter sits 4 units left of the column, its prey to its right.
    save(grid([(tile(26, c, (300, 210), dx=0.5), "") for c in range(9)], 9, label_height=0),
         "strip-swallow.png")

    form_strip()


def form_strip():
    """Parts changing kind: flagellum to tentacle, cilia band to fin, eye to lure, spines to shell
    plates, from not at all (left) to almost all the way (right). Each column is a quarter of the
    way further."""
    rows = [29, 35, 32, 41]
    tiles = [(tile(r, c, (300, 220)), f"{(c * 255 // 8) // 4 * 100 // 64}%")
             for r in rows for c in (0, 2, 4, 6, 8)]
    save(grid(tiles, 5), "strip-forms.png")


def main():
    command = sys.argv[1] if len(sys.argv) > 1 else ""
    if command == "gallery":
        capture_gallery()
        # Long bodies and the animation rows are cropped column by column; capture each centred.
        capture_gallery(rows=[17], columns=[0, 2, 5, 7])
        capture_gallery(rows=range(23, 27), columns=range(9))
    elif command == "strips":
        strips()
    elif command == "shot":
        split = sys.argv.index("--") if "--" in sys.argv else len(sys.argv)
        options = sys.argv[2:split]
        out = next(a for a in options if not a.startswith("--"))
        ok = capture(out, sys.argv[split + 1:], hide_ui="--hide-ui" in options)
        print("ok" if ok else "FAILED")
    else:
        print(__doc__)


if __name__ == "__main__":
    main()

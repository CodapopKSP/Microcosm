#!/usr/bin/env python3
"""Microcosm's app icon: the founder plankton, as the game draws it, on a deep-water tile.

    python3 packaging/icon/make_icon.py

writes, next to this script, `microcosm.png` (1024 px), `Microcosm.icns` (macOS) and
`microcosm.ico` (Windows), and the window icon the game loads at startup
(`crates/evo_app/assets/icon/icon-256.rgba`, raw RGBA). Needs Python 3 with Pillow and NumPy.

The cell follows `render/organism.wgsl` for a round photosynthesizer (body shading,
chloroplasts, nucleus, membrane rim), in the founder's colour (hue 0x55, saturation 0xC0).
"""

import colorsys
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
WINDOW_ICON = ROOT / "crates/evo_app/assets/icon/icon-256.rgba"

# The founder's colour, as `Traits::decode` makes it (sRGB).
FOUNDER = colorsys.hsv_to_rgb(0x55 / 256, 0.2 + 0.8 * 0xC0 / 255, 0.92)
# Which of the cell's random looks (wobble, chloroplasts, nucleus): picked by eye.
SEED = 0.81
# Deep water, top to bottom (sampled from the game).
WATER_TOP = (0x1D, 0x4E, 0x62)
WATER_BOTTOM = (0x06, 0x0E, 0x1F)


def to_linear(c):
    c = np.asarray(c, dtype=np.float64)
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def to_srgb(c):
    c = np.clip(c, 0.0, 1.0)
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * c ** (1 / 2.4) - 0.055)


def tonemap(c, white=2.2):
    """Soften highlights as the game's tonemapping does (so the green keeps its shading)."""
    return c * (1.0 + c / (white * white)) / (1.0 + c)


def smoothstep(a, b, x):
    t = np.clip((x - a) / (b - a), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def fract(x):
    return x - np.floor(x)


def hash21(px, py):
    qx, qy = fract(px * 123.34), fract(py * 456.21)
    d = qx * (qx + 45.32) + qy * (qy + 45.32)
    return fract((qx + d) * (qy + d))


def dots(px, py, size, seed):
    """The shader's soft cellular dots: ~1 inside a dot of radius `size` in each grid cell."""
    cx, cy = np.floor(px), np.floor(py)
    best = np.full(px.shape, 10.0)
    for j in (-1, 0, 1):
        for i in (-1, 0, 1):
            gx, gy = cx + i, cy + j
            jx = hash21(gx + seed, gy + seed)
            jy = hash21(gx + seed + 17.0, gy + seed + 17.0)
            best = np.minimum(best, np.hypot(px - gx - jx, py - gy - jy))
    return 1.0 - smoothstep(size * 0.6, size, best)


def cell(size, radius, seed):
    """The founder cell, centred in a `size` square: (linear rgb, alpha)."""
    ys, xs = np.mgrid[0:size, 0:size].astype(np.float64) + 0.5
    qx = (xs - size / 2) / radius
    qy = (size / 2 - ys) / radius
    d = np.hypot(qx, qy)
    angle = np.arctan2(qy, qx)
    aa = 1.2 / radius
    base = to_linear(FOUNDER)
    edge = 1.0 + 0.035 * np.sin(angle * 5.0 + seed * 30.0)
    alpha = 1.0 - smoothstep(edge - aa, edge + aa, d)
    col = base[None, None, :] * (1.25 + (0.75 - 1.25) * smoothstep(0.0, 1.0, d))[..., None]
    # Chloroplasts (green: they release oxygen).
    chloro = dots(qx * 5.0 + seed * 10.0, qy * 5.0 + seed * 10.0, 0.28, seed * 7.0) * (d <= 0.85)
    pigment = np.array([0.08, 0.45, 0.06])
    col = col + (pigment - col) * (chloro * 0.7)[..., None]
    # The nucleus.
    nx, ny = -0.15, 0.1 * np.sin(seed * 20.0)
    nucleus = 1.0 - smoothstep(0.24, 0.3, np.hypot(qx - nx, qy - ny))
    dark = col * 0.45 + np.array([0.03, 0.02, 0.05])
    col = col + (dark - col) * (nucleus * 0.8)[..., None]
    # Membrane rim light.
    col = col + base * 0.5 * smoothstep(edge - 0.18, edge - 0.02, d)[..., None]
    return col, alpha


def icon(size=1024, inset=0.0, seed=SEED):
    """The icon: a rounded tile of deep water (inset by `inset` of the size on each side, for
    macOS's margins) with the cell in the middle and a soft glow around it."""
    tile = size * (1.0 - 2.0 * inset)
    radius = tile * 0.36
    col, alpha = cell(size, radius, seed)
    ys, xs = np.mgrid[0:size, 0:size].astype(np.float64) + 0.5
    # Water: lighter at the top, with faint light from above.
    t = np.clip((ys - size * inset) / tile, 0.0, 1.0)[..., None]
    water = to_linear(np.array(WATER_TOP) / 255) * (1 - t) + to_linear(np.array(WATER_BOTTOM) / 255) * t
    rays = 0.025 * (0.5 + 0.5 * np.sin(xs / tile * 23.0 + np.sin(xs / tile * 7.0) * 2.0))
    water = water + (rays * (1.0 - t[..., 0]) ** 2)[..., None] * np.array([0.6, 0.9, 1.0])
    # Glow around the cell (the game's bloom).
    halo = Image.fromarray((alpha * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(radius * 0.18))
    glow = np.asarray(halo, dtype=np.float64)[..., None] / 255.0
    water = water + glow * to_linear(FOUNDER) * 0.35
    rgb = water * (1.0 - alpha[..., None]) + col * alpha[..., None]
    rgb = to_srgb(tonemap(rgb * 1.6))
    # The tile: a rounded square.
    c0, c1 = size * inset, size * (1.0 - inset)
    corner = tile * 0.225
    dx = np.maximum(np.maximum(c0 + corner - xs, xs - (c1 - corner)), 0.0)
    dy = np.maximum(np.maximum(c0 + corner - ys, ys - (c1 - corner)), 0.0)
    outside = np.hypot(dx, dy) - corner
    mask = 1.0 - smoothstep(-0.75, 0.75, outside)
    rgba = np.dstack([rgb, mask])
    return Image.fromarray((rgba * 255 + 0.5).astype(np.uint8), "RGBA")


def main():
    full = icon(1024)
    full.save(HERE / "microcosm.png")
    # macOS draws its icons inside a margin (about 10% each side).
    icon(1024, inset=0.1).save(HERE / "Microcosm.icns", sizes=[(s, s) for s in (16, 32, 64, 128, 256, 512, 1024)])
    full.save(HERE / "microcosm.ico", sizes=[(s, s) for s in (16, 24, 32, 48, 64, 128, 256)])
    WINDOW_ICON.parent.mkdir(parents=True, exist_ok=True)
    WINDOW_ICON.write_bytes(full.resize((256, 256), Image.LANCZOS).tobytes())
    for path in (HERE / "microcosm.png", HERE / "Microcosm.icns", HERE / "microcosm.ico", WINDOW_ICON):
        print(f"  {path.relative_to(ROOT)}")


if __name__ == "__main__":
    main()

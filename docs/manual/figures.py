"""Charts and diagrams for the manual, written to figures/*.svg.

The charts are computed from the same formulas as the simulation (crates/evo_core) with the
numbers in configs/default.toml, so rerun this after tuning:

    python3 docs/manual/figures.py

Needs matplotlib, plus tomli on Python < 3.11. The map needs msgpack, lz4 and numpy, and the PNG
copies of every SVG (which the in-game manual shows) need cairosvg.
"""

import math
import re
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

import matplotlib

matplotlib.use("svg")
import matplotlib.pyplot as plt

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
OUT = HERE / "figures"
CFG = tomllib.loads((ROOT / "configs" / "default.toml").read_text())

# Palette (light surface): ink, secondary and muted text, gridlines, categorical series.
INK, INK2, MUTED, GRID = "#0b0b0b", "#52514e", "#8a8984", "#e4e3df"
BLUE, ORANGE, AQUA, YELLOW = "#2a78d6", "#eb6834", "#1baf7a", "#eda100"
SANS = "Noto Sans, DejaVu Sans, Liberation Sans, sans-serif"

plt.rcParams.update(
    {
        "axes.unicode_minus": False,
        "svg.fonttype": "none",
        "font.family": ["Noto Sans", "DejaVu Sans", "sans-serif"],
        "font.size": 9,
        "axes.edgecolor": GRID,
        "axes.linewidth": 0.8,
        "axes.labelcolor": INK2,
        "axes.titlesize": 9.5,
        "axes.titleweight": "bold",
        "axes.titlecolor": INK,
        "axes.titlelocation": "left",
        "axes.spines.top": False,
        "axes.spines.right": False,
        "axes.grid": True,
        "grid.color": GRID,
        "grid.linewidth": 0.6,
        "xtick.color": INK2,
        "ytick.color": INK2,
        "xtick.major.size": 0,
        "ytick.major.size": 0,
        "lines.linewidth": 2.0,
        "lines.solid_capstyle": "round",
        "figure.facecolor": "white",
        "axes.facecolor": "white",
        "savefig.facecolor": "white",
    }
)


def save(fig, name):
    fig.savefig(OUT / name, bbox_inches="tight", pad_inches=0.08)
    plt.close(fig)
    # Spell out the `font` shorthand with a weight: some SVG renderers (WeasyPrint) misread it.
    path = OUT / name
    path.write_text(re.sub(r"font: (\d00) ([\d.]+px) ('[^']+')",
                           r"font-weight: \1; font-size: \2; font-family: \3", path.read_text()))


def end_label(ax, x, y, text, dx=4, dy=0, ha="left"):
    ax.annotate(text, (x, y), xytext=(dx, dy), textcoords="offset points",
                color=INK2, va="center", ha=ha, fontsize=8.5)


# ---------------------------------------------------------------- charts


def depth_profiles():
    """Light, temperature and the usual make-up of the water, by depth."""
    light, temp, chem = CFG["light"], CFG["temperature"], CFG["chemistry"]
    depth = [d * 10 for d in range(0, 601)]
    fig, axes = plt.subplots(1, 4, figsize=(7.2, 3.3), sharey=True)
    for ax in axes:
        ax.set_ylim(6000, 0)
        ax.set_yticks(range(0, 6001, 1000))
    axes[0].set_ylabel("Depth below the surface (units)")

    ax = axes[0]
    ax.plot([100 * 0.5 ** (d / light["half_depth"]) for d in depth], depth, color=BLUE)
    ax.set_xlim(0, 100)
    ax.set_title("Sunlight")
    ax.set_xlabel("% of surface light")
    one = light["half_depth"] * math.log2(100)
    ax.plot([1], [one], "o", ms=5, color=BLUE, mec="white", mew=1.5)
    end_label(ax, 1, one, f"1% at {one:,.0f}", dx=6)

    ax = axes[1]
    for surface, color, label in [
        (temp["surface"] + temp["season_amplitude"], ORANGE, "midsummer"),
        (temp["surface"], INK2, "spring, autumn"),
        (temp["surface"] - temp["season_amplitude"], BLUE, "midwinter"),
    ]:
        t = [temp["deep"] + (surface - temp["deep"]) * math.exp(-d / temp["thermocline_depth"])
             for d in depth]
        ax.plot(t, depth, color=color, lw=2 if color != INK2 else 1.2)
    ax.set_xlim(0, 30)
    ax.set_title("Temperature")
    ax.set_xlabel("°C (vents add up to +45)")
    ax.text(27, 150, "summer", color=INK2, fontsize=8, ha="center")
    ax.text(10, 280, "winter", color=INK2, fontsize=8, ha="center")

    ax = axes[2]
    surf = [math.exp(-d / chem["nutricline_depth"]) for d in depth]
    o2 = [chem["deep_oxygen"] + (1 - chem["deep_oxygen"]) * s for s in surf]
    co2 = [1.3 - 0.3 * s for s in surf]
    ax.plot(o2, depth, color=BLUE)
    ax.plot(co2, depth, color=ORANGE)
    ax.set_xlim(0.5, 1.5)
    ax.set_title("Dissolved gases")
    ax.set_xlabel("× air equilibrium")
    ax.text(o2[-1] - 0.03, 5600, "O₂", color=INK2, ha="right")
    ax.text(co2[-1] + 0.03, 5600, "CO₂", color=INK2)

    ax = axes[3]
    nut = [chem["nutrient_deep"] + (chem["nutrient_surface"] - chem["nutrient_deep"]) * s
           for s in surf]
    ax.plot(nut, depth, color=AQUA)
    ax.set_xlim(0, 0.6)
    ax.set_title("Nutrients")
    ax.set_xlabel("concentration")
    fig.tight_layout(w_pad=1.2)
    save(fig, "chart-depth.svg")


def day_cycle():
    """Sunlight at the surface over one day, in three seasons."""
    light = CFG["light"]
    hours = [h / 10 for h in range(0, 241)]

    def sun(hour, season):
        elevation = -math.cos(2 * math.pi * hour / 24)
        t = min(max((elevation + 0.15) / 0.55, 0), 1)
        daylight = t * t * (3 - 2 * t)
        return (1 + light["season_amplitude"] * season) * (
            light["night_level"] + (1 - light["night_level"]) * daylight)

    fig, ax = plt.subplots(figsize=(5.6, 2.5))
    for season, color, label in [(1, ORANGE, "midsummer"), (0, INK2, "spring, autumn"),
                                 (-1, BLUE, "midwinter")]:
        y = [100 * sun(h, season) for h in hours]
        ax.plot(hours, y, color=color, lw=2 if color != INK2 else 1.2)
        ax.text(12, y[120] + 4, label, color=INK2, fontsize=8.5, ha="center")
    ax.set_xlim(0, 24)
    ax.set_xticks(range(0, 25, 3))
    ax.set_xticklabels([f"{h:02d}:00" for h in range(0, 25, 3)])
    ax.set_ylim(0, 140)
    ax.set_ylabel("% of average noon sunlight")
    ax.set_title("Sunlight at the surface through one day (a day is 10 minutes at 1×)")
    save(fig, "chart-day.svg")


def swimmer_efficiency():
    """How well each kind of swimming organ works on bodies of each size."""
    parts = CFG["parts"]
    radii = [10 ** (k / 100) for k in range(0, 241)]
    small = [1 / (1 + (r / parts["small_swimmer_radius"]) ** 1.5) for r in radii]
    large = [0.3 + 0.7 * r / (r + parts["large_swimmer_radius"]) for r in radii]
    fig, ax = plt.subplots(figsize=(5.6, 2.6))
    ax.plot(radii, small, color=BLUE)
    ax.plot(radii, large, color=ORANGE)
    ax.set_xscale("log")
    ax.set_xlim(1, 250)
    ax.set_xticks([1, 2, 4, 8, 16, 32, 64, 128, 250])
    ax.set_xticklabels(["1", "2", "4", "8", "16", "32", "64", "128", "250"])
    ax.minorticks_off()
    ax.set_ylim(0, 1.05)
    ax.set_xlabel("Adult body radius (units)")
    ax.set_ylabel("Thrust multiplier")
    ax.set_title("Swimming organs by body size")
    end_label(ax, 2.3, small[36] + 0.02, "flagella, cilia", dy=4)
    end_label(ax, 60, large[178] - 0.04, "fins, flukes", dy=-6)
    save(fig, "chart-swimmers.svg")


def temperature_stress():
    """Metabolism and damage as the water drifts from the preferred temperature."""
    met = CFG["metabolism"]
    xs = [k / 100 for k in range(-300, 301)]
    s = [x * x for x in xs]
    mult = [1 + met["temp_stress_cost"] * min(v, 4) for v in s]
    dmg = [100 * met["temp_damage"] * min(max(v - 1, 0), 10) for v in s]
    fig, (a1, a2) = plt.subplots(1, 2, figsize=(6.6, 2.5), sharex=True)
    a1.plot(xs, mult, color=BLUE)
    a1.set_title("Basal metabolism")
    a1.set_ylabel("× normal")
    a1.set_ylim(0.8, 3.2)
    a2.plot(xs, dmg, color=ORANGE)
    a2.set_title("Damage")
    a2.set_ylabel("% of max HP per second")
    a2.set_ylim(0, 45)
    for ax in (a1, a2):
        ax.set_xlim(-3, 3)
        ax.set_xticks([-3, -2, -1, 0, 1, 2, 3])
        # En dashes for minus signs: some Noto Sans builds lack U+2212.
        ax.set_xticklabels(["–3", "–2", "–1", "0", "+1", "+2", "+3"])
        ax.set_xlabel("(T – preferred) ÷ tolerance")
        ax.axvspan(-1, 1, color="#f0efec", zorder=0, lw=0)
    a1.text(0, 2.9, "within tolerance", ha="center", color=INK2, fontsize=8)
    fig.tight_layout(w_pad=2)
    save(fig, "chart-temperature.svg")


def diet_mixing():
    """Capabilities of a full specialist taking up a second diet."""
    k = CFG["genes"]["generalist_penalty"]
    xs = [i / 100 for i in range(0, 101)]
    main = [1 * (1 - k * x) for x in xs]
    second = [x * (1 - k * 1) for x in xs]
    total = [a + b for a, b in zip(main, second)]
    fig, ax = plt.subplots(figsize=(5.6, 2.6))
    ax.plot(xs, total, color=INK2, lw=1.2)
    ax.plot(xs, main, color=BLUE)
    ax.plot(xs, second, color=ORANGE)
    end_label(ax, 1, total[-1], "total")
    ax.text(0.3, main[30] + 0.05, "first diet", color=INK2, fontsize=8.5, ha="center")
    ax.text(0.3, second[30] - 0.09, "second diet", color=INK2, fontsize=8.5, ha="center")
    ax.set_xlim(0, 1)
    ax.set_ylim(0, 1.3)
    ax.set_xticks([0, 0.25, 0.5, 0.75, 1])
    ax.set_xticklabels(["00", "40", "80", "BF", "FF"])
    ax.set_xlabel("Second diet gene (first diet gene at FF)")
    ax.set_ylabel("Effective capability")
    ax.set_title("Mixing diets: what a second diet gene buys")
    save(fig, "chart-diets.svg")


# ---------------------------------------------------------------- diagrams


def svg(width, height, body):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" '
            f'width="{width}" height="{height}" font-family="{SANS}">'
            f'<rect width="{width}" height="{height}" fill="white"/>{body}</svg>')


def text(x, y, s, size=11, color=INK, anchor="middle", weight="normal", family=None):
    fam = f' font-family="{family}"' if family else ""
    return (f'<text x="{x}" y="{y}" font-size="{size}" fill="{color}" text-anchor="{anchor}"'
            f' font-weight="{weight}"{fam}>{s}</text>')


MONO = "Ubuntu Mono, DejaVu Sans Mono, monospace"

# Chromosome colors (a light tint for the cell, a darker one for the letter).
CHROMOSOMES = [
    ("B", "Body", 6, "#cde2fb", "#1c5cab"),
    ("C", "Color", 3, "#fbd9cb", "#a8431a"),
    ("D", "Diet", 4, "#c9eedd", "#127a55"),
    ("G", "Chemistry", 6, "#fbe8b8", "#8a5e00"),
    ("H", "Health", 2, "#f6d3e0", "#a53863"),
    ("N", "Nature", 10, "#d6f0d6", "#006300"),
    ("P", "Parts", 24, "#dedaf3", "#4a3aa7"),
    ("R", "Reproduction", 3, "#f8d1d1", "#a83232"),
    ("T", "Temperature", 2, "#e4e3df", "#52514e"),
]
ANCESTOR = CFG["population"]["ancestor"]


def genome_map():
    """The 60 bytes of the ancestor's genome, grouped into chromosomes."""
    hexes = "".join(c for part in ANCESTOR.split() for c in part.split(":")[-1])
    values = [hexes[i:i + 2] for i in range(0, len(hexes), 2)]
    cell, gap, left, top = 22, 10, 10, 40
    # Two rows: B C D G H N | P R T
    rows = [CHROMOSOMES[:6], CHROMOSOMES[6:]]
    body, index = [], 0
    width = 0
    for r, row in enumerate(rows):
        x, y = left, top + r * 92
        for letter, name, n, fill, dark in row:
            w = n * cell
            body.append(text(x + w / 2, y - 22, f"{letter}", 15, dark, weight="bold"))
            body.append(text(x + w / 2, y - 8, name, 8.5, INK2))
            for k in range(n):
                cx = x + k * cell
                body.append(f'<rect x="{cx + 1}" y="{y}" width="{cell - 2}" height="26" rx="3" '
                            f'fill="{fill}"/>')
                body.append(text(cx + cell / 2, y + 17, values[index], 10.5, INK, family=MONO))
                if letter == "P" and k % 3 == 0:
                    body.append(text(cx + 1.5 * cell, y + 42, f"slot {k // 3 + 1}", 8.5, MUTED))
                elif letter != "P":
                    body.append(text(cx + cell / 2, y + 40, f"{letter}{k}", 8, MUTED))
                index += 1
            x += w + gap
        width = max(width, x)
    (OUT / "diagram-genome.svg").write_text(svg(width + left - gap, 200, "".join(body)))


def part_bytes():
    """How the three bytes of a part slot are read."""
    body = []
    bit = 22
    x0, y = 20, 46

    def byte(x, label, fields):
        body.append(text(x + 4 * bit, y - 24, label, 11.5, INK, weight="bold"))
        for b in range(8):
            body.append(text(x + b * bit + bit / 2, y - 8, str(7 - b), 8, MUTED))
        bx = x
        line = 0
        for nbits, fill, name, values in fields:
            w = nbits * bit
            body.append(f'<rect x="{bx + 1}" y="{y}" width="{w - 2}" height="28" rx="3" fill="{fill}"/>')
            body.append(text(bx + w / 2, y + 18, name, 10, INK))
            # A byte with two wide fields lists the second field's values below the first's.
            for v in values:
                body.append(text(x + 2 if stacked else bx + 2, y + 46 + line * 13, v, 9, INK2,
                                 anchor="start"))
                line += 1
            if not stacked:
                line = 0
            bx += w

    stacked = False

    byte(x0, "Type", [
        (4, "#dedaf3", "symmetry", ["0  single", "1  mirrored pair", "2–5  ring of 3–6"]),
        (4, "#cde2fb", "kind", ["0  empty slot", "1–F  part kind", "(see the catalog)"]),
    ])
    byte(x0 + 8 * bit + 30, "Size", [
        (8, "#c9eedd", "size 00–FF", ["strength, length,", "and how elaborate", "it looks"]),
    ])
    stacked = True
    byte(x0 + 16 * bit + 60, "Place", [
        (3, "#fbe8b8", "where", ["where: 0–5 segment 1–6, 6 every", "segment, 7 tip of the previous part"]),
        (5, "#fbd9cb", "angle", ["angle: 0–31 around the attachment", "point (0 front, 8 left, 16 back)"]),
    ])
    (OUT / "diagram-part-bytes.svg").write_text(svg(24 * bit + 110 + 2 * x0, 150, "".join(body)))


# Arrowheads are drawn as plain triangles: WeasyPrint misplaces SVG markers on lines that point
# left, and ignores paint-order (so halo-outlined labels vanish under their own outline).
def head(x, y, angle, color, size=8):
    """A filled arrowhead with its tip at (x, y), pointing along `angle` (radians, SVG axes)."""
    back, side = size, size * 0.45
    bx, by = x - back * math.cos(angle), y - back * math.sin(angle)
    px, py = -math.sin(angle) * side, math.cos(angle) * side
    return (f'<path d="M{x:.1f},{y:.1f} L{bx + px:.1f},{by + py:.1f} L{bx - px:.1f},{by - py:.1f} z" '
            f'fill="{color}"/>')


def arrow(x1, y1, x2, y2, color, width=1.5, size=8):
    """A straight arrow from (x1, y1) to (x2, y2)."""
    angle = math.atan2(y2 - y1, x2 - x1)
    ex, ey = x2 - size * 0.8 * math.cos(angle), y2 - size * 0.8 * math.sin(angle)
    return (f'<line x1="{x1:.1f}" y1="{y1:.1f}" x2="{ex:.1f}" y2="{ey:.1f}" stroke="{color}" '
            f'stroke-width="{width}"/>' + head(x2, y2, angle, color, size))


def label_box(x, y, s, size=9, color=INK2):
    """Text centred at (x, y) on a white box, so it reads over the lines under it."""
    w = 0.52 * size * len(s) + 6
    return (f'<rect x="{x - w / 2:.1f}" y="{y - size * 0.75:.1f}" width="{w:.1f}" height="{size * 1.4:.1f}" '
            f'fill="white"/>' + text(f"{x:.1f}", f"{y + size * 0.35:.1f}", s, size, color))


def placement_wheel():
    """The 32 angles of the placement byte, around a body facing right."""
    cx, cy, a, b = 210, 150, 70, 52
    body = [f'<ellipse cx="{cx}" cy="{cy}" rx="{a}" ry="{b}" fill="#d6f0d6" stroke="#4f9a4f" '
            f'stroke-width="1.5"/>',
            f'<circle cx="{cx + 12}" cy="{cy}" r="9" fill="#b3dbb3"/>']
    for k in range(32):
        th = k / 32 * 2 * math.pi
        # Point on the ellipse and the outward normal, as parts.rs computes it.
        px, py = a * math.cos(th), b * math.sin(th)
        n = math.atan2(math.sin(th) / b, math.cos(th) / a)
        length = 14 if k % 4 == 0 else 7
        x1, y1 = cx + px, cy - py
        x2, y2 = x1 + length * math.cos(n), y1 - length * math.sin(n)
        body.append(f'<line x1="{x1:.1f}" y1="{y1:.1f}" x2="{x2:.1f}" y2="{y2:.1f}" '
                    f'stroke="{INK2 if k % 4 == 0 else MUTED}" stroke-width="{1.5 if k % 4 == 0 else 1}"/>')
        if k % 4 == 0:
            lx, ly = x1 + 30 * math.cos(n), y1 - 30 * math.sin(n) + 4
            body.append(text(f"{lx:.1f}", f"{ly:.1f}", str(k), 11, INK, weight="bold"))
    body.append(text(cx + a + 66, cy + 4, "front", 10, INK2, anchor="start"))
    body.append(text(cx - a - 66, cy + 4, "back", 10, INK2, anchor="end"))
    body.append(text(cx, cy - b - 44, "left side (+90°)", 10, INK2))
    body.append(text(cx, cy + b + 52, "right side (–90°)", 10, INK2))
    # Heading arrow.
    body.append(arrow(cx - 30, cy, cx + 40, cy, "#2f6f2f"))
    (OUT / "diagram-placement.svg").write_text(svg(420, 300, "".join(body)))


def thrust_modes():
    """Which way each kind of movement organ moves the body."""
    body = []

    def panel(x, title, sub, organ, arrows):
        cx, cy = x + 95, 112
        body.append(text(cx, 22, title, 12, INK, weight="bold"))
        body.append(text(cx, 38, sub, 9.5, INK2))
        body.append(f'<ellipse cx="{cx}" cy="{cy}" rx="34" ry="24" fill="#d6f0d6" stroke="#4f9a4f" stroke-width="1.2"/>')
        body.append(organ(cx, cy))
        for (x1, y1, x2, y2) in arrows:
            body.append(arrow(cx + x1, cy + y1, cx + x2, cy + y2, BLUE, width=2.5, size=11))

    org = "#2f6f2f"
    panel(0, "Pushing", "flagellum, tail fluke",
          lambda cx, cy: f'<path d="M{cx - 34},{cy} q-12,-8 -22,0 t-22,0" fill="none" stroke="{org}" stroke-width="2"/>',
          [(40, 0, 78, 0)])
    panel(200, "Across", "fin, cilia band, leg",
          lambda cx, cy: (f'<path d="M{cx - 6},{cy - 22} l-14,-26 l20,10 z" fill="{org}"/>'
                          f'<path d="M{cx - 6},{cy + 22} l-14,26 l20,-10 z" fill="{org}"/>'),
          [(40, 0, 78, 0), (-40, 0, -78, 0)])
    panel(400, "Along", "tentacle",
          lambda cx, cy: f'<path d="M{cx},{cy - 24} q7,-8 0,-16 t0,-16" fill="none" stroke="{org}" stroke-width="2"/>',
          [(48, -4, 48, -40), (48, 4, 48, 40)])
    body.append(text(300, 186, "Arrows: directions the organ lets the body move, relative to the body. "
                     "Thrust follows the angle in between.", 9.5, INK2))
    (OUT / "diagram-thrust.svg").write_text(svg(600, 198, "".join(body)))


def feeding_reach():
    """The mouth, the rest of the body, and a tentacle's grasp."""
    cx, cy, r = 200, 150, 50
    # Mouth for a round body: radius 0.85 x head width (≈0.85 r), centre at nose − 0.6 x mouth radius.
    mr = 0.85 * (r * math.sqrt(1 - 0.25))  # head width sampled halfway along the body
    mr = max(mr, 0.4 * r)
    mx = cx + r - 0.6 * mr
    base = (cx - 30, cy - 40)
    tentacle = 92
    body = [
        f'<circle cx="{base[0]}" cy="{base[1]}" r="{tentacle}" fill="#fbe8b8" fill-opacity="0.55" stroke="#c99a2e" stroke-dasharray="4 3"/>',
        f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="#d6f0d6" stroke="#4f9a4f" stroke-width="1.5"/>',
        f'<circle cx="{mx:.1f}" cy="{cy}" r="{mr:.1f}" fill="#fbd9cb" fill-opacity="0.85" stroke="{ORANGE}" stroke-width="1.5"/>',
        f'<path d="M{base[0]},{base[1]} q-24,-22 -46,-18 t-44,-12" fill="none" stroke="#2f6f2f" stroke-width="2.5"/>',
        f'<circle cx="{base[0]}" cy="{base[1]}" r="3" fill="#2f6f2f"/>',
        text(mx + 6, cy + 4, "mouth", 10.5, INK, weight="bold"),
        text(cx - 18, cy + 30, "body", 10.5, INK, weight="bold"),
        text(base[0] - 78, base[1] - 44, "tentacle grasp", 10.5, INK, anchor="start", weight="bold"),
        text(cx + 96, cy - 70, "Mouth: eats at full rate", 10, INK2, anchor="start"),
        text(cx + 96, cy - 54, "Rest of the body: 30% of the rate", 10, INK2, anchor="start"),
        text(cx + 96, cy - 38, "Tentacle: grabs one thing within its", 10, INK2, anchor="start"),
        text(cx + 96, cy - 24, "length of its base and drags it to the mouth", 10, INK2, anchor="start"),
        arrow(cx - 60, cy + 80, cx + 70, cy + 80, "#2f6f2f"),
        text(cx + 4, cy + 96, "heading", 9.5, INK2),
    ]
    (OUT / "diagram-reach.svg").write_text(svg(560, 260, "".join(body)))


def flow_diagram(name, nodes, edges, width, height, groups=()):
    """Boxes and arrows. nodes: id -> (x, y, w, label lines, fill). edges: (a, b, label, bend),
    where bend is a curve's offset or a list of points to route through. groups: dashed boxes
    drawn behind, as (x0, y0, x1, y1, label)."""
    body, labels = [], []
    for x0, y0, x1, y1, label in groups:
        body.append(f'<rect x="{x0}" y="{y0}" width="{x1 - x0}" height="{y1 - y0}" rx="10" '
                    f'fill="#f7f7f5" stroke="{MUTED}" stroke-width="1" stroke-dasharray="5 4"/>')
        body.append(text(x0 + 12, y1 - 10, label, 9.5, INK2, anchor="start"))

    def anchor(n, toward):
        x, y, w, lines, _ = nodes[n]
        h = 16 + 14 * len(lines)
        tx, ty = toward
        dx, dy = tx - x, ty - y
        if dx == 0 and dy == 0:
            return x, y
        sx = (w / 2) / abs(dx) if dx else float("inf")
        sy = (h / 2) / abs(dy) if dy else float("inf")
        s = min(sx, sy)
        return x + dx * s, y + dy * s

    for a, b, label, bend in edges:
        if a == b:
            # A loop under the box, back into it.
            x, y, w, lines, _ = nodes[a]
            bottom = y + (16 + 14 * len(lines)) / 2
            x1, x2, depth = x - 22, x + 22, 46
            body.append(f'<path d="M{x1},{bottom} C{x1 - 8},{bottom + depth} {x2 + 8},{bottom + depth} '
                        f'{x2 + 1},{bottom + 7}" fill="none" stroke="{INK2}" stroke-width="1.4"/>')
            body.append(head(x2, bottom, math.atan2(-depth, -8), INK2))
            if label:
                labels.append(label_box(x, bottom + depth * 0.75 + 11, label))  # under the loop
            continue
        if isinstance(bend, list):
            # An explicit route: a polyline through the given points.
            (px, py), (qx, qy) = bend[-2], bend[-1]
            angle = math.atan2(qy - py, qx - px)
            end = (qx - 6 * math.cos(angle), qy - 6 * math.sin(angle))
            pts = " ".join(f"{x:.1f},{y:.1f}" for x, y in bend[:-1] + [end])
            body.append(f'<polyline points="{pts}" fill="none" stroke="{INK2}" stroke-width="1.4" '
                        f'stroke-linejoin="round"/>' + head(qx, qy, angle, INK2))
            if label:
                # On the middle of the route's longest leg.
                (sx, sy), (tx, ty) = max(zip(bend, bend[1:]),
                                         key=lambda st: math.dist(st[0], st[1]))
                labels.append(label_box((sx + tx) / 2, (sy + ty) / 2, label))
            continue
        ax_, ay_ = nodes[a][0], nodes[a][1]
        bx_, by_ = nodes[b][0], nodes[b][1]
        mx, my = (ax_ + bx_) / 2, (ay_ + by_) / 2
        # Control point offset perpendicular to the line.
        dx, dy = bx_ - ax_, by_ - ay_
        ln = math.hypot(dx, dy) or 1
        cx, cy = mx - dy / ln * bend, my + dx / ln * bend
        x1, y1 = anchor(a, (cx, cy))
        x2, y2 = anchor(b, (cx, cy))
        # The curve's direction at its end is from the control point to the end.
        angle = math.atan2(y2 - cy, x2 - cx)
        ex, ey = x2 - 6 * math.cos(angle), y2 - 6 * math.sin(angle)
        body.append(f'<path d="M{x1:.1f},{y1:.1f} Q{cx:.1f},{cy:.1f} {ex:.1f},{ey:.1f}" fill="none" '
                    f'stroke="{INK2}" stroke-width="1.4"/>' + head(x2, y2, angle, INK2))
        if label:
            lx, ly = 0.25 * x1 + 0.5 * cx + 0.25 * x2, 0.25 * y1 + 0.5 * cy + 0.25 * y2
            lines = label.split("|")
            for i, line in enumerate(lines):
                labels.append(label_box(lx, ly + i * 12 - (len(lines) - 1) * 6, line))
    body.extend(labels)  # over every line
    for n, (x, y, w, lines, fill) in nodes.items():
        h = 16 + 14 * len(lines)
        body.append(f'<rect x="{x - w / 2}" y="{y - h / 2}" width="{w}" height="{h}" rx="6" fill="{fill}"/>')
        for i, line in enumerate(lines):
            weight = "bold" if i == 0 else "normal"
            color = INK if i == 0 else INK2
            body.append(text(x, y - h / 2 + 20 + 14 * i, line, 10.5 if i == 0 else 9.5, color, weight=weight))
    (OUT / name).write_text(svg(width, height, "".join(body)))


def energy_flow():
    nodes = {
        "sun": (130, 36, 130, ["Sunlight"], "#fbe8b8"),
        "minerals": (520, 36, 170, ["Minerals", "vents, rotting detritus"], "#fbd9cb"),
        "photo": (130, 150, 150, ["Photosynthesizers", "live on light"], "#d6f0d6"),
        "chemo": (520, 150, 150, ["Chemosynthesizers", "live on minerals"], "#d6f0d6"),
        "hunt": (325, 235, 130, ["Hunters", "eat the living"], "#f8d1d1"),
        "scav": (520, 325, 140, ["Scavengers", "eat the dead"], "#dedaf3"),
        "carcass": (130, 445, 120, ["Carcasses"], "#e4e3df"),
        "snow": (325, 530, 170, ["Detritus", "(marine snow)"], "#e4e3df"),
    }
    # Every kind of organism dies into a carcass: one arrow from the box around them all.
    living = (40, 115, 615, 372, "Every living organism")
    edges = [
        ("sun", "photo", "", 0), ("minerals", "chemo", "", 0),
        ("photo", "hunt", "eaten", 0), ("chemo", "hunt", "eaten", 0),
        ("scav", "hunt", "eaten", 0),
        ("hunt", "hunt", "eat each other", 0),
        (None, "carcass", "die", [(130, 372), (130, 401), (130, 430)]),
        ("carcass", "scav", "eaten", 0), ("carcass", "hunt", "eaten", 0),
        ("carcass", "snow", "rot", 0), ("snow", "scav", "filtered", 0),
        ("snow", "minerals", "rots into minerals", [(410, 530), (680, 530), (680, 36), (607, 36)]),
    ]
    flow_diagram("diagram-energy.svg", nodes, edges, 740, 575, groups=[living])


def nutrient_cycle():
    nodes = {
        "surface": (110, 50, 170, ["Sunlit water", "few nutrients"], "#cde2fb"),
        "bodies": (400, 50, 170, ["Living bodies", "1 nutrient per mass"], "#d6f0d6"),
        "snow": (400, 210, 170, ["Marine snow", "sinks 3 units/s"], "#e4e3df"),
        "deep": (110, 210, 170, ["Deep water", "rich in nutrients"], "#184f95"),
    }
    edges = [
        ("surface", "bodies", "uptake", 24),
        ("bodies", "surface", "excreted", 24),
        ("bodies", "snow", "death, rotting", 0),
        ("snow", "deep", "released as it rots", 0),
        ("deep", "surface", "upwellings, vents,|slow mixing", 0),
    ]
    flow_diagram("diagram-nutrients.svg", nodes, edges, 520, 260)
    # Deep-water box: light text on the dark fill.
    p = OUT / "diagram-nutrients.svg"
    s = p.read_text()
    s = s.replace(f'fill="{INK}" text-anchor="middle" font-weight="bold">Deep water',
                  'fill="white" text-anchor="middle" font-weight="bold">Deep water')
    s = s.replace(f'fill="{INK2}" text-anchor="middle" font-weight="normal">rich in nutrients',
                  'fill="#cde2fb" text-anchor="middle" font-weight="normal">rich in nutrients')
    p.write_text(s)


def part_ring():
    """The part catalog in order: a type mutation moves one step around the ring."""
    kinds = ["Flagellum", "Cilia band", "Tentacle", "Frond", "Fin", "Tail fluke", "Leg",
             "Holdfast", "Stalk", "Eye", "Antenna", "Lure", "Spine", "Jaw", "Shell plate"]
    cx, cy, R = 300, 175, 125
    body = [f'<circle cx="{cx}" cy="{cy}" r="{R}" fill="none" stroke="{GRID}" stroke-width="2"/>']
    for i, k in enumerate(kinds):
        th = -math.pi / 2 + i / len(kinds) * 2 * math.pi
        x, y = cx + R * math.cos(th), cy + R * math.sin(th)
        body.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="13" fill="#dedaf3"/>')
        body.append(text(f"{x:.1f}", f"{y + 4:.1f}", f"{i + 1:X}", 11, "#4a3aa7", weight="bold", family=MONO))
        lx, ly = cx + (R + 24) * math.cos(th), cy + (R + 24) * math.sin(th) + 4
        anchor = "start" if math.cos(th) > 0.2 else "end" if math.cos(th) < -0.2 else "middle"
        body.append(text(f"{lx:.1f}", f"{ly:.1f}", k, 10.5, INK, anchor=anchor))
    body.append(text(cx, cy - 6, "kind nibble", 10, INK2))
    body.append(text(cx, cy + 10, "1 – F", 12, INK, weight="bold", family=MONO))
    (OUT / "diagram-part-ring.svg").write_text(svg(600, 350, "".join(body)))


def deep_ocean_map():
    """The Deep Ocean map: rock, caves, vents and start areas. Needs msgpack and lz4."""
    try:
        import lz4.block
        import msgpack
        import numpy as np
    except ModuleNotFoundError:
        print("skipping map-deep-ocean.svg (needs msgpack, lz4 and numpy)")
        return
    from matplotlib.colors import ListedColormap
    from matplotlib.patches import Rectangle

    raw = (ROOT / "configs" / "maps" / "Deep Ocean.map").read_bytes()
    assert raw.startswith(b"EVOMAP1\n")
    size = int.from_bytes(raw[8:12], "little")
    m = msgpack.unpackb(lz4.block.decompress(raw[12:], uncompressed_size=size), raw=False)
    nx, ny, cell, water = m["nx"], m["ny"], m["cell"], m["water_level"]

    def mask(words):
        bits = np.unpackbits(np.array(words, dtype="<u8").view(np.uint8), bitorder="little")
        return bits[: nx * ny].reshape(ny, nx).astype(bool)

    rock, cave = mask(m["bits"]), mask(m["cave"])
    rows = int(water / cell)  # the water; land above it is drawn as rock against white
    # 0 air, 1 water, 2 cave water, 3 rock
    grid = np.zeros((ny, nx), dtype=np.uint8)
    grid[:rows] = 1
    grid[cave] = 2
    grid[rock] = 3
    cmap = ListedColormap(["#ffffff", "#cde2fb", "#3e6fa8", "#a39d93"])
    fig, ax = plt.subplots(figsize=(7.2, 2.95))
    height = ny * cell
    # Row 0 is the bottom of the map (depth = water level); every second cell keeps the SVG small.
    ax.imshow(grid[::2, ::2], cmap=cmap, vmin=0, vmax=3, interpolation="nearest", origin="lower",
              extent=(0, nx * cell, water, water - height), aspect="auto")
    for (x, y), color, marker, size_ in (
        [((v[0], v[1]), ORANGE, "o", 4) for v in m["vent_spawns"]]
        + [((v[0], v[1]), "#e34948", "^", 7) for v in m["permanent_vents"]]
    ):
        ax.plot(x, water - y, marker, color=color, ms=size_, mec="white", mew=0.8)
    for area in m["spawn_areas"]:
        (x0, y0), (x1, y1) = area["min"], area["max"]
        ax.add_patch(Rectangle((x0, water - y1), x1 - x0, y1 - y0, fill=False,
                               ec=INK, lw=0.8, ls=(0, (3, 2))))
        ax.text(x0 + 120, water - y1 + 120, area["name"], fontsize=7.5, color=INK, va="top")
    ax.set_xlim(0, nx * cell)
    ax.set_ylim(water, -(height - water))
    ax.set_xlabel("x (units)")
    ax.set_ylabel("Depth (units)")
    ax.set_yticks(range(0, 6001, 1000))
    ax.grid(False)
    for spine in ax.spines.values():
        spine.set_visible(False)
    save(fig, "map-deep-ocean.svg")


def export_png(scale=1.25):
    """PNG copies of every SVG figure, for the in-game manual (the game shows PNGs only)."""
    try:
        import cairosvg
    except ModuleNotFoundError:
        print("skipping PNG copies for the game (needs cairosvg)")
        return
    for svg_path in sorted(OUT.glob("*.svg")):
        cairosvg.svg2png(url=str(svg_path), write_to=str(svg_path.with_suffix(".png")),
                         scale=scale, background_color="white")


if __name__ == "__main__":
    OUT.mkdir(exist_ok=True)
    deep_ocean_map()
    depth_profiles()
    day_cycle()
    swimmer_efficiency()
    temperature_stress()
    diet_mixing()
    genome_map()
    part_bytes()
    placement_wheel()
    thrust_modes()
    feeding_reach()
    energy_flow()
    nutrient_cycle()
    part_ring()
    export_png()
    print("figures written to", OUT)

# The manual

`manual.md` is the player's guide to how the world of Microcosm works. `Microcosm-Manual.pdf` is
the same text laid out for sharing. The game shows `manual.md` itself on the main menu's Documentation page (`crates/evo_app/src/manual.rs`), with the figures built in; its Open as PDF button opens the built-in PDF. Rebuild the game after changing the text, the figures or the PDF.

The game shows PNG and JPEG only: `figures.py` writes a PNG copy of every SVG, and the page uses those. Any `.png` or `.jpg` in `figures/` is built in automatically (see `crates/evo_app/build.rs`). Every number in it comes from `configs/default.toml` or the
simulation code; when either changes, update the text and rebuild.

| File | What |
|---|---|
| `manual.md` | The text. Plain Markdown: a figure is an image alone in its paragraph, and its alt text is the caption. |
| `build.py` | `manual.md` → `Microcosm-Manual.pdf` (cover, contents with page numbers, numbered figures). `--html` also writes `manual.html`. |
| `style.css` | The PDF's page layout |
| `figures.py` | Charts and diagrams (`figures/chart-*`, `diagram-*`, `map-deep-ocean.svg`, each with a PNG copy for the game), computed from `configs/default.toml` and the formulas in `crates/evo_core` |
| `screenshots.py` | Captures from the game: `gallery` captures the art gallery into `captures/` (git-ignored), `strips` crops them into `figures/strip-*.png`, `shot` takes one screenshot |
| `figures/` | Everything the manual shows |

## Rebuilding

```bash
pip install markdown weasyprint matplotlib tomli msgpack lz4 numpy pillow cairosvg
python3 docs/manual/figures.py                 # charts and diagrams
python3 docs/manual/screenshots.py gallery     # needs a release build, X11 and xdotool
python3 docs/manual/screenshots.py strips
python3 docs/manual/build.py
```

The world scenes (`shot-*`) were taken from a saved world (fourth game year) with `screenshots.py shot`:

| Figure | Game arguments |
|---|---|
| `shot-cover.jpg` | `--view 12750,5900,0.15`, interface hidden |
| `shot-surface.jpg` | `--view 15200,5930,0.15`, interface hidden |
| `shot-world.jpg`, `shot-oxygen.jpg` | `--view 10200,3100,13.2` (and `--map oxygen`), interface hidden, cropped to the water |
| `shot-vent.jpg` | `--view 13345,5170,0.22`, interface hidden |
| `shot-tree.png` | `--tree`, cropped to the charts window |
| `shot-inspector.png` | `--view <organism> --select`, then the Genome tab and its B row clicked; cropped |

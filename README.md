# Microcosm

A side-on ocean where life evolves on its own, built to be watched (for example, running on a stream for weeks). The world starts with a single kind of tiny floating green microbe with no body parts. Everything else has to evolve: flagella, fins, legs, tentacles, fronds, eyes, jaws, shells and the rest of the body plan; scavenging, predation and chemosynthesis at hydrothermal vents; breathing oxygen or fermenting, toxins, and large bodies. The water has real chemistry: photosynthesis releases oxygen, breathing and rotting use it, and nutrients cycle through bodies. Species get names, founders and family trees. They rise, spread, and go extinct.

Every organism has a **60-byte genome in nine lettered chromosomes**, one byte per gene, readable by a human:

```
B:20A050200000 C:55C000 D:FF000000 G:E0FF80000000 H:2030 N:80808080808080808000
P:000000 000000 000000 000000 000000 000000 000000 000000 R:FF00B0 T:80A0
```

Chromosome **P** holds up to eight body parts (type and symmetry, size, placement). Parts get copied, lost, and change into related kinds, so body plans evolve.

Click any organism to see what each gene does.

- [docs/VISION.md](docs/VISION.md): goals and principles
- [docs/DESIGN.md](docs/DESIGN.md): the rules (world, genome, ecology, behavior, species)
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): code layout, tick pipeline, performance
- [docs/IDEAS.md](docs/IDEAS.md): future ideas, parked until the current systems are balanced
- [docs/manual/Microcosm-Manual.pdf](docs/manual/Microcosm-Manual.pdf): the player's guide to how the world works (source: [docs/manual/manual.md](docs/manual/manual.md))

## Requirements

- Rust (stable, 1.95+)
- Linux: `sudo apt install libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev pkg-config`

## Run

```bash
# Viewer (release mode matters: the sim is much slower unoptimized).
# Opens the main menu: continue the autosave, set up a new world (map, seed, starting
# population, where life starts, the ancestor's genome), load any save, or read the manual
# (Documentation). The Menu
# button in the game saves and comes back here.
cargo run --release

# Skip the menu: start a fresh world (or: --config my.toml, --seed 7, --world-map "Deep Ocean")
cargo run --release -- --new

# Load a specific save
cargo run --release -- --load ~/.local/share/microcosm/saves/quicksave.bin

# Saves from before body parts (genome v2) can't be loaded; start a new world.
# Saves keep their own config. To apply your edited config to a saved world:
cargo run --release -- --load ~/.local/share/microcosm/saves/autosave.bin --config configs/default.toml

# Stream mode: UI hidden, auto camera
cargo run --release -- --stream
```

### Controls

| Input | Action |
|-------|--------|
| Drag / WASD / arrows | Pan |
| Scroll | Zoom toward the cursor |
| Home | Whole world |
| Click | Inspect and follow an organism (F toggles follow, Esc clears) |
| Space | Pause / resume |
| 1–7, +/- | Speed (½× … 64×) |
| Tab | Hide the UI (stream view) |
| C | Auto camera |
| L / G / E / T / H | Species list / charts (population, family tree) / event history / event ticker / help |
| N | Field notebook. In the inspector, "Record in Field Notebook" sketches the organism under its species; recording more of a species adds sketches side by side |
| M | Chemistry map: oxygen, nutrients, toxin |
| F5 | Quicksave (`quicksave.bin` in the saves folder; see Files and releases below). The world also autosaves every 10 minutes and on exit; the previous save is kept as `*.prev.bin` |
| Bolt button (toolbar) | Acts of God: move or kill organisms (one, or a dragged box), aim a plague, upwelling, meteor or rock wall, bring a heat wave, cold snap or storm over the whole sea, lower the sea, or pour nutrients and other substances where you click. Esc puts a tool down |
| Ctrl+1–8 | Pick up an Acts of God disaster to aim, as if clicking its tile: plague, upwelling, meteor, rock wall, heat wave, cold snap, storm, sea level |

### Files and releases

Saves (autosave, quicksave), maps made in the map editor and organisms made in the genome editor
(`organisms/<name>.toml`) live in your user data folder, not
the project: `~/.local/share/microcosm` on Linux, `%APPDATA%\Microcosm` on Windows,
`~/Library/Application Support/Microcosm` on macOS (`--save-dir` overrides the saves folder).
The default settings and built-in maps are built into the program. To build zips for players
(itch.io and so on), see [docs/RELEASING.md](docs/RELEASING.md).


### Art gallery

`cargo run --release -- --gallery` opens a paused world with rows of organisms, each row sweeping one gene from `00` to `FF`: the size of each of the fifteen body parts, fronds riding on swaying tentacles, elongation, segments, mouths, and the chemistry genes that show on the body. The last rows freeze the budding (small bud, fission), dying, swallowed and bitten animations at nine stages each. Use it to check how things look.

### Headless

Run the simulation without a window. This is useful for tuning, benchmarks, and fast-forwarding a world to load in the viewer later.

```bash
cargo run --release -p evo_core --bin evo_headless -- --hours 2 --report 300 --diag --save saves/two_hours.bin
cargo run --release -- --load saves/two_hours.bin
```

`--diag` adds energy-flow diagnostics and a mean-genome summary. `--metrics out.jsonl` writes the statistics time series.

## Configuration

All tunable numbers live in [`configs/default.toml`](configs/default.toml), with comments. Copy it, edit it, and pass it with `--config`. Every key is optional.

Maps can also be painted in the **Map editor** (from the main menu, or `--editor`): paint rock, open water or cave with a round or square brush of any size (caves are open water that's dark and keeps filling with detritus and nutrients: a home for anaerobic scavengers), place permanent vents and vent spawn points (temporary vents only open there), draw named start areas (offered on the New world screen as places for life to start), set the water level (rock above it stands out as land) and the map's size, then name and save it. Saved maps (`configs/maps/<name>.map`) appear in the New world map list.

Starting organisms can be built in the **Genome editor** (from the main menu, or `--genome-editor`; or "Edit in genome editor" next to the ancestor on the New world screen): every gene has a slider and a typed hex value with what it means, the eight part slots have pickers for the kind, symmetry, size, placement and angle, and the whole genome can be copied or pasted as text. The organism is drawn live by the game's own renderer, with the inspector's readout of its parts and abilities beside it. Undo/redo, a Mutate button, and New (start from a built-in preset) / Open (your saved organisms). Saved organisms appear under Preset on the New world screen, after the built-in ones.

The world's size and terrain live in map files in [`configs/maps/`](configs/maps/). The game ships with one map, **Deep Ocean** (`Deep Ocean.map`, painted in the editor). Maps you paint and save appear beside it. A config picks one with `[world] map = "Deep Ocean"`, and `--world-map NAME` overrides that. The map is the same for every world seed. A map can also be a `.toml` file naming a terrain generator and its settings (see `TerrainConfig`); without any map file the built-in generated ocean is used.

## Tests

```bash
cargo test --release -p evo_core
```

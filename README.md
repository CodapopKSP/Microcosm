# Evolution

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

## Requirements

- Rust (stable, 1.95+)
- Linux: `sudo apt install libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev pkg-config`

## Run

```bash
# Viewer (release mode matters: the sim is much slower unoptimized).
# Resumes saves/autosave.bin if it exists; otherwise starts a new world.
cargo run --release

# Start a fresh world (or: --config my.toml, --seed 7, --world-map small)
cargo run --release -- --new

# Load a specific save
cargo run --release -- --load saves/quicksave.bin

# Saves from before body parts (genome v2) can't be loaded; start a new world.
# Saves keep their own config. To apply your edited config to a saved world:
cargo run --release -- --load saves/autosave.bin --config configs/default.toml

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
| M | Chemistry map: oxygen, nutrients, toxin |
| F5 | Quicksave (`saves/quicksave.bin`). The world also autosaves every 10 minutes and on exit; the previous save is kept as `*.prev.bin` |
| Ctrl+1–4 | Trigger a plague, upwelling, meteor, or rock wall right now |

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

The world's size and terrain live in map files in [`configs/maps/`](configs/maps/). A config picks one with `[world] map = "ocean"`, and `--world-map NAME` overrides that. Each map has its own terrain seed, so the map is the same for every world seed; any `[terrain]` keys in a config override the map's. To make a new map, copy `ocean.toml`, change it and its `seed`, and select it by name.

## Tests

```bash
cargo test --release -p evo_core
```

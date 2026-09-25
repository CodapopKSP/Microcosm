# Evolution

A side-on ocean where life evolves on its own, built to be watched (for example, running on a stream for weeks). The world starts with a single kind of tiny floating green microbe. Swimming, senses, scavenging, predation, chemosynthesis at hydrothermal vents, armor and large bodies all have to evolve. Species get names, founders and family trees. They rise, spread, and go extinct.

Every organism has a **34-byte genome in ten lettered chromosomes**, one byte per gene, readable by a human:

```
A:000000 B:20A05020 C:55C000 D:FF000000 H:2030 M:000000 N:8080808080808080 R:FF00B0 S:0000 T:80A0
```

Click any organism to see what each gene does.

- [docs/VISION.md](docs/VISION.md): goals and principles
- [docs/DESIGN.md](docs/DESIGN.md): the rules (world, genome, ecology, behavior, species)
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): code layout, tick pipeline, performance

## Requirements

- Rust (stable, 1.95+)
- Linux: `sudo apt install libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev pkg-config`

## Run

```bash
# Viewer (release mode matters: the sim is much slower unoptimized).
# Resumes saves/autosave.bin if it exists; otherwise starts a new world.
cargo run --release

# Start a fresh world (or: --config my.toml, --seed 7)
cargo run --release -- --new

# Load a specific save
cargo run --release -- --load saves/quicksave.bin

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
| F5 | Quicksave (`saves/quicksave.bin`). The world also autosaves every 10 minutes and on exit; the previous save is kept as `*.prev.bin` |
| Ctrl+1–4 | Trigger a plague, upwelling, meteor, or rock wall right now |

### Headless

Run the simulation without a window. This is useful for tuning, benchmarks, and fast-forwarding a world to load in the viewer later.

```bash
cargo run --release -p evo_core --bin evo_headless -- --hours 2 --report 300 --diag --save saves/two_hours.bin
cargo run --release -- --load saves/two_hours.bin
```

`--diag` adds energy-flow diagnostics and a mean-genome summary. `--metrics out.jsonl` writes the statistics time series.

## Configuration

All tunable numbers live in [`configs/default.toml`](configs/default.toml), with comments. Copy it, edit it, and pass it with `--config`. Every key is optional.

## Tests

```bash
cargo test --release -p evo_core
```

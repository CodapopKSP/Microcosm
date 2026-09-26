# Architecture

Two crates in one Cargo workspace:

| Crate | What it is | Depends on |
|-------|-----------|------------|
| `crates/evo_core` | The simulation: world, genome, ecology, species, stats, save/load. Also the `evo_headless` binary. | glam, rayon, serde, postcard, toml |
| `crates/evo_app` | The viewer (`evo_app` binary): rendering, camera, UI. Reads the core's state; never changes the rules. | evo_core, Bevy 0.19, bevy_egui |

The core has **no rendering dependencies**. Headless runs (tuning, benchmarks, long runs) build in seconds and run as fast as the CPU allows.

## Core layout (`crates/evo_core/src`)

| Module | Responsibility |
|--------|----------------|
| `config.rs` | Every tunable number, with defaults that must match `configs/default.toml` (a test enforces this) |
| `genome.rs` | 34-byte genome, chromosome table, parsing/formatting (`A:000000 B:20A05020 …`), mutation, genetic distance |
| `traits.rs` | Decodes a genome into `Traits` (phenotype) once at birth; plain-English gene descriptions for the inspector |
| `organism.rs` | `Organism` (position, mass, energy, HP, traits, RNG, …) and `Carcass` |
| `world.rs` | `World` and the tick pipeline (below); feeding, reproduction, death, milestones; save/load |
| `environment.rs` | Clock (day, season, climate), light (column propagation with shading), temperature, minerals, detritus, vents |
| `currents.rs` | Curl-noise ocean currents |
| `events.rs` | Random world events (plague, upwelling, meteor impact + impact winter, rising rock walls) as Poisson processes |
| `terrain.rs` | Procedural seafloor and rocks → solid mask + signed distance field (collisions, light blocking) |
| `field.rs` | Generic scalar grid with diffusion, vertical advection, decay |
| `grid.rs` | Multi-level spatial hash for neighbor queries across a wide range of body sizes |
| `species.rs` | Species and genera: founding by genetic distance, names, populations, extinctions |
| `stats.rs` | Time series samples, event log, milestones |
| `rng.rs` | Small deterministic PRNG (wyrand) |

## The tick

`World::step()` advances one tick (1/30 sim-second by default):

1. **Environment and events**: advance the clock; roll for new events and expire old ones; every 3 ticks update minerals, detritus, vents and climate; temperature every second.
2. **Grids and light**: one parallel pass over organisms builds a compact entry array. From that: rebuild the spatial grids (counting sort), deposit bodies into the light field, then propagate light column by column (in parallel).
3. **Think** (parallel, read-only): each organism finds contacts (collision push, prey, carcass) and, if motile, senses neighbors and field gradients and combines its drives into a heading and throttle. Output: one `Intent` per organism.
4. **Act** (parallel; each organism mutates only itself): turning, movement, currents, buoyancy, terrain collision, metabolism, photosynthesis, temperature, healing, growth, aging. Then events apply (plague deaths, impacts, barrier terrain rebuilds).
5. **Feed** (sequential, because it touches shared state): minerals and detritus uptake from fields, carcass eating, hunting.
6. **Life**: carcasses sink and decay; reproduction (with mutation and species classification); deaths become detritus or carcasses; seed bank; events; stats.

**Determinism.** Parallel phases write only per-organism outputs, and every organism has its own RNG. The same seed and config reproduce the same run exactly, which a test checks.

## Performance

The main techniques:

- **Multi-level spatial grid.** Cell sizes 16/64/256/1024. Each body lives in the finest level whose cells fit it, so microbes and giants are both cheap to query. Large organisms don't perceive individual microbes (queries skip levels whose bodies are too small).
- **Compact staging.** Organisms are big structs. Hot passes read a 28-byte entry array instead.
- **No per-tick allocation** in the hot paths. Scratch buffers are reused.
- **rayon** for think/act and light propagation.
- **Counting sort** grid rebuild. `swap_remove` for deaths.

`World::timings` accumulates wall time per phase. `evo_headless` prints it at the end of a run.

## Viewer layout (`crates/evo_app/src`)

| Module | Responsibility |
|--------|----------------|
| `main.rs` | CLI, Bevy app setup, `--screenshot` for automated captures |
| `sim.rs` | Runs ticks inside the frame loop at the chosen speed, within a 13 ms budget; autosave |
| `camera.rs` | Pan/zoom/follow, click-to-select, the auto-camera "director" for streaming |
| `render/` | One dynamic mesh with a quad per visible organism, shaded procedurally in `organism.wgsl`. A world-sized quad with `background.wgsl` draws water, light, shadows, rock, vents and marine snow from field textures. HDR + bloom. |
| (effects) | `World::fx` holds short-lived visual events from the last few seconds (gulps, bites, carcass bites, impact shockwaves); the renderer draws them as extra quads. Organisms also carry `ate_tick`/`hurt_tick` for feeding and hurt flashes. |
| `ui/` | egui: top bar, organism inspector (chromosomes), species list, charts, event ticker, stream overlay |

The viewer interpolates organism positions between ticks, so motion stays smooth at any speed.

## Saves

`World::save`/`load` write lz4-compressed MessagePack **with field names**, so saves survive code changes: removed fields are ignored, and new fields marked `#[serde(default)]` take their defaults. Decoded traits aren't saved; they're re-decoded from genomes on load, so changes to gene formulas apply to existing worlds. The viewer autosaves to `saves/autosave.bin` every 10 minutes by default (`--autosave-minutes`) and quicksaves with F5. Resume with `--load`.

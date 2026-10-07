# Architecture

Two crates in one Cargo workspace:

| Crate | What it is | Depends on |
|-------|-----------|------------|
| `crates/evo_core` | The simulation: world, genome, ecology, species, stats, save/load. Also the `evo_headless` binary. | glam, rayon, serde, rmp-serde, lz4_flex, toml |
| `crates/evo_app` | The viewer (`evo_app` binary): rendering, camera, UI. Reads the core's state; never changes the rules. | evo_core, Bevy 0.19, bevy_egui |

The core has **no rendering dependencies**. Headless runs (tuning, benchmarks, long runs) build in seconds and run as fast as the CPU allows.

## Core layout (`crates/evo_core/src`)

| Module | Responsibility |
|--------|----------------|
| `config.rs` | Every tunable number, with defaults that must match `configs/default.toml` (a test enforces this) |
| `genome.rs` | 60-byte genome, chromosome table, parsing/formatting (`B:20A050200000 C:55C000 …`), mutation (including part duplication, deletion and type shifts), genetic distance |
| `parts.rs` | The body-part catalog, decoding the P chromosome, and laying parts out on the body (shared by the simulation and the renderer) |
| `traits.rs` | Decodes a genome into `Traits` (phenotype) once at birth, summing the parts into plain numbers (thrust, grip, eye cones, …); plain-English gene descriptions for the inspector |
| `organism.rs` | `Organism` (position, mass, energy, HP, traits, RNG, …) and `Carcass` |
| `world.rs` | `World` and the tick pipeline (below); feeding, reproduction, death; save/load |
| `environment.rs` | Clock (day, season, climate), light (column propagation with shading), temperature, minerals, detritus, vents; water chemistry (oxygen, CO₂, nutrients, toxin), the air, lure glow |
| `currents.rs` | Curl-noise ocean currents |
| `events.rs` | Random world events (plague, upwelling, meteor impact + impact winter, rising rock walls) as Poisson processes |
| `terrain.rs` | Procedural seafloor and rocks → solid mask + signed distance field (collisions, light blocking) |
| `field.rs` | Generic scalar grid with diffusion, vertical advection, decay |
| `grid.rs` | Multi-level spatial hash for neighbor queries across a wide range of body sizes |
| `species.rs` | Species and genera: founding by genetic distance, names, populations, extinctions |
| `stats.rs` | Time series samples, event log |
| `map.rs` | Painted maps from the map editor (`configs/maps/<name>.map`): a rock mask, cave mask (dark water that sheds detritus), water level, permanent vents and vent spawn points |
| `notebook.rs` | The field notebook: species the viewer recorded and their specimens' genomes (saved with the world, no effect on the simulation) |
| `rng.rs` | Small deterministic PRNG (wyrand) |

## The tick

`World::step()` advances one tick (1/30 sim-second by default):

1. **Environment and events**: advance the clock; roll for new events and expire old ones; every 3 ticks update minerals, detritus (and its rotting), vents and climate; dissolved gases, nutrients, toxin and the air five times a second; temperature every second.
2. **Grids and light**: one parallel pass over organisms builds a compact entry array. From that: rebuild the spatial grids (counting sort), deposit bodies (with their fronds) into the light field, then propagate light column by column (in parallel). Lures are splatted into a glow field.
3. **Think** (parallel, read-only): each organism finds contacts within its tentacle reach (collision push, prey, carcass) and, if motile, senses neighbors through its eye cones and antennae, reads field gradients, and combines its drives into a heading and throttle. Output: one `Intent` per organism.
4. **Act** (parallel; each organism mutates only itself): touching rock, anchoring, turning, swimming or walking, currents, buoyancy, terrain collision, metabolism with breathing or fermentation, photosynthesis (limited by CO₂ or minerals), nitrogen fixation, toxins, temperature, healing, nutrient-limited growth, aging. What it exchanged with the water is recorded in `Organism::chem`, not applied yet. Then events apply (plague deaths, impacts, barrier terrain rebuilds).
5. **Feed** (sequential, because it touches shared state): apply each organism's `chem` to the fields; minerals, nutrient and detritus uptake; carcass eating; hunting (with poison and nutrients passing to the hunter).
6. **Life**: carcasses sink and decay (releasing nutrients); reproduction (with mutation, nutrients, and species classification); deaths become detritus or carcasses; events; stats. If everything dies, the world goes on empty (no reseeding).

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
| `render/` | One dynamic mesh with a quad per visible body and one oriented quad per body part, shaded procedurally in `organism.wgsl` (each part kind has its own art that grows more elaborate with its size). Parts are laid out once per genome and cached, and only drawn once a body is a few pixels across. A world-sized quad with `background.wgsl` draws water, light, shadows, rock, vents, marine snow, dead zones, red tides and oxygen bubbles from field textures, plus the chemistry map (M). HDR + bloom. |
| (effects) | `World::fx` holds short-lived visual events from the last few seconds (gulps, bites, carcass bites, impact shockwaves); the renderer draws them as extra quads. Organisms also carry `ate_tick`/`hurt_tick` for feeding and hurt flashes. |
| `ui/` | egui: HUD (clock and speed, weather, life, round panel buttons), organism inspector (a card beside the toolbar with Overview / Body / Chemistry / Genome tabs), species list, charts, event ticker, field notebook (`notebook.rs`, with pencil sketches of organisms drawn part by part in `sketch.rs`, handwritten in the Kalam font from `assets/fonts`), stream overlay |

The viewer interpolates organism positions between ticks, so motion stays smooth at any speed.

## Saves

`World::save`/`load` write lz4-compressed MessagePack **with field names**, so saves survive code changes: removed fields are ignored, and new fields marked `#[serde(default)]` take their defaults. The file starts with a magic marker (`EVOSAVE2`); it was bumped when the genome grew to 60 bytes, since older saves can't be converted. Decoded traits aren't saved; they're re-decoded from genomes on load, so changes to gene formulas apply to existing worlds. The viewer autosaves to `saves/autosave.bin` every 10 minutes by default (`--autosave-minutes`) and quicksaves with F5. Resume with `--load`.

# Evolution Sim

Evolution Sim is a 2D evolution sandbox built on [Bevy](https://bevyengine.org/). Entities have DNA, metabolize energy, move, reproduce with mutation, and are shaped by an environment with light, hazards, and (later) vents and resource fields.

The current codebase implements **Phase 1** (genetics, metabolism, risky asexual reproduction), **Phase 2** (position-dependent photosynthesis, light field, spatial niches via `gradient_y`), and **Phase 3** (headless CLI, JSONL metrics, evolvable lifespan with `age_upkeep`). Phase 0 collision-despawn is removed; agents die from energy depletion, core HP, reproduction risk, or old age.

## Requirements

- Rust toolchain ([rustup](https://rustup.rs/))
- Linux (Pop/Ubuntu): `sudo apt install libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev pkg-config`

This PoC uses a slim Bevy feature set (no audio) so it builds without ALSA if you skip that package.

## Run

### Visual sim

```bash
cargo run
# or explicitly:
cargo run -- --config configs/default.toml
```

Loads `configs/default.toml` from the project directory unless `--config` points elsewhere.

The first compile can take several minutes while Bevy builds. The window size, population count, and speeds come from the config file. Startup prints whether `master_seed` is set (see `[run]` in the TOML).

**Debug overlays (visual mode):**

- **L** — toggle light-field background gradient
- **Click** an agent — light ring + energy bar vs `repro_threshold`
- **Esc** — clear selection

### Config

- Default: [`configs/default.toml`](configs/default.toml)
- Invalid TOML or unsupported `genetics.genome_version` exits with a readable error before the window opens.
- Comment out `master_seed` under `[run]` for nondeterministic RNG.

#### Energy model (Phase 2)

Default uses **Model A** local irradiance:

```toml
[environment]
energy_model = "local"
solar_constant = 2.8
light_mode = "gradient_y"
gradient_min = 0.15
gradient_max = 1.0
gradient_bright_at = "top"
crowding_blocks_photosynthesis = false
```

Per-tick photosynthetic gain: `photo_strength * light(pos) * solar_constant`. World influx does **not** scale with population count.

**Migrating from Phase 1 flat grant:**

| Phase 1 | Phase 2 default |
|---------|-----------------|
| `flat_energy_grant_per_tick = 0.8` (testing) | `flat_energy_grant_per_tick = 0.0` (ignored under `local`) |
| No `[environment]` | `solar_constant` + `light_mode` tune bright/shade viability |
| — | `energy_model = "legacy_flat"` restores flat grant for old regression configs |

Tune `solar_constant`, `gradient_min` / `gradient_max`, and `k_photo` together so ancestors thrive in mid–bright light and high-photo mutants pay for shade (see [PHASE_2.md](docs/PHASE_2.md)).

### Headless batch run (Phase 3)

```bash
# No window; runs until [run].max_ticks, writes JSONL to [run].metrics_path
cargo run -- --headless --config configs/default.toml

# Override master_seed after TOML load (determinism / A-B tests)
cargo run -- --headless --config configs/default.toml --seed 99
```

Requires `max_ticks > 0` and a set `master_seed` for reproducible metrics. See [SIMULATION.md](docs/SIMULATION.md) for the tick pipeline and metrics schema.

## Documentation

Design and planning docs live in `docs/`:

- [docs/VISION.md](docs/VISION.md) — Goals, non-goals, design principles, glossary
- [docs/SIMULATION.md](docs/SIMULATION.md) — Time model, randomness, config, headless vs visual
- [docs/GENOME.md](docs/GENOME.md) — Genome encoding, loci, mutations, morphology
- [docs/ENERGY_AND_ENVIRONMENT.md](docs/ENERGY_AND_ENVIRONMENT.md) — Energy models, light, vents, hazards, soup
- [docs/ENTITIES.md](docs/ENTITIES.md) — Components and lifecycle
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — Crate layout and data flow
- [docs/ROADMAP.md](docs/ROADMAP.md) — Phased implementation plan
- [docs/PHASE_1.md](docs/PHASE_1.md) — Phase 1 checklist and acceptance criteria
- [docs/PHASE_2.md](docs/PHASE_2.md) — Phase 2 light field and Model A gain

### Tests

```bash
cargo test
```

Phase acceptance highlights:

```bash
cargo test acceptance::
cargo test sim::light::
cargo test metabolism::
```

These documents describe where the project is going; later phases may get ahead of the current implementation.

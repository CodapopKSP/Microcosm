# Evolution Sim

Evolution Sim is a 2D evolution sandbox built on [Bevy](https://bevyengine.org/). Entities have DNA, metabolize energy, move, reproduce with mutation, and are shaped by an environment with light, hazards, and (later) vents and resource fields.

The current codebase implements **Phase 1**: heritable genomes, energy metabolism, risky asexual reproduction, and config-driven tuning. Phase 0 collision-despawn is removed; agents die from energy depletion, core HP, or reproduction risk only.

## Requirements

- Rust toolchain ([rustup](https://rustup.rs/))
- Linux (Pop/Ubuntu): `sudo apt install libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev pkg-config`

This PoC uses a slim Bevy feature set (no audio) so it builds without ALSA if you skip that package.

## Run

### Visual sim

```bash
cargo run
```

Loads `configs/default.toml` from the project directory (override with an optional path: `cargo run -- configs/default.toml`).

The first compile can take several minutes while Bevy builds. The window size, population count, and speeds come from the config file. Startup prints whether `master_seed` is set (see `[run]` in the TOML).

### Config

- Default: [`configs/default.toml`](configs/default.toml)
- Invalid TOML or unsupported `genetics.genome_version` exits with a readable error before the window opens.
- Comment out `master_seed` under `[run]` for nondeterministic RNG (used in later phases).

### Future targets (planned)

Once the simulation core and CLI from the docs are implemented, the intention is:

```bash
# Visual sim with chosen config
cargo run -- --config configs/default.toml

# Headless batch run (no window), metrics to file
cargo run -- --headless --config configs/default.toml
```

See [SIMULATION.md](docs/SIMULATION.md) for the planned CLI and tick pipeline.

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

Run acceptance tests: `cargo test acceptance population_survives`.

These documents describe where the project is going; later phases may get ahead of the current implementation.

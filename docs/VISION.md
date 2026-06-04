# Vision

This document defines what the Evolution Simulator is trying to achieve, what it deliberately excludes, and the design principles that guide implementation.

## Goals

1. **Approximate natural selection** — Heritable variation, differential survival and reproduction, and inheritance with mutation produce populations that adapt to their environment over time.

2. **Parameter-driven runs** — World size, energy rates, mutation probabilities, hazard placement, and similar knobs live in configuration files (`configs/*.toml`). Runs are driven by parameters, not hard-coded constants scattered through the codebase.

3. **Seeded randomness (optional, recommended)** — Mutations and stochastic events remain random, but an optional `master_seed` makes the *sequence* of random draws replayable. This supports debugging (“why did this lineage die?”) and fair A/B comparison of two configs. It is **not** required for the simulation to function.

4. **Diversity by design** — Traits use tradeoffs, ongoing costs, thresholds, and risky reproduction so lineages spread across niches instead of converging on a single “max efficiency” genotype. **Canonical trait model:** [Genome_Definition.md](Genome_Definition.md) (v2 target). **Current code:** [GENOME.md](GENOME.md) (v1).

5. **Dual runtime** — `SimCore` runs headless (batch ticks, metrics export). Bevy provides an optional visual observer that reads simulation snapshots and does not own sim state. See [SIMULATION.md](SIMULATION.md) and [ARCHITECTURE.md](ARCHITECTURE.md).

## Non-goals (v1)

| Excluded | Rationale |
|----------|-----------|
| Full biochemistry or realistic cell biology | Focus on selection dynamics, not molecular accuracy |
| 3D morphology physics | Stay 2D with simplified geometry and collision |
| Multi-species taxonomy UI | Species, if tracked, emerge implicitly from lineage data |
| Player-directed evolution | No manual gene editing during runs (config only) |
| Networked multiplayer | Single-process sim |

## Design principles

### Genotype ≠ phenotype

DNA is an opaque byte buffer (see [GENOME.md](GENOME.md)). Decoding applies documented formulas, clamps, and coupled costs to produce a cached `Phenotype`. Corrupt or unknown genome versions fail gracefully (stillborn spawn or safe ancestor defaults)—never silent “max all stats.”

### No thermodynamic cheats

Whichever energy model is configured ([ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md)), total energy entering the world must not scale linearly with population unless that is an explicit, documented design choice. Per-entity irradiance is valid for early phases; tradeoffs (photo upkeep, metabolism) must still cap runaway growth.

### Trait tradeoffs over raw efficiency

Genes that help in one context impose costs or risks elsewhere. Examples: higher `photo_strength` increases uptake but also `photo_upkeep`; `skin_hardness` reduces damage but adds armor upkeep and move penalty. Reproduction always carries configurable risk when energy is low.

### Complexity is phased

Appendage trees, soup compounds, vents, and sensory spectra are **specified early** in planning docs but **implemented** after a working photosynthetic lineage with core energy, health, and reproduction. See [ROADMAP.md](ROADMAP.md).

### Performance failsafe ≠ ecology

`max_entities` is a CPU safety valve only. Carrying capacity and niche structure emerge from energy, metabolism, environment heterogeneity, and trait costs—not from an artificial population cap.

## Relationship to current codebase

The repo currently contains **Phase 0**: a Bevy 2D proof of concept ([`src/main.rs`](../src/main.rs)) with colored circles, wall bounce, and despawn-on-collision. That collision behavior is a placeholder, not ecological death. Future phases replace it with energy- and health-based lifecycle systems described in [ENTITIES.md](ENTITIES.md).

## Glossary

| Term | Definition |
|------|------------|
| **Locus** | A fixed byte range in the core genome that decodes to one or more phenotype values |
| **Allele** | The raw byte value(s) at a locus for a given individual |
| **Genotype** | Full genome (core segment ± morphology extension) |
| **Phenotype** | Decoded traits used by simulation systems (radius, costs, thresholds, etc.) |
| **Internal energy** | Per-entity energy store spent on metabolism, movement, repair, and reproduction |
| **Photo upkeep** | Ongoing energy cost proportional to photosynthetic investment |
| **Light exposure** | Scalar from the light field at an entity’s position (0–1 or unbounded per config) |
| **Vent flux** | Local energy or compound availability near a thermal vent zone |
| **Soup compound** | Environmental resource (e.g. amino acids, sugars) requiring evolved uptake |
| **Threshold trait** | Gene whose benefit activates only after crossing a viability cliff (e.g. chemosynthesis near vents) |
| **Repro risk** | Stochastic failure during reproduction when energy margin is low |
| **Sim tick** | Fixed simulation step `Δt`; all rates convert to per-tick or per-second explicitly |
| **SimCore** | Headless simulation loop and state, independent of Bevy rendering |
| **master_seed** | Optional RNG seed for reproducible stochastic sequences |

## Further reading

- [SIMULATION.md](SIMULATION.md) — Time model, config, headless vs visual, system order
- [GENOME.md](GENOME.md) — Encoding, loci, mutations, morphology
- [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md) — Energy models, light, hazards, soup, seasons
- [ENTITIES.md](ENTITIES.md) — Components and lifecycle
- [ARCHITECTURE.md](ARCHITECTURE.md) — Code layout and data flow
- [ROADMAP.md](ROADMAP.md) — Phased implementation plan

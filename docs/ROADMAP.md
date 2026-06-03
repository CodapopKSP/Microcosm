# Roadmap

Phased implementation plan from the current proof of concept through sexual reproduction. Each phase lists scope, acceptance criteria, and explicit **out of scope** items to prevent creep.

**Current state:** Phase 1 complete ([`src/main.rs`](../src/main.rs), [PHASE_1.md](PHASE_1.md)).

## Overview

| Phase | Focus | Status |
|-------|--------|--------|
| 0 | Movement PoC | Done |
| 1 | Genome, energy, health, risky repro | Done |
| 2 | Light + photosynthesis (local energy) | Planned |
| 3 | Headless, metrics, evolvable lifespan | Planned |
| 4 | Hazards, per-part damage, skin | Planned |
| 5 | Sensors, vents, chemo threshold | Planned |
| 6 | Soup, spatial energy | Planned |
| 7 | Appendage tree + rendering | Planned |
| 8 | Sexual reproduction | Optional |

---

## Phase 0 — Proof of concept (done)

**Scope**

- Bevy 2D window; spawn N circles with random color, position, velocity.
- Bounce on world bounds.
- Despawn both entities on circle overlap.

**Acceptance**

- `cargo run` shows moving colored agents.
- Build succeeds on Linux with documented dependencies.

**Out of scope**

- DNA, energy, evolution, config files.

---

## Phase 1 — Core genetics and lifecycle

**Detailed plan:** [PHASE_1.md](PHASE_1.md) (checklist, sub-phases 1A–1L, acceptance tests).

**Scope**

- `SimConfig` TOML load; optional `master_seed`.
- `Genome` v1 core segment; decode to `Phenotype` with tradeoff formulas.
- `internal_energy`, `core_hp`; metabolism and trait upkeeps (no environmental gain yet—spawn energy or flat grant for testing).
- Asexual reproduction with `repro_cost`, `offspring_endowment`, cooldown, **repro risk**.
- Color and `body_radius` from genome.
- Remove collision-despawn; death from energy / core HP / repro risk only.
- `max_entities` failsafe stub.

**Acceptance**

- Headless or visual run: population survives multiple thousand ticks without instant extinction.
- Smoke test: two configs with different `p_death_on_repro_base` produce different population trajectories (with same seed).
- No single genotype obviously sweeps in a 10-run manual check (diversity proxy: color variance).

**Out of scope**

- Photosynthesis, light map, morphology, vents, soup, metrics file, sexual repro.

---

## Phase 2 — Light and photosynthesis

**Detailed plan:** [PHASE_2.md](PHASE_2.md) (checklist, sub-phases 2A–2L, crowding/overlap decision).

**Scope**

- `energy_model = "local"`.
- Light field: `uniform` and `gradient_y`.
- Photosynthetic gain from `photo_strength * light(pos) * solar_constant`.
- `photo_upkeep` and metabolism applied each tick.
- Locomotion (`max_speed` / `move_cost`) — **implemented in Phase 1**; Phase 2 verifies only.

**Acceptance**

- High-photo strains grow in bright regions but lose net energy in shade when upkeep exceeds gain.
- Low-photo, low-metabolism strains persist in shade (niche viability).
- Population does not grow unbounded without failsafe under default config tuning.

**Out of scope**

- Global pool (B), spatial shading (C), vents, sensors, part HP.

---

## Phase 3 — Headless CLI, metrics, and evolvable lifespan

**Detailed plan:** [PHASE_3.md](PHASE_3.md) (checklist, sub-phases 3A–3F, lifespan design).

**Scope**

- `clap` CLI: `--headless`, `--config`, `--seed`.
- `configs/default.toml` checked into repo.
- JSONL metrics per [ARCHITECTURE.md](ARCHITECTURE.md).
- Extract `lib.rs` + `sim` / `genome` modules; slim `main.rs`.
- **Lifespan / old age:** `max_age` core locus (bytes 26–27), `Age` component, per-tick `age_upkeep` tradeoff, death when `age_ticks >= phenotype.max_age_ticks` ([PHASE_3.md — 3D](PHASE_3.md#sub-phase-3d--lifespan-evolvable-old-age)). Offspring inherit and mutate lifespan like other traits—selection can push lineages toward longer or shorter life.

**Acceptance**

- `cargo run -- --headless --config configs/default.toml` exits after `max_ticks` and writes metrics file.
- `cargo run -- --config configs/default.toml` still renders.
- Same seed + config ⇒ identical metrics file hash (within float tolerance).
- Lifespan: mutation changes `max_age_ticks`; low-cap test agent dies with `AgeExceeded`; high-lifespan genotype pays higher `age_upkeep` than short-life clone at equal other loci.
- Metrics include lifespan aggregates and `age_exceeded` death counts.

**Out of scope**

- Hazards, morphology, pool energy model.
- Config-only max age without a heritable locus (unless added later as a hard ceiling).

---

## Phase 4 — Hazards, armor, per-part health

**Scope**

- Hazard zones from config; damage per tick.
- `skin_hardness` with armor upkeep and move penalty.
- `part_hp` when morphology absent: core only; prepare vector for Phase 7.
- Collision damage (replaces despawn); relative speed formula.
- Repair policies consuming energy.

**Acceptance**

- High-hardness entities survive hazard longer at equal energy but grow slower in safe zones (upkeep + move penalty).
- Repair drains energy measurably in metrics.

**Out of scope**

- Vents, soup, appendage rendering, sexual repro.

---

## Phase 5 — Sensors and thermal vents

**Scope**

- Vent zones; `chemo_potential` threshold + `chemo_upkeep`.
- Morphology optional: if no tree yet, use core-only `chemo_sensitivity` placeholder locus in core reserved bytes or config default.
- Sensory spectrum on prototype nodes OR tier fallback documented in [GENOME.md](GENOME.md).
- `sensor_upkeep` from `visual_bandwidth` / `sensory_gain`.
- Behavior stub: higher bandwidth improves gradient following (simple bias toward light).

**Acceptance**

- Vent-adjacent niche: lineages with invested `chemo_potential` sustain at vents; uninvested genotypes do not.
- High sensor investment reduces net energy in nutrient-poor areas.

**Out of scope**

- Full appendage tree rendering, soup fields, pool energy.

---

## Phase 6 — Soup and spatial energy

**Scope**

- Soup compound grid (1–2 compounds); transporter efficiencies (new loci or reserved bytes).
- `energy_model = "spatial"` (occlusion) on top of local or pool.
- Optional `energy_model = "pool"` if local + spatial still allows runaway.

**Acceptance**

- At least two distinct genotype clusters in metrics histograms across map regions in a 50k-tick headless run.
- Total world energy metric (if pool enabled) stays bounded.

**Out of scope**

- Full procedural appendages, sexual repro.

---

## Phase 7 — Appendage tree and morphology rendering

**Scope**

- Morphology extension encoding; mutations on tree.
- Per-node upkeep, drag, `part_hp`.
- Procedural polyline or multi-circle render; collision hull from phenotype.
- Locomotion/gather weights affect `move_cost` and reach.

**Acceptance**

- Visible structural diversity; morphology changes stats (not cosmetic only).
- Damaged appendage reduces locomotion or sensory function per [ENTITIES.md](ENTITIES.md).

**Out of scope**

- Sexual reproduction, 3D, predation.

---

## Phase 8 — Sexual reproduction (optional)

**Scope**

- Two-parent crossover on core; morphology subtree swap.
- Configurable mating radius and energy cost.
- Optional inbreeding penalty.

**Acceptance**

- Higher phenotypic variance rate than asexual-only at same mutation probabilities (metrics over 20 seeds).

**Out of scope**

- Multi-objective fitness objectives, external ML policies.

---

## Documentation maintenance

When implementation diverges from docs, update the relevant `docs/*.md` in the same PR as the code change. Genome version bumps require a new `GenomeVersion` section in [GENOME.md](GENOME.md).

## Further reading

- [PHASE_1.md](PHASE_1.md) — Phase 1 implementation checklist and sub-phases
- [PHASE_2.md](PHASE_2.md) — Phase 2 light field, Model A gain, overlap/crowding decision
- [PHASE_3.md](PHASE_3.md) — Phase 3 headless CLI, metrics, evolvable lifespan / old-age death
- [VISION.md](VISION.md) — Project goals
- [ARCHITECTURE.md](ARCHITECTURE.md) — Module targets per phase

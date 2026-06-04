# Roadmap

Phased implementation plan from the current proof of concept through sexual reproduction. Each phase lists scope, acceptance criteria, and explicit **out of scope** items to prevent creep.

**Current state:** Phase 4 complete; **G8 cutover** — default config is EVO2 ([`configs/default.toml`](../configs/default.toml)). v1 regression: [`configs/v1_regression.toml`](../configs/v1_regression.toml). Track: [GENOME_V2_ARCHITECTURE_ROADMAP.md](GENOME_V2_ARCHITECTURE_ROADMAP.md).

**Canonical redesign:** [Genome_Definition.md](Genome_Definition.md) supersedes v1 genetics. Phase 5 v1 sensor-byte plans are deferred; v2 uses sensors bytes 20–21 in the fixed core.

## Overview

| Phase | Focus | Status |
|-------|--------|--------|
| 0 | Movement PoC | Done |
| 1 | Genome, energy, health, risky repro | Done |
| 2 | Light + photosynthesis (local energy) | Done |
| 3 | Headless, metrics, evolvable lifespan | Done |
| 4 | Hazards, damage, skin, green photosynthesis | Done |
| 5 | Sensors, vents, chemo threshold | Planned |
| 6 | Soup, spatial energy | Planned |
| 7 | Appendage tree + rendering | Planned |
| 8 | Sexual reproduction | Optional |
| 9 | Seasons and climate cycles | Planned |

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

## Phase 2 — Light and photosynthesis (done)

**Detailed plan:** [PHASE_2.md](PHASE_2.md) (checklist, sub-phases 2A–2L, crowding/overlap decision).

**Scope**

- `energy_model = "local"` (default); `legacy_flat` for Phase 1 regression tests.
- Light field: `uniform` and `gradient_y` via `LightField` + `sense_exposure`.
- Photosynthetic gain: `photo_strength * light(pos) * solar_constant`.
- `photo_upkeep` and metabolism each tick; optional `crowding_blocks_photosynthesis` (default off).
- Locomotion (`max_speed` / `move_cost`) — implemented in Phase 1; verified in Phase 2G.
- Visual debug: light gradient overlay, selected-agent energy bar.

**Acceptance** (see `cargo test` — `sim::acceptance::`, `sim::light::`, `sim::systems::metabolism::`)

- High-photo strains gain net energy in bright bands; lose net energy in shade when upkeep exceeds gain.
- Low-photo, low-metabolism strains outlast high-photo strains in shade.
- Default `gradient_y` config: population survives 5k–10k ticks headless without unbounded growth past failsafe slack.

**Out of scope**

- Global pool (B), spatial shading (C), `light_mode = "map"`, vents, sensors, part HP.

---

## Phase 3 — Headless CLI, metrics, and evolvable lifespan (done)

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
- Color-mediated photosynthesis tradeoff (done in Phase 4).

---

## Phase 4 — Hazards, armor, per-part health, green photosynthesis (done)

**Detailed plan:** [PHASE_4.md](PHASE_4.md) (checklist, sub-phases 4A–4G, hazard/collision/repair and green-niche design).

**Scope**

- Hazard zones from config; damage per tick.
- `skin_hardness` with armor upkeep and move penalty.
- `part_hp` when morphology absent: core only; prepare vector for Phase 7.
- Collision damage (replaces despawn); relative speed formula.
- Repair policies consuming energy.
- **Green photosynthesis niche:** greenness from `color_r` / `color_g` / `color_b`; photosynthesis buff on local gain; smaller metabolism penalty on the same axis; config tuning (`[genetics.color_photosynthesis]` or decode ranges); color loci become **Tradeoff** in [GENOME.md](GENOME.md).

**Acceptance** (see `cargo test` — `sim::acceptance::`, `genome::phenotype::`, `sim::systems::interactions::`, `sim::systems::healing::`, `sim::metrics::`)

- High-hardness entities survive hazard longer at equal energy but grow slower in safe zones (upkeep + move penalty).
- Repair drains energy measurably (`repair_energy_spent` in JSONL when damage + repair run).
- Headless A/B: high-G / low-RB ancestor gains more net energy in bright `gradient_y` than an otherwise-identical low-G control; pays measurably higher metabolism per tick.
- Unit test: metabolism penalty from greenness is strictly less than photosynthesis buff at reference light ([PHASE_4.md §4.7](PHASE_4.md#47--green-photosynthesis-niche-canonical)).
- Metrics: survivor `color_bins` diverge when ancestor color differs across seeded 5k-tick runs.

**Out of scope**

- Vents, soup, appendage rendering, sexual repro.
- Full spectral light model, chlorophyll chemistry, or per-channel mutation operators beyond existing color loci.

---

## Phase 5 — Sensors and thermal vents

**Detailed plan:** [PHASE_5.md](PHASE_5.md) (checklist, sub-phases 5A–5F, vent/chemo/sensor/taxis design).

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

- Full appendage tree rendering, soup fields, pool energy, **seasons** (Phase 9).

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

- Full procedural appendages, sexual repro, **seasonal cycles** (Phase 9).

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

## Phase 9 — Seasons and climate cycles

**Detailed plan:** [PHASE_9.md](PHASE_9.md) (checklist, sub-phases 9A–9E, seasonal modulation design).

**Prerequisite:** Phases 2–6 (light, hazards, vents, soup) so seasons have environmental knobs to drive. Phase 3 lifespan and Phase 4 green photosynthesis provide selection pressure targets.

**Scope**

- Cyclic **season clock** from sim tick: `season_index`, `phase ∈ [0, 1)` within current season, optional year counter.
- **`environment_tick` implementation** — advance season state once per sim tick (currently no-op; pool recharge in Phase 6 shares this hook).
- **Modulated parameters** (configurable per season or interpolated): `solar_constant`, light field scale (`gradient_min` / `gradient_max`), hazard `damage_per_tick`, vent `vent_flux`, soup regen rates.
- **Waveforms:** piecewise-constant seasons (spring/summer/fall/winter table) or smooth sine/cosine blend between season keyframes.
- **Metrics:** `season_index`, `season_phase`, optional per-season population / energy aggregates in JSONL.
- **Visual (optional):** tint or overlay indicating current season; no gameplay in render layer.

**Design intent**

- No single genotype dominates all year — bright-season photo specialists, winter low-metabolism survivors, vent/soup opportunists during “lean” seasons.
- Lifespan and repro cooldown interact with cycle length (configurable `ticks_per_season` vs `max_age_ticks`).
- Same seed + config ⇒ identical season phase at each metrics sample (determinism preserved).

**Acceptance**

- Headless A/B: config with strong summer/winter solar swing → mean population energy correlates with season phase over 2+ full cycles.
- Low-photo, low-metabolism lineage outlasts high-photo lineage through a configured “winter” stretch at equal starting energy (harness test).
- Metrics lines include `season_index` / `season_phase`; values repeat with period `seasons.len() * ticks_per_season`.
- Default seasons **off** (`[[environment.seasons]]` empty or `seasons_enabled = false`) — Phase 2–8 behavior unchanged.

**Out of scope**

- Latitude, axial tilt physics, weather RNG (storms as separate feature).
- Heritable “hibernation” or diapause loci (defer unless reserved bytes available).
- Multi-decade climate *change* (non-cyclic drift) — future follow-up.
- Calendar UI, named holidays, player-triggered season skip.

---

## Documentation maintenance

When implementation diverges from docs, update the relevant `docs/*.md` in the same PR as the code change. Genome version bumps require a new `GenomeVersion` section in [GENOME.md](GENOME.md).

## Further reading

- [PHASE_1.md](PHASE_1.md) — Phase 1 implementation checklist and sub-phases
- [PHASE_2.md](PHASE_2.md) — Phase 2 light field, Model A gain (complete)
- [PHASE_3.md](PHASE_3.md) — Phase 3 headless CLI, metrics, evolvable lifespan / old-age death
- [PHASE_4.md](PHASE_4.md) — Phase 4 hazards, collision damage, armor, repair, green photosynthesis niche
- [PHASE_5.md](PHASE_5.md) — Phase 5 thermal vents, chemo threshold, core sensors, light taxis
- [PHASE_9.md](PHASE_9.md) — Phase 9 seasons, climate cycles, environment_tick modulation
- [VISION.md](VISION.md) — Project goals
- [ARCHITECTURE.md](ARCHITECTURE.md) — Module targets per phase

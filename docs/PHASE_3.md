# Phase 3 — Headless CLI, metrics, and lifespan

Detailed implementation plan for **Phase 3** from [ROADMAP.md](ROADMAP.md). Phases 1–2 deliver genetics, local photosynthesis, and lifecycle death from energy, core HP, and repro risk.

**Prerequisite reading:** [SIMULATION.md](SIMULATION.md), [GENOME.md](GENOME.md), [ENTITIES.md](ENTITIES.md), [ARCHITECTURE.md](ARCHITECTURE.md).

---

## Master checklist

- [x] **3A** — `clap` CLI: `--headless`, `--config`, `--seed` override
- [x] **3B** — Shared sim bootstrap; slim `main.rs` (visual vs headless entry)
- [x] **3C** — JSONL metrics per schema below (`snapshot_interval`, `metrics_path`)
- [x] **3D** — **Evolvable lifespan:** `max_age` locus, `Age` component, senescence upkeep, old-age death
- [x] **3E** — Unit + headless acceptance (determinism, lifespan tradeoff, metrics fields)
- [x] **3F** — Default `configs/default.toml` tuning; update [ROADMAP.md](ROADMAP.md) Phase 3 → **Done**

---

## Goals and non-goals

### Goals

1. **Headless reproducibility** — Same seed + config ⇒ identical metrics (see [Determinism policy](#determinism-policy-3e)).
2. **Observability** — Periodic population / energy / trait histograms for tuning and regression.
3. **Evolvable lifespan** — Heritable `max_age_ticks`; selection can favor longer or shorter life via explicit tradeoffs (not a free longevity buff).
4. **Old-age death** — Despawn when `age_ticks >= phenotype.max_age_ticks`; reason `DeathReason::AgeExceeded` → JSON field `deaths_age_exceeded`.

### Explicitly out of scope (Phase 3)

| Item | Lands in |
|------|----------|
| Hazards, collision damage, per-part HP | Phase 4 (done — see [PHASE_4.md](PHASE_4.md)) |
| Vents, soup, spatial pool | Phases 5–6 |
| Morphology tree | Phase 7 |
| Sexual reproduction | Phase 8 |
| Optional global `max_age` cap without a gene (config-only ceiling) | Defer unless needed for failsafe |

---

## Current codebase (pre–Phase 3)

| Area | State |
|------|--------|
| **Crate layout** | [`lib.rs`](../src/lib.rs) exports `config`, `genome`, `sim`, `visual`; [`main.rs`](../src/main.rs) is visual-only Bevy app |
| **Config load** | [`SimConfig`](../src/config.rs) + [`configs/default.toml`](../configs/default.toml); `[run]` has `max_ticks`, `snapshot_interval`, `metrics_path`, `master_seed` (fields unused at runtime) |
| **CLI** | Positional `args.nth(1)` as config path only; no `--headless` / `--seed` |
| **Tick pipeline** | `sense_exposure` → `metabolism_and_gains` → `behavior_movement` → `reproduction` → `death_and_despawn` → `performance_failsafe` ([`systems/mod.rs`](../src/sim/systems/mod.rs)) |
| **Headless tests** | [`sim/headless.rs`](../src/sim/headless.rs) — `#[cfg(test)]` simplified loop (no Bevy, no metrics, no age); used by [`acceptance.rs`](../src/sim/acceptance.rs) |
| **Metrics** | No `sim/metrics.rs`; no JSONL export |
| **Locus 26–27** | [`encoding.rs`](../src/genome/encoding.rs) `core::RESERVED`; default `ancestor_hex` bytes 26–27 = `0x0000` (min lifespan when decode lands) |
| **Death reasons** | `EnergyDepleted`, `CoreDestroyed`, `ReproFatality` only ([`components.rs`](../src/sim/components.rs)) |
| **Dependencies** | `serde`, `toml` in [`Cargo.toml`](../Cargo.toml); **`clap` and `serde_json` not yet added** |

---

## Design decision: headless runtime

**Canonical choice for Phase 3:** production headless runs use a **Bevy `App` with `MinimalPlugins`** and the **same chained sim systems** as visual mode (no `VisualDebugPlugin`, no window).

| Approach | Verdict |
|----------|---------|
| Bevy `MinimalPlugins` + shared sim chain | **Use this** — single source of truth for tick order, death, and metrics |
| Extend `sim/headless.rs` test loop for CLI | **Do not** — duplicates metabolism/repro/death logic; already diverges from Bevy pipeline (no `sense_exposure` component path) |

Implementation notes:

- Extract a `build_sim_app(config, headless: bool) -> App` (or similar) in `sim/` that registers resources, spawn startup, and the chained tick systems.
- `main.rs` calls it with `headless: false` + `DefaultPlugins` + visual, or `headless: true` + `MinimalPlugins`, runs until `SimTick >= max_ticks`, then exits.
- Keep `sim/headless.rs` for **fast unit tests** where a full Bevy app is heavy, but **migrate acceptance tests** that need pipeline fidelity to the Bevy headless path (3E). Optionally thin `run_headless_sim` to wrap the shared app for tests.

---

## Sub-phase 3A — CLI

### Deliverables

| Task | Detail |
|------|--------|
| Add crates | `clap` (derive), `serde_json` in [`Cargo.toml`](../Cargo.toml) |
| Args struct | `evolution_sim [--config PATH] [--headless] [--seed U64]` |
| `--config` | Default: `configs/default.toml` ([`config::default_config_path`](../src/config.rs)) |
| `--headless` | No window; run until `run.max_ticks`; require `max_ticks > 0` or exit with error |
| `--seed` | Override `run.master_seed` after TOML load (headless determinism tests use this) |
| Visual default | `cargo run -- --config configs/default.toml` unchanged in behavior |
| Remove | Positional-only config path in `main.rs` |

### Acceptance (3A)

- [x] `cargo run -- --headless --config configs/default.toml` starts without opening a window
- [x] `cargo run -- --config configs/default.toml` still opens the window
- [x] `--seed 99` overrides TOML `master_seed` for the run
- [x] Missing config file prints readable error and non-zero exit
- [x] `--headless` with `max_ticks = 0` fails fast with clear message

---

## Sub-phase 3B — Shared sim bootstrap

### Deliverables

| Task | Detail |
|------|--------|
| `sim/app.rs` (or `sim/bootstrap.rs`) | `build_sim_app(config, mode: SimRunMode) -> App` — resources, startup spawn, chained systems |
| `main.rs` | Parse CLI → load config → `build_sim_app` → `run()` |
| Headless runner | Fixed tick loop: advance until `SimTick >= max_ticks`; no render systems |
| Visual runner | Same sim chain + `VisualDebugPlugin` + camera setup (current behavior) |
| `lib.rs` | Export bootstrap helper if tests need it |

`lib.rs` module split is **already done**; remaining work is deduplicating app wiring between visual and headless.

### Code touchpoints

| File | Change |
|------|--------|
| [`main.rs`](../src/main.rs) | CLI + mode dispatch only |
| [`sim/mod.rs`](../src/sim/mod.rs) | `pub mod app` (or bootstrap), re-export |
| [`sim/systems/mod.rs`](../src/sim/systems/mod.rs) | Register `advance_age`, `metrics_sample` when implemented (3C, 3D) |

### Acceptance (3B)

- [x] Visual and headless share one system registration list (no duplicated chain in two files)
- [x] Headless run completes `max_ticks` without `DefaultPlugins` window
- [x] Existing `main.rs` integration tests still pass

---

## Sub-phase 3C — Metrics (JSONL)

### Deliverables

| Task | Detail |
|------|--------|
| `sim/metrics.rs` | `MetricsWriter` resource; append one JSON object + newline per sample |
| `metrics_sample` system | Last in tick chain; if `tick > 0 && tick % snapshot_interval == 0`, aggregate and write |
| Death counters | Track per-reason counts since last snapshot (`DeathReason` → JSON keys below) |
| Headless | Truncate or overwrite `metrics_path` at run start (document: **truncate** for reproducible file hash) |
| Float serialization | Round to **4 decimal places** in JSON for determinism (see 3E) |

### JSONL schema (frozen for Phase 3)

One JSON object per line. All fields **required** unless marked optional.

| Field | Type | Description |
|-------|------|-------------|
| `tick` | u64 | Current sim tick at sample |
| `population` | u32 | Living entities |
| `births` | u32 | Reproduction spawns since previous sample |
| `deaths` | u32 | Total despawn since previous sample |
| `deaths_energy_depleted` | u32 | Sub-count |
| `deaths_core_destroyed` | u32 | Sub-count |
| `deaths_repro_fatality` | u32 | Sub-count |
| `deaths_age_exceeded` | u32 | Sub-count (Phase 3D) |
| `mean_energy` | f32 | Population mean (4 dp in file) |
| `median_energy` | f32 | Population median (4 dp) |
| `mean_photo_strength` | f32 | From phenotypes (4 dp) |
| `mean_max_age_ticks` | f32 | Decoded lifespan mean (4 dp; 0 if empty pop) |
| `min_max_age_ticks` | f32 | Decoded lifespan min (4 dp) |
| `max_max_age_ticks` | f32 | Decoded lifespan max (4 dp) |
| `color_bins` | object | Map `"r,g,b"` (quantized 0–255) → count; coarse diversity proxy |
| `radius_bins` | object | Map `"6"`, `"12"`, … (rounded radius) → count |
| `genotype_hash_histogram` | object | Optional; omit if empty. Coarse fold of genome bytes → count |

### Phase 4 extensions (JSONL)

Added in Phase 4; **required** in headless metrics output (see [`sim/metrics.rs`](../src/sim/metrics.rs)):

| Field | Type | Description |
|-------|------|-------------|
| `mean_skin_hardness` | f32 | Population mean decoded hardness (4 dp) |
| `mean_core_hp_frac` | f32 | Mean `core_hp / core_max_hp` (4 dp) |
| `repair_energy_spent` | f32 | Sum repair energy since previous snapshot (4 dp) |
| `mean_greenness` | f32 | Population mean greenness (4 dp) |

Phase 3 lifespan fields use `mean_age_metabolism_scale` / min / max (log-age gene), not `mean_max_age_ticks`.

**Naming rule:** Rust `DeathReason::AgeExceeded` → JSON `deaths_age_exceeded` (snake_case, `deaths_` prefix). Same pattern for other death reasons.

### Example line

```json
{"tick":3600,"population":142,"births":8,"deaths":5,"deaths_energy_depleted":2,"deaths_core_destroyed":0,"deaths_repro_fatality":2,"deaths_age_exceeded":1,"mean_energy":24.5,"median_energy":21.0,"mean_photo_strength":0.42,"mean_max_age_ticks":41000.0,"min_max_age_ticks":2000.0,"max_max_age_ticks":80000.0,"color_bins":{"136,204,68":40},"radius_bins":{"12":142}}
```

### Code touchpoints

| File | Change |
|------|--------|
| [`sim/metrics.rs`](../src/sim/metrics.rs) | New: aggregation + `serde_json` write |
| [`sim/systems/mod.rs`](../src/sim/systems/mod.rs) | Chain `metrics_sample` after `performance_failsafe` |
| [`sim/world.rs`](../src/sim/world.rs) or metrics | `SimTick`, cumulative birth/death scratch since last sample |

### Acceptance (3C)

- [x] Headless run writes at least one line to `run.metrics_path`
- [x] Line count ≈ `max_ticks / snapshot_interval` (plus tick 0 policy: sample when `tick % interval == 0` and `tick > 0`, or include tick 0 if `max_ticks >= 0` — **pick `tick > 0` only** to avoid empty population at startup)
- [x] Parsed JSON contains all required fields
- [x] Death sub-counts sum to `deaths` for each line

---

## Sub-phase 3D — Lifespan (evolvable old age)

### Design summary

| Piece | Choice |
|-------|--------|
| **Locus** | Repurpose core bytes **26–27** (`core::RESERVED` → `core::MAX_AGE`) → `max_age_raw: u16` BE |
| **Decode** | `max_age_ticks = lerp_u16(raw, age_min_ticks, age_max_ticks)` from `[genetics.decode_ranges]` |
| **Tag** | **Tradeoff** — longer decoded lifespan increases per-tick `age_upkeep` |
| **Runtime** | Component `Age { ticks: u32 }`, set `0` on spawn, `+= 1` each sim tick in `advance_age` |
| **Death** | In `death_and_despawn`: if `age.ticks >= phenotype.max_age_ticks` → despawn, `DeathReason::AgeExceeded` |
| **Mutation** | Add `MAX_AGE` to `CORE_LOCI` / weighted picker like other core loci |

Longer life is never “free”: agents that mutate toward high `max_age_raw` pay more idle drain, so shade-niche / low-energy lineages may evolve shorter lifespans while stable bright-niche lineages can afford longer ones.

### Genome ([GENOME.md](GENOME.md))

```text
max_age_ticks = lerp_u16(max_age_raw, age_min_ticks, age_max_ticks)
age_upkeep    = k_age * (max_age_ticks / age_ref_ticks)   # per tick, added in metabolism
```

Config keys in `[genetics.decode_ranges]`:

| Key | Role |
|-----|------|
| `age_min_ticks` | Shortest viable lifespan (e.g. `2000.0`) |
| `age_max_ticks` | Longest viable lifespan (e.g. `80000.0`) |
| `age_ref_ticks` | Normalization for upkeep (e.g. `20000.0`) |
| `k_age` | Strength of longevity cost (e.g. `0.02`) |

**Ancestor default:** set bytes 26–27 to BE `0x8000` (`80 00` in hex) — mid-range lifespan. Current `ancestor_hex` ends with `…0000` (min raw); update in 3F.

### Entities ([ENTITIES.md](ENTITIES.md))

| Component | Fields | Phase |
|-----------|--------|-------|
| `Age` | `ticks: u32` | 3+ |

Add `Age` to `LifeBundle` and spawn paths. Death table: `age_ticks >= max_age_ticks` → Phase **3**.

### Simulation order

Dedicated `advance_age` system chained **after** `reproduction`, **before** `death_and_despawn`:

```text
… → reproduction → advance_age → death_and_despawn → failsafe → metrics_sample
```

### Metabolism

In `metabolism_and_gains`, add `phenotype.age_upkeep` to per-tick drain (same pass as `photo_upkeep` / idle metabolism).

### Code touchpoints

| File | Change |
|------|--------|
| [`genome/encoding.rs`](../src/genome/encoding.rs) | `MAX_AGE` offset, `read_max_age_raw()`, remove `reserved_bytes` or alias |
| [`genome/phenotype.rs`](../src/genome/phenotype.rs) | `max_age_ticks`, `age_upkeep` on `Phenotype` |
| [`genome/mutation.rs`](../src/genome/mutation.rs) | `CORE_LOCI` entry for `MAX_AGE` |
| [`config.rs`](../src/config.rs) | `DecodeRanges`: four age fields |
| [`sim/components.rs`](../src/sim/components.rs) | `Age` component, `DeathReason::AgeExceeded`, `LifeBundle` |
| [`sim/systems/age.rs`](../src/sim/systems/age.rs) | New: `advance_age` |
| [`sim/systems/death.rs`](../src/sim/systems/death.rs) | Age check |
| [`sim/systems/metabolism.rs`](../src/sim/systems/metabolism.rs) | Include `age_upkeep` in drain |

### Acceptance (3D)

- [x] Child inherits parent `max_age_raw` ± mutation; decode changes `max_age_ticks` measurably
- [x] Agent with artificially low `max_age_ticks` in test config dies at expected tick with `AgeExceeded`
- [x] High `max_age_raw` lineage shows higher mean `age_upkeep` and lower net energy than short-life clone at same loci (other bytes equal)
- [x] Headless 20k ticks: population still viable with default ancestor mid-range lifespan
- [ ] Optional: over 100k ticks with mutation on, `mean_max_age_ticks` in metrics drifts (not fixed at ancestor) for at least one seed

### Tests (3D)

| Test | Module |
|------|--------|
| `max_age_locus_decodes_in_range` | `genome::phenotype` |
| `age_upkeep_increases_with_max_age` | `genome::phenotype` |
| `death_at_max_age` | `sim::systems::death` |
| `mutate_offspring_can_change_max_age_raw` | `genome::mutation` |
| `population_viable_20k_ticks_with_age` | `sim::acceptance` |

---

## Sub-phase 3E — Acceptance (from ROADMAP)

### Determinism policy

| Rule | Detail |
|------|--------|
| Same inputs | Identical `SimConfig` (after `--seed` override) + same binary |
| Compare | SHA-256 of **entire** `metrics_path` file after headless run |
| Floats | Serialize with **4 decimal places** (`round(x * 1e4) / 1e4` or equivalent) |
| Scope | Headless Bevy path only; visual mode not hash-compared |
| Requirement | `master_seed` must be set for determinism tests |

### Phase acceptance table

| Criterion | How to verify |
|-----------|----------------|
| Headless completes and writes metrics | `cargo run -- --headless --config configs/default.toml` exits 0; `metrics.jsonl` exists |
| Visual unchanged | `cargo run -- --config configs/default.toml` opens window |
| Deterministic metrics | Two headless runs, same seed → identical file hash |
| Lifespan mutation | Unit test: offspring `max_age_raw` can differ from parent |
| Low cap old-age death | Unit/integration: agent dies with `AgeExceeded` at expected tick |
| Longevity tradeoff | Unit test: high `max_age_raw` → higher `age_upkeep` at equal other loci |
| Metrics lifespan fields | Parse JSONL; assert `mean_max_age_ticks`, `deaths_age_exceeded` present |
| Population viable | Headless 20k ticks, default config: `population > 0` at end |

### Suggested tests (add to `sim/acceptance.rs` or new `sim/metrics.rs` tests)

```rust
// Conceptual names
headless_writes_valid_jsonl()
headless_metrics_deterministic_same_seed()
headless_metrics_differ_different_seed()
population_viable_20k_ticks_with_age()
death_at_max_age_integration()
```

### `sim/headless.rs` migration

| Task | Detail |
|------|--------|
| Keep | Fast tests that do not need full ECS (optional) |
| Migrate | Pipeline-fidelity acceptance → Bevy headless app helper |
| Document | Module comment: test-only shortcut, not CLI path |

### Acceptance (3E)

- [x] All tests above implemented and passing in CI / `cargo test`
- [x] ROADMAP Phase 3 acceptance criteria covered

---

## Sub-phase 3F — Config and documentation

| Task | Detail |
|------|--------|
| `configs/default.toml` | Add age decode ranges; update `ancestor_hex` bytes 26–27 to `8000` (BE mid-range) |
| Tune | Ancestor neither dies of age in first ~5k ticks nor lives forever without repro pressure |
| [SIMULATION.md](SIMULATION.md) | Mark age decode keys; fix pipeline diagram |
| [ARCHITECTURE.md](ARCHITECTURE.md) | CLI implemented note; metrics schema cross-link |
| [README.md](../README.md) | Headless CLI examples |
| [ROADMAP.md](ROADMAP.md) | Phase 3 row → **Done** |

### Acceptance (3F)

- [x] Default headless 5k–10k tick run: stable population below `max_entities`
- [x] Docs and config consistent with shipped behavior

---

## Code migration map (Phase 2 → Phase 3)

| Phase 2 | Phase 3 target |
|---------|----------------|
| Positional config path | `clap` `--config` / `--headless` / `--seed` |
| Visual-only `main.rs` app wiring | Shared `build_sim_app` + mode dispatch |
| `[run]` fields unused | Headless honors `max_ticks`, `metrics_path`, `snapshot_interval` |
| `sim/headless.rs` test loop | Bevy headless for CLI; test harness thinned or wraps app |
| No metrics | `sim/metrics.rs` + `metrics_sample` system |
| `core::RESERVED` bytes | `core::MAX_AGE` + phenotype decode |
| Death: energy / HP / repro only | + `AgeExceeded` |
| Tick chain ends at failsafe | + `advance_age`, `metrics_sample` |

### Tick pipeline (target)

```text
advance_sim_tick → sense_exposure → metabolism_and_gains → behavior_movement
  → reproduction → advance_age → death_and_despawn → performance_failsafe → metrics_sample
```

---

## Config migration

Add to `[genetics.decode_ranges]` when implementing 3D:

```toml
age_min_ticks = 2000.0
age_max_ticks = 80000.0
age_ref_ticks = 20000.0
k_age = 0.02
```

Update `ancestor_hex` — set core bytes 26–27 to BE `0x8000` (`80 00`):

```toml
# Was: …000000000000 (bytes 26–27 = 0x0000)
# New: …000000008000
ancestor_hex = "45564f3188cc448040003000280080004000300080800000800000000000008000"
```

Tune so default ancestor neither dies of age in the first ~5k ticks nor lives forever without repro pressure.

---

## Documentation maintenance

When Phase 3 lands, update in the same PR:

- [GENOME.md](GENOME.md) — locus 26–27, tradeoff tag, formulas (remove “Phase 3” placeholder wording)
- [ENTITIES.md](ENTITIES.md) — `Age` component, death phase column
- [SIMULATION.md](SIMULATION.md) — `[genetics.decode_ranges]` age keys; pipeline diagram
- [ARCHITECTURE.md](ARCHITECTURE.md) — metrics schema, CLI status, dependencies table
- [configs/default.toml](../configs/default.toml) — decode ranges + ancestor hex

---

## Further reading

- [ROADMAP.md](ROADMAP.md) — Phase overview
- [PHASE_1.md](PHASE_1.md) — Death system baseline
- [PHASE_2.md](PHASE_2.md) — Environment / metabolism context

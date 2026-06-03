# Phase 3 — Headless CLI, metrics, and lifespan

Detailed implementation plan for **Phase 3** from [ROADMAP.md](ROADMAP.md). Phases 1–2 deliver genetics, local photosynthesis, and lifecycle death from energy, core HP, and repro risk.

**Prerequisite reading:** [SIMULATION.md](SIMULATION.md), [GENOME.md](GENOME.md), [ENTITIES.md](ENTITIES.md), [ARCHITECTURE.md](ARCHITECTURE.md).

---

## Master checklist

- [ ] **3A** — `clap` CLI: `--headless`, `--config`, `--seed` override
- [ ] **3B** — `lib.rs` module split; slim `main.rs` (visual vs headless entry)
- [ ] **3C** — JSONL metrics per [ARCHITECTURE.md](ARCHITECTURE.md) (`snapshot_interval`, `metrics_path`)
- [ ] **3D** — **Evolvable lifespan:** `max_age` locus, `Age` component, senescence upkeep, old-age death
- [ ] **3E** — Unit + headless acceptance (determinism, lifespan tradeoff, metrics fields)
- [ ] **3F** — Default `configs/default.toml` tuning; update [ROADMAP.md](ROADMAP.md) Phase 3 → **Done**

---

## Goals and non-goals

### Goals

1. **Headless reproducibility** — Same seed + config ⇒ identical metrics (within float tolerance).
2. **Observability** — Periodic population / energy / trait histograms for tuning and regression.
3. **Evolvable lifespan** — Heritable `max_age_ticks`; selection can favor longer or shorter life via explicit tradeoffs (not a free longevity buff).
4. **Old-age death** — Despawn when `age_ticks >= phenotype.max_age_ticks`; reason `AgeExceeded` in metrics.

### Explicitly out of scope (Phase 3)

| Item | Lands in |
|------|----------|
| Hazards, collision damage, per-part HP | Phase 4 |
| Vents, soup, spatial pool | Phases 5–6 |
| Morphology tree | Phase 7 |
| Sexual reproduction | Phase 8 |
| Optional global `max_age` cap without a gene (config-only ceiling) | Defer unless needed for failsafe |

---

## Sub-phase 3D — Lifespan (evolvable old age)

### Design summary

| Piece | Choice |
|-------|--------|
| **Locus** | Repurpose core bytes **26–27** (`reserved` in v1 table) → `max_age_raw: u16` |
| **Decode** | `max_age_ticks = lerp_u16(raw, age_min_ticks, age_max_ticks)` from `[genetics.decode_ranges]` |
| **Tag** | **Tradeoff** — longer decoded lifespan increases per-tick `age_upkeep` (see below) |
| **Runtime** | Component `Age { ticks: u32 }`, set `0` on spawn, `+= 1` each sim tick before death |
| **Death** | In `death_and_despawn`: if `age.ticks >= phenotype.max_age_ticks` → despawn, `DeathReason::AgeExceeded` |
| **Mutation** | Include `max_age` in `CORE_LOCI` / weighted picker like other core loci (no special-case rate required) |

Longer life is never “free”: agents that mutate toward high `max_age_raw` pay more idle drain, so shade-niche / low-energy lineages may evolve shorter lifespans while stable bright-niche lineages can afford longer ones.

### Genome ([GENOME.md](GENOME.md))

Update the core table: offset 26–27 = `max_age` (2 bytes, Tradeoff).

```text
max_age_ticks = lerp_u16(max_age_raw, age_min_ticks, age_max_ticks)
age_upkeep    = k_age * (max_age_ticks / age_ref_ticks)   # per tick, added in metabolism
```

Config knobs (in `[genetics.decode_ranges]` or `[economy]` — pick one section in implementation, document in TOML):

| Key | Role |
|-----|------|
| `age_min_ticks` | Shortest viable lifespan (e.g. 2_000) |
| `age_max_ticks` | Longest viable lifespan (e.g. 80_000) |
| `age_ref_ticks` | Normalization for upkeep (e.g. 20_000) |
| `k_age` | Strength of longevity cost (e.g. `0.02`) |

**Ancestor default:** mid-range `max_age_raw` (e.g. `0x8000`) so starting population dies of old age only after many ticks under default tuning—not immediately.

### Entities ([ENTITIES.md](ENTITIES.md))

| Component | Fields | Phase |
|-----------|--------|-------|
| `Age` | `ticks: u32` | 3+ |

Death table row: `age_ticks >= max_age_ticks` → Phase **3** (required when `max_age_ticks` finite).

### Simulation order

Insert age increment in the tick chain **before** `death_and_despawn` (after reproduction or with metabolism—document final choice in code; recommended: start of death pass or dedicated `advance_age` system chained just before death):

```text
… → reproduction → advance_age → death_and_despawn → failsafe → metrics
```

### Metabolism

In `metabolism_and_gains`, add `phenotype.age_upkeep()` to per-tick drain (same pass as `photo_upkeep` / idle metabolism).

### Metrics (3C + 3D)

Per snapshot (optional aggregates):

- `mean_max_age_ticks`, `min` / `max` of decoded lifespan in population
- `deaths_age_exceeded` since last sample (or cumulative counter)

Enables verifying evolution is shifting lifespan over long headless runs.

### Acceptance (3D)

- [ ] Child inherits parent `max_age_raw` ± mutation; decode changes `max_age_ticks` measurably
- [ ] Agent with artificially low `max_age_ticks` in test config dies at expected tick with `AgeExceeded`
- [ ] High `max_age_raw` lineage shows higher mean `age_upkeep` and lower net energy than short-life clone at same loci (other bytes equal)
- [ ] Headless 20k ticks: population still viable with default ancestor mid-range lifespan
- [ ] Optional: over 100k ticks with mutation on, `mean_max_age_ticks` in metrics drifts (not fixed at ancestor) for at least one seed

### Tests (suggested)

| Test | Module |
|------|--------|
| `max_age_locus_decodes_in_range` | `genome::phenotype` |
| `age_upkeep_increases_with_max_age` | `genome::phenotype` |
| `death_at_max_age` | `sim::systems::death` |
| `mutate_offspring_can_change_max_age_raw` | `genome::mutation` |

---

## Sub-phases 3A–3C (summary)

### 3A — CLI

- `evolution_sim --config PATH [--headless] [--seed U64]`
- Headless: `MinimalPlugins` + sim chain, no `VisualDebugPlugin`
- Visual: current window path unchanged

### 3B — Crate layout

Align with [ARCHITECTURE.md](ARCHITECTURE.md): `lib.rs` exports `config`, `genome`, `sim`, `visual`.

### 3C — Metrics

- Sample on `tick % snapshot_interval == 0`
- Write JSONL lines to `run.metrics_path`
- Fields: tick, population, mean energy, color keys / radius bins, death counts by reason (including `age_exceeded`), lifespan stats from 3D

---

## Config migration

Add to `configs/default.toml` when implementing 3D:

```toml
[genetics.decode_ranges]
age_min_ticks = 2000.0
age_max_ticks = 80000.0
age_ref_ticks = 20000.0
k_age = 0.02
```

Tune so default ancestor neither dies of age in the first ~5k ticks nor lives forever without repro pressure.

---

## Documentation maintenance

When 3D lands, update in the same PR:

- [GENOME.md](GENOME.md) — locus 26–27, tradeoff tag, formulas
- [ENTITIES.md](ENTITIES.md) — `Age` component, death phase column
- [SIMULATION.md](SIMULATION.md) — pipeline step for `advance_age`
- [configs/default.toml](../configs/default.toml) — decode ranges + ancestor if raw bytes change

---

## Further reading

- [ROADMAP.md](ROADMAP.md) — Phase overview
- [PHASE_1.md](PHASE_1.md) — Death system baseline
- [PHASE_2.md](PHASE_2.md) — Environment / metabolism context

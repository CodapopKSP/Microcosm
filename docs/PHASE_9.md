# Phase 9 — Seasons and climate cycles

Detailed implementation plan for **Phase 9** from [ROADMAP.md](ROADMAP.md). Phases 2–6 establish static environmental fields (light, hazards, vents, soup). Phase 9 adds **time-varying** modulation so selection pressure shifts over the sim year — complementing spatial niches (`gradient_y`, hazard bands) with temporal ones.

**Prerequisite reading:** [SIMULATION.md](SIMULATION.md), [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md), [ARCHITECTURE.md](ARCHITECTURE.md), [PHASE_2.md](PHASE_2.md), [PHASE_4.md](PHASE_4.md).

---

## Master checklist

- [ ] **9A** — Season config schema, `SeasonClock` resource, `environment_tick` system
- [ ] **9B** — Modulate light / `solar_constant` from season keyframes
- [ ] **9C** — Modulate hazards, vents, soup regen (when those phases exist)
- [ ] **9D** — Metrics: `season_index`, `season_phase`; optional per-season aggregates
- [ ] **9E** — Acceptance tests, default off; update [ROADMAP.md](ROADMAP.md) Phase 9 → **Done**

---

## Goals and non-goals

### Goals

1. **Temporal heterogeneity** — Cyclic environment prevents a single “always optimal” genotype; pairs with Phase 3 lifespan and Phase 8 sexual repro (optional) for bet-hedging strategies.
2. **Single hook** — `environment_tick` runs first each sim tick; updates effective world parameters before `sense_exposure` reads them.
3. **Config-driven** — Named seasons or numeric keyframes in TOML; no hard-coded spring/summer in Rust.
4. **Deterministic** — Season phase is a pure function of `SimTick` and config (no season RNG).
5. **Backward compatible** — Seasons disabled by default; existing configs unchanged.

### Explicitly out of scope (Phase 9)

| Item | Notes |
|------|-------|
| Stochastic weather events | Separate follow-up (storms, drought spikes) |
| Heritable hibernation / diapause genes | Defer unless core reserved bytes allocated |
| Non-cyclic climate drift (global warming curve) | Future follow-up |
| Axial tilt, latitude, day-length geometry | Stay abstract: scale factors only |
| Player or script API to jump season | Config-only phase offset at load (`season_start_tick`) is enough |

---

## Why a new phase (not Phase 6)

| Option | Verdict |
|--------|---------|
| Fold into Phase 6 (soup / spatial) | **Reject** — Phase 6 already adds pool, occlusion, compounds; seasons touch every prior environment system |
| Fold into Phase 2 (light only) | **Reject** — Underuses hazards/vents/soup; would need rework when those land |
| **Dedicated Phase 9** | **Use this** — Cross-cutting modulation after static ecology exists; shares `environment_tick` with Phase 6 pool recharge |

Phase 6 implements `environment_tick` for pool/soup regen. Phase 9 **extends** the same system (or sub-schedulers) with season interpolation — implement 6 first, then 9.

---

## Current codebase (pre–Phase 9)

| Area | State |
|------|--------|
| **`environment_tick`** | Listed in [ARCHITECTURE.md](ARCHITECTURE.md) chain; **not registered** in [`systems/mod.rs`](../src/sim/systems/mod.rs) — no-op by omission |
| **`solar_constant`** | Static per run from [`EnvironmentConfig`](../src/config.rs) |
| **`LightField`** | Built at startup; samples static `gradient_min` / `gradient_max` |
| **Hazards / vents / soup** | Phases 4–6 (planned); season plan assumes effective-* parameter pattern |
| **Metrics** | Phase 3 JSONL; no season fields |
| **Lifespan** | `max_age_ticks` heritable (Phase 3); no coupling to season length yet |

---

## Design decisions

### 9.1 — Season clock (canonical)

```text
ticks_per_season = config.seasons[t].duration_ticks   # or global uniform duration
year_tick        = sim_tick - season_start_tick
season_index     = (year_tick / ticks_per_season) % num_seasons
phase            = (year_tick % ticks_per_season) as f32 / ticks_per_season   # ∈ [0, 1)
```

Resource:

```rust
// Conceptual
struct SeasonClock {
    pub season_index: u32,
    pub phase: f32,           // 0..1 within current season
    pub year: u32,            // full cycles completed
    pub effective: SeasonOverrides,  // interpolated scalars for this tick
}
```

Config flag: `environment.seasons_enabled = false` (default).

### 9.2 — Season keyframes (canonical)

Table of named seasons with **overrides** (all optional — omitted keys keep base config value):

```toml
[environment]
seasons_enabled = true
season_start_tick = 0
interpolation = "linear"   # "linear" | "smoothstep" | "hold"

[[environment.seasons]]
name = "spring"
duration_ticks = 9000
solar_constant = 2.4
gradient_max = 0.85

[[environment.seasons]]
name = "summer"
duration_ticks = 9000
solar_constant = 3.2
gradient_max = 1.0

[[environment.seasons]]
name = "autumn"
duration_ticks = 9000
solar_constant = 2.0
gradient_max = 0.7

[[environment.seasons]]
name = "winter"
duration_ticks = 9000
solar_constant = 1.2
gradient_min = 0.08
gradient_max = 0.35
# hazard_damage_scale = 1.2   # optional multiplier on all zone hazards
# vent_flux_scale = 0.8
# soup_regen_scale = 0.5
```

**Interpolation:** between season *i* end and season *i+1* start, lerp override fields. Last season wraps to first.

**Hold mode:** jump at season boundaries (no blend) — useful for tests.

### 9.3 — Application order (canonical)

Each tick, before sense/metabolism:

1. `environment_tick` advances `SeasonClock` from `SimTick`.
2. Write **effective** scalars into resources:
   - `EffectiveEnvironment { solar_constant, gradient_min, gradient_max, hazard_scale, vent_scale, soup_regen_scale }`
   - Or mutate `LightField` / `HazardField` in place (document: prefer **effective overlay** so base TOML stays readable).
3. Phase 6 pool recharge (if enabled) uses effective `solar_influx` when seasons modulate it.

`LightField::sample(pos)` uses effective gradient bounds. Metabolism uses effective `solar_constant`.

### 9.4 — Selection pressure (design intent)

| Season profile | Favored traits (examples) |
|----------------|---------------------------|
| High solar / bright gradient | High `photo_strength`, greenness (Phase 4), bright-band positioning |
| Low solar / winter | Low metabolism, low photo upkeep, short lifespan if cycle << `max_age_ticks` |
| High hazard scale | `skin_hardness`, repair efficiency |
| High vent / soup scale | `chemo_potential`, transporters (Phase 5–6) |

Tune `duration_ticks` relative to `age_max_ticks` and `repro_cooldown` so at least one full reproduction cycle fits in a favorable season.

### 9.5 — Metrics extensions

Add to JSONL (required when seasons enabled):

| Field | Type | Description |
|-------|------|-------------|
| `season_index` | u32 | Current season table index |
| `season_phase` | f32 | 0..1 within season (4 dp) |
| `season_name` | string | Optional; from config `name` |
| `effective_solar_constant` | f32 | After modulation (4 dp) |

Optional stretch: `population_by_season` histogram accumulated across run.

---

## Sub-phase 9A — Clock and config

### Deliverables

| Task | Detail |
|------|--------|
| `SeasonConfig`, `SeasonKeyframe` | Serde structs under `EnvironmentConfig` |
| `SeasonClock` resource | Updated in `environment_tick` |
| `environment_tick` system | First in tick chain; register in [`systems/mod.rs`](../src/sim/systems/mod.rs) |
| Default | `seasons_enabled = false`; empty `seasons` vec |

### Acceptance (9A)

- [ ] Unit test: given `sim_tick` and table, `season_index` and `phase` match hand calculation
- [ ] Wrap: after last season, index returns to 0
- [ ] Disabled seasons: `SeasonClock` static, effective params equal base config

---

## Sub-phase 9B — Light and solar modulation

### Deliverables

| Task | Detail |
|------|--------|
| `EffectiveEnvironment` resource | Holds modulated scalars for tick |
| `LightField` or sample path | Read effective `gradient_min` / `gradient_max` |
| `metabolism_and_gains` | Use effective `solar_constant` |

### Acceptance (9B)

- [ ] At summer keyframe tick, sample gain > winter keyframe at same position (unit/harness)
- [ ] Seasons off: identical gain to pre-9 behavior (regression)

---

## Sub-phase 9C — Hazard, vent, soup scales

### Deliverables

| Task | Detail |
|------|--------|
| `hazard_damage_scale` | Multiply zone `damage_per_tick` in interactions |
| `vent_flux_scale` | Multiply vent flux in chemo gain (Phase 5+) |
| `soup_regen_scale` | Multiply compound regen (Phase 6+) |

Implement stubs that no-op when respective systems absent; full acceptance when 4–6 land.

### Acceptance (9C)

- [ ] Winter config with `hazard_damage_scale = 1.5` → faster core HP loss in hazard vs summer same seed
- [ ] Scales default to `1.0` when omitted from keyframe

---

## Sub-phase 9D — Metrics and visual

### Deliverables

| Task | Detail |
|------|--------|
| JSONL fields | §9.5 |
| Visual debug (optional) | Season name in HUD or background tint lerp |

### Acceptance (9D)

- [ ] Headless seasons run: every metrics line includes `season_index`, `season_phase`
- [ ] Two samples one full year apart: same `season_index` and `phase` (± float tolerance)

---

## Sub-phase 9E — Acceptance and documentation

### Phase acceptance table

| Criterion | How to verify |
|-----------|----------------|
| Energy tracks season | 2+ cycles: correlate mean energy with effective solar |
| Winter survival tradeoff | Low-photo beats high-photo through configured winter |
| Determinism | Same seed + seasons config → identical metrics hash |
| Default off | `default.toml` unchanged behavior vs Phase 8 complete build |

### Suggested tests

| Test | Module |
|------|--------|
| `season_index_and_phase_from_tick` | `sim::season` |
| `solar_interpolates_between_keyframes` | `sim::season` |
| `summer_gain_exceeds_winter` | `sim::acceptance` |
| `low_photo_outlasts_high_photo_in_winter` | `sim::acceptance` |
| `metrics_include_season_fields` | `sim::metrics` |

### Documentation maintenance

| Doc | Update |
|-----|--------|
| [ROADMAP.md](ROADMAP.md) | Phase 9 → **Done** |
| [SIMULATION.md](SIMULATION.md) | `[environment.seasons]` schema; `environment_tick` behavior |
| [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md) | Seasons section |
| [ARCHITECTURE.md](ARCHITECTURE.md) | `SeasonClock`, effective env resources |

---

## Code migration map (Phase 8 → Phase 9)

| Before | Phase 9 target |
|--------|----------------|
| No `environment_tick` | Registered first in chain |
| Static `solar_constant`, light bounds | `EffectiveEnvironment` each tick |
| Metrics tick-only | + season fields |
| Spatial/temporal niches independent | Temporal modulation layered on spatial |

### Tick pipeline (target)

```text
environment_tick → sense_exposure → metabolism_and_gains → behavior_movement
  → interactions → healing → reproduction → advance_age → death_and_despawn
  → performance_failsafe → metrics_sample
```

---

## Config example (acceptance)

`configs/four_seasons.toml` — copy `default.toml`, enable seasons, shorten `max_ticks` for CI (~40k ticks = one year if 4×9000).

---

## Risks and mitigations

| Risk | Mitigation |
|------|------------|
| Winter extinction | Tune winter `solar_constant` above starvation threshold; repair + low-meta genotypes survive |
| Phase 6/9 merge conflicts on `environment_tick` | Single system file; season block runs before pool/soup block |
| Lifespan >> year length | Selection averages across seasons — document; optional `ticks_per_season` vs `age_ref_ticks` tuning guide |
| Interpolation bugs at wrap | Unit test season N→0 boundary explicitly |

---

## Further reading

- [ROADMAP.md](ROADMAP.md) — Phase overview
- [ROADMAP.md](ROADMAP.md) — Phase 6 (soup/spatial) shares `environment_tick`
- [PHASE_3.md](PHASE_3.md) — Lifespan timescales vs season length

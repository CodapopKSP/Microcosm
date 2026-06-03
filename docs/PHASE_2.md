# Phase 2 — Light and photosynthesis (local energy)

Detailed implementation plan for **Phase 2** from [ROADMAP.md](ROADMAP.md). Phase 1 ([PHASE_1.md](PHASE_1.md)) is complete: genomes, metabolism upkeeps, movement, reproduction, and death without collision despawn.

**Prerequisite reading:** [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md), [SIMULATION.md](SIMULATION.md), [GENOME.md](GENOME.md), [ARCHITECTURE.md](ARCHITECTURE.md).

---

## Master checklist

Use this list for PR tracking. Sub-phases below expand each item.

- [x] **2A** — `[environment]` config: `energy_model`, `solar_constant`, `light_mode`, gradient knobs
- [x] **2B** — `LightField` sampler in `sim::light` (`uniform`, `gradient_y`)
- [x] **2C** — `sense_exposure` (or inline sample): store per-entity `light_sample` for metabolism / debug
- [x] **2D** — Model **A** photosynthetic gain: `photo_strength * light(pos) * solar_constant`
- [x] **2E** — Remove Phase 1 flat grant from default path; tune `solar_constant` + `k_photo` for viability
- [x] **2F** — **Retire binary overlap blocking** as default (see [Crowding and overlap](#crowding-and-overlap-interim-vs-target))
- [x] **2G** — Locomotion tradeoffs: verify `max_speed` / `move_cost` (already in Phase 1; acceptance only)
- [x] **2H** — Optional visual debug: light sample or energy bar overlay
- [ ] **2I** — Unit tests: light at corners, gain formula, shade vs bright net energy
- [ ] **2J** — Headless acceptance: niche viability (high-photo bright, low-photo shade)
- [ ] **2K** — Default `configs/default.toml` tuning; document migration from `flat_energy_grant_per_tick`
- [ ] **2L** — Phase acceptance + update [ROADMAP.md](ROADMAP.md) row to **Done**

---

## Goals and non-goals

### Goals

1. **Position-dependent energy** — Replace the Phase 1 flat grant with [Model A (local irradiance)](ENERGY_AND_ENVIRONMENT.md#model-a--per-entity-irradiance): world influx does not scale with population count.
2. **Spatial niches via light** — `gradient_y` (and later `map`) makes bright vs shaded regions; `photo_strength` genes trade uptake against `photo_upkeep`.
3. **Selectable energy model hook** — Config `energy_model = "local"` is the only implemented path in this phase; schema reserves `pool` / `spatial` for later phases.
4. **Preserve Phase 1 lifecycle** — Reproduction, repro risk, death rules, and collision policy (overlap ignored for damage) unchanged.

### Explicitly out of scope (Phase 2)

| Item | Lands in |
|------|----------|
| Global solar pool (`energy_model = "pool"`) | Phase 6 (if still needed) |
| Continuous spatial occlusion (`energy_model = "spatial"`) | Phase 6 |
| `light_mode = "map"` (image / procedural grid) | Phase 2 optional stretch or early Phase 5 |
| Vents, chemo gain, sensors | Phase 5 |
| Hazards, collision damage, per-part HP | Phase 4 |
| Soup compounds | Phase 6 |
| `clap` headless CLI, JSONL metrics export | Phase 3 |
| Morphology extension | Phase 7 |

---

## Current codebase (pre–Phase 2 completion)

| Area | State |
|------|--------|
| Photosynthetic **gain** | Still `flat_energy_grant_per_tick` in [`metabolism.rs`](../src/sim/systems/metabolism.rs) |
| `photo_strength` | Drives `photo_upkeep` only in gain formula; locus ready for Phase 2 multiplier |
| **Overlap crowding** | **Impromptu:** any circle–circle overlap sets photosynthetic gain to **0** that tick for **both** entities (costs still apply) |
| Locomotion | **Done in Phase 1:** `max_speed` locus, `locomotion_investment` → `move_cost`, moving metabolism in [`movement.rs`](../src/sim/systems/movement.rs) |
| Light / environment resources | Not implemented in [`world.rs`](../src/sim/world.rs) |
| Tick pipeline | `metabolism_and_gains` → `behavior_movement` → `reproduction` → `death` → `failsafe` ([`systems/mod.rs`](../src/sim/systems/mod.rs)) |

---

## Design decision: crowding and overlap (interim vs target)

### What exists today (impromptu)

After Phase 1, overlap blocking was added so crowded agents do not receive the flat photosynthetic grant:

```text
if entity overlaps any other agent circle this tick:
  photosynthetic_gain = 0
else:
  photosynthetic_gain = flat_energy_grant_per_tick
```

Implementation: [`circles_overlap`](../src/sim/systems/metabolism.rs), [`entities_blocked_by_overlap`](../src/sim/systems/metabolism.rs), wired in `metabolism_and_gains`. Metabolism and trait upkeeps are unchanged.

### Why this is not the Phase 2 target

| Issue | Detail |
|-------|--------|
| Not in the roadmap | Phase 2 acceptance is **bright vs shade** via the light field, not binary crowding |
| Wrong shape | All-or-nothing: touching at one pixel zeros gain; no partial shading |
| Wrong phase for “canopy” | [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md) Model **C** (`energy_model = "spatial"`) reduces `light(pos)` by occlusion — Phase **6** |
| Conflicts with Model A story | Under `gradient_y`, selection should push lineages toward light; overlap blocking adds a second, unrelated spatial filter |
| Tuning hazard | With real gain `∝ photo_strength`, overlap blocking can starve dense clusters entirely while sparse identical genotypes thrive — hard to tune alongside `solar_constant` |

### Decision (canonical for Phase 2)

1. **Ship position-based gain** as the primary environmental selection pressure.
2. **Default `crowding_blocks_photosynthesis = false`** (or remove overlap logic entirely when light is enabled).
3. **Do not** treat binary overlap blocking as part of Model A acceptance tests.
4. **Optional:** keep overlap blocking behind an explicit config flag for A/B experiments until Phase 6 spatial shading exists; default **off** in `configs/default.toml`.

If you need density-dependent photosynthesis before Phase 6, prefer a **soft** preview (e.g. multiply gain by `1 - min(1, k * local_overlap_fraction)`) over a hard zero — but that is still optional stretch, not required for Phase 2 done.

---

## Sub-phase 2A — Environment config

### Deliverables

| Task | Detail |
|------|--------|
| `EnvironmentConfig` | New section in [`config.rs`](../src/config.rs) + [`configs/default.toml`](../configs/default.toml) |
| `energy_model` | Enum/string: Phase 2 implements **`local` only**; deserialize `pool` / `spatial` but error or warn if selected |
| `solar_constant` | Scales per-entity irradiance (Model A) |
| `light_mode` | `uniform` \| `gradient_y` (required); `map` reserved |
| Gradient knobs | e.g. `gradient_axis = "y"`, `gradient_min = 0.2`, `gradient_max = 1.0`, `gradient_bright_at = "top"` |

### Example TOML

```toml
[environment]
energy_model = "local"
solar_constant = 0.35
light_mode = "gradient_y"
gradient_min = 0.15
gradient_max = 1.0
gradient_bright_at = "top"   # top | bottom

# Optional experiment only — default false for Phase 2 canonical runs
crowding_blocks_photosynthesis = false
```

### Economy migration

| Key | Phase 2 behavior |
|-----|------------------|
| `flat_energy_grant_per_tick` | **Deprecated** for `energy_model = "local"`; keep in schema for Phase 1 regression tests or `energy_model = "legacy_flat"` if you want one config file for both eras |
| `metabolism_floor`, `k_photo` | Unchanged; primary balance knobs with `solar_constant` |

### Acceptance (2A)

- [x] `cargo run` loads new section without panic
- [x] Unknown `light_mode` fails fast with readable error
- [x] `energy_model = "pool"` does not silently run Model A

---

## Sub-phase 2B — Light field sampler

### Deliverables

| Task | Detail |
|------|--------|
| `LightField` resource | Lives in [`sim/world.rs`](../src/sim/world.rs) (or `sim/light.rs`) |
| `sample(pos: Vec2) -> f32` | Unitless multiplier in `[gradient_min, gradient_max]` or `[0, 1]` per config docs |
| `uniform` | Constant `1.0` (or `gradient_max`) everywhere |
| `gradient_y` | Linear interpolate by world Y within bounds; respect `gradient_bright_at` |
| Coordinates | Use same space as agent `Transform` (centered origin, ±half width/height) |

### Sampling notes

- No bilinear grid yet (no `map` mode).
- Light does **not** depend on population (Model A).
- Document whether values are clamped to `[0, 1]` or can exceed `1.0` if `solar_constant` is the only global scale.

### Acceptance (2B)

- [x] Unit tests: top vs bottom of world differ under `gradient_y`
- [x] `uniform` returns constant at corners and center

---

## Sub-phase 2C — Sense / exposure

### Deliverables

| Task | Detail |
|------|--------|
| `sense_exposure` system | Register **before** `metabolism_and_gains` ([SIMULATION.md](SIMULATION.md) order) |
| Per-entity storage | Component `LightExposure(f32)` or scratch `HashMap<Entity, f32>` cleared each tick |
| Sample | `light = light_field.sample(position)` |

Phase 2 does not need vents, hazards, or soup samples.

### Acceptance (2C)

- [x] Every living agent has a light sample each tick before metabolism runs
- [x] Stationary agent at fixed position has stable `light` across ticks

---

## Sub-phase 2D — Model A photosynthetic gain

### Per-tick formula (Phase 2 canonical)

```text
light_i     = LightField.sample(pos_i)
gain_i      = photo_strength_i * light_i * solar_constant
cost_i      = metabolism_idle_i + photo_upkeep_i + armor_upkeep_i + chemo_upkeep_i
Δenergy_i   = gain_i - cost_i
```

Optional crowding (non-default):

```text
if crowding_blocks_photosynthesis && overlaps_any_other_agent:
  gain_i = 0
```

### Code touchpoints

| File | Change |
|------|--------|
| [`metabolism.rs`](../src/sim/systems/metabolism.rs) | `photosynthetic_gain_per_tick(phenotype, light, environment, crowding_blocked)` |
| [`systems/mod.rs`](../src/sim/systems/mod.rs) | Chain `sense_exposure` before metabolism; update module comment |
| [`reproduction.rs`](../src/sim/systems/reproduction.rs) | Repro risk / energy previews must use same gain formula if they simulate metabolism |

### Acceptance (2D)

- [x] `photo_strength = 0` → zero gain (still pays metabolism unless other grants exist)
- [x] Doubling `solar_constant` doubles gain at fixed `light` and `photo_strength`
- [x] Population doubling does **not** change per-entity `gain` at fixed positions (Model A)

---

## Sub-phase 2E — Tuning and flat-grant removal

### Tuning targets

| Knob | Role |
|------|------|
| `solar_constant` | Global photosynthesis scale |
| `k_photo` | Upkeep per unit `photo_strength`; must exceed marginal gain in shade for high-photo genotypes |
| `gradient_min` / `gradient_max` | Shade depth; shade niche must allow `gain < cost` for high-photo |
| `metabolism_floor`, ancestor `photo_strength` | Baseline viability in mid-light |

### Sanity checks

| Scenario | Expected |
|----------|----------|
| Ancestor in **bright** band (`light ≈ gradient_max`) | Net positive Δenergy over 500–1000 ticks |
| High-photo mutant in **shade** (`light ≈ gradient_min`) | Net negative Δenergy over 500 ticks |
| Low-photo, low-metabolism mutant in shade | Net energy flat or slow decline (can survive with movement off) |
| `solar_constant = 0`, no flat grant | Population dies (extends Phase 1 `zero_grant` test) |

### Acceptance (2E)

- [x] Default config: population survives **3000+** ticks headless (reuse / extend [`sim/acceptance.rs`](../src/sim/acceptance.rs))
- [x] `flat_energy_grant_per_tick` not used when `energy_model = "local"`

---

## Sub-phase 2F — Crowding migration (overlap blocking)

### Deliverables

| Task | Detail |
|------|--------|
| Default off | `crowding_blocks_photosynthesis = false` in default config |
| Gate overlap logic | Only apply [`entities_blocked_by_overlap`](../src/sim/systems/metabolism.rs) when flag true |
| Tests | Update [`overlap_blocks_photosynthetic_gain_only`](../src/sim/systems/metabolism.rs) to run under flag or move to `#[cfg(test)]` legacy module |
| Docs | One paragraph in [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md) pointing to Phase 6 for occlusion |

### Acceptance (2F)

- [x] Default headless acceptance passes **without** overlap blocking
- [x] With flag `true`, overlap still zeros gain (regression for experiment branch)

---

## Sub-phase 2G — Locomotion (verification only)

Locomotion investment and `max_speed` locus were implemented in **Phase 1G** per [GENOME.md](GENOME.md). Phase 2 does not re-derive formulas unless a bug is found.

### Verify

| Check | Location |
|-------|----------|
| `move_cost = m_base + k_m * locomotion_investment + k_drag * body_radius` | [`phenotype.rs`](../src/genome/phenotype.rs) |
| `max_speed = lerp(max_speed_raw) - move_penalty` | same |
| Movement deducts `move_cost` + moving metabolism when speed > ε | [`movement.rs`](../src/sim/systems/movement.rs) |

### Acceptance (2G)

- [x] Existing movement unit tests pass
- [x] High `locomotion_investment` increases energy drain when moving (no regression)

---

## Sub-phase 2H — Visual debug (optional)

| Task | Detail |
|------|--------|
| Debug overlay | Sample `LightField` as background gradient or per-agent gizmo text |
| Energy bar | Optional `internal_energy / repro_threshold` on select entity |

**Controls (visual mode):** **L** toggles light gradient; **click** selects an agent (light ring + energy bar); **Esc** clears selection.

### Acceptance (2H)

- [x] Light-field gradient visible under `gradient_y` default config
- [x] Selected agent shows energy bar vs `repro_threshold`

---

## Sub-phase 2I — Unit tests

| Test | Module |
|------|--------|
| `light_gradient_top_brighter_than_bottom` | `sim::world` or `light` |
| `photosynthetic_gain_scales_with_strength_and_light` | `metabolism` |
| `shade_net_negative_for_high_photo` | `metabolism` |
| `local_gain_independent_of_population_count` | `metabolism` or `acceptance` |
| `crowding_flag_off_by_default` | `config` | ✓ (2F) |

---

## Sub-phase 2J — Phase acceptance (from ROADMAP)

| Criterion | How to verify |
|-----------|----------------|
| High-photo strains **gain** net energy in bright regions | Headless: spawn or mutate high `photo_strength`, fix position in bright Y band, assert Δenergy > 0 over N ticks |
| Same strains **lose** net energy in shade when upkeep > gain | Low `light` sample + high `photo_strength` |
| Low-photo, low-metabolism strains **persist** in shade | Niche viability test; population fraction in bottom quartile of map |
| No unbounded growth under default tuning | 5k–10k tick headless; population stable below `max_entities`; energy finite |
| Model A | Total influx ∝ sum of `photo_strength * light` not ∝ entity count alone |

### Suggested tests (to implement in 2J)

```rust
// Conceptual names — add to sim/acceptance.rs or metabolism tests
high_photo_bright_band_net_positive()
high_photo_shade_band_net_negative()
low_photo_shade_survives_longer_than_high_photo_shade()
population_stable_5000_ticks_gradient_default_config()
```

### Diversity (manual / light)

With `gradient_y`, expect vertical clustering of high-photo lineages over many seeds; color variance alone is insufficient. Formal metrics wait for Phase 3.

---

## Sub-phase 2K — Config and documentation

| Task | Detail |
|------|--------|
| `configs/default.toml` | `[environment]` block; reduce or zero `flat_energy_grant_per_tick` |
| README | Note Phase 2 energy model and light modes |
| [SIMULATION.md](SIMULATION.md) | Mark `[environment]` keys as implemented |
| [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md) | Cross-link Phase 2; note overlap experiment flag if kept |

---

## Code migration map (Phase 1 → Phase 2)

| Phase 1 | Phase 2 target |
|---------|----------------|
| `flat_energy_grant_per_tick` | `solar_constant` + `LightField.sample` |
| `photo_strength` → upkeep only | Uptake multiplier in gain |
| Overlap → zero gain (impromptu) | **Off by default**; Phase 6 spatial occlusion |
| No `environment_tick` | Still no-op for pool (Phase 6) |
| `metabolism_and_gains` only | After `sense_exposure` |

### Tick pipeline (target)

```text
environment_tick     # no-op (pool recharge in Phase 6)
sense_exposure         # light sample per entity
metabolism_and_gains   # Model A gain − costs
behavior_movement
reproduction
death_and_despawn
performance_failsafe
```

---

## Risks and mitigations

| Risk | Mitigation |
|------|------------|
| Runaway population under Model A | Tune `solar_constant`, repro cost/threshold; keep `max_entities` failsafe |
| High `photo_strength` dominates all Y | Increase `k_photo`; widen `gradient_min` shade band |
| Removing flat grant kills everyone | Tune ancestor + `solar_constant` before dropping grant; keep regression test with grant for Phase 1 CI optional job |
| Overlap blocking + gradient_y double-penalizes clusters | Disable overlap by default (2F) |
| `f32` light at boundaries | Document; use same bounds as movement |
| ROADMAP lists locomotion in Phase 2 | Already done; 2G is verify-only — update ROADMAP text when merging |

---

## Definition of done

Phase 2 is **complete** when:

1. All items in the **Master checklist** are checked.
2. **Phase acceptance** in sub-phase 2J pass in `cargo test`.
3. [ROADMAP.md](ROADMAP.md) Phase 2 row is **Done**.
4. Default config uses `energy_model = "local"` and position-based gain, not flat grant.
5. Binary overlap photosynthesis blocking is **not** enabled in default config (optional flag documented if retained).
6. Any intentional deviation from [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md) is noted in that doc in the same PR.

---

## Further reading

- [ROADMAP.md](ROADMAP.md) — Phase 3+ scope
- [PHASE_1.md](PHASE_1.md) — Completed baseline
- [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md) — Models A/B/C
- [GENOME.md](GENOME.md) — `photo_strength`, locomotion loci
- [SIMULATION.md](SIMULATION.md) — Tick order and config schema

# Phase 5 — Sensors and thermal vents

Detailed implementation plan for **Phase 5** from [ROADMAP.md](ROADMAP.md). Phases 1–4 deliver genetics, local photosynthesis, headless metrics, hazards, repair, and green photosynthesis. Phase 5 adds **chemosynthetic vent niches** (threshold-gated gain) and **sensor economics** (upkeep + simple light-taxis behavior) so spatial heterogeneity extends beyond light and hazards.

**Prerequisite reading:** [SIMULATION.md](SIMULATION.md), [GENOME.md](GENOME.md), [ENTITIES.md](ENTITIES.md), [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md), [ARCHITECTURE.md](ARCHITECTURE.md), [PHASE_4.md](PHASE_4.md).

---

## Master checklist

- [x] **5A** — Vent config, `VentField` resource, `VentExposure` component, `sense_exposure` sampling
- [x] **5B** — Chemo gain in metabolism: cliff, `chemo_potential`, `chemo_sensitivity`, `chemo_upkeep` (already decoded)
- [ ] **5C** — Core sensor decode: `visual_bandwidth`, `sensory_gain`, `sensor_upkeep`; document tier fallback in [GENOME.md](GENOME.md)
- [ ] **5D** — Behavior stub: light-gradient bias in `behavior_movement` scaled by `visual_bandwidth`
- [ ] **5E** — Pipeline integration, default config with vent band, acceptance configs (`vent_lab.toml`, `sensor_ab.toml`)
- [ ] **5F** — Metrics, acceptance tests; update [ROADMAP.md](ROADMAP.md) Phase 5 → **Done**

---

## Goals and non-goals

### Goals

1. **Vent niches** — Circular vent zones supply chemosynthetic energy; only lineages with `chemo_potential` above `chemo_cliff` while overlapping a vent sustain there.
2. **Threshold tradeoff is real** — Sub-cliff `chemo_potential` still pays `chemo_upkeep` (already in metabolism) but receives **no** vent gain until the gene crosses the cliff.
3. **Sensor costs** — `sensor_upkeep` from `sensory_gain` and `visual_bandwidth` drains net energy away from vents and bright bands.
4. **Behavior stub** — Higher `visual_bandwidth` biases velocity toward brighter light (simple gradient following), not a full policy or ML controller.
5. **Phase 7 readiness** — Same exposure pattern as hazards (`*Exposure` components in sense pass); morphology nodes can replace core sensor bytes later without changing vent math.

### Explicitly out of scope (Phase 5)

| Item | Lands in |
|------|----------|
| Soup compound grid, transporters | Phase 6 |
| `energy_model = "pool"` / `"spatial"` | Phase 6 |
| Appendage tree encoding, per-node `part_hp` from morphology | Phase 7 |
| `light_mode = "map"` (2D image grid) | Phase 5+ optional stretch; defer unless needed for sensor tests |
| Vent rim barrier damage / outward force | Optional stretch; document only |
| Sexual reproduction | Phase 8 |
| Seasonal `vent_flux` modulation | Phase 9 ([PHASE_9.md](PHASE_9.md)) |
| Full spectral light, predation, feeding | Future |

---

## Current codebase (Phase 4 complete)

| Area | State |
|------|--------|
| **Tick pipeline** | `sense_exposure` → `metabolism_and_gains` → `behavior_movement` → `interactions` → `healing` → `reproduction` → `advance_age` → `death_and_despawn` → `failsafe` → `metrics_sample` ([`systems/mod.rs`](../src/sim/systems/mod.rs)) |
| **`chemo_potential` / `chemo_upkeep` / `chemo_sensitivity`** | Decoded in [`phenotype.rs`](../src/genome/phenotype.rs); gain via `chemo_gain_per_tick` in [`metabolism.rs`](../src/sim/systems/metabolism.rs) |
| **`chemo_cliff`** | In [`decode_ranges`](../src/config.rs) / [`configs/default.toml`](../configs/default.toml) (`0.35`); `Phenotype::is_chemo_above_cliff` exists |
| **Ancestor** | `chemo_potential_raw = 0` — inactive until vents and mutation |
| **`repro_cost` locus (bytes 12–13)** | Legacy / unused in v1 payment — **candidate bytes for core sensor encoding** (see §5.3) |
| **Movement** | Random velocity + cap; **no** environmental gradient bias ([`movement.rs`](../src/sim/systems/movement.rs)) |
| **Sense** | `LightExposure`, `HazardExposure`, `VentExposure` ([`sense.rs`](../src/sim/systems/sense.rs)) |
| **Vents** | [`VentField`](../src/sim/vents.rs) + `[[environment.vents]]` in config |
| **Hazard template** | [`HazardField`](../src/sim/hazards.rs) — mirrored by `VentField` |
| **Metrics** | Phase 4 fields; no `mean_chemo_potential`, `mean_vent_exposure`, or sensor aggregates |

---

## Design decisions

### 5.1 — Vent zone shape (canonical)

Match hazards ([PHASE_4.md](PHASE_4.md) §4.1): **axis-aligned circles** in world XY.

```toml
[[environment.vents]]
center = [0.0, 400.0]
radius = 160.0
vent_flux = 0.04
```

| Field | Type | Notes |
|-------|------|-------|
| `center` | `[f32; 2]` | World position |
| `radius` | f32 | Inside test: `distance(center, pos) <= radius` |
| `vent_flux` | f32 | Raw flux at center before falloff and phenotype scaling |

Empty `vents` array ⇒ no vent gain (backward compatible).

**Optional falloff (stretch):** inside radius, `flux_mult = 1 - (dist/radius)` (linear) or `1 - (dist/radius)²`. **Default for 5A:** constant flux inside circle (same as hazard damage sum) for simplicity and test stability.

### 5.2 — Chemosynthetic gain (canonical)

From [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md):

```text
vent_sample   = sum over overlapping zones of vent_flux   # or falloff-weighted
chemo_active  = chemo_potential > chemo_cliff
chemo_gain    = chemo_active
                ? chemo_potential * vent_sample * chemo_sensitivity
                : 0
```

| Symbol | Source |
|--------|--------|
| `chemo_potential` | Core loci 22–23 (`u16 / 65535`) — already decoded |
| `chemo_cliff` | `[genetics.decode_ranges].chemo_cliff` — already config |
| `chemo_upkeep` | `CHEMO_UPKEEP_SCALE * chemo_potential` when raw > 0 — already in `metabolism_tick_cost` |
| `chemo_sensitivity` | See §5.3 |
| `vent_sample` | `VentExposure.vent_flux` from sense pass |

Apply `chemo_gain` in `metabolism_and_gains` **after** photosynthetic gain, same tick as upkeeps:

```text
Δinternal_energy += photo_gain - metabolism_costs + chemo_gain
```

**Threshold invariant (unit test):** at same position inside a vent, genotype with `chemo_potential` just below cliff has `chemo_gain == 0` but `chemo_upkeep > 0` when raw > 0.

### 5.3 — Core sensors without morphology (canonical)

Morphology extension is Phase 7. Phase 5 needs **heritable** sensor investment for acceptance (“high sensor investment hurts in nutrient-poor areas”).

**Canonical encoding:** repurpose legacy **`repro_cost`** bytes (core offsets 12–13, width 2, already in `CORE_LOCI` for mutation):

| Byte | Offset | Phase 5 semantic | Decode |
|------|--------|------------------|--------|
| 12 | `REPRO_COST` | `visual_bandwidth_raw` | `raw / 255.0` → `[0, 1]` |
| 13 | `REPRO_COST + 1` | `sensory_gain_raw` | `raw / 255.0` → `[0, 1]` |

Document in [GENOME.md](GENOME.md): `repro_cost` row becomes **“legacy payment unused; Phase 5+ sensor bytes”** without bumping `GenomeVersion` (layout unchanged).

**`chemo_sensitivity` (no extra bytes):** derive from investment so vent specialists co-evolve uptake and resolution:

```text
chemo_sensitivity = clamp(chemo_potential / chemo_cliff, 0, 1)   # when chemo_active
                  = 0                                             # otherwise
```

Override via `[genetics.core_sensors].chemo_sensitivity_floor` only if tests need decoupling (default `0` = use formula above).

**Sensor upkeep (canonical):**

```text
sensor_upkeep = k_sensor_gain * sensory_gain
              + k_visual_bw * visual_bandwidth * (body_radius / r_ref)
```

Add `k_sensor_gain`, `k_visual_bw` to `[genetics.decode_ranges]` or new `[genetics.core_sensors]` table (preferred for tuning isolation).

**Tier fallback (prototyping):** optional `visual_bandwidth_tiers` in config maps bands to `eye_tier_0`…`eye_tier_3` for debug HUD only; behavior and upkeep use **continuous** values. Document in GENOME § sensory spectrum; remove tiers before Phase 7 if unused.

### 5.4 — Light-gradient behavior stub (canonical)

Run **before** velocity integration in `behavior_movement` (or split `behavior_taxis` system chained immediately before movement).

```text
ε = light_sample_epsilon   # e.g. 8 world units
light_center = light(pos)
light_x_plus = light(pos + Vec2(ε, 0))
light_y_plus = light(pos + Vec2(0, ε))
gradient ≈ Vec2(light_x_plus - light_center, light_y_plus - light_center)

if gradient.length_squared() > 1e-8:
    bias_strength = k_light_taxis * visual_bandwidth * gradient.normalize()
    vel += bias_strength   # then clamp to max_speed as today
```

| Config key | Suggested default | Role |
|------------|-------------------|------|
| `light_sample_epsilon` | `8.0` | Finite-difference step for gradient |
| `k_light_taxis` | `0.35` | Max bias speed fraction of `max_speed` at `visual_bandwidth = 1` |

**Design intent:** photo specialists in `gradient_y` worlds benefit from investing in bandwidth; vent specialists in shade are not forced to max bandwidth. High bandwidth still costs upkeep everywhere.

**Determinism:** use `LightField::sample` only (no RNG); fixed ε and iteration order.

### 5.5 — Tick pipeline (target)

Extend sense and metabolism; optional tiny behavior hook in movement:

```text
advance_sim_tick → sense_exposure → metabolism_and_gains → behavior_movement
  → interactions → healing → reproduction → advance_age → death_and_despawn
  → performance_failsafe → metrics_sample
```

| System | Change |
|--------|--------|
| `sense_exposure` | Add `VentField` → `VentExposure { vent_flux: f32 }` |
| `metabolism_and_gains` | Add `chemo_gain`; add `sensor_upkeep` to tick cost |
| `behavior_movement` | Light-gradient bias using `LightExposure` + `visual_bandwidth` |

**Prefer:** `VentExposure` component set in sense pass (mirrors `HazardExposure`).

### 5.6 — Death and metrics

No new death reasons required.

JSONL additions (backward compatible until Phase 5 ships; then required in parser tests):

| Field | Type | Description |
|-------|------|-------------|
| `mean_chemo_potential` | f32 | Population mean (4 dp) |
| `mean_vent_flux` | f32 | Mean `VentExposure.vent_flux` over agents (0 if no vents) |
| `mean_visual_bandwidth` | f32 | Population mean |
| `mean_sensor_upkeep` | f32 | Mean decoded sensor upkeep (4 dp) |
| `chemo_gain_total` | f32 | Sum chemo gain applied this snapshot period (optional, for tuning) |

---

## Sub-phase 5A — Vent config and sensing

### Deliverables

| Task | Detail |
|------|--------|
| `VentZone` struct | `center`, `radius`, `vent_flux` |
| `EnvironmentConfig` | `#[serde(default)] vents: Vec<VentZone>` |
| `VentField` resource | Built at startup (like `HazardField`) |
| `VentExposure` component | `vent_flux: f32` — sum or falloff-weighted flux at position |
| `sense_exposure` | After hazard sample, set `VentExposure` per agent |

### Code touchpoints

| File | Change |
|------|--------|
| [`config.rs`](../src/config.rs) | `VentZone`, `EnvironmentConfig.vents` |
| New [`sim/vents.rs`](../src/sim/vents.rs) | `VentField::flux_at(pos) -> f32` |
| [`sim/mod.rs`](../src/sim/mod.rs) | `pub mod vents` |
| [`sim/components.rs`](../src/sim/components.rs) | `VentExposure` component |
| [`sim/systems/sense.rs`](../src/sim/systems/sense.rs) | Sample vents |
| [`sim/app.rs`](../src/sim/app.rs) | Insert `VentField` resource |
| [`configs/default.toml`](../configs/default.toml) | One vent away from main hazard cluster |

### Acceptance (5A)

- [x] Unit test: point inside vent ⇒ `vent_flux > 0`; outside ⇒ `0`
- [x] Multiple overlapping vents sum flux (same pattern as hazards)
- [x] Config with no `vents` key loads (default empty)

---

## Sub-phase 5B — Chemo gain in metabolism

### Deliverables

| Task | Detail |
|------|--------|
| `chemo_gain_per_tick(...)` | Pure function in `metabolism.rs` or `genome/phenotype.rs` |
| `metabolism_and_gains` | Add vent gain when `VentExposure` + phenotype active |
| `Phenotype` | `chemo_sensitivity` field (derived per §5.3) |
| Tunables | `k_chemo` global scale optional in `[genetics.decode_ranges]` if raw flux too strong |

### Code touchpoints

| File | Change |
|------|--------|
| [`genome/phenotype.rs`](../src/genome/phenotype.rs) | `chemo_sensitivity`, `chemo_gain_per_tick` helper |
| [`sim/systems/metabolism.rs`](../src/sim/systems/metabolism.rs) | Query `VentExposure`; apply gain |
| [`configs/vent_lab.toml`](../configs/vent_lab.toml) (new) | Strong vent, short run, high-chemo ancestor hex |

### Acceptance (5B)

- [x] Unit test: above cliff + inside vent ⇒ positive gain; below cliff ⇒ zero gain, positive upkeep if raw > 0
- [x] Unit test: outside vent ⇒ zero gain regardless of cliff
- [x] Zero vent flux ⇒ chemo gain always 0 (regression)

---

## Sub-phase 5C — Core sensor decode and upkeep

### Deliverables

| Task | Detail |
|------|--------|
| Decode | `visual_bandwidth`, `sensory_gain` from bytes 12–13 |
| Upkeep | `sensor_upkeep` on `Phenotype`; add to `metabolism_tick_cost` |
| `CoreSensorsConfig` | Optional TOML section for `k_sensor_gain`, `k_visual_bw`, tier table |
| [GENOME.md](GENOME.md) | Update `repro_cost` row; sensory spectrum cross-link; tier fallback note |
| Ancestor | Keep bytes 12–13 mid-range (`0x40, 0x00` from `0x4000`) or tune for low sensor default |

### Code touchpoints

| File | Change |
|------|--------|
| [`genome/encoding.rs`](../src/genome/encoding.rs) | `visual_bandwidth_raw`, `sensory_gain_raw` accessors |
| [`genome/phenotype.rs`](../src/genome/phenotype.rs) | Decode + `sensor_upkeep` |
| [`config.rs`](../src/config.rs) | `CoreSensorsConfig` |
| [`genome/mutation.rs`](../src/genome/mutation.rs) | Rename comments for `REPRO_COST` locus (still same offsets) |

### Acceptance (5C)

- [ ] Unit test: higher byte 12 ⇒ higher `visual_bandwidth` and `sensor_upkeep`
- [ ] Unit test: `sensor_upkeep` strictly increasing in `sensory_gain` and `visual_bandwidth`
- [ ] Mutation on bytes 12–13 changes decoded sensors (existing `CORE_LOCI` coverage)

---

## Sub-phase 5D — Light-gradient behavior stub

### Deliverables

| Task | Detail |
|------|--------|
| `apply_light_taxis` | Function in `movement.rs` using `LightField` + `LightExposure` |
| Config | `light_sample_epsilon`, `k_light_taxis` under `[genetics.core_sensors]` or `[environment]` |
| Clamp | Bias applied before `clamp_velocity_to_max_speed` |

### Code touchpoints

| File | Change |
|------|--------|
| [`sim/systems/movement.rs`](../src/sim/systems/movement.rs) | Taxis bias; pass `Res<LightField>` |
| [`sim/systems/sense.rs`](../src/sim/systems/sense.rs) | Ensure `LightExposure` set before movement (already true) |

### Acceptance (5D)

- [ ] Unit test: agent at fixed position in `gradient_y` with `visual_bandwidth = 0` ⇒ velocity unchanged by bias
- [ ] Unit test: same setup with `visual_bandwidth = 1` ⇒ velocity component toward brighter neighbor increases (sign check vs `gradient_bright_at`)
- [ ] Headless smoke: default config 10k ticks still viable (taxis not strong enough to eject all agents from bounds)

---

## Sub-phase 5E — Selection pressure and configs

### Deliverables

| Task | Detail |
|------|--------|
| `configs/vent_lab.toml` | Vent flux high; ancestor with `chemo_potential` above cliff; low photo |
| `configs/sensor_ab.toml` | Two ancestor hex strings: high `visual_bandwidth` vs low, equal other loci |
| Default vent placement | Complement `gradient_y` (e.g. vent in dim lower band, light at top) |
| Visual debug (optional) | Vent circles in `VisualDebugPlugin` (mirror hazard gizmos) |

### Acceptance (5E)

- [ ] Headless A/B: high-chemo clone sustains net positive energy at vent; zero-chemo ancestor does not (5k ticks, `vent_lab.toml`)
- [ ] Headless A/B: high-chemo clone **loses** net energy vs control when held outside all vents (upkeep, no gain)
- [ ] Headless A/B: high-bandwidth vs low-bandwidth in nutrient-poor region (uniform low light config) — high bandwidth lower mean energy
- [ ] Default `configs/default.toml` + vents enabled: population survives 10k ticks

---

## Sub-phase 5F — Metrics, acceptance, documentation

### Phase acceptance table (from [ROADMAP.md](ROADMAP.md))

| Criterion | How to verify |
|-----------|----------------|
| Vent-adjacent niche sustains invested `chemo_potential` | `vent_lab` harness: high-chemo vs zero-chemo |
| Uninvested genotypes do not sustain at vents | Same harness; mean energy diverges |
| High sensor investment hurts in nutrient-poor areas | `sensor_ab` harness; mean energy / `mean_sensor_upkeep` |
| Headless determinism preserved | Same seed + config ⇒ identical metrics hash (new fields 4 dp) |
| Phase 2–4 regression | `k_green_photo = 0` configs; no-vent configs unchanged energy |

### Suggested tests

| Test | Module |
|------|--------|
| `vent_flux_inside_zone` | `sim::vents` or `sense` |
| `chemo_gain_requires_cliff_and_vent` | `genome::phenotype` or `metabolism` |
| `chemo_upkeep_without_gain_below_cliff` | `metabolism` |
| `sensor_upkeep_scales_with_bandwidth` | `genome::phenotype` |
| `light_taxis_zero_when_bandwidth_zero` | `movement` |
| `light_taxis_toward_brighter_in_gradient_y` | `movement` |
| `chemo_vent_survival_tradeoff` | `sim::acceptance` |
| `high_sensor_cost_in_low_light` | `sim::acceptance` |
| `headless_metrics_include_phase5_fields` | `sim::metrics` |
| `population_stable_10000_ticks_with_vents` | `sim::acceptance` |

### Documentation maintenance (same PR as implementation)

| Doc | Update |
|-----|--------|
| [ROADMAP.md](ROADMAP.md) | Phase 5 → **Done**; link this file |
| [SIMULATION.md](SIMULATION.md) | `vents` implemented; sense row; metabolism chemo + sensor upkeep |
| [GENOME.md](GENOME.md) | `repro_cost` sensor bytes; `chemo_potential` active; tier fallback |
| [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md) | Thermal vents section → **implemented** |
| [ENTITIES.md](ENTITIES.md) | `VentExposure`; live tick chemo gain |
| [ARCHITECTURE.md](ARCHITECTURE.md) | `sim/vents.rs`; metrics fields |
| [PHASE_4.md](PHASE_4.md) | Out-of-scope table already points here |
| [README.md](../README.md) | Brief vent + sensor config example |

---

## Code migration map (Phase 4 → Phase 5)

| Phase 4 | Phase 5 target |
|---------|----------------|
| Sense: light + hazard | + vent flux |
| Metabolism: photo + upkeeps incl. `chemo_upkeep` | + `chemo_gain`, + `sensor_upkeep` |
| Movement: passive velocity | + light-gradient bias × `visual_bandwidth` |
| `repro_cost` ignored | Bytes 12–13 → `visual_bandwidth`, `sensory_gain` |
| `chemo_potential` upkeep only | + vent gain above cliff |
| Metrics: Phase 4 aggregates | + chemo, vent, sensor fields |
| Tick chain (10 systems) | Same count; extend 3 modules |

### Config migration

Add to `[environment]`:

```toml
[[environment.vents]]
center = [0.0, -450.0]
radius = 150.0
vent_flux = 0.032
```

Add `[genetics.core_sensors]` (example):

```toml
[genetics.core_sensors]
k_sensor_gain = 0.006
k_visual_bw = 0.010
light_sample_epsilon = 8.0
k_light_taxis = 0.35
# visual_bandwidth_tiers = [0.0, 0.25, 0.5, 0.75]  # optional debug only
```

Optional acceptance ancestor patch (core bytes 22–23 only; keep header):

```toml
# chemo_potential ≈ 0.55 (above cliff 0.35) — adjust hex in vent_lab.toml
```

Regression: `vents = []` and bytes 12–13 = `0` restores Phase 4 energy behavior (aside from sensor decode of legacy `0x4000` — set ancestor bytes 12–13 to `0x00` in regression configs if needed).

---

## Risks and mitigations

| Risk | Mitigation |
|------|------------|
| Vents + bright top dominate runaway growth | Tune `vent_flux` below typical photo gain at `gradient_max`; chemo upkeep on partial investment |
| Repurposing `repro_cost` confuses readers | Single GENOME row + PHASE_5 §5.3; no version bump |
| Taxis ejects agents from world | Keep `k_light_taxis` modest; bounce bounds unchanged |
| Chemo specialists extinct globally | Place vent in viable dim band; 10k-tick default config test |
| O(n²) unchanged | Vents sampled O(zones) per agent like hazards |
| Metrics schema drift | Add fields in one PR; note “Phase 5 extensions” in [PHASE_3.md](PHASE_3.md) |
| Below-cliff chemo raw = 0 vs upkeep edge case | Document: raw `0` ⇒ no upkeep (existing); test raw low but above 0 below cliff |

---

## Further reading

- [ROADMAP.md](ROADMAP.md) — Phase overview and acceptance summary
- [PHASE_4.md](PHASE_4.md) — Hazard / sense patterns to mirror
- [PHASE_2.md](PHASE_2.md) — Light field and gradient context for taxis
- [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md) — Vent and sensor upkeep formulas
- [PHASE_9.md](PHASE_9.md) — Seasonal `vent_flux` scale (future)

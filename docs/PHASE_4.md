# Phase 4 — Hazards, armor, per-part health, green photosynthesis

Detailed implementation plan for **Phase 4** from [ROADMAP.md](ROADMAP.md). Phases 1–3 deliver genetics, local photosynthesis, headless metrics, and evolvable lifespan. Phase 4 turns **latent** loci (`skin_hardness`, `repair_efficiency`) and **documented** interaction slots into active ecology: localized hazard niches, collision damage, energy-funded repair, and **fitness-linked color** (greenness buffs photosynthesis with a smaller metabolism penalty).

**Prerequisite reading:** [SIMULATION.md](SIMULATION.md), [GENOME.md](GENOME.md), [ENTITIES.md](ENTITIES.md), [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md), [ARCHITECTURE.md](ARCHITECTURE.md), [PHASE_3.md](PHASE_3.md).

---

## Master checklist

- [x] **4A** — Hazard zones: config schema, `HazardField` resource, sense → exposure component
- [x] **4B** — Damage application: zone hazards + collision (relative speed, mitigation)
- [x] **4C** — Health model: `part_hp` vector (core-only until Phase 7); damage routing
- [x] **4D** — Healing: repair policies, energy cost via `repair_efficiency`
- [x] **4E** — Pipeline integration: `interactions` + `healing` systems; tick order
- [x] **4G** — Green photosynthesis niche: greenness decode, gain buff, metabolism penalty
- [x] **4F** — Metrics, acceptance tests, default config; update [ROADMAP.md](ROADMAP.md) Phase 4 → **Done**

---

## Goals and non-goals

### Goals

1. **Environmental heterogeneity** — Configurable circular hazard zones apply ongoing damage so no single genotype dominates all map regions (complements Phase 2 `gradient_y`).
2. **Armor tradeoff is real** — `skin_hardness` reduces incoming damage via documented mitigation while `armor_upkeep` and `move_penalty` (already decoded) remain costly in safe zones.
3. **Collision as ecology** — Entity overlap applies damage from relative speed; **no** instant despawn (Phase 1 policy preserved).
4. **Repair loop** — Agents spend `internal_energy` to restore `core_hp` / `part_hp` per policy; high `repair_efficiency` lowers cost.
5. **Phase 7 readiness** — `Health.part_hp` is a `Vec<f32>` aligned with future morphology nodes; Phase 4 uses length `0` or `1` (core-only alias).
6. **Green photosynthesis niche** — Color loci affect fitness: greener agents gain more from local photosynthesis in bright bands but pay a smaller metabolism tax so shade/low-light strategies remain viable.

### Explicitly out of scope (Phase 4)

| Item | Lands in |
|------|----------|
| Thermal vents, `chemo_potential` gain | Phase 5 |
| Soup compounds, pool / spatial energy | Phase 6 |
| Morphology tree, appendage rendering, per-node exposure | Phase 7 |
| Predation, absorption, feeding | Future |
| Sexual reproduction | Phase 8 |
| Full spectral light / chlorophyll model | — |
| Hazard **gizmos** in visual mode | Optional stretch (debug overlay only) |

---

## Current codebase (Phase 4 complete)

| Area | State |
|------|--------|
| **Tick pipeline** | `sense_exposure` → `metabolism_and_gains` → `behavior_movement` → `interactions` → `healing` → `reproduction` → `advance_age` → `death_and_despawn` → `failsafe` → `metrics_sample` ([`systems/mod.rs`](../src/sim/systems/mod.rs)) |
| **`skin_hardness`** | Active: `armor_upkeep`, `move_penalty`, `damage_mitigation` on hazard/collision damage |
| **`repair_efficiency`** | `healing` system spends energy per `repair_policy`; tracked in `RepairStats` → JSONL `repair_energy_spent` |
| **`Health.part_hp`** | Empty at spawn; core-only damage routing until Phase 7 |
| **Collisions** | Symmetric relative-speed damage in `interactions`; no instant despawn |
| **`environment.hazards`** | `HazardField` + `HazardExposure` + per-tick core damage |
| **Death reasons** | Hazard/collision HP loss → `CoreDestroyed` when `core_hp <= 0` |
| **Metrics** | Phase 3 fields + `mean_skin_hardness`, `mean_core_hp_frac`, `repair_energy_spent`, `mean_greenness` |
| **Color loci** | **Tradeoff:** greenness buffs photosynthesis, small metabolism penalty ([`color_photosynthesis`](../configs/default.toml)) |

---

## Design decisions

### 4.1 — Hazard zone shape (canonical)

Match vents (Phase 5) and docs: **axis-aligned circles** in world XY.

```toml
[[environment.hazards]]
center = [0.0, -400.0]
radius = 180.0
damage_per_tick = 2.5
```

| Field | Type | Notes |
|-------|------|-------|
| `center` | `[f32; 2]` | World position |
| `radius` | f32 | Overlap test: `distance(center, pos) <= radius` |
| `damage_per_tick` | f32 | Applied while overlapping, **before** mitigation |

Empty `hazards` array ⇒ no zone damage (backward compatible).

### 4.2 — Damage mitigation (canonical)

From [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md):

```text
mitigation(h) = h / (h + k_mitigate)     # h = skin_hardness ∈ [0, 1]
damage_applied = raw_damage * (1 - mitigation)
```

Add `k_mitigate` to `[genetics.decode_ranges]` (suggested default `0.5`). Expose on `Phenotype` as `damage_mitigation` for systems and tests.

**Zone hazard:** `raw = damage_per_tick` per overlapping zone (sum if multiple zones overlap).

**Collision:** For each unordered pair overlapping this tick:

```text
relative_speed = |v_a - v_b|   # or closing speed along separation; document choice in code
raw_collision = k_collision * relative_speed
damage_applied = raw_collision * (1 - mitigation_self)
```

Add `k_collision` to decode ranges or `[economy]` (suggested: `economy.collision_damage_scale = 0.15`).

**Pair handling:** Apply damage to **both** entities once per pair per tick (symmetric). Use deterministic pair iteration (sort entity ids) for reproducibility.

### 4.3 — Damage routing without morphology

| Morphology | Route |
|------------|--------|
| Absent (`part_hp` empty) | All damage to `core_hp` |
| Present (Phase 7+) | Appendages by exposure, overflow to core |

Phase 4: keep `part_hp.len() == 0`; optionally initialize `part_hp = vec![core_max_hp]` with a single slot mirroring core — **prefer empty vec + core-only** to avoid dual bookkeeping until Phase 7.

### 4.4 — Repair policies (canonical)

Config: `[economy] repair_policy = "core_first"` (default).

| Policy | Behavior per tick (if energy available) |
|--------|----------------------------------------|
| `core_first` | Repair `core_hp` deficit first; then parts if any |
| `cheapest` | Repair smallest HP deficit across core + parts for one “unit” of repair |
| `symmetric` | Round-robin across damaged slots |

Cost per HP repaired:

```text
energy_cost = (hp_repaired / repair_efficiency) * repair_energy_per_hp
```

Clamp repair so `internal_energy` does not go negative; stop when energy exhausted. Repair runs **after** damage, **before** reproduction ([SIMULATION.md](SIMULATION.md) order).

### 4.5 — Tick pipeline (target)

Insert new systems without reordering existing death/metrics logic:

```text
advance_sim_tick → sense_exposure → metabolism_and_gains → behavior_movement
  → interactions → healing → reproduction → advance_age → death_and_despawn
  → performance_failsafe → metrics_sample
```

| System | Role |
|--------|------|
| `sense_exposure` | Extend: keep `LightExposure`; add `HazardExposure { damage_raw: f32 }` or sample hazards inline in `interactions` |
| `interactions` | Zone damage + collision damage → subtract `core_hp` |
| `healing` | Spend energy, restore HP per policy |

**Design choice:** Either extend `sense_exposure` to write hazard overlap sum, or compute overlap in `interactions`. **Prefer:** `HazardExposure` component set in sense pass (mirrors light) for tests and future vent sampling.

### 4.7 — Green photosynthesis niche (canonical)

Follow-up to Phase 2 Model A; uses existing color bytes 0–2 (no new loci).

**Greenness** (from decoded `color_r`, `color_g`, `color_b` ∈ [0, 1]):

```text
green_excess = g - 0.5 * (r + b)
greenness    = clamp(green_excess / greenness_scale, 0, greenness_max)
```

| Config key | Role |
|------------|------|
| `greenness_scale` | Denominator; e.g. `0.5` so strong green → `greenness` ≈ 1 |
| `greenness_max` | Clamp upper bound (default `1.0`) |

**Per-tick effects** (applied in `metabolism_and_gains` after base photo gain computed):

```text
photo_gain      = photo_strength * light(pos) * solar_constant   # existing
photo_gain     *= 1 + k_green_photo * greenness
metabolism_idle += k_green_meta * greenness   # additive penalty; k_green_meta < k_green_photo * light_ref
```

| Config key | Suggested role |
|------------|----------------|
| `k_green_photo` | Photosynthesis multiplier strength (e.g. `0.25`) |
| `k_green_meta` | Extra idle metabolism per unit greenness (e.g. `0.12`) |
| `light_ref` | Reference light for acceptance inequality (e.g. `gradient_max` = `1.0`) |

**Tradeoff invariant (unit test):** at `light = light_ref`, extra gain per tick from greenness is strictly greater than extra metabolism cost:

```text
k_green_photo * light_ref * photo_strength_ref * solar_constant > k_green_meta
```

Use `photo_strength_ref = 1.0` or ancestor mean in test; document chosen reference in test comment.

**Design intent**

- Selection can favor greenish lineages in high-light bands without decoupling color from fitness entirely.
- Shade / low-light lineages should still favor low-photo, non-green strategies when upkeep dominates.

**Config placement:** prefer `[genetics.color_photosynthesis]` in TOML (optional section; defaults disable effect with `k_green_photo = 0` for regression configs).

**GENOME.md:** retag bytes 0–2 from **Neutral** → **Tradeoff**; note greenness formulas.

### 4.6 — Death and metrics

- Hazard/collision damage that reduces `core_hp <= 0` uses existing `DeathReason::CoreDestroyed` (no new enum variant required).
- Add cumulative resource `RepairStats { energy_spent_this_period: f32 }` for metrics.
- JSONL additions (backward compatible: new fields optional in parser tests, **required** once Phase 4 ships):

| Field | Type | Description |
|-------|------|-------------|
| `mean_skin_hardness` | f32 | Population mean (4 dp) |
| `mean_core_hp_frac` | f32 | `core_hp / core_max_hp` mean |
| `repair_energy_spent` | f32 | Sum since last snapshot |

---

## Sub-phase 4A — Hazard config and sensing

### Deliverables

| Task | Detail |
|------|--------|
| `HazardZone` struct | `center`, `radius`, `damage_per_tick` |
| `EnvironmentConfig` | `#[serde(default)] hazards: Vec<HazardZone>` |
| `HazardField` resource | Built at app startup from config (like `LightField`) |
| `HazardExposure` component | `damage_raw: f32` — sum of `damage_per_tick` for all overlapping zones |
| `sense_exposure` | After light sample, set `HazardExposure` per agent |

### Code touchpoints

| File | Change |
|------|--------|
| [`config.rs`](../src/config.rs) | `HazardZone`, `EnvironmentConfig.hazards` |
| [`sim/light.rs`](../src/sim/light.rs) or new `sim/hazards.rs` | `HazardField::damage_at(pos) -> f32` |
| [`sim/components.rs`](../src/sim/components.rs) | `HazardExposure` component |
| [`sim/systems/sense.rs`](../src/sim/systems/sense.rs) | Sample hazards |
| [`configs/default.toml`](../configs/default.toml) | One hazard band (e.g. bottom third) for acceptance |

### Acceptance (4A)

- [x] Unit test: point inside zone ⇒ `damage_raw > 0`; outside ⇒ `0`
- [x] Multiple overlapping zones sum damage
- [x] Config with no `hazards` key loads (default empty)

---

## Sub-phase 4B — Interactions (hazard + collision damage)

### Deliverables

| Task | Detail |
|------|--------|
| `interactions` system | New module [`sim/systems/interactions.rs`](../src/sim/systems/interactions.rs) |
| Zone damage | `apply = hazard_exposure.damage_raw * (1 - mitigation)` → `core_hp` |
| Collision detection | Broad phase: spatial hash or O(n²) with pop cap — **start O(n²)** if `max_entities` ≤ 5000; document upgrade path |
| Collision damage | Symmetric pair damage from relative speed |
| Mitigation | `Phenotype::damage_mitigation()` from `skin_hardness` + `k_mitigate` |

### Code touchpoints

| File | Change |
|------|--------|
| [`genome/phenotype.rs`](../src/genome/phenotype.rs) | `damage_mitigation`, decode uses `k_mitigate` |
| [`config.rs`](../src/config.rs) | `k_mitigate`, `collision_damage_scale` |
| [`sim/systems/interactions.rs`](../src/sim/systems/interactions.rs) | New |
| [`sim/systems/mod.rs`](../src/sim/systems/mod.rs) | Chain after `behavior_movement` |

### Acceptance (4B)

- [x] Unit test: mitigation increases with hardness; zero hardness ⇒ full raw damage
- [x] Two overlapping agents with non-zero relative velocity lose `core_hp` (no despawn until death system)
- [x] `no_collision_or_tick_despawn_outside_death_system` still passes ([`death.rs`](../src/sim/systems/death.rs))
- [x] Deterministic collision: same seed + positions ⇒ same HP after tick

---

## Sub-phase 4C — Per-part health (core-only)

### Deliverables

| Task | Detail |
|------|--------|
| Spawn | `Health { core_hp: core_max_hp, core_max_hp, part_hp: [] }` unchanged |
| Damage | Only `core_hp` mutated in Phase 4 |
| Docs | [ENTITIES.md](ENTITIES.md): Phase 4 = core routing; Phase 7 fills `part_hp` from morphology |
| Optional helper | `Health::apply_damage_to_core(amount)` for interactions/healing |

### Acceptance (4C)

- [x] `part_hp` remains empty for all spawned agents in default run
- [x] Integration test stub: when `part_hp = vec![50.0]` manually in test, damage policy documented for 4D `cheapest` / `symmetric`

---

## Sub-phase 4D — Healing (repair)

### Deliverables

| Task | Detail |
|------|--------|
| `RepairPolicy` enum | `core_first`, `cheapest`, `symmetric` in config |
| `healing` system | After `interactions`, before `reproduction` |
| Energy spend | Track in `RepairStats` resource for metrics |
| Clamp | Do not repair below max; do not spend more energy than available |

### Code touchpoints

| File | Change |
|------|--------|
| [`economy` config](../src/config.rs) | `repair_policy` |
| [`sim/systems/healing.rs`](../src/sim/systems/healing.rs) | New |
| [`sim/metrics.rs`](../src/sim/metrics.rs) | `repair_energy_spent`, `mean_skin_hardness`, `mean_core_hp_frac` |

### Acceptance (4D)

- [x] Unit test: damaged core + energy ⇒ `core_hp` increases, energy decreases
- [x] High `repair_efficiency` genotype repairs same HP for less energy than low efficiency (equal other loci)
- [x] With hazards on and repair off (policy disabled or zero energy), population in hazard dies faster than safe zone control

---

## Sub-phase 4E — Selection pressure and tuning (4F prep)

### Deliverables

| Task | Detail |
|------|--------|
| Ancestor | Consider mid `skin_hardness` (`0x4000`) in hazard acceptance configs only; keep default ancestor soft (`0`) for Phase 3 regression parity unless global retune |
| Mutation | `skin_hardness` and `repair_efficiency` already in `CORE_LOCI` — verify weights |
| Metabolism | Confirm `armor_upkeep` already in drain ([`metabolism.rs`](../src/sim/systems/metabolism.rs)) |
| Visual debug (optional) | Semi-transparent hazard circles in `VisualDebugPlugin` |

### Acceptance (4E)

- [x] Headless A/B: high-hardness clone survives longer in hazard at equal starting energy than softness (harness test)
- [x] High-hardness clone has lower mean net energy in **safe** zone (no hazard overlap) due to upkeep + move penalty
- [x] Default config + one hazard: population survives 10k ticks (same bar as Phase 2/3)

---

## Sub-phase 4G — Green photosynthesis niche

### Deliverables

| Task | Detail |
|------|--------|
| `ColorPhotosynthesisConfig` | `greenness_scale`, `greenness_max`, `k_green_photo`, `k_green_meta`, `light_ref` (serde defaults: effect off when coeffs 0) |
| Phenotype | `greenness`, `green_photo_mult`, `green_metabolism_extra` (or compute inline in metabolism) |
| `decode_phenotype` | `greenness` from `color_*` after existing color decode |
| `metabolism_and_gains` | Multiply photo gain by `(1 + k_green_photo * greenness)`; add `k_green_meta * greenness` to idle drain |
| [GENOME.md](GENOME.md) | Color loci **Tradeoff**; formulas cross-link |
| Optional metrics | `mean_greenness` in JSONL (4 dp) for tuning |

### Code touchpoints

| File | Change |
|------|--------|
| [`config.rs`](../src/config.rs) | `ColorPhotosynthesisConfig` under `GeneticsConfig` |
| [`genome/phenotype.rs`](../src/genome/phenotype.rs) | `greenness` + helpers |
| [`sim/systems/metabolism.rs`](../src/sim/systems/metabolism.rs) | Apply buff and penalty |
| [`configs/default.toml`](../configs/default.toml) | `[genetics.color_photosynthesis]` tuned for subtle selection pressure |
| `configs/green_ab.toml` (optional) | Acceptance: high-G vs low-G ancestor hex, 5k ticks |

### Acceptance (4G)

- [x] Unit test: `greenness` increases when G↑ and R,B↓ at fixed other bytes
- [x] Unit test: `green_metabolism_penalty < green_photo_buff` at `light_ref` per §4.7 invariant
- [x] Headless A/B: same seed, high-G/low-RB genome vs low-G control (other loci equal) → higher mean energy in bright band over 5k ticks
- [x] High-G control pays strictly higher mean metabolism drain per tick than low-G
- [x] Metrics: `color_bins` and `mean_energy` diverge between green A/B configs (same seed; survivor color keys)
- [x] Default population still viable 10k ticks with green coeffs enabled (`population_stable_10000_ticks_gradient_default_config`)

### Tests (4G)

| Test | Module |
|------|--------|
| `greenness_increases_with_g_over_rb` | `genome::phenotype` |
| `green_penalty_less_than_buff_at_light_ref` | `genome::phenotype` |
| `green_ancestor_higher_net_energy_in_bright_band` | `sim::acceptance` |

---

## Sub-phase 4F — Metrics, acceptance, documentation

### Phase acceptance table (from [ROADMAP.md](ROADMAP.md))

| Criterion | How to verify |
|-----------|----------------|
| High-hardness survives hazard longer | Integration: spawn clones in hazard, compare time-to-death |
| Safe-zone slowdown | Same clones, no hazard: high-hardness lower mean energy / speed |
| Repair drains energy in metrics | JSONL `repair_energy_spent > 0` when damage occurs |
| Headless determinism preserved | Same seed → identical metrics hash (new fields rounded 4 dp) |
| No collision despawn | Existing death tests + overlap scenario |
| Green niche A/B | High-G vs low-G: energy + metabolism per ROADMAP / §4G |
| Green tradeoff inequality | Unit test §4.7 invariant |
| Metrics diverge by greenness | 5k-tick seeded runs, `color_bins` / `mean_energy` |

### Suggested tests

| Test | Module |
|------|--------|
| `mitigation_increases_with_hardness` | `genome::phenotype` |
| `hazard_damage_inside_zone` | `sim::hazards` or `sense` |
| `collision_reduces_core_hp` | `sim::systems::interactions` |
| `repair_cost_scales_with_efficiency` | `sim::systems::healing` |
| `hardness_hazard_survival_tradeoff` | `sim::acceptance` |
| `headless_metrics_include_phase4_fields` | `sim::metrics` |
| `greenness_increases_with_g_over_rb` | `genome::phenotype` |
| `green_penalty_less_than_buff_at_light_ref` | `genome::phenotype` |
| `green_ancestor_higher_net_energy_in_bright_band` | `sim::acceptance` |

### Documentation maintenance (same PR as implementation)

| Doc | Update |
|-----|--------|
| [ROADMAP.md](ROADMAP.md) | Phase 4 → **Done**; link this file |
| [SIMULATION.md](SIMULATION.md) | `hazards` implemented; pipeline steps 5–6 |
| [GENOME.md](GENOME.md) | `skin_hardness` active; color loci **Tradeoff** + greenness; remove “Phase 4+” on ancestor table |
| [PHASE_3.md](PHASE_3.md) | Out-of-scope note: color fitness → Phase 4 (if mentioned) |
| [ENTITIES.md](ENTITIES.md) | Interactions phase column; repair in live tick |
| [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md) | Hazards section status → implemented |
| [ARCHITECTURE.md](ARCHITECTURE.md) | System chain; metrics fields |
| [README.md](../README.md) | Brief hazard config example |

---

## Code migration map (Phase 3 → Phase 4)

| Phase 3 | Phase 4 target |
|---------|----------------|
| Sense: light only | + hazard exposure |
| Movement: overlap ignored | Collision damage in `interactions` (still no despawn) |
| `armor_upkeep` only | + mitigation on damage |
| `repair_efficiency` decode only | + `healing` system |
| `part_hp` always `[]` | Documented; optional repair on parts in tests |
| Metrics: lifespan fields | + hardness, core HP frac, repair spend, optional `mean_greenness` |
| Color: cosmetic only | Greenness → photo buff + metabolism penalty |
| Tick chain (8 systems) | + `interactions`, `healing` (10 systems) |

### Config migration

Add to `[environment]`:

```toml
[[environment.hazards]]
center = [0.0, -500.0]
radius = 200.0
damage_per_tick = 1.8
```

Add to `[genetics.decode_ranges]`:

```toml
k_mitigate = 0.5
```

Add to `[economy]`:

```toml
repair_policy = "core_first"
collision_damage_scale = 0.15
```

Optional acceptance-only config: `configs/hazard_lab.toml` with extreme hazard + short run for CI.

Add `[genetics.color_photosynthesis]` (example):

```toml
[genetics.color_photosynthesis]
greenness_scale = 0.5
greenness_max = 1.0
k_green_photo = 0.25
k_green_meta = 0.12
light_ref = 1.0
```

Set `k_green_photo = 0` and `k_green_meta = 0` in regression configs to preserve Phase 2–3 energy behavior.

---

## Risks and mitigations

| Risk | Mitigation |
|------|------------|
| O(n²) collision too slow | Cap pairs per tick or grid hash if profiling fails; population failsafe already limits N |
| Hazard + gradient kills everyone | Tune `damage_per_tick` below typical regen; repair enabled; hazard in minority of map |
| Armor becomes mandatory | Keep ancestor soft; `k_armor` / `k_mitigate` tunable; safe-zone energy penalty |
| Determinism breaks on collision order | Sort entity pairs by `(min_id, max_id)` before applying damage |
| Metrics schema drift | Add fields in one PR; update PHASE_3 schema section with “Phase 4 extensions” |
| `part_hp` dual core bookkeeping | Defer non-empty `part_hp` until Phase 7 morphology spawn |
| Green sweeps all bright niches | Keep `k_green_meta` close to but below buff at `light_ref`; shade still punishes high-photo green via upkeep |
| Ancestor already greenish | Default hex `88 CC 44` gains buff — retune `k_*` so default 10k-tick viability holds |

---

## Further reading

- [ROADMAP.md](ROADMAP.md) — Phase overview and acceptance summary
- [PHASE_3.md](PHASE_3.md) — Headless pipeline and metrics baseline
- [PHASE_2.md](PHASE_2.md) — Light field / niche context hazards complement
- [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md) — Damage and repair formulas

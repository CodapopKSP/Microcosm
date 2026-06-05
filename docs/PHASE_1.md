# Phase 1 — Core genetics and lifecycle

Detailed implementation plan for **Phase 1** from [ROADMAP.md](ROADMAP.md). Phase 0 ([`src/main.rs`](../src/main.rs)) is complete; this phase replaces placeholder collision death with heritable traits, energy, health, and asexual reproduction.

**Prerequisite reading:** [VISION.md](VISION.md), [GENOME.md](GENOME.md), [ENTITIES.md](ENTITIES.md), [SIMULATION.md](SIMULATION.md), [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md), [ARCHITECTURE.md](ARCHITECTURE.md).

---

## Master checklist

Use this list for PR tracking. Sub-phases below expand each item.

- [x] **1A** — Dependencies, `SimConfig` TOML load, `configs/default.toml`, optional `master_seed`
- [x] **1B** — Genome v1 encoding (header + 28-byte core), validation, ancestor template
- [x] **1C** — `Phenotype` decode with tradeoff formulas and trait upkeeps (no morphology extension)
- [x] **1D** — ECS components: `Genome`, `Phenotype`, `Energy`, `Health`, `Lineage`, `Cooldowns`
- [x] **1E** — Spawn pipeline: decode, stillborn handling, genome-driven color and `body_radius`
- [x] **1F** — Metabolism tick: base metabolism, trait upkeeps, **flat energy grant** (no photosynthesis yet)
- [x] **1G** — Movement: `max_speed` / `move_cost` from phenotype; bounds bounce; **no collision despawn**
- [x] **1H** — Asexual reproduction: cost, endowment, cooldown, mutations, repro risk roll
- [x] **1I** — Death: `internal_energy <= 0`, `core_hp <= 0`, repro fatality; remove `resolve_collisions` despawn
- [x] **1J** — `max_entities` failsafe stub (`stop_spawns` minimum)
- [x] **1K** — RNG sub-streams keyed off `master_seed` (spawn, mutation, repro risk)
- [x] **1L** — Acceptance: population survives 3k+ ticks; repro-risk A/B; manual diversity check

---

## Goals and non-goals

### Goals

1. **Heritable variation** — Fixed-layout core genome mutates on reproduction; phenotype drives stats and appearance.
2. **Lifecycle without collision hacks** — Entities die from energy, core HP, or reproduction risk only.
3. **Tradeoffs before environment** — Metabolism, photo-related upkeeps (from genes), repro cost/endowment, and repro risk create selective pressure even before light/photosynthesis (Phase 2).
4. **Config-driven tuning** — World size, population, economy floors, and genetics live in TOML, not hard-coded constants in `main.rs`.
5. **Seeded stochasticity** — Optional `master_seed` for replayable mutation and repro-risk sequences.

### Explicitly out of scope (Phase 1)

| Item | Lands in |
|------|----------|
| Photosynthesis, light field, `energy_model` environmental gain | Phase 2 |
| `clap` CLI, headless mode, JSONL metrics | Phase 3 |
| Hazards, collision damage, `skin_hardness` effects, per-part HP | Phase 4 |
| Vents, chemo gain, sensors | Phase 5 |
| Soup, spatial/pool energy | Phase 6 |
| Morphology extension, appendage rendering | Phase 7 |
| Sexual reproduction | Phase 8 |
| Full `lib.rs` module split (acceptable as incremental refactor in 1A) | Phase 3 target layout |

---

## Sub-phase 1A — Config and project wiring

### Deliverables

| Task | Detail |
|------|--------|
| Add crates | `serde`, `serde_derive` (or `serde` with derive feature), `toml` in [`Cargo.toml`](../Cargo.toml) |
| `config.rs` | `SimConfig` struct mirroring [SIMULATION.md](SIMULATION.md) sections needed for Phase 1 |
| `configs/default.toml` | Checked-in defaults; document in README how to run with config |
| Load at startup | `main.rs` reads path (hard-coded `configs/default.toml` until Phase 3 CLI) |
| `master_seed` | Optional `u64` in `[run]`; if absent, use `rand::thread_rng()` for nondeterministic runs |

### `SimConfig` sections (Phase 1 minimum)

```toml
[world]
width = 900.0
height = 700.0
boundary = "bounce"          # bounce | wrap
initial_population = 80

[performance]
max_entities = 5000
failsafe_policy = "stop_spawns"   # stop_spawns | despawn_lowest_energy | pause

[genetics]
genome_version = 1
core_length = 32
ancestor_hex = "..."         # full 32-byte genome hex including EVO1 header
p_bit_flip = 0.02
p_point = 0.15
p_resample = 0.05
p_dup = 0.0                  # extension-only in later phases; 0 ok for v1 core
p_del = 0.0
mutations_per_offspring = 2
delta_max = 8

[economy]
metabolism_floor = 0.1
spawn_energy = 50.0          # initial internal_energy per entity
flat_energy_grant_per_tick = 0.8   # Phase 1 substitute for photosynthesis
core_max_hp = 100.0
repro_safety_margin = 0.25
p_death_on_repro_base = 0.15
repair_energy_per_hp = 1.0   # unused until healing in Phase 4; include for schema stability

[genetics.decode_ranges]
# min/max for lerp_u16 and radius loci — see GENOME.md table
body_radius_min = 6.0
body_radius_max = 20.0
photo_max = 1.0
meta_min = 0.05
meta_max = 2.0
# ... remaining loci ranges as documented in GENOME.md

[run]
master_seed = 42             # optional; comment out for random runs
```

### Decode range keys

Expose `min`/`max` per locus from config (or nested `[genetics.decode_ranges]`) so tuning does not require recompiling. Reference table: [GENOME.md — Core loci](GENOME.md#core-loci-table-v1).

### Acceptance (1A)

- [x] `cargo run` loads `configs/default.toml` without panic
- [x] Invalid TOML or wrong `genome_version` fails fast with readable error
- [x] World bounds and `initial_population` come from config, not `const` in `main.rs`

---

## Sub-phase 1B — Genome encoding

### Deliverables

| Task | Detail |
|------|--------|
| `Genome` struct | `bytes: Vec<u8>` (or `[u8; 32]`), `version: u8` |
| Header validation | Magic `b"EVO1"`, `version == config.genome_version` |
| Core layout | 4-byte header + 28-byte payload = 32 bytes total; offsets per [GENOME.md](GENOME.md) |
| Locus accessors | `read_u8`, `read_u16_be` at documented offsets (relative to core segment start) |
| Ancestor | Parse `ancestor_hex` from config; use for initial population and stillborn fallback policy |
| Unit tests | Known byte pattern decodes expected raw values; wrong magic → error |

### Invalid genome handling

| Condition | Phase 1 behavior |
|-----------|------------------|
| Wrong magic / version | Stillborn: do not spawn entity |
| Length mismatch vs `core_length` | Reject at load or stillborn on spawn |

Do **not** silently clamp alleles to “best” values.

### Acceptance (1B)

- [x] `cargo test` genome encoding tests pass
- [x] Ancestor hex from default config produces valid `Genome` for every initial spawn

---

## Sub-phase 1C — Phenotype decode

### Deliverables

| Task | Detail |
|------|--------|
| `Phenotype` struct | All scalar traits from core loci + derived fields |
| `decode(genome, &SimConfig) -> Result<Phenotype, DecodeError>` | Single entry point |
| Tradeoff formulas | Implement per [GENOME.md](GENOME.md): radius metabolism scale, `photo_upkeep`, `armor_upkeep` (computed but inactive until Phase 4), `chemo_upkeep` if raw > 0 |
| Derived traits | `max_speed`, `move_cost`, `metabolism_moving` from locomotion + radius (+ hardness move penalty) |
| Repro risk inputs | Store decoded `repro_threshold`, `repro_cost`, `offspring_endowment`, `repro_cooldown`, `repro_risk_factor` on phenotype |
| Cache policy | Rebuild only on spawn and after child mutation |

### Phenotype fields (minimum)

| Field | Source |
|-------|--------|
| `color` | `color_r/g/b` → `[0,1]³` |
| `body_radius` | Locus 3 |
| `photo_strength`, `photo_upkeep` | Loci 4–5 + formula |
| `metabolism_base` | Loci 6–7 |
| `locomotion_investment`, `max_speed`, `move_cost` | Loci 8–9 + derived |
| Repro scalars | Loci 10–17 |
| `skin_hardness`, `repair_efficiency`, `chemo_potential` | Loci 18–23 (chemo gain inactive) |
| `core_max_hp` | From config, not DNA in v1 |

### Acceptance (1C)

- [x] Unit tests: mid-range ancestor → phenotype within expected bounds
- [x] Extreme alleles clamp via config min/max, not unbounded growth
- [x] `photo_upkeep` increases monotonically with `photo_strength` (tradeoff sanity)

---

## Sub-phase 1D — Entity components and resources

### Deliverables

Align with [ENTITIES.md](ENTITIES.md):

| Component | Fields |
|-----------|--------|
| `Genome` | bytes, version |
| `Phenotype` | decoded struct (or `CachedPhenotype` resource on entity) |
| `Energy` | `internal_energy: f32` |
| `Health` | `core_hp`, `core_max_hp`; `part_hp: Vec<f32>` empty in Phase 1 |
| `Transform` / `Velocity` | Retain from PoC |
| `Agent` | `radius` copied from `phenotype.body_radius` |
| `Lineage` | `id: u64`, `parent_id: Option<u64>`, `generation: u32` |
| `Cooldowns` | `repro_ticks_remaining: u32` |

### Resources

| Resource | Role |
|----------|------|
| `WorldBounds` | From config width/height (keep window sync optional) |
| `SimTick` | `tick: u64`, increment once per pipeline |
| `NextEntityId` | Monotonic id for lineage |
| `SimRng` | Master + sub-streams (1K) |

### Acceptance (1D)

- [x] Query `With<Genome>` returns population with all lifecycle components
- [x] `Agent.radius` matches phenotype after spawn

---

## Sub-phase 1E — Spawn pipeline

### Steps (per [ENTITIES.md — Spawn](ENTITIES.md#spawn))

1. Genome from ancestor template (initial pop) or parent mutation (reproduction — 1H).
2. Validate header → stillborn if invalid.
3. `decode` → `Phenotype`.
4. `Health { core_hp: core_max_hp, core_max_hp, part_hp: [] }`.
5. `Energy { internal_energy: spawn_energy }` (config).
6. Random `Transform` / `Velocity` within bounds (RNG: `stream_spawn`).
7. Spawn Bevy mesh: `Circle::new(body_radius)`, `ColorMaterial` from phenotype color.
8. `Lineage { id, parent_id: None, generation: 0 }` for initial; child increments generation.
9. `Cooldowns { repro_ticks_remaining: 0 }` or small random offset optional.

### Initial population

- Spawn `initial_population` entities from ancestor with optional spawn-position jitter.
- Apply `mutations_per_offspring = 0` on initial spawn **or** one light mutation pass — document choice in config comment (recommend: **no** mutation on worldgen, only on reproduce).

### Acceptance (1E)

- [x] Visual run shows circles with colors and sizes varying after mutations appear (post-1H); initial pop uniform if no worldgen mutation
- [x] Stillborn decode does not leak entities (count stays stable)

---

## Sub-phase 1F — Energy and metabolism (no environmental gain)

Phase 1 **does not** sample light or apply photosynthetic gain. Use a config **flat grant** so populations remain viable while upkeeps and metabolism still exert selection.

### Per-tick formula (Phase 1)

```text
gain = flat_energy_grant_per_tick
cost = metabolism_base * (body_radius / r_ref)²
     + photo_upkeep
     + armor_upkeep      // from skin_hardness locus; often 0 on ancestor
     + chemo_upkeep      // if chemo_potential raw > 0
Δinternal_energy = gain - cost
```

Clamp `internal_energy` to `>= 0` after apply; do not go negative until death system runs (or treat `<= 0` as death same tick).

### System placement

Register as `metabolism_and_gains` in tick pipeline ([SIMULATION.md](SIMULATION.md)). For Phase 1, skip `environment_tick` and `sense_exposure` or leave as no-ops.

### Stub behavior for unused loci

| Locus | Phase 1 |
|-------|---------|
| `photo_strength` | Affects `photo_upkeep` only, not gain |
| `chemo_potential` | Upkeep if raw > 0; no vent gain |
| `skin_hardness` | Upkeep + move penalty in derived traits; no hazard mitigation |

### Acceptance (1F)

- [x] With `flat_energy_grant_per_tick = 0`, population eventually dies (sanity check)
- [x] With default grant, mean energy stabilizes above repro threshold for ancestor-like genomes over 1k ticks

---

## Sub-phase 1G — Movement

### Deliverables

| Task | Detail |
|------|--------|
| Retain PoC motion | Integrate velocity each tick with `Δt` from `Time` or fixed `1/60` |
| Speed cap | Clamp velocity magnitude to `phenotype.max_speed` |
| Move cost | If `internal_energy >= move_cost`, deduct `move_cost` when speed > ε; else skip movement or damp (document: **skip move** if insufficient energy) |
| Moving metabolism | Optional: `metabolism_base * (1 + k * |v|/max_speed)` per GENOME.md |
| Bounds | `bounce` or `wrap` from config (PoC already has bounce) |
| **Remove** | `resolve_collisions` despawn system entirely |

### Collision policy (Phase 1)

- Entity–entity overlap: **ignored** (no damage, no despawn).
- Document in code comment: collision damage arrives Phase 4.

### Acceptance (1G)

- [x] Agents bounce on walls; population count unchanged by overlap
- [x] High `locomotion_investment` genotypes pay higher `move_cost` per tick when moving

---

## Sub-phase 1H — Asexual reproduction and mutation

### Preconditions ([ENTITIES.md](ENTITIES.md#reproduce))

- `repro_ticks_remaining == 0`
- `internal_energy >= repro_threshold`
- Failsafe not blocking spawns (1J)

### Steps

1. Pay `repro_cost` from parent `internal_energy`.
2. Compute repro death probability ([GENOME.md](GENOME.md)):

   ```text
   energy_margin = internal_energy / repro_threshold
   risk_mult = repro_risk_factor   // decoded 0..1
   p_death = p_death_on_repro_base * risk_mult
            * max(0, 1 - energy_margin / repro_safety_margin)
   ```

3. Roll `stream_repro_risk` → on failure, parent dies (despawn), no child.
4. On success: clone genome, apply `mutations_per_offspring` passes using config probabilities.
5. Decode child; stillborn → parent keeps paid cost (config: energy lost, no child).
6. Child `internal_energy = offspring_endowment`; deduct endowment from parent (after cost).
7. Spawn child near parent (offset RNG).
8. Set child `Lineage { parent_id: Some(parent), generation: parent + 1 }`.
9. Reset parent `repro_ticks_remaining = phenotype.repro_cooldown`.

### Mutation operators (core only)

**v1 (this sub-phase):** Implement from [GENOME.md](GENOME.md) — bit flip, point noise, resample.

**v2 (current default):** [Genome_Definition.md — Mutation (v2)](Genome_Definition.md#mutation-v2) — `[genetics.mutation_v2]`, 0–4 gene-level ±increment passes; no bit flip / resample.

| Operator | Phase 1 (v1) |
|----------|---------|
| Bit flip | Yes |
| Point noise | Yes, one hex nibble ±1 per affected byte (`delta_max` = 1 in default config) |
| Locus resample | Yes |
| Dup / del | No-op on core (probabilities 0) |

### Acceptance (1H)

- [x] Population increases over time when grant and thresholds tuned
- [x] Child genome differs from parent after mutations (hex or hash check in test)
- [x] Low-energy reproduce attempts sometimes kill parent (repro risk)

---

## Sub-phase 1I — Death and tick order

### Death conditions

| Condition | Action |
|-----------|--------|
| `internal_energy <= 0` | Despawn, reason `EnergyDepleted` |
| `core_hp <= 0` | Despawn, reason `CoreDestroyed` (no damage source in 1 except optional debug) |
| Repro risk failure | Despawn parent, reason `ReproFatality` |
| Failsafe cull | Phase 1 stub only (1J) |

### Canonical tick order (Phase 1 subset)

Implement chained systems in this order:

1. ~~Environment~~ (no-op)
2. ~~Sense~~ (no-op)
3. **Metabolism and gains** (flat grant − costs)
4. **Movement** (cost, bounds)
5. ~~Interactions~~ (no-op)
6. ~~Healing~~ (no-op)
7. **Reproduction**
8. **Death and despawn**
9. **Performance failsafe**
10. ~~Metrics~~ (no-op until Phase 3)

Decrement `repro_ticks_remaining` at end of tick (or start — pick one and document).

### Acceptance (1I)

- [x] No code path despawns on circle overlap
- [x] Death only via table above

---

## Sub-phase 1J — Performance failsafe (stub)

Minimum implementation per [SIMULATION.md](SIMULATION.md):

| Policy | Phase 1 implementation |
|--------|------------------------|
| `stop_spawns` | **Required** — set resource flag; reproduction system checks before spawn |
| `despawn_lowest_energy` | Optional stub (log + skip if not implemented) |
| `pause` | Optional stub |

When `population > max_entities`:

- Log warning with tick and count (stderr or `tracing`).
- Apply policy; **do not** use as fitness selection.

### Acceptance (1J)

- [x] Artificially low `max_entities` blocks new spawns but does not crash

---

## Sub-phase 1K — RNG architecture

```text
master_seed (optional)
  └─ stream_spawn
  └─ stream_mutation(entity_id, generation)
  └─ stream_repro_risk(entity_id, tick)
```

- Use SplitMix64 or `rand` seeded per stream from hashed `(master_seed, stream_id, salt)`.
- Without `master_seed`, each stream falls back to thread RNG independently.

### Acceptance (1K)

- [x] Same `master_seed` + config → same reproduction outcomes over 500 ticks (integration test, allow small float drift if movement uses floats)

---

## Sub-phase 1L — Tuning and phase acceptance

### Config tuning guidelines

| Knob | Purpose |
|------|---------|
| `flat_energy_grant_per_tick` | Balance so ancestor survives; not so high that zero-metabolism mutants dominate |
| `spawn_energy` | Enough buffer for early repro attempts |
| `p_death_on_repro_base` | A/B test driver (see below) |
| `initial_population` | 50–120 for visual; lower for fast tests |

### Phase acceptance criteria (from ROADMAP)

| Criterion | How to verify |
|-----------|----------------|
| Population survives **multiple thousand ticks** without instant extinction | `cargo test population_survives_5000_ticks` (headless) or visual run |
| **Repro risk A/B** | `cargo test repro_risk_ab_same_seed_different_trajectories` |
| **Diversity proxy** | `cargo test diversity_proxy_color_variance_across_seeds` (+ single-run color test) |
| Build | `cargo build` / `cargo test` on Linux |

### Smoke tests (implemented)

| Test | Module |
|------|--------|
| `ancestor_from_default_config_is_valid` | `genome::encoding` |
| `mutation_changes_child_genome` | `genome::mutation` |
| `repro_risk_higher_when_low_energy` | `sim::systems::reproduction` |
| `no_collision_or_tick_despawn_outside_death_system` | `sim::systems::death` |
| `population_survives_5000_ticks` | `sim::acceptance` |
| `repro_risk_ab_same_seed_different_trajectories` | `sim::acceptance` |
| `diversity_proxy_color_variance_across_seeds` | `sim::acceptance` |

Phase 3 adds formal headless metrics export; Phase 1 uses `sim/headless.rs` in unit tests.

---

## Code migration map (Phase 0 → Phase 1)

| Current (`main.rs`) | Phase 1 target |
|---------------------|----------------|
| `AGENT_COUNT`, `AGENT_RADIUS` | `[world].initial_population`, phenotype radius |
| Random palette colors | `Phenotype.color` from genome |
| `resolve_collisions` despawn | **Deleted** |
| `move_agents` | `metabolism` + `movement` systems with phenotype caps |
| `WorldBounds` | Config-driven; optional window resize sync |
| Hard-coded speeds | `phenotype.max_speed` |

Incremental refactor is acceptable: introduce `config.rs` and `genome/` in-repo before full `lib.rs` split ([ARCHITECTURE.md](ARCHITECTURE.md)).

---

## Dependencies

| Crate | Phase 1 use |
|-------|-------------|
| `bevy` 0.15 | ECS, rendering |
| `rand` 0.8 | Mutation, spawn, repro risk |
| `serde`, `toml` | Config |

Defer `clap`, `serde_json` to Phase 3 unless needed early for tests.

---

## Risks and mitigations

| Risk | Mitigation |
|------|------------|
| Flat grant hides bad tradeoffs | Keep grant modest; verify population dies when grant = 0 |
| Photo upkeep without photo gain punishes high photo | Expected in Phase 1; Phase 2 adds gain; tune `k_photo` |
| Population explosion before failsafe | Set `max_entities`; tune repro threshold/cost |
| All entities identical color | Mutations on reproduce only; verify mutation rates > 0 |
| `f32` non-determinism across machines | Document; use integer tick counters for RNG; avoid order-dependent floats in repro |

---

## Definition of done

Phase 1 is **complete** when:

1. All items in the **Master checklist** are checked.
2. **Phase acceptance criteria** in sub-phase 1L pass.
3. [ROADMAP.md](ROADMAP.md) Phase 1 row updated to **Done** (in the PR that merges implementation).
4. Any intentional deviation from [GENOME.md](GENOME.md) or [ENTITIES.md](ENTITIES.md) is reflected in those docs in the same PR.

---

## Further reading

- [ROADMAP.md](ROADMAP.md) — Phase 2+ scope
- [PHASE_2.md](PHASE_2.md) — Light field and Model A photosynthesis (next)
- [GENOME.md](GENOME.md) — Loci bytes and formulas
- [ENTITIES.md](ENTITIES.md) — Lifecycle state machine
- [SIMULATION.md](SIMULATION.md) — Full tick pipeline and config schema
- [ARCHITECTURE.md](ARCHITECTURE.md) — Target module layout

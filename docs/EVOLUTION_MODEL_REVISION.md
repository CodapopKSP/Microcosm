# Evolution model revision — implementation plan

**Status:** Design / not implemented  
**Supersedes (when done):** mutation operator tables in [GENOME.md](GENOME.md), color-nibble-cap rules in [Genome_Definition.md](Genome_Definition.md), collision-damage sections in [PHASE_4.md](PHASE_4.md) and [ENTITIES.md](ENTITIES.md).

This document records **decisions from review (2026-06-05)** and the concrete code/doc work to align the simulator with them.

---

## Decisions summary

| Topic | Current behavior | Desired behavior |
|-------|------------------|------------------|
| Repro cooldown | `repro_cooldown_ticks = 192` blocks attempts after each repro/stillborn | **Keep 192** (~3.2 s at 60 Hz) as a practical anti-spam brake; not a gene |
| Spawn vitality | Initial pop uses `spawn_energy` + `spawn_health_frac × max_health`; offspring use parent endowment only | **Confirm:** `spawn_energy` / `spawn_health_frac` apply **only** to generation-0 spawns (ancestor template + initial population) |
| Genome v1 (`EVO1`) | Full dual-path code + `configs/v1_regression.toml` | **Deprecate and remove** after v2 acceptance; keep regression config only until cutover |
| Color vs efficiency | Color **high nibbles cap** photo/biomass/minerals decode (`phenotype_v2.rs`) | **Decouple:** color is cosmetic / smell signature only; efficiency bytes decode to full `0xFF` range (triad coupling may remain) |
| Collision damage | `interactions.rs` applies `collision_damage_scale × relative_speed` | **Remove** entity–entity collision damage; keep hazard-zone damage only |
| Mutation operators | Bit flip / nibble step / resample on random bytes; fixed `mutations_per_offspring` | **Gene-level ±increment** (usually ±1, rarely ±3–4); **0–4 mutations per child** (random); no bit flips or resamples |

---

## Answers to review questions (current code)

### Reproduction cooldown

**Yes, it is implemented.** After a successful birth or stillborn, the parent’s `Cooldowns.repro_ticks_remaining` is set to `economy.repro_cooldown_ticks` (192 in `configs/default.toml`). The reproduction system decrements the counter each tick and skips agents while `> 0`.

- At 60 Hz, 192 ticks ≈ **3.2 seconds** between attempts.
- Config validation **rejects** `repro_cooldown_ticks = 0` (`src/config.rs`).
- Agents still need `internal_energy ≥ repro_threshold` each attempt; cooldown is an extra gate, not the primary cost.

**Action:** No change required if 192 ticks remains acceptable. Optional follow-up: allow `0` in config for “repro immediately when threshold met” runs.

### Spawn energy vs parent endowment

**Mostly correct for v2 offspring; initial population differs.**

| Spawn path | Energy | Health |
|------------|--------|--------|
| **Initial population** (`spawn_initial_population` → `build_spawn_params`) | `economy.spawn_energy` (default 50) | `spawn_health_frac × max_health` (default **1.0** → full health) |
| **v2 child** (`attempt_reproduction_v2` → `build_offspring_spawn_params`) | Overflow after health fill | `min(spent, child_max_health)` first |

v2 endowment split (`split_endowment_to_child` in `reproduction_v2.rs`) matches the stated rule:

> Parent spends 120 → child gets 100 HP (if `max_health = 100`) + 20 energy (capped by `max_energy`).

**Gaps vs desired:**

1. **v1 offspring** still spawn with `child_start_energy = 0.0` only (HP from converted repro energy).
2. **Initial population** receives both global `spawn_energy` **and** full decoded max health — not “energy only.” Document whether generation-0 should start at **full pools** (current) or **only** `spawn_energy` with HP from a separate knob.
3. `ENTITIES.md` still says “spawn energy or offspring endowment” without distinguishing generation 0 vs child.

**Action:** Clarify in docs; add an explicit `is_initial_spawn: bool` or separate `build_initial_spawn_params` so `spawn_energy` cannot leak into offspring paths. Add acceptance test: reproduced child never reads `economy.spawn_energy`.

### Genome v1 in code

**Yes — v1 is still live code**, not docs-only:

- Types: `Genome` / `Phenotype` (`encoding.rs`, `phenotype.rs`)
- Mutation: `mutation.rs`
- Reproduction: `attempt_reproduction` in `reproduction.rs`
- Dual dispatch: `AnyGenome`, `decode.rs`, version branches across ~15+ modules
- Regression: `configs/v1_regression.toml`, `SimConfig::load_v1_regression()`

Default runtime uses **v2** (`configs/default.toml`, `genome_version = 2`).

**Action:** Remove v1 after this revision lands (see [§ Deprecation / deletion](#deprecation--deletion)).

### Color nibble caps

**Currently enforced in code**, contrary to desired design:

- `high_nibble_cap()` in `src/genome/phenotype_v2.rs`
- `apply_triad(..., photo_cap / biomass_cap / minerals_cap, ...)`
- Documented in `Genome_Definition.md` § Color and § Vitality

**Action:** Remove caps from decode; color bytes affect render RGB and signature matching (`0x0000`) only.

### Collision damage

**Currently enabled.** `src/sim/systems/interactions.rs` applies pairwise collision damage when relative speed ≥ 25% of the slower agent’s `max_speed`. Default `collision_damage_scale = 0.15` in all main configs.

Hazards (zone overlap) are separate and should **remain**.

**Action:** Delete collision branch from `accumulate_interaction_damage`; remove or zero `collision_damage_scale` from config schema.

---

## Mutation model (new spec)

Replace `[genetics] p_bit_flip`, `p_point`, `p_resample`, `p_dup`, `p_del`, and fixed `mutations_per_offspring` with:

### Per-offspring count

```toml
[genetics.mutation_v2]
passes_min = 0
passes_max = 4   # inclusive; draw Uniform{0,1,2,3,4} per child
```

### Per-pass operator

1. Pick **one gene** uniformly from the mutable gene registry (see table below).
2. Pick direction: +1 or −1 (50/50).
3. Pick **magnitude**:
   - **Default (~90%):** 1 (one step on the gene’s logical integer value).
   - **Rare (~10%):** uniform 3 or 4 (configurable `large_step_values = [3, 4]`, `p_large_step = 0.10`).
4. Apply `new = clamp(old + direction × magnitude, gene_min, gene_max)`.
5. Write back to genome bytes (big-endian for multi-byte genes).

**No** bit flips, **no** uniform byte resample, **no** segment dup/del (v2).

### EVO2 mutable gene registry

Offsets are post-header (`HEADER_LEN`). Variable payload genes apply only when slot counts > 0.

| Gene ID | Name | Byte range (core) | Width | Step domain |
|---------|------|-------------------|-------|-------------|
| `color` | Color RGB | 0–2 | 3×u8 | Each channel 0–255 independently **or** treat as 3 separate genes — **pick one in impl** (recommend 3 genes: `color_r`, `color_g`, `color_b` for simpler ±1 steps) |
| `body_radius` | Body radius | 3–4 | u16 BE | 0–65535 |
| `armor` | Armor | 5–6 | u16 BE | 0–65535 |
| `healing_rate` | Healing rate | 7–8 | u16 BE | 0–65535 |
| `energy_health_ratio` | E/H ratio | 9 | u8 | 0–255 |
| `repro_threshold` | Repro threshold % | 10 | u8 | 0–255 |
| `offspring_endowment` | Offspring endowment % | 11 | u8 | 0–255 |
| `movement_desire` | Movement desire | 12 | u8 | 0–255 |
| `turning_desire` | Turning desire | 13 | u8 | 0–255 |
| `max_speed` | Max speed | 14 | u8 | 0–255 |
| `movement_efficiency` | Movement efficiency | 15 | u8 | 0–255 |
| `photo_efficiency` | Photosynthesis | 16 | u8 | 0–255 |
| `biomass_efficiency` | Biomass digestion | 17 | u8 | 0–255 |
| `minerals_efficiency` | Minerals digestion | 18 | u8 | 0–255 |
| `sensors` | Sensor mask + strength | 19–20 | 2×u8 | Step each byte 0–255 **or** single gene with combined step — recommend **mask byte** and **strength byte** as two genes |
| `pursue_slot_count` | Pursue list count | 21 | u8 | 0–`max_pursue_slots` |
| `avoid_slot_count` | Avoid list count | 22 | u8 | 0–`max_avoid_slots` |
| `pursue_slot_i` | Pursue slot payload | var | 6×n | Per-slot: step `type u16` and `value u32` as separate genes **or** whole-slot resample forbidden — recommend stepping `value` fields only for simple slots |
| `avoid_slot_i` | Avoid slot payload | var | 6×n | Same as pursue |

**Pursue/avoid payload:** minimum viable impl — mutate slot **counts** and per-slot **`value u32`** / **`type u16`** with ±1 (rare ±3–4) while preserving layout; do not change genome length in v1 of this revision (no dup/del of slots).

### Config shape (proposed)

```toml
[genetics.mutation_v2]
passes_min = 0
passes_max = 4
p_large_step = 0.10
large_step_min = 3
large_step_max = 4
```

Remove from `[genetics]`: `p_bit_flip`, `p_point`, `p_resample`, `p_dup`, `p_del`, `mutations_per_offspring`, `delta_max` (or keep `delta_max` as alias for large step max during migration).

### Files to implement

| File | Change |
|------|--------|
| `src/genome/mutation_v2.rs` | Rewrite: gene registry, increment operator, random pass count |
| `src/genome/mutation.rs` | Delete after v1 removal **or** align v1 to same gene-increment model until deleted |
| `src/config.rs` | Add `MutationV2Config`; deprecate old probability fields |
| `configs/*.toml` | New `[genetics.mutation_v2]` block; remove old mutation keys |
| `src/genome/mutation_v2.rs` tests | Gene step bounds, 0–4 pass count distribution, no bit-flip paths |

---

## Color decoupling

### Code

| File | Change |
|------|--------|
| `src/genome/phenotype_v2.rs` | Remove `high_nibble_cap`, `photo_nibble_cap` / `biomass_nibble_cap` / `minerals_nibble_cap` from `PhenotypeV2` (or keep fields always `1.0` during transition) |
| `src/genome/phenotype_v2.rs` | `apply_triad(photo_gene_eff, 1.0, ...)` — cap argument unused or removed |
| `src/genome/signatures.rs` | No change to fuzzy RGB matching |
| `src/visual/inspector.rs` | Remove nibble-cap display if present |

### Docs to edit

| File | Delete / change |
|------|-----------------|
| `docs/Genome_Definition.md` | Remove high-nibble cap bullets under **Color**; remove cap references in Vitality, ancestor table, doc-map row |
| `docs/CONFIG.md` | Remove `nibble_levels` cap semantics for efficiency; keep for fuzzy match if needed |
| `docs/GENOME_V2_ARCHITECTURE_ROADMAP.md` | Remove “capped by color high nibble” |
| `docs/GENOME.md` | §4.7 greenness photo niche — mark legacy or rewrite without color cap |

---

## Remove collision damage

### Code

| File | Change |
|------|--------|
| `src/sim/systems/interactions.rs` | Remove pairwise collision loop; keep hazard accumulation only |
| `src/config.rs` | Remove `collision_damage_scale` **or** ignore when 0 with deprecation warning |
| `configs/default.toml` (and siblings) | Remove key |
| `src/sim/acceptance.rs` | Delete/update collision damage tests |
| `src/sim/systems/interactions.rs` tests | Delete collision cases |

### Docs to edit

| File | Change |
|------|--------|
| `docs/ENTITIES.md` | Interactions table: collision → overlap only, no damage |
| `docs/ENERGY_AND_ENVIRONMENT.md` | Remove collision damage formula |
| `docs/SIMULATION.md` | Tick order step 8: hazards only |
| `docs/Genome_Definition.md` | Remove `collision_damage_scale` from global mechanics table |
| `docs/PHASE_4.md` | Strike Phase 4B collision damage; note removal |
| `docs/ARCHITECTURE.md` | Update interactions description |
| `README.md` | Remove collision damage bullet |

**Armor** stroke remains visual + metabolism/speed penalties; hazard mitigation may stay if armor still reduces hazard damage (confirm in `mitigated_damage` — today uses phenotype hardness; v2 armor may differ).

---

## Spawn vitality clarification

### Code

| File | Change |
|------|--------|
| `src/sim/spawn.rs` | Rename/split: `build_initial_spawn_params` (uses `spawn_energy`, `spawn_health_frac`) vs `build_offspring_spawn_params` (explicit vitals only) |
| `src/sim/spawn.rs` | Assert `parent_id.is_some()` ⇒ never read `spawn_energy` |
| `src/sim/components.rs` | Document `vitality_spawn_pools_for` as initial-spawn only |

### Docs

| File | Change |
|------|--------|
| `docs/ENTITIES.md` | Spawn §: generation 0 = config; children = endowment only |
| `docs/Genome_Definition.md` | `spawn_energy` row: “initial population / ancestor only” |
| `docs/CONFIG.md` | Clarify `spawn_energy` + `spawn_health_frac` scope |

---

## Reproduction cooldown

No implementation change if 192 ticks is kept.

Optional doc tweak: `Genome_Definition.md` § Reproduction — describe cooldown as “short global anti-spam (~3 s), not a viability mechanic.”

To allow true zero cooldown later:

| File | Change |
|------|--------|
| `src/config.rs` | Allow `repro_cooldown_ticks = 0` (skip timer) |
| `src/sim/systems/reproduction.rs` | When 0, never set/increment cooldown |

---

## Deprecation / deletion

### Phase A — implement revisions on v2 (this plan)

### Phase B — remove genome v1

| Delete or gut | Notes |
|---------------|-------|
| `src/genome/encoding.rs` (v1 `Genome`) | After migration |
| `src/genome/phenotype.rs` (v1 `Phenotype`) | After migration |
| `src/genome/mutation.rs` | Replace with v2-only mutation module |
| `configs/v1_regression.toml` | Remove when CI uses v2 regression |
| `SimConfig::load_v1_regression()` | Remove |
| `AnyGenome::V1`, `AnyPhenotype::V1` | Collapse to `GenomeV2` / `PhenotypeV2` only |
| `attempt_reproduction` (v1) | Keep only `attempt_reproduction_v2` |
| v1 branches in `metabolism.rs`, `movement.rs`, `sense.rs`, `healing.rs`, `death.rs`, `headless.rs` | Delete |
| `docs/GENOME.md` | Archive or redirect to `Genome_Definition.md` |
| `docs/GENOME_REVISIONS.md` | Mark historical |

### Docs that reference old mutation operators (update or strike)

| File | Section |
|------|---------|
| `docs/GENOME.md` | Mutation operators table |
| `docs/PHASE_1.md` | §1H mutation operators |
| `docs/SIMULATION.md` | `[genetics]` mutation keys table |
| `docs/CONFIG.md` | `p_bit_flip`, `p_point`, etc. |
| `docs/ARCHITECTURE.md` | `mutation.rs` description |

---

## Acceptance criteria

- [ ] v2 child spawn vitality comes **only** from `split_endowment_to_child`; never `spawn_energy`.
- [ ] Initial population still spawns from `spawn_energy` + `spawn_health_frac`.
- [ ] `photo_efficiency_raw = 0xFF` with `color_g = 0x00` yields **full** photo gene efficiency (triad may still reduce effective).
- [ ] Entity overlap at speed does **not** reduce `core_hp`; hazard zones still do.
- [ ] One reproduction produces `passes` mutations where `passes ∈ [0, 4]`; each pass changes exactly **one gene** by ±1 (or ±3/±4 with `p_large_step`).
- [ ] No code path calls bit flip or resample for v2 offspring.
- [ ] Headless + acceptance tests updated; CI green on `default.toml`.
- [ ] `Genome_Definition.md` updated to match (color cosmetic, mutation section, no collision damage).

---

## Suggested implementation order

1. **Color cap removal** — small decode change, update tests.
2. **Collision damage removal** — isolated systems change.
3. **Spawn vitality guard** — clarify initial vs offspring paths + tests.
4. **Mutation v2 rewrite** — config migration, new operator, delete old probabilities.
5. **Doc sweep** — files listed above.
6. **v1 deprecation** — separate PR after v2 regression stable.

---

## Related files (quick index)

```
src/genome/mutation_v2.rs      # rewrite
src/genome/mutation.rs         # delete (phase B)
src/genome/phenotype_v2.rs     # remove nibble caps
src/sim/systems/interactions.rs
src/sim/spawn.rs
src/sim/systems/reproduction_v2.rs
src/config.rs
configs/default.toml
docs/Genome_Definition.md
docs/ENTITIES.md
docs/CONFIG.md
```

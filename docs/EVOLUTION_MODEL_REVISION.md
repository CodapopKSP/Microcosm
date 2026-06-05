# Evolution model revision — agent checklist

**Status:** Complete (58 / 58 tasks)  
**Last updated:** 2026-06-05 (Phase G complete)  
**Decisions locked:** [review 2026-06-05](#decisions-locked)  
**Spec reference:** [appendix](#appendix-spec-reference)  
**Full inventory:** [audit — delete / edit](#audit--delete--edit-inventory)

---

## How to use this doc (multi-agent handoff)

1. **Pick one task** with status `[ ]` and no unmet **Depends on** items.
2. Set status to `[~]` and add your agent/session id under **Owner** (e.g. `agent-abc / 2026-06-05`).
3. Do the work; run `cargo test` for touched areas before marking done.
4. Set status to `[x]`, fill **Done notes** (commit hash, PR, caveats).
5. Update the **Progress** line at the top (`N / 58 tasks`).

### Status legend

| Mark | Meaning |
|------|---------|
| `[ ]` | Not started |
| `[~]` | In progress (only one agent per task) |
| `[x]` | Complete |
| `[—]` | Cancelled / won't do (explain in Done notes) |

### Phase overview

| Phase | ID prefix | Parallel? | Gate |
|-------|-----------|-----------|------|
| **A** Color decoupling | `EMR-A` | Yes (A1∥A2 after A1 code) | Tests green |
| **B** Collision removal | `EMR-B` | Yes (B1∥B2) | Tests green |
| **C** Spawn vitality | `EMR-C` | Mostly serial | Tests green |
| **D** Mutation rewrite | `EMR-D` | D1→D2→D3; D4∥D5 after D3 | Tests green |
| **E** Doc sweep | `EMR-E` | Yes after A–D code done | Docs consistent |
| **F** Final acceptance | `EMR-F` | Serial | Full `cargo test` |
| **G** v1 deprecation | `EMR-G` | After F | Separate PR |

---

## Progress tracker

```
Phase A  [x] 8/8   Color decoupling
Phase B  [x] 9/9   Collision removal
Phase C  [x] 7/7   Spawn vitality
Phase D  [x] 11/11 Mutation rewrite
Phase E  [x] 16/16 Doc sweep
Phase F  [x] 5/5   Final acceptance
Phase G  [x] 5/5   v1 deprecation
─────────────────
Total    [x] 58/58
```

---

## Phase A — Color decoupling

> **Goal:** Color bytes are cosmetic / smell only. Efficiency genes decode to full `0xFF` range. Triad inverse coupling may remain.

| ID | Task | Depends on | Owner | Done notes |
|----|------|------------|-------|------------|
| EMR-A1 | [x] Remove `high_nibble_cap()` usage in `src/genome/phenotype_v2.rs`; pass full gene efficiency into `apply_triad` (no color cap arg) | — | agent | `phenotype_v2.rs` |
| EMR-A2 | [x] Remove `photo_nibble_cap`, `biomass_nibble_cap`, `minerals_nibble_cap` from `PhenotypeV2` struct + all constructors/readers | EMR-A1 | agent | |
| EMR-A3 | [x] Update `src/genome/phenotype_v2.rs` tests: `color_g = 0x00` + `photo_efficiency_raw = 0xFF` ⇒ full photo gene eff (pre-triad) | EMR-A1 | agent | `color_does_not_cap_photo_gene_efficiency` |
| EMR-A4 | [x] Grep/fix any other references to nibble caps (`inspector.rs`, `metrics.rs`, `acceptance.rs`) | EMR-A2 | agent | no `src/` refs remain |
| EMR-A5 | [x] Update `docs/Genome_Definition.md` Color § + Vitality § + ancestor table + doc-map row | EMR-A1 | agent | Phase E |
| EMR-A6 | [x] Update `docs/CONFIG.md`, `docs/GENOME_V2_ARCHITECTURE_ROADMAP.md` (remove cap semantics) | EMR-A5 | agent | Phase E |
| EMR-A7 | [x] Delete `high_nibble_cap()` entirely; simplify `apply_triad()` (drop `cap` parameter) | EMR-A1 | agent | merged with A1 |
| EMR-A8 | [x] Keep `nibble_levels` in config for **turning/sensor** decode only — update `CONFIG.md` row (not efficiency caps) | EMR-A6 | agent | Phase E |

**Files:** `src/genome/phenotype_v2.rs`, `docs/Genome_Definition.md`, `docs/CONFIG.md`, `docs/GENOME_V2_ARCHITECTURE_ROADMAP.md`

**Not in scope (v1 only):** `[genetics.color_photosynthesis]` / `greenness` in `src/genome/phenotype.rs` — remove in Phase G, not Phase A.

---

## Phase B — Remove collision damage

> **Goal:** Entity overlap does not damage HP. Hazard zones still do.

| ID | Task | Depends on | Owner | Done notes |
|----|------|------------|-------|------------|
| EMR-B1 | [x] Remove pairwise collision loop from `src/sim/systems/interactions.rs` (`accumulate_interaction_damage`); keep hazard branch | — | agent | `accumulate_hazard_damage` |
| EMR-B2 | [x] Delete/update collision tests in `interactions.rs` `#[cfg(test)]` | EMR-B1 | agent | |
| EMR-B3 | [x] Remove `collision_damage_scale` from `EconomyConfig` in `src/config.rs` + validation | EMR-B1 | agent | |
| EMR-B4 | [x] Remove `collision_damage_scale` from all `configs/*.toml` | EMR-B3 | agent | 7 configs |
| EMR-B5 | [x] Fix `src/sim/acceptance.rs` and any integration tests that assert collision damage | EMR-B1 | agent | |
| EMR-B6 | [x] Update docs: `ENTITIES.md`, `ENERGY_AND_ENVIRONMENT.md`, `SIMULATION.md`, `PHASE_4.md`, `ARCHITECTURE.md`, `README.md`, `Genome_Definition.md` global table | EMR-B1 | agent | Phase E |
| EMR-B7 | [x] Rename/simplify `accumulate_interaction_damage` → hazard-only (drop `collision_damage_scale` param); update `interactions` module doc comment | EMR-B1 | agent | |
| EMR-B8 | [x] Edit code comments: `src/sim/systems/movement.rs` (line 3), `src/sim/components.rs` (lines 114, 445, 458), `src/sim/systems/death.rs` test name/message | EMR-B1 | agent | |
| EMR-B9 | [x] Delete integration test `overlapping_agents_lose_hp_without_despawn` in `interactions.rs` (or rewrite: overlap → **no** HP loss) | EMR-B2 | agent | rewritten |

**Files:** `src/sim/systems/interactions.rs`, `src/config.rs`, `configs/*.toml`, `src/sim/acceptance.rs`, `src/sim/systems/movement.rs`, `src/sim/components.rs`

**Note:** v2 hazard mitigation is currently **always 0** (`interactions.rs` reads `damage_mitigation` from v1 only). Armor stroke is visual + metabolism/speed only. Wiring v2 armor → hazard mitigation is **out of scope** unless requested separately.

---

## Phase C — Spawn vitality (initial vs offspring)

> **Goal:** `spawn_energy` / `spawn_health_frac` apply **only** to generation-0 spawns. Offspring use parent endowment only (already correct in v2 repro; harden + test).

| ID | Task | Depends on | Owner | Done notes |
|----|------|------------|-------|------------|
| EMR-C1 | [x] Split `build_spawn_params` → `build_initial_spawn_params` in `src/sim/spawn.rs` (uses `spawn_energy` + `spawn_health_frac`) | — | agent | |
| EMR-C2 | [x] Ensure `build_offspring_spawn_params` never reads `config.economy.spawn_energy`; add `debug_assert!(parent_id.is_some())` or separate code path | EMR-C1 | agent | |
| EMR-C3 | [x] Update `spawn_initial_population` to call `build_initial_spawn_params` | EMR-C1 | agent | |
| EMR-C4 | [x] Add test: v2 repro child vitality matches `split_endowment_to_child` only (e.g. 120 spent → 100 HP + 20 energy when caps allow) | EMR-C2 | agent | `child_vitality_from_endowment_not_spawn_energy` |
| EMR-C5 | [x] Update `docs/ENTITIES.md`, `docs/Genome_Definition.md`, `docs/CONFIG.md` spawn sections | EMR-C1 | agent | Phase E |
| EMR-C6 | [x] Guard `LifeBundle::from_any_parts` / `from_parts` — must not be used for offspring (defaults to `spawn_energy`); add `#[deprecated]` note or `debug_assert!(parent_id.is_none())` | EMR-C1 | agent | |
| EMR-C7 | [x] Fix `src/sim/spawn.rs` test `offspring_spawn_has_empty_part_hp` — stop passing `spawn_energy` as offspring HP (misleading fixture) | EMR-C2 | agent | uses explicit 25/80 vitals |

**Files:** `src/sim/spawn.rs`, `src/sim/components.rs`, `src/sim/systems/reproduction_v2.rs`, `src/sim/headless.rs`, `src/main.rs` (spawn_energy assert is initial-only — OK)

**No change needed:** `repro_cooldown_ticks = 192` stays as-is (optional zero-cooldown = future task, not in scope).

---

## Phase D — Mutation rewrite (gene-level increments)

> **Goal:** 0–4 random passes per child; each pass ±1 on one gene (rarely ±3–4). No bit flips, resample, dup, del.

| ID | Task | Depends on | Owner | Done notes |
|----|------|------------|-------|------------|
| EMR-D1 | [x] Add `MutationV2Config` to `src/config.rs` (`passes_min`, `passes_max`, `p_large_step`, `large_step_min`, `large_step_max`) | — | agent | `require_mutation_v2()` |
| EMR-D2 | [x] Add `[genetics.mutation_v2]` to **all** v2 configs; remove old `p_*` keys from `[genetics]` in: `default.toml`, `genome_v2.example.toml`, `mineral_vent_v2.toml`, **and** lab configs `vent_lab.toml`, `hazard_lab.toml`, `green_ab.toml`, `v1_regression.toml` (until Phase G) | EMR-D1 | agent | |
| EMR-D3 | [x] Implement gene registry + `step_gene()` in `src/genome/mutation_v2.rs` (see [gene registry](#evo2-mutable-gene-registry)) | EMR-D1 | agent | |
| EMR-D4 | [x] Rewrite `mutate_offspring_v2`: draw `Uniform(passes_min..=passes_max)`; loop `apply_one_gene_increment` | EMR-D3 | agent | |
| EMR-D5 | [x] Remove dead code from `mutation_v2.rs`: `bit_flip_v2`, `resample_byte_v2`, `nibble_step_v2`, old probability roll | EMR-D4 | agent | |
| EMR-D6 | [x] Unit tests: pass count in [0,4]; single gene changed per pass; bounds clamping; large step probability | EMR-D4 | agent | |
| EMR-D7 | [x] Deprecate/remove old mutation fields from `GeneticsConfig` + validation (or `#[serde(deny_unknown_fields)]` migration) | EMR-D2 | agent | removed in Phase G |
| EMR-D8 | [x] Update `src/genome/mutation.rs` tests only if v1 still present — **or** skip until Phase G | EMR-D4 | agent | v1 tests kept with explicit params |
| EMR-D9 | [x] Add mutation spec section to `docs/Genome_Definition.md` (new operator; strike old table in `GENOME.md`) | EMR-D4 | agent | Phase E |
| EMR-D10 | [x] Delete `configs/default.toml.bak` (stale mutation keys) | — | agent | |
| EMR-D11 | [x] Update `src/genome/mod.rs` exports after rewrite (`nibble_step_byte` may move or become private) | EMR-D5 | agent | |

**Files:** `src/genome/mutation_v2.rs`, `src/genome/mutation.rs`, `src/config.rs`, `configs/*.toml`, `src/genome/mod.rs`

**Gene impl choices (lock when starting D3):**
- Color: **3 genes** (`color_r`, `color_g`, `color_b`)
- Sensors: **2 genes** (`sensor_mask`, `sensor_strength`)
- Pursue/avoid: count genes + per-slot `type` / `value` step genes (no length change)

---

## Phase E — Doc sweep (remaining)

> Run after Phases A–D code is merged or complete. Tasks are parallelizable.

| ID | Task | Depends on | Owner | Done notes |
|----|------|------------|-------|------------|
| EMR-E1 | [x] `docs/GENOME.md` — strike mutation operators table; add pointer to `Genome_Definition.md` | EMR-D9 | agent | |
| EMR-E2 | [x] `docs/PHASE_1.md` — §1H mutation operators | EMR-D9 | agent | |
| EMR-E3 | [x] `docs/SIMULATION.md` — `[genetics]` keys table → `[genetics.mutation_v2]` | EMR-D2 | agent | |
| EMR-E4 | [x] `docs/CONFIG.md` — full pass: mutation keys, spawn scope, remove collision | EMR-B6, EMR-C5, EMR-D2 | agent | |
| EMR-E5 | [x] `docs/ARCHITECTURE.md` — interactions + mutation module description | EMR-B6, EMR-D5 | agent | |
| EMR-E6 | [x] `docs/GENOME.md` §4.7 greenness — legacy note, no color cap | EMR-A5 | agent | derived traits table |
| EMR-E7 | [x] `docs/Genome_Definition.md` — global mechanics table (collision gone); repro cooldown note (~3 s anti-spam) | EMR-B6 | agent | |
| EMR-E8 | [x] `docs/EVOLUTION_MODEL_REVISION.md` — update progress tracker to `[x]` | EMR-F5 | agent | this update |
| EMR-E9 | [x] `README.md` — align feature list with revised model | EMR-B6, EMR-D9 | agent | |
| EMR-E10 | [x] `docs/ROADMAP.md` — remove collision-damage milestone if stated as active | EMR-B6 | agent | |
| EMR-E11 | [x] `docs/ENERGY_AND_ENVIRONMENT.md` — confirm hazard-only damage | EMR-B6 | agent | |
| EMR-E12 | [x] `docs/ENTITIES.md` — confirm spawn + interaction tables final | EMR-C5, EMR-B6 | agent | |
| EMR-E13 | [x] `docs/PHASE_4.md` — strike §4B collision damage, acceptance tests, config examples (`collision_damage_scale`, collision integration test refs) | EMR-B6 | agent | v2 supersession notes |
| EMR-E14 | [x] `docs/GENOME_REVISIONS.md` — mark mutation-weight / `movement_pace` pick prob as historical | EMR-D9 | agent | |
| EMR-E15 | [x] `docs/PHASE_5.md` — update `genome/mutation.rs` references | EMR-D9 | agent | |
| EMR-E16 | [x] `docs/PHASE_3.md`, `docs/PHASE_9.md`, `docs/VISION.md` — collision / greenness cross-refs | EMR-B6, EMR-A5 | agent | |

---

## Phase F — Final acceptance

| ID | Task | Depends on | Owner | Done notes |
|----|------|------------|-------|------------|
| EMR-F1 | [x] `cargo test` full workspace green | EMR-A4, EMR-B5, EMR-C4, EMR-D6 | agent | 243 lib + integration tests |
| EMR-F2 | [x] Headless run on `configs/default.toml` completes without panic | EMR-F1 | agent | 600-tick release run, seed 42, exit 0 |
| EMR-F3 | [x] Manual verify: two agents overlap at speed → no HP loss | EMR-B1 | agent | `overlapping_v2_agents_do_not_lose_hp_from_collision` |
| EMR-F4 | [x] Manual verify: green ancestor + mutations can raise movement genes | EMR-D4 | agent | `mutation_can_raise_movement_from_sedentary_green_ancestor` |
| EMR-F5 | [x] Check all **Acceptance criteria** below marked `[x]` | EMR-F1 | agent | AC-1–AC-8 |

### Acceptance criteria (check when F5 runs)

- [x] **AC-1** v2 child vitality from `split_endowment_to_child` only; never `spawn_energy`
- [x] **AC-2** Initial population uses `spawn_energy` + `spawn_health_frac`
- [x] **AC-3** `photo_efficiency_raw = 0xFF` + `color_g = 0x00` ⇒ full photo gene (pre-triad)
- [x] **AC-4** Collision overlap does not reduce `core_hp`; hazards still do
- [x] **AC-5** Offspring mutations: `passes ∈ [0, 4]`; one gene per pass; ±1 (rare ±3/4)
- [x] **AC-6** No v2 bit-flip or resample mutation paths remain
- [x] **AC-7** `Genome_Definition.md` matches implemented behavior
- [x] **AC-8** CI / full test suite green on `default.toml`

---

## Phase G — v1 deprecation (separate PR, after Phase F)

> **Do not start** until Phase F is complete and v2 regression is stable.

| ID | Task | Depends on | Owner | Done notes |
|----|------|------------|-------|------------|
| EMR-G1 | [x] Remove `AnyGenome::V1` / `AnyPhenotype::V1`; collapse dispatch in `src/genome/decode.rs` | EMR-F5 | | |
| EMR-G2 | [x] Delete `src/genome/encoding.rs`, `phenotype.rs`, `mutation.rs` (v1); remove v1 branches in sim systems | EMR-G1 | | |
| EMR-G3 | [x] Remove `configs/v1_regression.toml`, `SimConfig::load_v1_regression()`, v1 tests | EMR-G2 | | |
| EMR-G4 | [x] Archive `docs/GENOME.md` + `docs/GENOME_REVISIONS.md` (redirect to `Genome_Definition.md`) | EMR-G2 | | |
| EMR-G5 | [x] Delete v1-only `ColorPhotosynthesisConfig` / `[genetics.color_photosynthesis]` + `greenness` decode in `phenotype.rs`; update `inspector.rs` greenness row | EMR-G2 | | |

---

## Decisions locked

| Topic | Decision |
|-------|----------|
| Repro cooldown | **Keep** `repro_cooldown_ticks = 192` (~3.2 s @ 60 Hz) |
| Spawn vitality | **Initial pop only:** `spawn_energy` + `spawn_health_frac`; offspring = parent endowment |
| Color | **Cosmetic only**; no efficiency caps from color nibbles |
| Collision | **No** entity–entity damage; hazards remain |
| Mutation | Gene-level ±increment; **0–4** passes/child; no bit flip / resample |
| Genome v1 | Remove in **Phase G** after v2 revision ships |

---

## Appendix: spec reference

### Spawn vitality (current vs target)

| Spawn path | Energy | Health |
|------------|--------|--------|
| Initial population | `economy.spawn_energy` | `spawn_health_frac × max_health` |
| v2 child | Endowment overflow | `min(spent, child_max_health)` first |

Endowment example: parent spends **120**, child `max_health = 100` → **100 HP + 20 energy**.

### Mutation config (target)

```toml
[genetics.mutation_v2]
passes_min = 0
passes_max = 4
p_large_step = 0.10
large_step_min = 3
large_step_max = 4
```

**Per pass:** pick one gene → direction ±1 → magnitude 1 (90%) or 3–4 (10%) → clamp → write bytes.

**Remove from `[genetics]`:** `p_bit_flip`, `p_point`, `p_resample`, `p_dup`, `p_del`, `mutations_per_offspring`, `delta_max`.

### EVO2 mutable gene registry

Offsets = post-header (`HEADER_LEN`).

| Gene | Core bytes | Width | Domain |
|------|------------|-------|--------|
| `color_r` / `color_g` / `color_b` | 0, 1, 2 | u8 each | 0–255 |
| `body_radius` | 3–4 | u16 BE | 0–65535 |
| `armor` | 5–6 | u16 BE | 0–65535 |
| `healing_rate` | 7–8 | u16 BE | 0–65535 |
| `energy_health_ratio` | 9 | u8 | 0–255 |
| `repro_threshold` | 10 | u8 | 0–255 |
| `offspring_endowment` | 11 | u8 | 0–255 |
| `movement_desire` | 12 | u8 | 0–255 |
| `turning_desire` | 13 | u8 | 0–255 |
| `max_speed` | 14 | u8 | 0–255 |
| `movement_efficiency` | 15 | u8 | 0–255 |
| `photo_efficiency` | 16 | u8 | 0–255 |
| `biomass_efficiency` | 17 | u8 | 0–255 |
| `minerals_efficiency` | 18 | u8 | 0–255 |
| `sensor_mask` / `sensor_strength` | 19, 20 | u8 each | 0–255 |
| `pursue_slot_count` | 21 | u8 | 0–`max_pursue_slots` |
| `avoid_slot_count` | 22 | u8 | 0–`max_avoid_slots` |
| `pursue_slot_i` / `avoid_slot_i` | var | 6 bytes/slot | step `type u16` + `value u32` |

No genome length change in this revision (no slot dup/del).

### File index

```
src/genome/phenotype_v2.rs       # Phase A
src/genome/mutation_v2.rs        # Phase D
src/genome/mutation.rs           # Phase G (delete)
src/sim/systems/interactions.rs  # Phase B
src/sim/spawn.rs                 # Phase C
src/config.rs                    # Phases B, D
configs/*.toml                   # Phases B, D
docs/Genome_Definition.md        # Phases A, C, D, E
```

### Suggested pick-up order for a single agent

```
A1 → A2 → A3 → A4 ─┐
B1 → B2 → B3 → B4 → B5 ─┤
C1 → C2 → C3 → C4 ─────┼→ F1 → F2 → F5
D1 → D2 → D3 → D4 → D6 ┘
         E* (parallel after code)
         G* (after F)
```

### Optional (out of scope)

- [ ] Allow `repro_cooldown_ticks = 0` in config
- [ ] Pursue/avoid slot dup/del mutations (genome resize)

---

## Audit — delete / edit inventory

Verified against repo on **2026-06-05**. Use this when a task ID is unclear or grep misses a reference.

### Code — delete or rewrite

| Path | Action | Revision |
|------|--------|----------|
| `src/genome/phenotype_v2.rs` | **Delete** `high_nibble_cap()`, cap fields on `PhenotypeV2`, cap args to `apply_triad` | A |
| `src/genome/mutation_v2.rs` | **Delete** `bit_flip_v2`, `nibble_step_v2`, `resample_byte_v2`, `apply_one_mutation_pass_v2`; **rewrite** `mutate_offspring_v2` | D |
| `src/genome/mutation.rs` | **Delete entire file** (Phase G) or gut operators | G |
| `src/sim/systems/interactions.rs` | **Delete** `COLLISION_DAMAGE_MIN_SPEED_FRACTION`, pairwise loop in `accumulate_interaction_damage`, collision_scale read; **delete tests** `collision_*`, `deterministic_collision_*`, `overlapping_agents_lose_hp_without_despawn` | B |
| `src/config.rs` | **Delete** `collision_damage_scale`, `p_bit_flip`, `p_point`, `p_resample`, `p_dup`, `p_del`, `mutations_per_offspring`, `delta_max`, `default_mutations_per_offspring`, `default_delta_max`; **add** `MutationV2Config` | B, D |
| `configs/default.toml.bak` | **Delete file** | D |
| `src/genome/encoding.rs` | **Delete** (v1) | G |
| `src/genome/phenotype.rs` | **Delete** (v1) | G |
| `configs/v1_regression.toml` | **Delete** (v1) | G |

### Code — edit (keep file)

| Path | What to change | Revision |
|------|----------------|----------|
| `src/sim/spawn.rs` | Split initial vs offspring builders; fix offspring test fixture | C |
| `src/sim/components.rs` | `vitality_spawn_pools_for` doc; `from_any_parts` spawn_energy footgun; collision comments | C, B |
| `src/sim/systems/reproduction_v2.rs` | Test sets `mutations_per_offspring = 0` → use new `mutation_v2` config | D |
| `src/sim/systems/movement.rs` | Module comment re collision damage | B |
| `src/sim/systems/death.rs` | Test comment listing `interactions.rs` | B |
| `src/sim/systems/mod.rs` | Pipeline comment if it says "collision" | B |
| `src/sim/acceptance.rs` | Hazard/armor tests OK; no collision-specific asserts found — re-run after B | B |
| `src/genome/mod.rs` | Re-exports after mutation rewrite | D |
| `src/visual/inspector.rs` | v1 `Greenness` row — remove in Phase G | G |
| `src/sim/metrics.rs` | `mean_greenness` / `mean_skin_hardness` — v1/v2 dual; OK for now | G |
| `src/config.rs` | `ColorPhotosynthesisConfig` — delete with v1 | G |

### Config TOML — edit all 7 + delete 1

| File | Keys to remove | Keys to add |
|------|----------------|-------------|
| `configs/default.toml` | `p_bit_flip` … `delta_max`, `collision_damage_scale` | `[genetics.mutation_v2]` |
| `configs/genome_v2.example.toml` | same | same |
| `configs/mineral_vent_v2.toml` | same | same |
| `configs/vent_lab.toml` | same | same |
| `configs/hazard_lab.toml` | same | same |
| `configs/green_ab.toml` | same | same |
| `configs/v1_regression.toml` | same (until G) | same or skip until G |
| `configs/default.toml.bak` | — | **delete file** |

Keep `nibble_levels` in `[genetics.decode_v2]` — still used for turning/sensor nibble denominators.

### Docs — edit

| File | Sections / topic |
|------|------------------|
| `docs/Genome_Definition.md` | Color §54 cap bullet; ancestor table G-nibble; Vitality §242–243 caps; global table collision; interactions §279; doc-map §314; **add** mutation § |
| `docs/GENOME.md` | Mutation operators table §211–224; color greenness tradeoff §74; §4.7 greenness formulas |
| `docs/CONFIG.md` | `nibble_levels` cap row; `collision_damage_scale`; `p_bit_flip` etc.; `digest_eff` + nibble cap; `color_photosynthesis` remove note |
| `docs/ENTITIES.md` | Spawn §54; Reproduce §81; collision §109–116, §124 |
| `docs/SIMULATION.md` | Tick step 8; `[genetics]` mutation keys §89–91; `color_photosynthesis` row |
| `docs/ENERGY_AND_ENVIRONMENT.md` | Collision row §190 |
| `docs/ARCHITECTURE.md` | Collision despawn/damage §104; mutation in diagram §22, §75 |
| `docs/PHASE_4.md` | **Large:** collision throughout (§4B, tests, config examples, acceptance) |
| `docs/PHASE_1.md` | §1H mutation operators; config example §82–88 |
| `docs/PHASE_3.md` | Collision damage ref §33 |
| `docs/PHASE_5.md` | `mutation.rs` REPRO_COST comment §119, §285 |
| `docs/PHASE_9.md` | greenness / repro_cooldown tuning §155 |
| `docs/ROADMAP.md` | Collision-despawn §56; Phase 4 collision §126–278 |
| `docs/GENOME_V2_ARCHITECTURE_ROADMAP.md` | Nibble cap §96, §232; interactions comment §172 |
| `docs/GENOME_REVISIONS.md` | Mutation weights §242; movement_pace prob §368 |
| `docs/VISION.md` | Despawn-on-collision placeholder §51 |
| `README.md` | Collision + `collision_damage_scale` §73–84, §119 |

### Docs — no change needed (already aligned or historical)

| File | Why |
|------|-----|
| `docs/EVOLUTION_MODEL_REVISION.md` | This checklist |
| `docs/GENOME_V2_ARCHITECTURE_ROADMAP.md` §22 sessile ancestor | Still valid |

### Known gaps (not in revision scope)

| Issue | Location |
|-------|----------|
| `LifeBundle::try_from_genome_bytes` uses `from_any_parts` → `spawn_energy` | `components.rs` — initial-spawn only; offspring use `from_any_parts_with_vitals` |
| Sexual reproduction / morphology mutation | Not started (future) |

### Grep commands for handoff verification

```bash
# After Phase A
rg 'high_nibble_cap|photo_nibble_cap|biomass_nibble_cap|minerals_nibble_cap' src/

# After Phase B
rg 'collision_damage|COLLISION_DAMAGE' src/ configs/ docs/

# After Phase D
rg 'p_bit_flip|p_point|p_resample|bit_flip|resample_byte|mutations_per_offspring' src/ configs/

# Spawn leak check
rg 'spawn_energy' src/sim/spawn.rs src/sim/systems/reproduction
```

---

## Changelog

| Date | Agent | Summary |
|------|-------|---------|
| 2026-06-05 | — | Created checklist from review decisions |
| 2026-06-05 | audit | Full delete/edit inventory; +11 tasks (58 total) |
| 2026-06-05 | audit | Post-completion doc/config drift cleanup (armor mitigation, lab configs, color_photosynthesis) |

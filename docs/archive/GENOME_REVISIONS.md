# Genome & reproduction revisions (backlog)

Design issues and proposed changes for core loci v1. Tackle **one item at a time**; check off acceptance when done and update [GENOME.md](GENOME.md) / configs.

**Related:** [GENOME.md](GENOME.md) (current layout), [ENTITIES.md](ENTITIES.md) (spawn/repro/death), [`src/sim/systems/reproduction.rs`](../src/sim/systems/reproduction.rs), [`src/sim/systems/healing.rs`](../src/sim/systems/healing.rs).

| # | Topic | Status |
|---|--------|--------|
| 1 | Repro threshold ceiling (`e_max`) | Done |
| 2 | Offspring endowment ceiling | Done |
| 3 | Locomotion investment purpose | Done |
| 4 | Repro cooldown vs threshold | Done |
| 5 | Remove `repro_risk_factor` | Done |
| 6 | Repair: drop efficiency gene, add repair speed | Done |
| 7 | [Movement pace modifier](#7-movement-pace-modifier-speed--energy) | Done |
| — | [Movement calibration](#movement-calibration-target) | Done |

---

## Movement calibration target

**Design invariant (default ancestor and all body sizes):**

```text
move_energy_spent = distance_traveled / (10 * body_radius)
```

- Traveling exactly **10 × `body_radius`** world units costs **1 energy** for that agent.
- **Canonical example:** `body_radius = 2.5` → **25** world units for **1** energy.
- **Default ancestor** (`body_radius ≈ 12` at `r_ref`) → **120** world units for **1** energy.
- This is a **distance** rule, not “1 energy per second while moving.”

**Scaling intuition (vs current code)**

- **1 energy always buys the same multiple of your own size** (10 radii of travel), whether you are tiny or huge.
- For a **fixed absolute distance**, **smaller** bodies pay **more** energy (cost per world unit ∝ `1 / body_radius`). Example: **25** units costs **1** energy at `r = 2.5`, but only **~0.21** energy at `r = 12`.
- That is the **opposite** of today’s `(body_radius / r_ref)²` per-second discount for small agents — small bodies should not get cheaper movement in absolute world units.

**Why it matters**

- Ties locomotion spend to morphology players can see (body size).
- Makes `locomotion_investment` and [#7 movement pace](#7-movement-pace-modifier-speed--energy) meaningful: structural speed vs optional “spend more energy to go faster.”
- Replaces the old “~1 J/s at half max speed at reference size” tuning goal ([ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md) — superseded here until code matches).

**Current code (gap)**

- Movement is billed **per tick from speed ratio**, with `(body_radius / r_ref)²` scaling ([`movement_energy_per_tick`](../src/genome/phenotype.rs)) — favors small bodies in absolute terms.
- Default ancestor **`max_speed` locus = 0** → cannot move, so the invariant is untestable at worldgen.
- Implementation options (pick one in #3):
  1. **Distance-indexed (preferred):** `move_energy_tick += |Δposition| / (10 * body_radius)`; drop or repurpose area-based scaling.
  2. **Calibrate time-based model:** tune economy + speed so a straight-line trip of `10 * body_radius` totals 1 energy (bridge only; harder to keep exact when speed or radius changes mid-trip).

**Ancestor defaults (when implementing)**

- Ancestor must have **non-zero** `max_speed` (and consistent locomotion bytes) so movement is possible.
- Acceptance test: at `body_radius = 2.5`, straight-line travel of **25** units costs **~1** energy; at default ancestor radius, **120** units costs **~1** energy (`|Δenergy - distance/(10*r)| < ε`).

**Touch points:** [`src/sim/systems/movement.rs`](../src/sim/systems/movement.rs), [`src/genome/phenotype.rs`](../src/genome/phenotype.rs), `[economy]` in configs, #3 and #7 below.

**Related:** [#7 Movement pace modifier](#7-movement-pace-modifier-speed--energy) — evolvable “run faster, pay more” on top of the base distance rate.

---

## 1. Repro threshold should scale above 100

**Loci:** core bytes 10–11 (`repro_threshold`), decoded via `[genetics.decode_ranges]` `e_min` / `e_max`.

**Current behavior**

- `u16` locus maps linearly to `[e_min, e_max]` (default **15–100** in `configs/default.toml`).
- Ancestor `0xFFFF` → **100** energy required to attempt reproduction.
- Parent pays **`repro_threshold` energy** on success ([`reproduction.rs`](../src/sim/systems/reproduction.rs)); that same value caps child HP conversion (`min(endowment, threshold, core_max_hp)`).

**Problem**

- 100 is effectively the **top** of the viable range today, not a “low” baseline.
- As total energy budget and `core_max_hp` evolve upward (e.g. predators stockpiling **500** before repro), threshold must be able to decode **much higher** without hitting a config ceiling.
- `economy.core_max_hp` (default 100) also caps how much energy/HP the system can represent per entity unless that evolves too.

**Proposed direction**

- Raise or decouple `e_max` (and likely `core_max_hp` / energy storage) so genotypes can encode thresholds in hundreds+.
- Treat **~100 as a low ancestor default**, not `e_max`.
- Decide whether threshold and max energy/HP share one decode range or separate loci (document in GENOME.md after choice).

**Open questions**

- Upper bound: fixed config max (e.g. 2000), or tie `e_max` to evolved `core_max_hp`?
- Should paying 500 to reproduce always imply transferring 500 HP to the child, or can threshold and endowment diverge? (See #2.)

**Touch points**

- `configs/*.toml` → `[genetics.decode_ranges]`, `[economy]`
- `src/genome/phenotype.rs` decode
- `src/sim/systems/reproduction.rs`, spawn/energy components
- Tests: `ancestor_phenotype_within_expected_bounds`, repro integration tests

**Acceptance (draft)**

- [x] Config + docs define new `e_min` / `e_max` (100 at or below mid-range).
- [x] Ancestor hex still viable at worldgen; at least one test reproduces with threshold > 100 when gene maxed.
- [x] No silent clamp at 100 unless explicitly documented as a balance cap.

---

## 2. Offspring endowment should scale above 100

**Loci:** core bytes 14–15 (`offspring_endowment`), same decode range as threshold today (`e_min` / `e_max`).

**Current behavior**

- Decoded to child starting **`core_hp`** after reproduction: `child_start_hp = min(offspring_endowment, repro_threshold, core_max_hp)`.
- Ancestor `0xFFFF` → **100** HP child (with default caps).

**Problem**

- Same ceiling as #1: high-investment parents cannot encode “large, well-funded offspring” if endowment tops out at 100 while economy might later support larger pools.

**Proposed direction**

- Align endowment max with revised energy/HP scale (#1).
- Clarify tradeoff: high endowment = more child HP per repro vs parent leaving with 0 energy after paying full threshold.

**Open questions**

- Independent locus range vs always `min(endowment, threshold)`?
- Should `core_max_hp` itself become evolvable (separate locus) rather than only config?

**Touch points**

- Same as #1 plus `build_offspring_spawn_params` / spawn energy=0 contract in [ENTITIES.md](ENTITIES.md)

**Acceptance (draft)**

- [x] Endowment gene can decode above 100 when config ranges allow.
- [x] Child HP formula documented and tested for threshold=500, endowment=400, etc.

---

## 3. Locomotion investment — clarify or redesign

**Loci:** core bytes 8–9 (`locomotion_investment`); bytes 24–25 today (`max_speed` multiplier) — candidate home for [#7 `movement_pace`](#7-movement-pace-modifier-speed--energy).

**Design target:** see [Movement calibration target](#movement-calibration-target) — **1 energy per 10 body radii** at **`movement_pace = 1`**. Locomotion sets baseline musculature / speed cap; pace gene scales speed and energy together ([#7](#7-movement-pace-modifier-speed--energy)).

**Current behavior**

- Decodes to `[0, loco_max]` (default `loco_max = 1.0`).
- Enters **max speed** only:  
  `max_speed = (v_base + k_v * locomotion_investment) * (body_radius/r_ref) * speed_mult - move_penalty`  
  ([`phenotype.rs`](../src/genome/phenotype.rs)).
- Ongoing movement cost uses **speed-based** `movement_energy_per_tick` with `(body_radius/r_ref)²`, not distance / `(10 * body_radius)`.
- Does **not** directly add idle metabolism (`k_move_meta = 0` by default).
- Default ancestor sets **`max_speed` locus (bytes 24–25) to `0x0000`**, so locomotion investment **does not move the ancestor** today despite non-zero locomotion bytes.

**Problem (design confusion)**

- Reads like “pay metabolism for nothing” when speed multiplier is zero.
- Two genes (`locomotion_investment`, `max_speed` locus) both feed `max_speed` without a coupled energy tradeoff ([#7](#7-movement-pace-modifier-speed--energy) fixes that for the pace locus).
- Per-second, area-scaled billing is the **wrong direction**: it makes small bodies cheaper per world unit; the target makes them **more** expensive per world unit (while 1 energy still buys `10 * body_radius` distance).

**Proposed direction**

1. Implement [movement calibration](#movement-calibration-target) (distance-indexed billing preferred).
2. **Locomotion investment** = musculature / speed cap (how quickly you spend the 1-energy “10 radii” budget in wall-clock time). Optional small idle upkeep if high locomotion should not be free at rest.
3. **Repurpose bytes 24–25** as `movement_pace` ([#7](#7-movement-pace-modifier-speed--energy)) instead of a free speed multiplier.
4. **Ancestor hex:** `movement_pace = 1.0` + locomotion so default spawn can move; verify **120** world units (at `r = 12`) and **25** units (at `r = 2.5`) each cost ~1 energy at pace 1.

**Open questions**

- Keep dual loci (locomotion + speed multiplier) or consolidate?
- Should `v_base` / `k_v` be retuned after distance billing so ancestor cruise speed feels right in the UI?

**Touch points**

- `src/genome/phenotype.rs`, `src/sim/systems/movement.rs`
- [GENOME.md](GENOME.md), [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md)
- `configs/default.toml` → `ancestor_hex`, `[economy]` move fields

**Acceptance (draft)**

- [x] Movement spends energy per [calibration target](#movement-calibration-target) (test at `r_ref` and at least one non-reference radius).
- [x] Default ancestor can move; locomotion + `movement_pace` loci documented; no dead investment at worldgen.
- [x] Player-facing note: at `movement_pace = 1`, **1 energy buys 10 × body_radius**; higher pace finishes trips faster but costs more energy ([#7](#7-movement-pace-modifier-speed--energy)).

---

## 7. Movement pace modifier (speed ↔ energy)

**Proposed locus:** core bytes **24–25** (repurpose current `max_speed` multiplier), decoded as **`movement_pace`** (`u16` → `[pace_min, pace_max]`, ancestor **`pace = 1.0`**).

**Tag:** Tradeoff — **double speed, double energy** (linear coupling).

**Design**

Baseline [movement calibration](#movement-calibration-target) applies at `movement_pace = 1`:

```text
base_move_energy = distance_traveled / (10 * body_radius)

max_speed_base = (v_base + k_v * locomotion_investment) * (body_radius / r_ref) - move_penalty

movement_pace = decode(bytes 24–25)   // e.g. [0.5, 2.0], default 1.0

max_speed = max_speed_base * movement_pace

move_energy_spent = base_move_energy * movement_pace
```

**Linear rule (required):**

```text
move_energy_spent ∝ movement_pace
max_speed       ∝ movement_pace
```

- **`pace = 2`:** **2×** top speed and **2×** energy per world unit (for a fixed trip: **2×** total energy, **~½** time).
- **`pace = 0.5`:** half speed, half energy per distance (slow, efficient crawling).
- At **`pace = 1`:** matches calibration examples (`r = 2.5` → 25 units / 1 energy; `r = 12` → 120 units / 1 energy).

**Not** independent “free speed” from bytes 24–25: pace always multiplies **both** speed and distance-indexed cost. Locomotion (bytes 8–9) remains the structural investment in how fast the body *can* move at `pace = 1`.

**Config (draft)**

```toml
# [genetics.decode_ranges]
pace_min = 0.5
pace_max = 2.0
```

Tune range so mutation can explore sluggish vs sprinting niches without breaking sim (optional hard cap at `pace_max` in movement system).

**Current behavior**

- Bytes 24–25 scale `max_speed` only; energy uses separate speed-ratio billing with `(body_radius/r_ref)²` — **no** 1:1 speed–energy tradeoff.

**Touch points**

- `src/genome/encoding.rs` (`core::MAX_SPEED` rename), `phenotype.rs` decode, `movement.rs` cost + clamp
- [GENOME.md](GENOME.md) core table, [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md)
- `configs/*.toml` `ancestor_hex` (pace raw → 1.0, e.g. mid-range `u16` for 1.0 in `[pace_min, pace_max]`)
- ~~Mutation weights in `mutation.rs` (`CORE_LOCI`)~~ — **historical (v1).** v2 uses uniform gene pick in `mutation_v2.rs` ([Genome_Definition.md](Genome_Definition.md#mutation-v2)).

**Acceptance (draft)**

- [x] At `pace = 1`, calibration tests from [movement target](#movement-calibration-target) pass.
- [x] `movement_pace` scales move energy linearly with pace (see `movement_energy_scales_linearly_with_pace`).
- [x] At higher pace, distance moved per energy decreases proportionally (same formula).
- [x] Docs and GENOME table list `movement_pace`; no decoupled “speed only” locus at 24–25.

---

## 4. Repro cooldown — still needed?

**Decision (2026-06-04): Option C — config-only fixed cooldown.** Byte 16 reserved; not evolvable.

**Rationale:** `repro_threshold` is the primary economic gate, but fast energy income (vents, bright bands) plus a low threshold could allow per-tick reproduction spam without a timer. After #5 removed per-agent `repro_risk_factor`, a fixed cooldown remains the simple anti-spam brake. Evolvable cooldown added genome complexity without a clear niche once threshold scales to hundreds (#1–2).

**Config:** `[economy] repro_cooldown_ticks` (default **192** — matches former ancestor decode with raw byte `0x08` and old `cd_min`/`cd_max`).

**Migration:** Saved hex genomes may still carry byte 16; it is ignored on decode. Remove `cd_min` / `cd_max` from `[genetics.decode_ranges]`.

**Loci:** core byte 16 — **reserved** (was `repro_cooldown`).

**Previous behavior**

- Decoded to tick count in `[cd_min, cd_max]` (default **120–2400** ticks).
- After reproduction, `repro_ticks_remaining` blocks further attempts until cooldown expires ([`reproduction.rs`](../src/sim/systems/reproduction.rs)), independent of energy.

**Acceptance**

- [x] Decision recorded in GENOME.md.
- [x] Byte 16 reserved; migration for saved hex genomes noted.
- [x] Cooldown applied from `economy.repro_cooldown_ticks` in reproduction system.

---

## 5. Remove `repro_risk_factor`

**Decision (2026-06-04): Removed.** Byte 17 reserved. Config `p_death_on_repro_base` / `p_damage_on_repro_base` apply at full rate (no per-agent scaling).

**Loci:** core byte 17 — **reserved** (was `repro_risk_factor`).

**Previous behavior**

- Scaled config `p_death_on_repro_base` and `p_damage_on_repro_base`.
- Death chance also increased when post-transfer energy was low relative to `repro_safety_margin` ([`repro_death_probability`](../src/sim/systems/reproduction.rs)).

**Acceptance**

- [x] No phenotype field / genome byte for per-agent repro risk.
- [x] Reproduction tests updated; docs no longer list `repro_risk_factor` as v1 locus.

---

## 6. Repair — direct energy↔HP, add repair speed

**Decision (2026-06-04):** Bytes 20–21 encode **`repair_rate`** (`u16 → [repair_rate_min, repair_rate_max]`, ancestor **`rate = 1.0`**). Energy cost is **`hp_repaired * repair_energy_per_hp`** only. Per-tick throughput cap: **`repair_rate * economy.repair_hp_per_tick_at_rate_1`**.

**Loci:** core bytes 20–21 (was `repair_efficiency`).

**Current behavior**

- Decoded efficiency in `[0.1, 2.0]` (gene range, not config).
- Cost per tick: `(hp_repaired / repair_efficiency) * repair_energy_per_hp` ([`healing.rs`](../src/sim/systems/healing.rs)).
- Global `repair_energy_per_hp` in config sets baseline Joules per HP.

**Problem**

- **Efficiency gene** breaks a simple model: “energy and HP should be directly related” at a fixed exchange rate.
- Missing **repair speed** tradeoff: healing faster should cost **more energy per second** (or per HP), not cheaper per HP via efficiency.

**Proposed direction**

- Remove `repair_efficiency` from genome; use config `repair_energy_per_hp` (and optional policy) as the single conversion rate.
- Add new locus (candidate: bytes 20–21 freed by efficiency removal, or byte 17 if #5 frees it): **`repair_rate`** or similar — HP restored per tick scales up, energy spend scales up proportionally (same HP:energy ratio, higher throughput).

**Open questions**

- Cap repair rate per tick vs unlimited burst?
- Interaction with `skin_hardness` / hazard damage — armor still reduces damage, repair restores at chosen rate?

**Touch points**

- `src/sim/systems/healing.rs`, `Phenotype`, GENOME.md locus table, Phase 4 docs

**Acceptance (draft)**

- [x] Repair cost = `hp_gained * repair_energy_per_hp` with no per-agent efficiency divisor.
- [x] Evolvable repair speed with documented energy tradeoff; tests for “fast repair drains energy faster”.

---

## Cross-cutting: energy, HP, and config caps

Several items share **`economy.core_max_hp`** and decode **`e_max`**:

| Knob | Default | Tied to |
|------|---------|--------|
| `core_max_hp` | 100 | Spawn HP, repro child cap, starvation death |
| `e_max` | 100 | `repro_threshold`, `offspring_endowment` decode max |
| Ancestor `0xFFFF` on repro loci | — | Hits both caps simultaneously |

A coherent pass should decide:

1. Which limits are **config balance** vs **evolvable**.
2. New default ancestor targets (~100 = “low”) after ranges expand.
3. Whether `repro_cost` bytes (12–13) stay reserved or get repurposed.

---

## Suggested implementation order

1. **Movement calibration** + **#3** + **#7** — distance billing, `movement_pace` locus (2× speed = 2× energy), movable ancestor at `pace = 1`.
2. **#1 + #2 + cross-cutting** — raise scales and document; unblocks predator/high-investment scenarios.
3. **#5** — remove repro risk (small, isolated).
4. **#4** — decide cooldown after #5 and low-threshold behavior are clear.
5. **#6** — repair model (depends on freed bytes from #5 / #4).

---

## Changelog

| Date | Note |
|------|------|
| 2026-06-03 | Initial backlog from design review (ancestor at `e_max` = 100). |
| 2026-06-03 | Movement calibration: **1 energy = 10 × body_radius** (e.g. `r=2.5` → 25 units). |
| 2026-06-03 | **#7** `movement_pace` locus: linear **2× speed = 2×** move energy (repurpose bytes 24–25). |
| 2026-06-04 | Implemented **#1–3**, **#7**, movement calibration (`e_max`/`core_max_hp` 2000; ancestor repro ~100). |
| 2026-06-04 | **#4** config-only `repro_cooldown_ticks`; byte 16 reserved. **#5** removed `repro_risk_factor`; byte 17 reserved. |
| 2026-06-04 | **#6** `repair_rate` locus (bytes 20–21); fixed `repair_energy_per_hp`; removed `repair_efficiency`. |

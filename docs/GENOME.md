# Genome

> **Legacy (implementation v1):** The repo still uses this `EVO1` layout. The **canonical target design** for genetics and related simulation behavior is [Genome_Definition.md](Genome_Definition.md). Implementation plan: [GENOME_V2_ARCHITECTURE_ROADMAP.md](GENOME_V2_ARCHITECTURE_ROADMAP.md).

This document defines DNA encoding (version 1), trait design philosophy, core loci layout, phenotype decoding formulas, morphology and sensors, mutation operators, and initial population guidelines.

## Encoding (GenomeVersion = 1)

**Source of truth:** versioned binary buffer (`Vec<u8>` or fixed `[u8; CORE_LEN]`).

### Layout

| Region | Bytes | Contents |
|--------|-------|----------|
| **Header** | 4 | Magic `b"EVO1"`, `version` (u8), reserved |
| **Core segment** | 28 | Fixed loci (see table below) |
| **Extension** | variable | Morphology tree (Phase 7+); absent in early phases |

Total core size: **32 bytes** (4 header + 28 payload). `CORE_LEN` in config must match.

### Debug views

Hex or alphanumeric strings are **display/export only**, not authoritative. Tools may render `genome.to_hex()` for logging; mutations always operate on bytes.

### Invalid genomes

| Condition | Behavior |
|-----------|----------|
| Wrong magic or version | Stillborn (no spawn) or revert to ancestor template |
| Extension parse failure | Ignore extension; use core phenotype only |

## Trait design philosophy

Avoid naked “efficiency” genes that monotonically improve fitness when maximized. Every locus is tagged:

| Tag | Meaning | Examples |
|-----|---------|----------|
| **Tradeoff** | Higher helps X, hurts Y via coupled formula | `body_radius`, `photo_strength`, `offspring_endowment`, `color_*` (greenness) |
| **Cost** | Benefit requires ongoing or one-time energy / mobility tax | `skin_hardness`, sensor bandwidth on nodes |
| **Threshold** | Benefit only after viability cliff | `chemo_potential` near vents |

### Reproduction risk

Reproduction carries config-only stochastic risk (no per-agent gene):

```
energy_margin = internal_energy / repro_threshold   // after vitality transfer
p_death = p_death_on_repro_base * max(0, 1 - energy_margin / repro_safety_margin)
p_damage = p_damage_on_repro_base                    // separate roll; may chip core HP
```

Reproduction also pays **vitality transfer** (see below); low post-transfer energy increases `p_death`.

### Reproduction cooldown

Fixed **`economy.repro_cooldown_ticks`** (default **192** ≈ 3.2 s at 60 Hz) blocks further attempts after success or stillbirth. Not evolvable — byte 16 reserved. Prevents low-`repro_threshold` spam when energy income is fast (vents, bright bands); `repro_threshold` remains the primary economic gate.

### Anti-convergence tuning

If one genotype dominates all light levels, hazard zones, and densities:

1. Add or strengthen coupling (upkeep, move penalty).
2. Increase environmental heterogeneity (gradient, hazards, vents).
3. Add spatial shading (energy model C) before adding new loci.

Do not add more independent “efficiency” sliders without tradeoffs.

## Core loci table (v1)

Byte offsets are relative to **start of core segment** (byte 0 = first byte after header).

| Offset | Locus | W | Tag | Decode & phenotype |
|--------|-------|---|-----|------------------|
| 0–2 | `color_r`, `color_g`, `color_b` | 1 each | Tradeoff | `channel / 255.0` → render color; **greenness** drives photosynthesis buff + metabolism penalty ([PHASE_4.md](PHASE_4.md) §4.7) |
| 3 | `body_radius` | 1 | Tradeoff | `radius = r_min + (raw/255) * (r_max - r_min)`; metabolism scale `(radius/r_ref)²` |
| 4–5 | `photo_strength` | 2 | Tradeoff | `u16 → [0, photo_max]`; uptake multiplier; gain also × `(body_radius/r_ref)²`; `photo_upkeep = k_photo * photo_strength` |
| 6–7 | `metabolism_base` | 2 | Tradeoff | `u16 → [meta_min, meta_max]` idle drain per tick |
| 8–9 | `locomotion_investment` | 2 | Tradeoff | `u16 → [0, loco_max]`; drives body-scaled `max_speed` |
| 10–11 | `repro_threshold` | 2 | Neutral/Risk | `u16 → [e_min, e_max]` energy required to attempt repro |
| 12–13 | `repro_cost` | 2 | Legacy | Reserved in v1 layout; **not spent** — use `offspring_endowment` as transfer size |
| 14–15 | `offspring_endowment` | 2 | Tradeoff | Child starting **core HP** from converted parent energy (capped by this gene); child spawns at **0 energy** |
| 16 | `desire_to_move` | 1 | Tradeoff | `raw / 255` → per-tick probability of attempting movement (0 = never, 255 = always); ancestor **`0x00`** |
| 17 | *(reserved)* | 1 | — | Was `repro_risk_factor`; ignored on decode ([revision #5](GENOME_REVISIONS.md#5-remove-repro_risk_factor)) |
| 18–19 | `skin_hardness` | 2 | Cost | Damage reduction; `armor_upkeep = k_armor * hardness`; `move_penalty = k_move * hardness` |
| 20–21 | `repair_rate` | 2 | Tradeoff | `u16 → [repair_rate_min, repair_rate_max]`; max HP/tick = `rate * repair_hp_per_tick_at_rate_1`; cost = `hp * repair_energy_per_hp` ([revision #6](GENOME_REVISIONS.md#6-repair--direct-energyhp-add-repair-speed)) |
| 22–23 | `chemo_potential` | 2 | Threshold | Active only if `raw/65535 > cliff` AND near vent; `chemo_upkeep` always if raw > 0 |
| 24–25 | `movement_pace` | 2 | Tradeoff | `u16 → [pace_min, pace_max]`; scales `max_speed` and move energy **together** (2× pace → 2× speed, 2× energy per distance); see [GENOME_REVISIONS.md](GENOME_REVISIONS.md#7-movement-pace-modifier-speed--energy) |
| 26–27 | `age_metabolism_scale` | 2 | Tradeoff | `u16 → [age_metabolism_scale_min, age_metabolism_scale_max]`; scales logarithmic age metabolism (replaces old-age cap) |

### Derived traits (not stored in DNA)

| Trait | Formula (conceptual) |
|-------|----------------------|
| `max_speed` | `max_speed_base * movement_pace` where `max_speed_base = (v_base + k_v * locomotion) * (body_radius / r_ref) - move_penalty` |
| `movement_pace` | From bytes 24–25; ancestor **`1.0`**. Linear tradeoff with speed and move cost — [§7](GENOME_REVISIONS.md#7-movement-pace-modifier-speed--energy). |
| `desire_to_move` | From byte 16; `raw / 255` gates movement each tick; velocity cleared when idle (no collision drift). |
| `move_cost` | Energy per world unit at pace 1: `1 / (10 * body_radius)`; tick spend = `(distance / (10 * body_radius)) * movement_pace` — [calibration](GENOME_REVISIONS.md#movement-calibration-target) |
| `metabolism_moving` | `metabolism_idle * (1 + k_move_meta * speed_ratio)` (movement energy is separate) |
| `age_metabolism_scale` | Gene multiplier on log-age metabolism (see below) |
| `age_metabolism_per_tick` | `cap_per_tick * ln(1 + age/ref) / ln(1 + cap_ticks/ref) * scale` ([`economy`](../configs/default.toml)) |
| `greenness` | `clamp((g - 0.5*(r+b)) / greenness_scale, 0, greenness_max)` from decoded RGB |
| `green_photo_mult` | `1 + k_green_photo * greenness` — scales local photosynthetic gain |
| `green_metabolism_extra` | `k_green_meta * greenness` — added to idle metabolism per tick |

Decode helper:

```text
fn lerp_u16(raw: u16, min: f32, max: f32) -> f32 {
    min + (raw as f32 / 65535.0) * (max - min)
}
```

Config supplies `min`/`max` per locus; genes pick points along viable ranges, not unbounded optima.

## Default ancestor (Phase 1)

Mid-range bytes for viability (hex of core payload after header):

| Locus | Suggested raw | Rationale |
|-------|---------------|-----------|
| color | `0x88, 0xCC, 0x44` | Greenish, visible |
| body_radius | `109` | `r_ref` (12) — 100% photo/movement size scale |
| photo_strength | `0xFFFF` | ~3.0 uptake (`photo_max = 3`) |
| metabolism_base | `0x3000` | Moderate |
| locomotion_investment | `0x2800` | Moderate speed/cost |
| repro_threshold | `0x0AF6` | ~100 energy to attempt repro (`e_max = 2000`; `0xFFFF` → max) |
| offspring_endowment | `0x0AF6` | Child starts at ~100 core HP (capped by threshold / `core_max_hp`) |
| repro_cost | `0x4000` | Legacy locus (unused in v1 payment) |
| movement_pace | `0x5555` | **1.0** in `[0.5, 2.0]`; movable ancestor; **1 energy / 10 radii** at pace 1 ([calibration](GENOME_REVISIONS.md#movement-calibration-target)) |
| age_metabolism_scale | `0x8000` | Mid scale on log-age penalty |
| skin_hardness | `0` | No armor at worldgen; can evolve |
| chemo_potential | `0` | Inactive until vents |
| `desire_to_move` | `0x00` | Sedentary ancestor (no movement rolls) |
| byte 17 | `0x80` | Legacy ancestor value; **ignored** on decode (reserved) |

Exact byte pattern: `configs/default.toml` → `[genetics.ancestor_hex]`.

## Age metabolism (replaces max-age death)

**Starvation:** `internal_energy == 0` is not fatal. Each tick still runs metabolism; any net deficit after energy is exhausted is applied to `core_hp`. Death is `CoreDestroyed` when HP reaches zero (photosynthesis can still refill energy from empty).

There is **no** hard despawn at a decoded lifespan. Instead, each tick adds a logarithmic penalty (60 Hz assumed):

```text
cap_per_tick = age_metabolism_cap_per_sec / 60
age_metabolism = cap_per_tick
  * ln(1 + age_ticks / age_metabolism_ref_ticks)
  / ln(1 + age_metabolism_cap_ticks / age_metabolism_ref_ticks)
  * age_metabolism_scale
```

Default tuning: negligible at ~1 minute (`age_ticks ≈ 3600`), reaching **+1 energy/s** at ~5 minutes (`age_metabolism_cap_ticks = 18000`). Agents eventually starve from compounding costs unless they reproduce or stay in bright bands.

## Energy → HP reproduction

When `internal_energy >= repro_threshold` and cooldown is zero:

1. Parent pays **`repro_threshold` energy** (parent **core HP unchanged**).
2. Roll `p_death`, then `p_damage` (may reduce parent HP).
3. On success, child spawns with `internal_energy = 0` and `core_hp = min(offspring_endowment, repro_threshold, core_max_hp)`.

Example (ancestor): threshold 100, endowment 100, parent energy 100 → parent ends at 0 energy / 100 HP; child has 0 energy / 100 HP.

`repro_cost` bytes remain in the genome for layout compatibility but are **not** deducted in v1.

## Morphology: appendages & sensors (Phase 5–7)

Separate **morphology extension** buffer (tagged TLV or length-prefixed tree). Nodes are **spectrum-based**, not discrete “unlock eye” organs.

### Node schema (conceptual)

```text
MorphNode {
    parent_index: u16,      // 0xFFFF = attach to core body
    length: f32,
    angle: f32,             // radians relative to parent
    role_weights: {
        locomotion: f32,    // 0..1
        gather: f32,
        armor: f32,
        sensory: f32,
        visual: f32,        // subset of sensory
    },
}
```

### Sensory spectrum (preferred)

| Field | Range | Effect | Cost |
|-------|-------|--------|------|
| `sensory_gain` | 0–1 | General environmental sampling quality | `sensor_upkeep ∝ sensory_gain` |
| `visual_bandwidth` | 0–1 | Light gradients, contrast, mate hue | `∝ visual_bandwidth * body_radius` |
| `chemo_sensitivity` | 0–1 | Soup/vent gradient resolution | Pairs with `chemo_potential` cliff |

High `visual_bandwidth` improves movement/orientation decisions (when behavior system uses it) but drains energy continuously.

### Discrete fallback (prototyping only)

Phase 5 may temporarily map bands of `visual_bandwidth` to tiers (`eye_tier_0` … `eye_tier_3`) for simpler behavior. Migration path: replace tier table with continuous sampling before Phase 7.

### Node costs & damage

- Each node: `upkeep`, `drag` → `appendage_drag`, `repair_surface` → entry in `part_hp[]`
- Damaged sensory nodes: effective bandwidth reduced proportionally to `part_hp / max_hp`
- Mutations: add child, delete subtree, resize, reattach, duplicate branch, nudge `role_weights` ±ε

### Phenotype coupling

Appendages feed derived `move_cost`, `max_speed`, collision hull (polyline or compound circle), reach for gather, and render mesh (procedural polylines in Bevy).

## Mutation operators

Applied after asexual copy from parent. Probabilities from `[genetics]` config.

| Operator | Applies to | Effect |
|----------|------------|--------|
| **Bit flip** | Any byte | Toggle one random bit in chosen byte (rare; larger jump) |
| **Point noise** | One byte of a core locus | One **hex nibble** (high or low 4 bits) steps by ±1; e.g. color `0x88` → `0x98`, `0x89`, `0x78`, or `0x87` |
| **Locus resample** | One locus | Replace locus bytes with RNG uniform in range |
| **Segment duplication** | Core or ext | Copy locus block or subtree; may extend buffer |
| **Segment deletion** | Extension only | Remove subtree; clamp morphology size |
| **Insert junk** | Extension optional | Random bytes in morphology buffer |

`mutations_per_offspring` independent passes (or single pass with multiple hits—document in config).

### Sexual reproduction (Phase 8, future)

- **Crossover:** single-point or uniform on core segment between two parents
- **Morphology:** swap subtrees or per-node blend of `role_weights`
- **Risk:** inbreeding depression optional (not v1)

## Phenotype cache

On spawn and after mutation:

1. Decode core bytes → `Phenotype` struct (all scalar traits + derived values).
2. Parse extension if present → append `Morphology` + extra costs.
3. Store on entity; systems read `Phenotype`, not raw bytes, during tick.

Rebuild phenotype only when genome changes.

## Further reading

- [GENOME_REVISIONS.md](GENOME_REVISIONS.md) — Open design backlog (repro caps, cooldown, risk, repair, locomotion)
- [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md) — How `photo_strength` and `chemo_potential` interact with the world
- [ENTITIES.md](ENTITIES.md) — Spawn, reproduction, death
- [ROADMAP.md](ROADMAP.md) — Phase boundaries for morphology and sensors

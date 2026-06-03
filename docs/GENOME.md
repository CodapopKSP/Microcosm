# Genome

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
| **Tradeoff** | Higher helps X, hurts Y via coupled formula | `body_radius`, `photo_strength`, `offspring_endowment` |
| **Cost** | Benefit requires ongoing or one-time energy / mobility tax | `skin_hardness`, sensor bandwidth on nodes |
| **Threshold** | Benefit only after viability cliff | `chemo_potential` near vents |
| **Neutral** | Context-dependent or cosmetic | `color_*`, `repro_cooldown` |
| **Risk** | Stochastic failure under stress | `repro_risk_factor` at low energy |

### Reproduction risk

Reproduction always carries inherent risk (config + genes):

```
energy_margin = internal_energy / repro_threshold
risk_mult = repro_risk_factor_decoded  // 0..1
p_death = p_death_on_repro_base * risk_mult * max(0, 1 - energy_margin / repro_safety_margin)
```

Short `repro_cooldown` is not automatically optimal: frequent reproduction at low `energy_margin` increases exposure to `p_death`, failed spawns, and hazard windows while `repro_cost` is sunk.

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
| 0–2 | `color_r`, `color_g`, `color_b` | 1 each | Neutral | `channel / 255.0` → render color; no fitness v1 |
| 3 | `body_radius` | 1 | Tradeoff | `radius = r_min + (raw/255) * (r_max - r_min)`; metabolism scale `(radius/r_ref)²` |
| 4–5 | `photo_strength` | 2 | Tradeoff | `u16 → [0, photo_max]`; uptake multiplier; `photo_upkeep = k_photo * photo_strength` |
| 6–7 | `metabolism_base` | 2 | Tradeoff | `u16 → [meta_min, meta_max]` idle drain per tick |
| 8–9 | `locomotion_investment` | 2 | Tradeoff | `u16 → [0, loco_max]`; drives `move_cost` (not top speed) |
| 10–11 | `repro_threshold` | 2 | Neutral/Risk | `u16 → [e_min, e_max]` energy required to attempt repro |
| 12–13 | `repro_cost` | 2 | Cost | Energy removed from parent on reproduce |
| 14–15 | `offspring_endowment` | 2 | Tradeoff | Energy transferred to child; high values starve parent |
| 16 | `repro_cooldown` | 1 | Neutral | `ticks = cd_min + (raw/255) * (cd_max - cd_min)` |
| 17 | `repro_risk_factor` | 1 | Risk | `raw/255` scales `p_death_on_repro` |
| 18–19 | `skin_hardness` | 2 | Cost | Damage reduction; `armor_upkeep = k_armor * hardness`; `move_penalty = k_move * hardness` |
| 20–21 | `repair_efficiency` | 2 | Tradeoff | Energy per HP repaired: `cost = base / efficiency` |
| 22–23 | `chemo_potential` | 2 | Threshold | Active only if `raw/65535 > cliff` AND near vent; `chemo_upkeep` always if raw > 0 |
| 24–25 | `max_speed` | 2 | Tradeoff | `u16 → [speed_min, speed_max]` sim units/sec; mutates independently of locomotion cost |
| 26–27 | `max_age` | 2 | Tradeoff | **Phase 3:** `u16 → [age_min_ticks, age_max_ticks]`; `age_upkeep = k_age * (max_age_ticks / age_ref_ticks)` per tick ([PHASE_3.md](PHASE_3.md)) |

### Derived traits (not stored in DNA)

| Trait | Formula (conceptual) |
|-------|----------------------|
| `max_speed` | `lerp(max_speed_raw) - move_penalty(hardness, appendages)` |
| `move_cost` | `m_base + k_m * locomotion_investment + k_drag * body_radius + appendage_drag` |
| `metabolism_moving` | `metabolism_base * (1 + k_move_meta * |velocity| / max_speed)` |
| `max_age_ticks` | `lerp_u16(max_age_raw, age_min_ticks, age_max_ticks)` (Phase 3) |
| `age_upkeep` | `k_age * (max_age_ticks / age_ref_ticks)` per tick (Phase 3) |

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
| body_radius | `128` | Medium |
| photo_strength | `0x4000` | Moderate uptake + upkeep |
| metabolism_base | `0x3000` | Moderate |
| locomotion_investment | `0x2800` | Moderate speed/cost |
| repro_* | mid per config | Can reproduce without instant death risk |
| max_speed | `0x0000` | Immobile at worldgen; offspring can mutate faster |
| skin_hardness | `0` | Phase 4+ |
| chemo_potential | `0` | Inactive until vents |

Exact byte pattern lives in `configs/default.toml` under `[genetics.ancestor_hex]` when implemented.

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

- [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md) — How `photo_strength` and `chemo_potential` interact with the world
- [ENTITIES.md](ENTITIES.md) — Spawn, reproduction, death
- [ROADMAP.md](ROADMAP.md) — Phase boundaries for morphology and sensors

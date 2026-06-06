# Configuration reference

All simulation tuning lives in **`configs/*.toml`**. Rust code reads values at load time; **no balance literals** in systems except identifiers and struct defaults that mirror this file.

**Authority pairing:**

| Doc | Role |
|-----|------|
| [Genome_Definition.md](Genome_Definition.md) | What is **heritable** (bytes in DNA) |
| **This file** | What is **global** (same for every organism) and **how to decode** each chromosome |

**Rule (v2):** If it is not a chromosome or pursue/avoid slot in Genome_Definition, it is **not per-agent variable** — only config can change it between runs.

Example template: [`configs/genome_v2.example.toml`](../configs/genome_v2.example.toml).

---

## Top-level sections (v2 target)

| Section | Purpose |
|---------|---------|
| `[world]` | Size, boundary, initial population |
| `[performance]` | Failsafe cap (not ecological) |
| `[genetics]` | Version, ancestor hex, slot limits |
| `[genetics.mutation_v2]` | v2 offspring mutation (gene-level ±increment) |
| `[genetics.decode_v2]` | Scale every **genome byte → phenotype** formula |
| `[genetics.behavior]` | Pursue/avoid limits, fuzzy match, sensor upkeep |
| `[genetics.triad]` | Photo / biomass / minerals inverse coupling |
| `[economy]` | Vitality, repro risk, age, movement baseline, healing baseline |
| `[environment]` | Light, hazards, vents, minerals, corpses |
| `[run]` | Headless ticks, seed, metrics path |

v1 configs keep `[genetics.decode_ranges]` and `[genetics.color_photosynthesis]` until cutover.

---

## `[genetics]` — heritable variation machinery

| Key | Applies to | Description |
|-----|------------|-------------|
| `genome_version` | loader | `2` for EVO2 |
| `core_length` | validation | Minimum bytes (24 fixed + slot payload); may use `max_genome_bytes` |
| `ancestor_hex` | worldgen | Full EVO2 genome including header |
| `max_genome_bytes` | validation | Hard cap on variable pursue/avoid growth |
| `max_pursue_slots` / `max_avoid_slots` | mutation | Slot count limits |

### `[genetics.mutation_v2]` — offspring mutation (v2 only)

| Key | Default | Description |
|-----|---------|-------------|
| `passes_min` | 0 | Minimum mutation passes per child |
| `passes_max` | 4 | Maximum mutation passes per child |
| `p_large_step` | 0.10 | Probability of ±3–4 step instead of ±1 |
| `large_step_min` | 3 | Large-step magnitude lower bound |
| `large_step_max` | 4 | Large-step magnitude upper bound |

Required when `genome_version = 2`. See [Genome_Definition.md](Genome_Definition.md#mutation-v2).

Only `genome_version = 2` is supported. Legacy v1 mutation keys (`p_bit_flip`, `decode_ranges`, etc.) were removed in Phase G.

---

## `[genetics.decode_v2]` — decode scales (genome → phenotype)

Each row maps prose in [Genome_Definition.md](Genome_Definition.md) to config keys. Formulas use `t = raw / max_raw` (255 for u8, 65535 for u16) unless noted.

### Color (bytes 0–2)

| Key | Default (spec prose) | Use |
|-----|----------------------|-----|
| `nibble_levels` | 16 | Turning/sensor nibble decode only (**not** efficiency caps) |
| `channel_max` | 255 | RGB decode to [0,1] for render/smell |

### Body radius (bytes 3–4, u16)

| Key | Default | Use |
|-----|---------|-----|
| `radius_at_zero` | 4.0 | Units at raw 0 |
| `radius_at_max` | 1314.70 | Added linear term at raw 65535 (spec: 1310.70 + 4) |
| `energy_bonus_at_max` | 6553.5 | Added to max energy at raw 65535 |
| `metabolism_bonus_at_max` | 65.535 | Added metabolism demand at raw 65535 |

### Armor (bytes 5–6, u16)

| Key | Default | Use |
|-----|---------|-----|
| `stroke_max` | 25.5 | Render/collision stroke at raw 65535 |
| `metabolism_pct_at_max` | 0.65535 | Fractional increase **after** radius metabolism |
| `speed_penalty_at_max` | 0.65535 | Fraction removed from max speed |

### Healing rate (bytes 7–8, u16)

| Key | Default | Use |
|-----|---------|-----|
| `heal_base_hp_per_sec` | 1.0 | At raw 0 (before multiplier) |
| `heal_speed_mult_at_max` | 3.27675 | × baseline rate at raw 65535 |
| `heal_cost_mult_at_max` | 13.107 | Energy per HP multiplier at raw 65535 |

### Energy / health ratio (bytes 9–10)

| Key | Default | Use |
|-----|---------|-----|
| `energy_cap_frac_at_byte0` | 0.99 | When first byte = 0 |
| `energy_cap_frac_at_byte255` | 0.01 | When first byte = 255 |
| `vitality_budget_from_radius` | true | If true, total cap scales with body decode before split |

### Single-byte % loci (bytes 11–16)

| Key | Default | Use |
|-----|---------|-----|
| `byte_scale` | 255 | Divisor for repro %, endowment %, desire, efficiencies |
| `turn_chance_max_per_sec` | 0.30 | High nibble F on turning byte |
| `turn_degrees_max` | 180.0 | Low nibble F on turning byte |
| `max_speed_at_max_byte` | 255.0 | World units/s when max speed byte = 255 |
| `movement_cost_reduction_at_max` | 0.52 | Movement efficiency byte FF |

### Photosynthesis (byte 17)

| Key | Default | Use |
|-----|---------|-----|
| `photo_rate_per_sec_per_radius` | 0.1 | Gain = this × `body_radius` × `photo_eff` × light × … |
| `photo_eff_from_byte` | linear 0..1 | `byte / 255` |

### Digestion (bytes 18–19)

| Key | Default | Use |
|-----|---------|-----|
| `digest_eff_from_byte` | linear 0..1 | Base efficiency before triad coupling |

### Sensors (bytes 20–21)

| Key | Default | Use |
|-----|---------|-----|
| `sensor_distance_at_high_nibble_f` | 610.0 | 600 + 10 at nibble F |
| `sensor_width_deg_at_low_nibble_f` | 35.0 | 30 + 5 at nibble F |
| `sensor_ray_count` | 8 | Bits in mask (45° steps) |

### Triad (bytes 17–19 + color nibbles)

See `[genetics.triad]` — coupling function and weights are **config**, not DNA.

---

## `[genetics.triad]` — inverse efficiency coupling

| Key | Description |
|-----|-------------|
| `coupling_mode` | `max_other`, `mean_other`, or `product_other` (pick one; document in repo) |
| `coupling_strength` | 0..1 scaler on cross-term |

Effective efficiency = `gene_eff × (1 - strength × f(other genes))`. Color bytes do not cap pathways (revision 2026-06).

---

## `[genetics.behavior]` — signatures & sensors (global tuning)

| Key | Description |
|-----|-------------|
| `fuzzy_nibble_delta` | Default 1 (hex digit ±1) |
| `avoid_flee_heading_offset_deg` | 180 |
| `sensor_upkeep_per_eye_per_tick` | Energy cost per enabled mask bit |
| `sensor_upkeep_strength_scale` | Multiplier from strength byte |
| `pursue_turn_rate_deg_per_tick` | Cap turn toward target (or use gene turning nibble) |

---

## `[economy]` — same for all organisms (not in genome)

### Vitality & spawn

| Key | v1 exists | v2 notes |
|-----|-----------|----------|
| `spawn_energy` | yes | **Generation-0 only** — initial `internal_energy` |
| `spawn_health_frac` | new | **Generation-0 only** — fraction of decoded max health at birth |
| `metabolism_floor` | yes | Minimum idle drain after decode |
| `repro_payment_order` | new | Parent endowment spend order: default `energy_first` (then `health_first`, `proportional`) |

### Reproduction (global)

| Key | v1 exists | Notes |
|-----|-----------|-------|
| `repro_cooldown_ticks` | yes | **Not a gene** |
| `p_death_on_repro_base` | yes | **Not a gene** |
| `p_damage_on_repro_base` | yes | **Not a gene** |
| `repro_safety_margin` | yes | Scales death chance vs post-transfer energy |
| `repro_damage_fraction_of_max_hp` | yes | On damage roll |

### Age metabolism (global)

| Key | v1 exists | Notes |
|-----|-----------|-------|
| `age_metabolism_enabled` | new | Default true for parity with v1 runs |
| `age_metabolism_cap_per_sec` | yes | **No `age_metabolism_scale` gene in v2** |
| `age_metabolism_cap_ticks` | yes | |
| `age_metabolism_ref_ticks` | yes | |

### Movement (global baseline)

| Key | v1 exists | v2 notes |
|-----|-----------|----------|
| `move_energy_units_per_radius` | yes (default 10) | v2: cost = distance / (this × body_radius) × `movement_cost_multiplier` |
| `move_energy_pace_in_gene` | — | Replaced by movement efficiency **byte** |

### Healing baseline (global)

| Key | v1 exists | v2 notes |
|-----|-----------|----------|
| `repair_energy_per_hp` | yes | Base exchange rate; gene only scales speed/cost mult |
| `repair_policy` | yes | `core_first`, etc. |

### Failsafe

Entity–entity collision damage was removed in the evolution model revision (2026-06). Hazards remain under `[environment]`.

---

## `[environment]` — world fields (global)

| Key | Description |
|-----|-------------|
| `energy_model`, `solar_constant`, `light_mode`, … | [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md) |
| `[[environment.hazards]]` | Zones |
| `[[environment.vents]]` | Bridge until minerals grid exists |
| `minerals_enabled` | new | Mineral field for signature `0x0201` |
| `corpses_enabled` | new | Dead agents for `0x0200` |

---

## Implementation contract

1. **`phenotype_v2::decode`** — Only reads `Genome` bytes + `SimConfig` (including `decode_v2`). No `const` balance numbers in decode.
2. **Systems** — `metabolism`, `reproduction`, `healing`, `movement`, `behavior` take `Phenotype` + `Res<SimConfig>`; age and repro risk read `economy.*` only.
3. **Validation** — On TOML load, reject unknown keys (strict) or warn (loose); require `energy_cap_frac_at_byte0 > energy_cap_frac_at_byte255`.
4. **Tests** — Golden tests use a committed TOML fixture; changing defaults changes tests explicitly.

---

## Migration from v1 `configs/default.toml`

| v1 | v2 action |
|----|-----------|
| `[genetics.decode_ranges]` | Replace with `[genetics.decode_v2]` when `genome_version = 2` |
| `age_metabolism_scale_min/max` | **Remove** — age is global-only |
| `color_photosynthesis` | **Remove in v2** — v1 greenness niche only; v2 color is cosmetic |
| `p_bit_flip`, `mutations_per_offspring`, … | **v1 only** — v2 uses `[genetics.mutation_v2]` |
| `collision_damage_scale` | **Removed** — no entity–entity collision damage |
| `chemo_cliff`, `k_chemo` | Minerals digestion + environment; tune under `decode_v2` / vents |
| `core_max_hp` | May become derived from radius + ratio; or cap in `decode_v2` |

**Cutover complete:** [`configs/default.toml`](../configs/default.toml) is the only supported genetics profile.

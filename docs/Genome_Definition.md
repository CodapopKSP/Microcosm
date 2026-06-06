# Genome & simulation definition (canonical)

**This document is the authoritative design for the Evolution Simulator** — not only DNA layout, but also vitality (energy + health), metabolism, movement, sensing, behavior, reproduction, feeding, and how agents interact with environmental fields (light, minerals, corpses, future hazards).

Older docs ([GENOME.md](GENOME.md), [GENOME_REVISIONS.md](GENOME_REVISIONS.md), phase plans that assume `EVO1`) are **archived legacy** specs in [`docs/archive/`](archive/). When anything conflicts, **this file wins**.

Implementation status: **genome v2 only** (`EVO2`, 27-byte core + pursue/avoid slots). v1 (`EVO1`) was removed in Phase G; historical specs live in [`docs/archive/`](../docs/archive/).

### Genome vs global config

| Source | What it controls |
|--------|------------------|
| **Genome bytes** | Per-organism **heritable** variation only — the chromosomes and pursue/avoid slots listed in this document. |
| **`configs/*.toml`** | **Everything else**, including all numeric constants used to **decode** genome bytes into phenotypes. Same config values apply to **every** organism in a run. |

**Rule:** If a mechanic is **not** represented by a chromosome or signature slot in this file (e.g. age metabolism, reproduction death/damage risk, reproduction cooldown, initial spawn energy, solar constant, hazard zones), it is **not evolvable** — it is a **global** parameter only. Do not read per-agent bytes for those mechanics in v2.

**Rule:** Numbers appearing in prose below (6553.5, 0.1 photo rate, 52% move discount, ±1 fuzzy nibble, …) are **design defaults** for documentation. Implementation must load them from config ([CONFIG.md](CONFIG.md), `[genetics.decode_v2]` and related tables), not hard-code literals in Rust.

---

## Fixed core byte map

All offsets are relative to the first byte **after** a 4-byte header (`b"EVO2"`, `version` u8, `reserved` u8, `reserved` u8). Multi-byte integers are **big-endian**. Total fixed core = **23 bytes** (variable slots follow).

| Off | W | Chromosome |
|-----|---|------------|
| 0–2 | 3 | Color |
| 3–4 | 2 | Body radius |
| 5–6 | 2 | Armor |
| 7–8 | 2 | Healing rate |
| 9 | 1 | Energy / health ratio |
| 10 | 1 | Reproduction threshold |
| 11 | 1 | Offspring endowment |
| 12 | 1 | Movement desire |
| 13 | 1 | Turning desire |
| 14 | 1 | Max speed |
| 15 | 1 | Movement efficiency |
| 16 | 1 | Photosynthesis efficiency |
| 17 | 1 | Biomass digestion efficiency |
| 18 | 1 | Minerals digestion efficiency |
| 19–20 | 2 | Sensors |
| 21 | 1 | Pursue list slot count |
| 22 | 1 | Avoid list slot count |
| 23+ | var | Pursue then avoid slots (see [Pursue and Avoid Lists](#pursue-and-avoid-lists)) |

---

## Chromosomes

Color
- 3 bytes — R/G/B (`u8` each)
- **Cosmetic / smell only** — render color and `0x0000` signature matching; **does not cap** photosynthesis, biomass, or minerals efficiency genes (revision 2026-06; see [EVOLUTION_MODEL_REVISION.md](EVOLUTION_MODEL_REVISION.md))
- Triad inverse coupling between photo / biomass / minerals efficiency bytes remains

Body Radius
- 2 bytes
- Larger radius = more energy storage, FFFF = +6553.5 energy storage
- Larger radius = more metabolism requirements, FFFF = +65.535 metabolism requirements
- 0000 is 0+4 units, FFFF is 1310.70 + 4 units radius

Armor
- 2 bytes (`u16` raw)
- Denoted by stroke/line thickness around organism in the visual layer
- More armor = more metabolism requirements; `FFFF` = **+65.535%** increase in metabolism requirements (**after** adding radius-based metabolism)
- More armor = max speed penalty; `FFFF` = **65.535% reduction** in max speed (applied when decoding max speed)
- Thickness: `0000` = none; `FFFF` → **25.5** world units of stroke width (linear: `raw / 65535 × 25.5`)

Healing Rate
- 2 bytes
- Higher = healing cost penalty
- FFFF = 327.675% faster, 1310.7% more expensive
- Base healing rate is 1/s

Energy/Health Ratio
- 1 bytes
- `00` → **99%** of combined vitality budget to **max energy**, **1%** to **max health**
- `FF` → **1%** max energy, **99%** max health
- Combined budget scales from body radius (see Body Radius); current pools cannot exceed decoded caps

Reproduction Threshold
- 1 byte
- A percentage of max energy, 00 = 0%, FF = 100%
- Organism will reproduce as soon as threshold is met

Offspring Endowment
- 1 byte
- A percentage of max energy + max health, 0 = 0%, FF = 100% (of the combined total)
- Energy+health used gets converted into offspring's health (and energy if overflow)

Movement Desire
- 1 byte
- % chance of moving per second for 1 second, 00 = 0%, FF = 100%

Turning Desire
- 1 byte
- First digit is the % chance of turning a random direction per second, 0 = 0%, F = 30%
- Second digit is the max magnitude of the turn, 0 = 0 degrees, F = 180 degrees

Max Speed
- 1 byte
- 00 = not capable of moving, FF = able to move up to 255 units per second

Movement Efficiency
- 1 byte
- 00 = no reduction, FF = 52% reduction (after Armor penalty is calculated)

Photosynthesis Efficiency
- 1 byte
- 00 = no photosynthesis, FF = 100% efficiency, 0.1 per second per unit of body radius
- Scales in inverse of Biomass and Minerals Efficiency

Biomass Digestion Efficiency
- 1 byte
- 00 = no biomass digestion, FF = 100% biomass digestion
- Uptake rate (health-energy units per second): `body_radius × biomass_effective × biomass_uptake_per_radius_per_sec` (config default `10.0` — e.g. radius 10 at 10% digestion → 10 units/s; radius 20 at 100% → 200 units/s)
- Overlapping corpses or living prey: evaluated once per second (`biomass_feeding_interval_ticks`, default 60; `biomass_feeding_phase_ticks`, default 30 — staggered from sensor rays) for agents with `biomass_effective > 0`; drain up to one second of uptake; 1 drained HP becomes 1 predator energy
- If uptake per second ≥ prey remaining HP, kill prey that tick and grant **max health** as energy (not just remaining HP)
- Corpses store the dead agent's **max health** as consumable energy
- Triad coupling still applies to `biomass_effective` vs photo/minerals genes

Minerals Digestion Efficiency
- 1 byte
- 00 = no minerals digestion, FF = 100% minerals digestion, calculated at the time the minerals are eaten
- Scales in inverse of Photosynthesis and Biomass Efficiency

Sensors
- 2 bytes
- **Byte 0 (mask):** bit pattern for sensor placement — bit `i` set = one sensor at heading + `i × 45°` clockwise (`10000000` → 0°, `01000000` → 45°). Requires agent **heading** (see [Simulation contract](#simulation-contract)).
- **Byte 1 (strength):** high nibble = distance reach, low nibble = arc width. Each enabled sensor uses **per-eye** strength: decode nominal reach/width from the byte, then **divide by the number of set bits** in the mask (population count of “eyes”).
- Per-nibble maxima: high nibble `F` → **600 + 10** world units distance; low nibble `F` → **30 + 5** degrees half-width (implementation may treat as full cone width — document in config)

Pursue/Avoid list counts
- 2 bytes (core offsets 22–23)
- Byte 0: number of **OR slots** on the pursue list (not bytes — slots)
- Byte 1: number of **OR slots** on the avoid list
- If a sensor detects an avoid signature, it will turn 180 degrees away and move at max speed until it has crossed the same distance as its sensor strength or has run out of energy
- If a sensor detects a pursue signature, it will continuously turn towards the target but move normally

## Example genome (ancestor)

Fixed core only (no pursue/avoid slot payload). Hex groups match [fixed core byte map](#fixed-core-byte-map):

```text
Color   Body  Armr  Heal    Repr  Movement  Energy  Sens  Inter
|----|  |--|  |--|  |----|  |--|  |------|  |----|  |--|  |---|
00FF05  000A  0000  000080  FF0F  00000000  FF0000  0000  0000
```

| Field | Bytes | Ancestor | Notes |
|-------|-------|----------|-------|
| Color | `00 FF 05` | green-heavy | Cosmetic green; photo/biomass/mineral genes independent of color nibbles |
| Body | `00 0A` | raw 10 | Small radius (~4 + linear term); sets storage & metabolism baseline |
| Armor | `00 00` | none | No stroke, no armor tax |
| Healing | `00 00` | base rate | 1 HP/s baseline, base cost |
| E/H ratio | `80` | mid | Byte `0x80` → mixed energy/health caps |
| Repro threshold | `FF` | 100% | Reproduce when current energy ≥ 100% of **max energy** |
| Offspring endowment | `0F` | ~6% | Transfer up to ~6% of **max energy + max health** combined |
| Movement ×4 | `00 00 00 00` | sedentary | No move/turn/speed/eff — **sessile** ancestor |
| Photo / biomass / mineral | `FF 00 00` | autotroph | Full photo gene; no digestion |
| Sensors | `00 00` | blind | No rays; pursue/avoid lists empty (`00 00`) |

**Intentional sessile ancestor:** `max_speed = 00` means no locomotion; lineage is viable via photosynthesis only until mutation adds movement bytes.

Pursue/avoid payload (when counts > 0) is appended **after** byte 22: all pursue slots first, then all avoid slots.

### Pursue and Avoid Lists

These lists are identical. They have slots that contain one or more Chromosomes or Chemical signatures. A chromosome could be any of the above listed chromosomes, and a chemical signature could be any of the following:
- Dead organisms
- Minerals
- Higher concentration of sunlight compared to the current location
- Lower concentration of sunlight compared to the current location
- Any other hazards or objects to be added in the future

Each **simple slot** (one OR branch) is **6 bytes**: `type u16` (big-endian) + `value u32` (big-endian).

**AND slot** (one OR branch, all atoms required): `type u16 = 0x0100`, `arity u8`, `reserved u8`, then `arity` × (type u16, value u32). Max arity **4**.

**List logic:** any slot can match (OR). Inside an AND slot, every atom must match (AND).

#### Signature type registry (`type u16`)

| Type ID | Name | Value `u32` layout | Matches |
|---------|------|-------------------|---------|
| **Chromosome / trait** (smell another agent’s decoded traits) | | | |
| `0x0000` | Color | Low 24 bits = `R`, `G`, `B` (same as Color chromosome) | Target RGB (fuzzy per channel) |
| `0x0001` | Body radius | Low 16 bits = radius raw (`u16`); `0xFFFF` = largest | Observed body radius raw |
| `0x0002` | Armor | Low 16 bits = armor raw (`u16`) | Observed armor raw |
| `0x0003` | Healing rate | Low 16 bits = healing raw (`u16`) | Observed healing raw |
| `0x0004` | Energy / health ratio | Low 8 bits = ratio byte (`u8`) | Observed ratio byte |
| `0x0005` | Reproduction threshold | Low 8 bits = `%` byte (`u8`) | Observed repro-threshold byte |
| `0x0006` | Offspring endowment | Low 8 bits = `%` byte (`u8`) | Observed endowment byte |
| `0x0007` | Movement desire | Low 8 bits = `%` byte (`u8`) | Observed movement-desire byte |
| `0x0008` | Turning desire | Low 8 bits = packed turn byte (`u8`) | Observed turning-desire byte |
| `0x0009` | Max speed | Low 8 bits = speed byte (`u8`) | Observed max-speed byte |
| `0x000A` | Movement efficiency | Low 8 bits = efficiency byte (`u8`) | Observed movement-efficiency byte |
| `0x000B` | Photosynthesis efficiency | Low 8 bits = efficiency byte (`u8`) | Observed photo-efficiency byte |
| `0x000C` | Biomass digestion efficiency | Low 8 bits = efficiency byte (`u8`) | Observed biomass-efficiency byte |
| `0x000D` | Minerals digestion efficiency | Low 8 bits = efficiency byte (`u8`) | Observed minerals-efficiency byte |
| `0x000E`–`0x00FF` | *(reserved)* | — | Future core traits / organs |
| **Structural** | | | |
| `0x0100` | **AND group** | See AND slot layout above (not a single `u32`) | — |
| `0x0101`–`0x01FF` | *(reserved)* | — | Future combinators (e.g. NOT, OR-in-slot) |
| **Chemical / environment** (smell non-agent or local field) | | | |
| `0x0200` | Dead organism | `0` = any; low 16 bits optional filter (future) | Corpse / dead agent marker in range |
| `0x0201` | Minerals | Low 16 bits = concentration band target | Local mineral concentration |
| `0x0202` | Brighter sunlight | Low 16 bits = Δlight target (`u16`; `0xFFFF` = much brighter than here) | Light sample > self light |
| `0x0203` | Dimmer sunlight | Low 16 bits = Δlight target (`u16`; `0xFFFF` = much dimmer than here) | Light sample < self light |
| `0x0204`–`0x02FF` | *(reserved)* | — | Future hazards, vents, soup compounds |
| `0x0300`–`0xFFFF` | *(reserved)* | — | Do not use until version bump |

#### Examples (hex, spaces for reading)

**Simple — pursue greenish color** (`00FF05` in low 24 bits):

```text
0000 0000FF05
```

**AND — pursue body `000A` and armor `0000` (one OR slot):**

```text
0100 0200  0001 0000000A  0002 00000000
^^^^      ^^^^^^^^^^^^  ^^^^^^^^^^^^
AND×2     Body          Armor
```

**Three OR pursuits** (large **or** armored **or** small-and-unarmored): pursue slot count = **3** — slots 1–2 are simple atoms; slot 3 is the AND line above.

Invalid or unknown `type u16` in a slot is treated as **non-matching** (that slot never fires).

**Fuzzy matching:** when comparing a signature `value` to an observation, each **hex digit** (4 bits) of the compared bitfield may differ by ±1 (e.g. `0x0A` matches `0x09`, `0x0B`, not `0x07`). Apply per nibble on the portions defined in the type table (RGB: per channel bytes; `u16` traits: low 16 bits; `u8` traits: low 8 bits).

---

## Simulation contract

Rules implied by this spec that extend beyond the genome crate:

### Vitality

- Each agent has **`internal_energy`** and **`core_hp`**, each with decoded **maximum** from body radius and the energy/health ratio locus.
- Metabolism and upkeep drain **energy first**; any deficit at `internal_energy == 0` drains **health** (starvation death at `core_hp <= 0`).
- **Photosynthesis:** `gain_per_sec = photo_rate_per_sec_per_radius × body_radius × photo_effective` (convert to per-tick with `Δt`). `photo_effective` includes triad inverse scaling only (no color cap).
- **Digestion:** biomass/minerals gains apply at ingest time using digestion bytes and triad scaling.

### Movement & heading

- Agents need a **heading** angle (radians) in addition to position/velocity. Sensors and turning behavior are relative to heading.
- **Movement desire:** per second, probability of attempting movement (not per-tick unless engine converts with `Δt`).
- **Turning desire:** high nibble = chance per second of a random turn; low nibble = max turn magnitude (0–180°).
- **Movement efficiency:** `FF` = **52% reduction in movement energy cost** (cheaper travel), applied **after** armor max-speed penalty is computed.
- **Avoid behavior:** on match, turn **180°** from stimulus bearing and move at **max speed** until traveled distance ≥ decoded sensor reach (per-eye, after eye-count division) **or** `internal_energy == 0`.
- **Pursue behavior:** on match, **continuously turn toward** target bearing; move under normal movement desire / speed rules.

### Reproduction

- Trigger when `internal_energy ≥ (repro_threshold_byte / 255) × max_energy` and **`economy.repro_cooldown_ticks`** elapsed (global; not a gene). Default **192** ticks ≈ **3.2 s** at 60 Hz — anti-spam brake when energy refills quickly; not evolvable.
- **Endowment:** spend up to `(endowment_byte / 255) × (max_energy + max_health)` from parent’s **current** energy+health pools (**energy first**, then health if energy is insufficient; default `economy.repro_payment_order = energy_first`). Convert the collected amount into child **health first**, overflow to child **energy**.
- **Reproduction risk** (`p_death_on_repro_base`, `p_damage_on_repro_base`, `repro_safety_margin`, damage fraction): **global only** — same rolls for all agents ([CONFIG.md](CONFIG.md) `[economy]`).

### Environment (signature sources)

| Signature | World requirement |
|-----------|-------------------|
| `0x0200` Dead organism | Corpse entities or dead-agent markers in range |
| `0x0201` Minerals | Mineral field sampled along sensor rays |
| `0x0202` / `0x0203` Sunlight Δ | Local light field vs self `LightExposure` |
| `0x0204+` | Reserved (vents, hazards, soup) — see type table |

Chemosynthetic **vents** are not a separate core chromosome; vent niches use **minerals digestion** + pursue/avoid `0x0201` / future `0x0204` types.

### Per-tick order (target)

1. Environment fields tick (light, minerals, corpses, hazards, …)
2. Sample exposures at position (light, minerals, …)
3. **Sense rays** → pursue/avoid matches
4. **Behavior** (avoid / pursue / wander intent)
5. **Metabolism & gains** (photo, digestion, upkeep, sensor cost)
6. **Movement** (integrate velocity, pay move cost)
7. **Interactions** — **hazard zones only** (entity overlap does not damage HP; revision 2026-06)
8. **Healing** (rate + cost penalty from gene)
9. **Reproduction**
10. **Death & despawn**

### Global mechanics (not in genome — config only)

These apply **uniformly** to every organism. Tune in [CONFIG.md](CONFIG.md); never decode from DNA in v2:

| Mechanic | Typical config section |
|----------|-------------------------|
| Age metabolism | `[economy.age_metabolism]` |
| Repro cooldown | `economy.repro_cooldown_ticks` |
| Repro death / damage risk | `economy.p_death_on_repro_base`, `p_damage_on_repro_base`, … |
| Initial spawn energy | `economy.spawn_energy` — **generation-0 only**; offspring use parent endowment |
| Initial spawn health | `economy.spawn_health_frac` — fraction of decoded max health at generation-0 spawn |
| Metabolism floor | `economy.metabolism_floor` |
| Movement distance calibration | `economy.move_energy_units_per_radius` |
| Repair / heal baseline exchange | `economy.repair_energy_per_hp`, `heal_base_hp_per_sec` |
| Light, hazards, vents, minerals fields | `[environment]` |
| Mutation (v2) | `[genetics.mutation_v2]` — see [Mutation](#mutation-v2) |

### Mutation (v2)

Applied on asexual reproduction after cloning the parent genome. Config: `[genetics.mutation_v2]` ([CONFIG.md](CONFIG.md)).

| Key | Default | Meaning |
|-----|---------|---------|
| `passes_min` / `passes_max` | 0 / 4 | Independent mutation passes per offspring (uniform in range) |
| `p_large_step` | 0.10 | Chance a pass uses magnitude 3–4 instead of 1 |
| `large_step_min` / `large_step_max` | 3 / 4 | Large-step magnitude range |

**Per pass:** pick one gene from the [mutable gene registry](EVOLUTION_MODEL_REVISION.md#evo2-mutable-gene-registry) → direction ±1 → magnitude 1 (90%) or `large_step_min..=large_step_max` (10%) → clamp to gene domain → write bytes.

**Not used in v2:** bit flip, locus resample, segment dup/del, `mutations_per_offspring`, `p_bit_flip`, `p_point`, etc. (v1 legacy — [GENOME.md](GENOME.md)).

Implementation: [`src/genome/mutation_v2.rs`](../src/genome/mutation_v2.rs).

### Explicitly out of scope here (future extension only via new chromosomes)

- **Morphology / appendage tree** — future bytes past core + slots (would become heritable once added to this doc)
- **Sexual reproduction** — future
- **EVO1** 28-byte layout — deprecated ([GENOME.md](GENOME.md) legacy)

---

## Doc map (what this file supersedes)

| Topic | Was | Now |
|-------|-----|-----|
| Core loci | [GENOME.md](GENOME.md) | This file |
| Greenness photo niche | [GENOME.md](GENOME.md) §4.7 | **v1 only** (`color_photosynthesis`); v2 color is cosmetic |
| `chemo_potential` gene | Phase 5 / [GENOME.md](GENOME.md) | Minerals digestion + `0x0201` signatures |
| Phase 5 sensor bytes in `repro_cost` | [PHASE_5.md](PHASE_5.md) | Sensors bytes 20–21 here |
| Movement calibration | [GENOME_REVISIONS.md](GENOME_REVISIONS.md) | Still recommended unless config overrides; efficiency gene modulates cost |
| Entity lifecycle | [ENTITIES.md](ENTITIES.md) | Update to match vitality + repro % when implementing |

Implementation roadmap: [GENOME_V2_ARCHITECTURE_ROADMAP.md](GENOME_V2_ARCHITECTURE_ROADMAP.md).  
All tunable numbers: [CONFIG.md](CONFIG.md). Example v2 TOML: [`configs/genome_v2.example.toml`](../configs/genome_v2.example.toml).
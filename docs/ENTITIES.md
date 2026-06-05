# Entities

This document describes logical ECS components, the entity lifecycle (spawn, live, reproduce, die), interactions, and the performance failsafe.

## Component model

Logical components for implementation (names may map 1:1 to Bevy components):

| Component | Fields (conceptual) | Role |
|-----------|---------------------|------|
| `Genome` | `bytes: Vec<u8>`, `version: u8` | Heritable DNA; mutated on reproduction |
| `Phenotype` | Decoded scalars + derived traits + optional `Morphology` | Cache rebuilt on birth / mutation |
| `Energy` | `internal_energy`, `max_internal_energy` | Spendable store; v2 caps from radius + E/H ratio |
| `Health` | `core_hp`, `core_max_hp`, `part_hp: Vec<f32>` | v2 `core_max_hp` from decoded `max_health`; v1 from config decode |
| `Transform` | position, rotation (2D) | World placement |
| `Heading` | radians | v2: sensor rays and steering |
| `Velocity` | `Vec2` | Motion; capped by `phenotype.max_speed` |
| `MineralExposure` | concentration `0..1` | v2 vent/mineral field sample |
| `Corpse` | `remaining_biomass`, `radius`, TTL | v2 dead-agent marker (not an `Agent`) |
| `Agent` | `radius` (from phenotype), collision flags | Physics and rendering hull |
| `Lineage` | `id`, `parent_id`, `generation` | Optional tracking and metrics |
| `Cooldowns` | `repro_ticks_remaining` | Reproduction timer |
| `Age` | `ticks: u32` | Sim age since spawn; drives logarithmic age metabolism |

### Phase availability

| Component | Phase 0 (now) | Phase 1+ |
|-----------|---------------|----------|
| Genome, Phenotype, Energy, Health | — | Core |
| Lineage, Cooldowns | — | Phase 1 |
| Age | — | Phase 3 (**implemented**) |
| part_hp | — | Phase 4: always `[]` at spawn (core-only damage); Phase 7: one entry per morphology node |

## Lifecycle

```mermaid
stateDiagram-v2
  [*] --> Spawn: birth_or_initial
  Spawn --> Live: decode_ok
  Spawn --> [*]: stillborn
  Live --> Live: tick_metabolism_move
  Live --> Reproduce: threshold_met
  Reproduce --> Live: parent_survives
  Reproduce --> [*]: parent_repro_death
  Live --> [*]: energy_zero_or_core_dead
  Reproduce --> Live: child_spawned
```

### Spawn

1. Obtain genome (ancestor template, random viable, or mutated child).
2. Validate header / version; on failure → stillborn (no entity).
3. Decode → `Phenotype`; build `Health` (full core HP; `part_hp: []` until Phase 7 morphology).
4. Set vitality: **generation-0** uses `economy.spawn_energy` + `spawn_health_frac × max_health`; **offspring** use parent endowment only (`build_offspring_spawn_params`).
5. Assign `Transform` / `Velocity` (random or offset from parent).
6. Spawn visual representation from `color_*` and `body_radius`.

### Live (each tick)

Order follows [SIMULATION.md](SIMULATION.md):

1. Sample environment at position.
2. Apply feeding gains (v2: minerals, corpses, soup) then metabolism (photosynthesis; v1 chemo at vents).
3. Subtract upkeeps and metabolism.
4. Movement if `internal_energy` covers speed-scaled movement cost (`movement_energy_per_tick`); optional moving metabolism when `k_move_meta > 0`.
5. Resolve damage to parts / core (Phase 4: core only when `part_hp` empty).
6. Repair if policy allows and energy available.
7. Decrement repro cooldown.

### Reproduce

**Preconditions:**

- `repro_ticks_remaining == 0`
- v1: `internal_energy >= repro_threshold` (absolute); v2: `internal_energy >= repro_threshold_frac × max_energy`

**Steps:**

1. Pay `repro_threshold` energy from parent; parent core HP unchanged ([GENOME.md](GENOME.md) energy→HP transfer).
2. Roll `p_death` and `p_damage` from [GENOME.md](GENOME.md) (config rates; death scales with post-transfer energy margin).
3. On success: clone genome, apply v2 `[genetics.mutation_v2]` passes (or v1 `mutations_per_offspring`), decode child.
4. v1 child: `internal_energy = 0`, `core_hp` from endowment. v2 child: health + energy from `split_endowment_to_child` (see [Genome_Definition.md](Genome_Definition.md#reproduction)).
5. Place child near parent; set `Lineage.parent_id`, increment `generation`.
6. Reset parent cooldown from `economy.repro_cooldown_ticks`.

**Failure modes:**

- Lethal HP cost — parent dies before rolls if vitality payment empties core HP
- `p_death` — parent dies, no child
- Stillborn — rolls passed but child genome invalid (parent survives, transfer already paid)
- `p_damage` — parent survives but loses a fraction of `core_max_hp`

Fixed cooldown (`economy.repro_cooldown_ticks`) limits attempt rate when energy refills quickly; `repro_threshold` remains the main economic cost.

### Death

Despawn when any of:

| Condition | Phase |
|-----------|-------|
| `core_hp <= 0` | 1+ (includes starvation: metabolic deficit at `internal_energy == 0`) |
| Repro risk roll kills parent | 1+ |
| Performance failsafe cull | emergency only |

Old-age despawn was removed; aging adds logarithmic metabolism instead ([GENOME.md](GENOME.md)).

`internal_energy == 0` is **not** fatal. Unpaid metabolic deficit (costs minus photosynthetic gain) drains `core_hp` each tick until the agent dies as `CoreDestroyed`.

**Revision 2026-06:** Entity overlap does **not** damage HP. Death only via `death_and_despawn` (starvation, hazards, repro risk, etc.).

## Interactions

| Interaction | Phase | Behavior |
|-------------|-------|----------|
| World bounds | 0+ | `bounce` or `wrap` per config |
| Entity–entity collision | — | **No damage** (revision 2026-06); overlap allowed |
| Hazard zones | 4 (live) | Damage per tick while overlapping; v1 `skin_hardness` mitigates |
| Predation / absorption | future | Optional; out of v1 scope |

### Damage routing (Phase 4 vs 7)

| `part_hp` | Phase | Incoming damage |
|-----------|-------|-----------------|
| Empty (`[]`) | 4 (now) | All **hazard** damage subtracts from `core_hp` via `Health::apply_damage_to_core` |
| Non-empty | 7+ | Route to appendages by exposure; overflow to core |

Repair (Phase 4D) may restore `core_hp` and any manually populated `part_hp` slots per `repair_policy`; interaction damage remains core-only until Phase 7.

## Performance failsafe (not ecology)

When `population > max_entities` ([SIMULATION.md](SIMULATION.md)):

- Log warning with tick and count.
- Apply `failsafe_policy`: `stop_spawns`, `despawn_lowest_energy`, or `pause`.
- **Do not** bias despawn toward low-fitness genetically—`despawn_lowest_energy` is a CPU relief valve, not selection.

Natural population regulation:

- Energy scarcity (model B or C)
- Metabolism and upkeeps
- Reproduction cost and risk
- Hazards and localized niches

## Metrics hooks

Per-entity events (optional log):

- `Birth { id, parent_id, generation, tick }`
- `Death { id, reason, tick }`
- `Reproduction { parent_id, child_id, tick }`

Aggregates in [ARCHITECTURE.md](ARCHITECTURE.md).

## Config vs genome (v2 target)

Per [Genome_Definition.md](Genome_Definition.md): only chromosomes and pursue/avoid slots vary by agent. Mechanics such as **age metabolism**, **reproduction risk**, **cooldown**, and **spawn energy** are **global** — see [CONFIG.md](CONFIG.md) `[economy]`. Decode formulas for genes use [CONFIG.md](CONFIG.md) `[genetics.decode_v2]`, not hard-coded Rust constants.

## Further reading

- [Genome_Definition.md](Genome_Definition.md) — Canonical heritable model (v2)
- [CONFIG.md](CONFIG.md) — All tunable parameters
- [GENOME.md](GENOME.md) — Legacy v1 layout
- [ENERGY_AND_ENVIRONMENT.md](ENERGY_AND_ENVIRONMENT.md) — Gains, costs, damage
- [ARCHITECTURE.md](ARCHITECTURE.md) — Where systems live in code
- [ROADMAP.md](ROADMAP.md) — When each interaction lands

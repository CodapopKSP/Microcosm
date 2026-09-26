# Design

The rules of the simulation: the world, the genome, how organisms live, eat, reproduce and die, and how species are tracked. Numbers here are **defaults** — every constant lives in [`configs/default.toml`](../configs/default.toml) and is expected to be tuned.

Guiding principles (see [VISION.md](VISION.md)):

- **Energy is (mostly) conserved.** Energy enters the world only as sunlight and vent minerals, and leaves as metabolic heat. Organisms compete for it; population limits emerge from that competition, not from caps.
- **Every capability costs something**, and most genes matter in more than one situation (pleiotropy). That is what makes trade-offs, niches, and exaptation possible.
- **Readable over realistic.** A viewer should be able to click an organism, read its chromosomes, and roughly understand it.

---

## 1. The world

A **side-on cross-section of an ocean**. `y` grows upward; the water surface is at the top of the world, the seafloor is procedurally generated terrain at the bottom. Default size 16,000 × 6,000 units, explored with pan/zoom.

| Region | What makes it distinct |
|--------|------------------------|
| Sunlit surface | Most light; warm; phototrophs bloom and shade each other |
| Twilight / open water | Light fades exponentially with depth; hunters and drifters |
| Deep / seafloor | Dark and cold; detritus ("marine snow") settles into sediment |
| Vents | Point sources on the seafloor: hot, mineral-rich; chemotroph oases |
| Shelf | A shallow region where the seafloor itself is sunlit |
| Rocks & crevices | Block movement and cast shade; narrow gaps shelter small organisms |
| Currents | Slow gyres everywhere and faster eddies in the upper ~450 units stir plankton through the sunlit water |

### Environmental fields

Fields live on coarse grids and are what organisms sense and consume.

| Field | Grid | Source | Sink |
|-------|------|--------|------|
| **Light** | 16 u | Sun at the surface (day/night × season) | Water attenuation (halves every ~400 u), absorption by bodies, rock |
| **Temperature** | 64 u | Surface temperature (season × climate), cold depths, hot vents | — (recomputed) |
| **Minerals** | 32 u | Vents; decaying detritus | Chemotroph uptake; slow precipitation; diffusion |
| **Detritus** | 32 u | Small corpses, uneaten meal leftovers, decaying large corpses | Scavenger uptake; decay into minerals; sinks and settles on the floor |

**Light competition.** Light is computed column by column from the surface down. Each body in a cell intercepts light in proportion to its width and absorptance (`α = 0.1 body + 0.8 × photo capability`); what is absorbed by pigment becomes energy for that organism, what is absorbed by the body is lost, and what passes through continues downward. A dense bloom at the surface therefore shades everything beneath it — visibly, because the renderer draws the light field.

### Time

- 1 tick = 1/30 s of sim time. "1×" speed = 30 ticks per real second.
- **Day/night**: 10 minutes per day. Nights are dim, not black.
- **Seasons**: 6 days per year (1 hour at 1×). Modulate sunlight (±30%) and surface temperature (±6 °C).
- **Climate**: a slow random walk of global temperature (±8 °C) over many years — ice ages and warm eras.

---

## 2. The genome

A genome is **34 bytes** in ten lettered chromosomes. **Every byte is one gene** and means one thing. Genes are written in hex, e.g. `B: 20 A0 60 30`.

| Chr | Byte | Gene | Decoding (g = byte, f = g/255) | Upkeep* |
|-----|------|------|---------------------------|---------|
| **A** Arms | A0 | Armor | Damage taken × (1 − 0.75 f); max speed × (1 − 0.3 f); denser (sinks) | 0.6 f |
| | A1 | Spikes | Attackers take damage = 0.6 f × damage they deal | 0.3 f |
| | A2 | Jaws | Bite damage × (0.2 + 1.8 f); faster carcass eating | 0.3 f |
| **B** Body | B0 | Size | Adult radius = 2^(g/32) (every `+0x20` doubles size: `20`=2, `40`=4, `80`=16, `C0`=64, `FF`≈250) | — |
| | B1 | Buoyancy | `00` sinks, `80` neutral, `FF` floats | 0.3 f |
| | B2 | Reserves | Energy storage = (0.3 + 2.7 f) × mass | 0.3 f |
| | B3 | Longevity | Lifespan = 300 s × √radius × (1 + 3 f) | 0.4 f |
| **C** Color | C0 | Hue | 0–360° | none |
| | C1 | Saturation | 20–100% | none |
| | C2 | Pattern | High 2 bits = style (plain/spots/stripes/rings), rest = scale | none |
| **D** Diet | D0 | Photosynthesis | Pigment absorptance | 0.3 c |
| | D1 | Chemosynthesis | Mineral uptake | 0.3 c |
| | D2 | Scavenging | Detritus / carcass uptake | 0.3 c |
| | D3 | Hunting | Attacking, engulfing and digesting live prey | 0.4 c |
| **H** Health | H0 | Regeneration | 0.2–5% of max HP per second (1.5 energy per HP) | 0.2 f |
| | H1 | Toughness | Max HP = (0.3 + 1.7 f) × mass | 0.3 f |
| **M** Motion | M0 | Speed | Max speed = f × 20 × √radius (0 = can't swim) | 0.5 f |
| | M1 | Agility | Turn rate 0.3–6 rad/s | 0.2 f |
| | M2 | Restlessness | Cruising throttle and random wandering when nothing is sensed | — |
| **N** Nature | N0–N7 | Drives | Signed: `80` = 0, `FF` = +1 (toward), `00` = −1 (away). See §4 | — |
| **R** Reproduction | R0 | Offspring size | Newborn mass = 0.5 × 2^(−(255−g)/32) × adult mass (`FF` = 50%, fission-like; `00` ≈ 0.2%) | — |
| | R1 | Brood size | 1 + (g >> 5) offspring per event (1–8) | — |
| | R2 | Patience | Reproduce when reserves ≥ (30 + 70 f)% full | — |
| **S** Senses | S0 | Range | f × 40 radii (0 = blind: contact only) | 0.5 f × (0.5 + fov) |
| | S1 | Field of view | 20°–360° | (above) |
| **T** Temperature | T0 | Preferred temp | 0–40 °C | — |
| | T1 | Tolerance | ±1–15 °C | 0.4 f |

\* **Upkeep** terms add to a multiplier on basal metabolism: `U = 1 + Σ upkeep`. A do-nothing blob is cheap; every capability has a running cost.

### Diet capability (generalist penalty)

With `x_i` the four diet genes as fractions, each diet is weakened by the investment in the others:

```
c_i = x_i × (1 − 0.6 × Σ_{j≠i} x_j)
```

A specialist (one gene only) gets `c = x`. Taking up a second diet is gradual: a small second gene works linearly and costs the main diet a little. But true generalists lose overall: two full diets give 0.4 each (0.8 total). Upkeep is charged on **capability**, not on the raw gene, so a weak unused diet gene is cheap. It drifts as cryptic variation until the environment makes it useful — one of the main routes to **exaptation**.

### Example: the ancestor

A tiny, sessile, floating, green photosynthesizer — every other ability has to evolve.

```
A: 00 00 00        no armor, spikes or jaws
B: 20 A0 50 20     radius 2, floats gently, modest reserves, lives ~10 min
C: 55 C0 00        green, plain
D: FF 00 00 00     pure photosynthesis
H: 20 30           slow regeneration, fragile
M: 00 00 00        cannot swim
N: 80 80 80 80 80 80 80 80   no drives
R: FF 00 B0        fission-like: one half-size offspring when ~75% full
S: 00 00           blind
T: 80 A0           prefers ~20 °C, ±10 °C
```

### Mutation

Applied independently to each gene of each offspring:

- Probability **1%** per gene (3% for the three color genes, a fast-ticking "molecular clock" so lineages visibly diverge in color). About two thirds of offspring are exact copies.
- 5% of mutations **re-roll** the gene to a random value. The rest step by ±1 (50%), ±2 (25%), ±4 (12.5%) or ±8 (12.5%).
- Values clamp at `00` and `FF`.

Low mutation rates keep lineages coherent, so species persist and drift is visible.

---

## 3. Life of an organism

### Size, mass, growth

- `mass = radius²`. Organisms are born small and **grow** toward their adult size (gene B0) by investing surplus energy, at up to 5% of their mass per second, costing **1.25 energy per unit mass**.
- Juveniles keep half their reserves as a buffer and invest the rest in growth.
- Only adults reproduce, so large organisms have long juvenile phases. Juveniles eat different things and are vulnerable to different predators — **ontogenetic niche shifts** come for free.

### Energy budget (per second)

```
basal     = 0.003 × mass^0.75 × U × temperature_stress
movement  = 0.003 × mass^0.75 × 3 × speed_gene × throttle²
+ photosynthesis, chemosynthesis, scavenging, hunting (below)
```

- Metabolism scales with `mass^0.75` (Kleiber): large organisms are cheaper per unit mass but need more in total.
- If reserves hit zero, the deficit comes out of HP (starvation).

### Food sources

| Diet | Mechanic | Scales with |
|------|----------|-------------|
| Photosynthesis | Share of light absorbed in the organism's light cell(s) (§1) | Width (big photosynthesizers are inefficient per mass) |
| Chemosynthesis | Uptake from the mineral field: `2 × c × radius × concentration`, depleting it | Perimeter (favors small) |
| Scavenging | Uptake from the detritus field, `12 × c × radius × concentration` (intercepting falling marine snow), plus a filter-feeding bonus when swimming; eating large carcasses on contact | Width; width × speed when swimming |
| Hunting | See below | Size, jaws |

**Hunting.** On contact with a non-kin organism:

- **Engulf** if the prey's radius is ≤ 50% of the hunter's: the prey dies instantly, with the chance per second reduced by prey armor. The hunter's spikes don't matter, but the **prey's** spikes hurt the hunter.
- Otherwise **bite**: the hunter tears off `0.3 × c_hunt × (0.2 + 1.8 × jaws) × radius²` of the prey's mass per second, reduced by prey armor. Bitten prey shrink and lose HP, and must regrow. The prey's spikes reflect damage.
- The hunter assimilates **80% × c_hunt** of the prey's body energy (1 per unit mass, plus the prey's reserves), proportionally to damage dealt. Anything it can't store becomes detritus.

Energy moves up the food chain with losses, so predators are rarer than prey, and populations can oscillate.

### Health

- Max HP scales with mass and toughness (H1). Regeneration (H0) costs energy per HP.
- **Temperature stress**: `s = ((T − T_pref) / tolerance)²`. Metabolism × (1 + 0.5 s); beyond tolerance (s > 1), HP damage.

### Reproduction

When an organism is **adult** and its reserves are at least the patience threshold (R2), it produces up to `brood` offspring (R1) of the newborn size (R0). Each offspring costs `newborn_mass × 1.25` plus the newborn's starting reserves (50% of its capacity). It produces as many as it can afford. Offspring appear beside the parent and mutate independently.

- **r-strategists**: many tiny offspring, most of which die young.
- **K-strategists**: few large offspring.

### Death

| Cause | Result |
|-------|--------|
| HP reaches 0 (starvation, predation, temperature, spikes) | Corpse |
| Old age (past lifespan: 20% chance per second) | Corpse |
| Engulfed | No corpse — eaten whole |

Every corpse becomes a **carcass**: a grey, torn version of the organism that sinks, can be eaten on contact (bites show), and decays into the **detritus field** (marine snow). Small carcasses decompose within about a minute; large ones last much longer, so a whale fall feeds a seafloor community.

Scavengers live two ways: eating carcasses on contact ("Eating a carcass"), and filtering marine snow from the water and the sediment that piles up on the seafloor ("Eating marine snow").

### Movement and physics

- Swimming: the organism turns toward its desired heading (limited by agility) and thrusts at `throttle × max speed`. Small organisms stop instantly (low Reynolds number); large ones coast.
- **Buoyancy** gives a vertical drift proportional to √radius. **Brownian jitter** affects only tiny organisms. **Currents** carry everything: large slow gyres, plus eddies in the upper 450 units (curl noise, so the flow neither piles up nor empties out).
- Overlapping organisms push each other apart by mass. Rocks are solid.

---

## 4. Behavior: drives

Motile organisms (Speed > 0) steer by a weighted sum of **drives** (chromosome N). Each drive is a signed weight: positive = move toward, negative = move away.

| Gene | Drive | Stimulus |
|------|-------|----------|
| N0 | Light | Light gradient |
| N1 | Warmth | Temperature gradient |
| N2 | Minerals | Mineral gradient |
| N3 | Carrion | Detritus gradient and sensed carcasses |
| N4 | Smaller | Sensed non-kin organisms under 80% of own radius |
| N5 | Larger | Sensed non-kin organisms over 125% of own radius |
| N6 | Kin | Sensed organisms of similar color (schooling or spacing) |
| N7 | Rise | Straight up (+) or down (−) |

- Food-related drives (light, minerals, carrion, smaller) are scaled by **hunger**, so fed organisms relax.
- When nothing is sensed, restlessness (M2) sets a cruising throttle and random turning.
- **Senses**: organisms sense others within their range and field of view. They only perceive individuals at least ~1/8 of their own size — a whale doesn't track single bacteria; it swims toward dense food fields instead.
- **Kin** = similar color (hue and saturation). Hunters never attack kin. This makes color matter for predators, so camouflage-by-mimicry is possible.

Examples of what the drives can produce: phototaxis (+Light), vent-seeking (+Minerals, +Warmth), hunting (+Smaller), fleeing (−Larger), schooling (+Kin), bottom-dwelling (−Rise).

---

## 5. Species

- Every organism carries a **species ID** inherited from its parent.
- Distance between genomes = weighted mean of per-gene differences. Color genes weigh 25%, everything else 100%. Each gene contributes at most 0.3 of its range, so one large jump can't make a species on its own: it takes changes in several genes.
- If a newborn's distance from its species' **founder genome** exceeds the species threshold, it founds a **new species**. The new species records its parent species and founding time.
- A species is **established** (named, listed, charted) once it reaches 8 living members. Variants that die out before that are forgotten.
- **Names** are generated pseudo-Latin binomials. The genus is inherited unless the distance from the genus founder exceeds a larger threshold, so related species share a genus name (*Rubrovorax lentus*, *Rubrovorax magnus*).
- The resulting records form a **phylogeny**: parent species, founding and extinction times, peak population.

---

## 6. Environment dynamics and events

| Event | Timescale | Effect |
|-------|-----------|--------|
| Day/night, seasons | 10 min / 1 h | Pulses of light and temperature: reserves, and strategies that last the night, pay off |
| Climate | Days | Global temperature drifts ±8 °C: range shifts, temperature specialists rise and fall |
| Vent lifecycle | Hours | Vents flare, fade and die; new ones open elsewhere. Colonization and founder effects |
| **Upwelling** | ~every 3 h | Minerals well up into the sunlit water for 30 min: a surface bonanza for chemotrophs and mixotrophs |
| **Plague** | ~every 8 h | Strikes the most common species (only if it holds ≥35% of all life); ~70% of its members die over 10 min. Kill-the-winner keeps any one species from taking over |
| **Rock wall** | ~every 30 h | A wall rises from the seafloor to the surface over about 6 minutes and splits the ocean in two for ~20 h, then sinks. Separated populations drift apart genetically (allopatric speciation), then meet again |
| **Meteor impact** | ~every 48 h | Kills everything within 700 units, releases minerals, and dims the sun by up to 70% for ~4 days (an impact winter) |
| **Seed bank** | Always | If population falls below a minimum, the ancestor recolonizes so the world never goes dark |

## 7. Future extensions

- **Sexual reproduction**: whole-chromosome inheritance from each parent. Mating is only possible below a genetic-distance threshold, and color preference drives assortative mating. This gives real reproductive isolation. Ideally the capacity for sex itself evolves.
- Body shape (elongation), appendages, bioluminescence, nutrient limitation for phototrophs.
- Viewer interaction: chat commands wired to the existing event triggers (`World::trigger`, Ctrl+1–4 in the viewer).

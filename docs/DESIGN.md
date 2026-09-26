# Design

The rules of the simulation: the world, the genome, how organisms live, eat, reproduce and die, and how species are tracked. Numbers here are **defaults** — every constant lives in [`configs/default.toml`](../configs/default.toml) and is expected to be tuned.

Guiding principles (see [VISION.md](VISION.md)):

- **Energy is (mostly) conserved.** Energy enters the world only as sunlight and vent minerals, and leaves as metabolic heat. Organisms compete for it; population limits emerge from that competition, not from caps. Nutrients cycle between water, bodies and the dead in the same way.
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
| **Minerals** | 32 u | Vents; decaying detritus (more without oxygen) | Chemotroph and sulfur-photosynthesis uptake; slow precipitation; diffusion |
| **Detritus** | 32 u | Small corpses, uneaten meal leftovers, decaying large corpses, fermentation waste | Scavenger uptake; decay (using oxygen); sinks and settles on the floor |
| **Oxygen, CO₂** | 32 u | Photosynthesis (O₂), breathing, fermentation and rot (CO₂), the air, vents (CO₂) | Breathing and rot (O₂), photosynthesis (CO₂), vents (O₂), the air |
| **Nutrients** | 32 u | Deep water, rotting bodies, excretion, vents, upwelling | Uptake by organisms |
| **Toxin** | 32 u | Toxic organisms | Breaks down (half-life ~35 s) |

All the chemistry is described in §4.

**Light competition.** Light is computed column by column from the surface down. Each body in a cell intercepts light in proportion to its width and absorptance (`α = 0.1 body + 0.8 × photo capability`); what is absorbed by pigment becomes energy for that organism, what is absorbed by the body is lost, and what passes through continues downward. A dense bloom at the surface therefore shades everything beneath it — visibly, because the renderer draws the light field.

### Time

- 1 tick = 1/30 s of sim time. "1×" speed = 30 ticks per real second.
- **Day/night**: 10 minutes per day. Nights are dim, not black.
- **Seasons**: 6 days per year (1 hour at 1×). Modulate sunlight (±30%) and surface temperature (±6 °C).
- **Climate**: a slow random walk of global temperature (±8 °C) over many years — ice ages and warm eras.

---

## 2. The genome

A genome is **60 bytes** in nine lettered chromosomes. **Every byte is one gene** and means one thing. Genes are written in hex, e.g. `B:20A050200000`.

| Chr | Byte | Gene | Decoding (g = byte, f = g/255) | Upkeep* |
|-----|------|------|---------------------------|---------|
| **B** Body | B0 | Size | Adult radius = 2^(g/32) (every `+0x20` doubles size: `20`=2, `40`=4, `80`=16, `C0`=64, `FF`≈250) | — |
| | B1 | Buoyancy | `00` sinks, `80` neutral, `FF` floats (heavy parts pull it down) | 0.3 f |
| | B2 | Reserves | Energy storage = (0.3 + 2.7 f) × mass | 0.3 f |
| | B3 | Longevity | Lifespan = 300 s × √radius × (1 + 3 f) | 0.4 f |
| | B4 | Elongation | Round → 4:1 long (same area). Less drag, slower turns | — |
| | B5 | Segments | 1–6 body segments; parts can repeat on every segment | — |
| **C** Color | C0 | Hue | 0–360° | none |
| | C1 | Saturation | 20–100% | none |
| | C2 | Pattern | High 2 bits = style (plain/spots/stripes/rings), rest = scale | none |
| **D** Diet | D0 | Photosynthesis | Pigment absorptance | 0.3 c |
| | D1 | Chemosynthesis | Mineral uptake | 0.3 c |
| | D2 | Scavenging | Detritus / carcass uptake | 0.3 c |
| | D3 | Hunting | Attacking, engulfing and digesting live prey | 0.4 c |
| **G** Chemistry | G0 | Respiration | Share of needs met by breathing oxygen (the rest is fermented) | — |
| | G1 | Oxygenic | Share of photosynthesis that uses CO₂ and releases O₂ (the rest uses minerals) | — |
| | G2 | Nutrient uptake | Nutrient uptake from the water | 0.2 f |
| | G3 | N-fixation | Makes nutrients from energy | 0.2 f |
| | G4 | Toxin | Leaks poison into the water; poisonous to eat (below `06`: none) | 0.3 f |
| | G5 | Resistance | Ignores up to 90% of toxins | 0.3 f |
| **H** Health | H0 | Regeneration | 0.2–5% of max HP per second (1.5 energy per HP) | 0.2 f |
| | H1 | Toughness | Max HP = (0.3 + 1.7 f) × mass | 0.3 f |
| **N** Nature | N0–N8 | Drives | Signed: `80` = 0, `FF` = +1 (toward), `00` = −1 (away). See §5 | — |
| | N9 | Restlessness | Cruising throttle and random wandering when nothing is sensed | — |
| **P** Parts | P0–P23 | Eight body parts, 3 bytes each | See §3 | per part |
| **R** Reproduction | R0 | Offspring size | Newborn mass = 0.5 × 2^(−(255−g)/32) × adult mass (`FF` = 50%, fission-like; `00` ≈ 0.2%) | — |
| | R1 | Brood size | 1 + (g >> 5) offspring per event (1–8) | — |
| | R2 | Patience | Reproduce when reserves ≥ (30 + 70 f)% full | — |
| **T** Temperature | T0 | Preferred temp | 0–40 °C | — |
| | T1 | Tolerance | ±1–15 °C | 0.4 f |

\* **Upkeep** terms add to a multiplier on basal metabolism: `U = 1 + Σ upkeep`. A do-nothing blob is cheap; every capability has a running cost.

There are no "animal" or "plant" genes. What an organism takes from and gives to the water is the sum of the processes its genes run (§4), so "oxygen producer" or "CO₂ producer" is an outcome, not a category.

### Diet capability (generalist penalty)

With `x_i` the four diet genes as fractions, each diet is weakened by the investment in the others:

```
c_i = x_i × (1 − 0.6 × Σ_{j≠i} x_j)
```

A specialist (one gene only) gets `c = x`. Taking up a second diet is gradual: a small second gene works linearly and costs the main diet a little. But true generalists lose overall: two full diets give 0.4 each (0.8 total). Upkeep is charged on **capability**, not on the raw gene, so a weak unused diet gene is cheap. It drifts as cryptic variation until the environment makes it useful — one of the main routes to **exaptation**.

### Example: the ancestor

A tiny, floating, green photosynthesizer with no body parts, so it can neither move nor sense anything. Every other ability has to evolve.

```
B: 20 A0 50 20 00 00   radius 2, floats gently, modest reserves, lives ~10 min, round
C: 55 C0 00            green, plain
D: FF 00 00 00         pure photosynthesis
G: E0 FF 80 00 00 00   breathes oxygen for 88% of its needs, releases oxygen, takes up nutrients
H: 20 30               slow regeneration, fragile
N: 80 80 80 80 80 80 80 80 80 00   no drives, never wanders
P: 000000 000000 000000 000000 000000 000000 000000 000000   no parts
R: FF 00 B0            fission-like: one half-size offspring when ~75% full
T: 80 A0               prefers ~20 °C, ±10 °C
```

### Mutation

Applied independently to each gene of each offspring:

- Probability **1%** per gene (3% for the three color genes, a fast-ticking "molecular clock" so lineages visibly diverge in color). About half of offspring are exact copies.
- 5% of mutations **re-roll** the gene to a random value. The rest step by ±1 (50%), ±2 (25%), ±4 (12.5%) or ±8 (12.5%).
- Values clamp at `00` and `FF`.
- A part's **type** byte doesn't step: it shifts to a neighboring kind in the catalog (fin ↔ leg, spine ↔ jaw, eye ↔ antenna, …) or gains or loses symmetry (single → pair → ring).
- Whole parts: **duplication** (0.4% per offspring: a part is copied into an empty slot, often to a new place), **deletion** (0.3%), and **new parts** (1%: a random kind grows in an empty slot from its leftover bytes).

Low mutation rates keep lineages coherent, so species persist and drift is visible. Deleted parts leave their size and placement bytes behind as cryptic variation that a later part can reuse.

---

## 3. Body plans

Bodies are ellipses of constant area: the Elongation gene stretches them from round to 4:1, and the Segments gene divides them into 1–6 segments. On the body sit up to **eight parts** from a fixed catalog of fifteen kinds. Every kind has its own look and job. How many, where, how big, mirrored or not, and what they attach to all evolve.

Each part is three bytes in chromosome P:

| Byte | Meaning |
|------|---------|
| Type | Low 4 bits: kind (0 = empty slot). High 4 bits: symmetry: 0 single, 1 mirrored pair, 2–5 a ring of 3–6 |
| Size | Strength, length, and how elaborate it looks (a small eye is a pigment spot, a big one a camera eye) |
| Place | High 3 bits: segment 1–6, 6 = every segment, 7 = on the tip of the previous part. Low 5 bits: angle around the body (0 = forward, 16 = back) |

Symmetry and repetition multiply a part: a pair of legs on every segment of a 4-segment body is 8 legs. Parts attached "on the previous part" make trees: an eye on a stalk, fronds on stems, claws on legs.

| Kind | Job | Notes |
|------|-----|-------|
| Flagellum | Swimming thrust | Best for small bodies; pushing or pulling |
| Cilia band | A little thrust, turning, filter feeding | Best for small bodies |
| Tentacle | Reach: grabs prey and carcasses beyond the body | Also filters and grips |
| Frond | Catches light over a wider span, takes up nutrients | Makes large photosynthesizers viable |
| Fin | Swimming and turning | Best for larger bodies; works best on the sides |
| Tail fluke | Powerful swimming | Best for larger bodies; must point back |
| Leg | Walking on rock and the seafloor; grip against currents | Paddles weakly in open water |
| Holdfast | Anchors to rock on contact; draws extra nutrients there | Anchored organisms can't be moved by currents |
| Stalk | Raises whatever is attached to its tip | Parts on a stalk work 30% better |
| Eye | Sight in a cone where it faces | Bigger = farther, narrower |
| Antenna | Smell in every direction | Shorter range than eyes |
| Lure | Glows; anything drawn to light swims toward it | The anglerfish trick |
| Spine | Hurts attackers | |
| Jaw | Bite strength | Works best facing forward |
| Shell plate | Armor | Heavy: slows and sinks the body |

**What parts add up to.** At birth the parts are laid out on the body and summed into plain numbers, so the per-tick simulation never looks at parts again:

- **Swimming** = 1 − e^(−thrust / drag). Each swimming part adds thrust according to its size, its direction, and how well it suits the body's size. Every part adds drag; elongated bodies have less. Max speed = swimming × 20 × √radius.
- **Turning** from fins, cilia and flagella; long bodies turn more slowly.
- **Walking** speed from legs, **grip** from legs, tentacles and holdfasts, **anchoring** from holdfasts. Only organisms with such parts check whether they touch rock.
- **Senses**: each eye is a view cone (up to six are tracked); antennae give a smell radius. No eyes and no antennae means blind (contact only).
- **Reach** from the longest tentacle; **light span** and nutrient-uptake surface from fronds; **filtering** from cilia and tentacles; **glow** from lures.
- **Armor** from shell plates, **spikes** from spines, **jaws** from jaws, each with diminishing returns (1 − e^(−x)).
- Every part costs **upkeep** and has **mass**: building a body with parts costs more, and heavy parts sink it.

---

## 4. Chemistry

The water carries oxygen, CO₂, nutrients and toxin, alongside minerals and detritus. Oxygen and CO₂ are measured so that 1.0 is water in equilibrium with today's air. Everything an organism exchanges with the water comes from processes its genes run:

| Process | Takes | Gives | Run by |
|---------|-------|-------|--------|
| Oxygen-producing photosynthesis | light, CO₂ | energy, O₂ | Photosynthesis × Oxygenic share. Slows when CO₂ runs low |
| Sulfur photosynthesis | light, minerals | energy | Photosynthesis × (1 − Oxygenic). Needs minerals; no oxygen released |
| Chemosynthesis | minerals | energy | Chemosynthesis |
| Breathing (aerobic) | O₂ | CO₂ | Respiration gene, if there's oxygen |
| Fermentation | — | CO₂, organic waste | Whatever breathing doesn't cover |
| Building body mass | nutrients | — | Growth and reproduction |
| Uptake / fixation | nutrients from water / energy | nutrients (stored) | Uptake and N-fixation genes |
| Excretion | — | nutrients | Anything eaten beyond the store |
| Toxin | — | toxin | Toxin gene |

**Breathing and fermenting.** Each second an organism needs energy for basal metabolism and movement. It breathes oxygen for up to its Respiration share, if the water has enough (below 25% saturation, breathing falters). Everything else must be fermented, which costs 60% more and leaks half the extra as organic waste into the marine snow (food for scavengers). Fermentation capacity is at least 25%, plus whatever breathing capacity it lacks. Needs that neither can meet **suffocate** the organism (HP loss). Strict fermenters (Respiration below 50%) are **poisoned by oxygen**.

So aerobes are cheap to run but die in dead zones; fermenters live anywhere oxygen is scarce but pay more; a mix pays a little more everywhere.

**Nutrients.** Building a unit of body mass takes one unit of nutrients. Organisms store up to their body mass in nutrients, take them up from the water (Uptake × radius × frond surface × concentration; ×3 while anchored by a holdfast), fix them at an energy cost (N-fixation, hampered by oxygen), or eat them in other bodies. A parent pays for each child's body and a starting store. When an organism dies or is eaten, its nutrients go to the eater, the carcass, or the water. Surface water is poor and deep water rich, so upwellings cause blooms, and animals that eat and excrete near photosynthesizers fertilize them.

**Toxins.** Toxic organisms leak toxin that spreads and breaks down within a minute or so. It damages organisms in proportion to its concentration (small bodies suffer more), so it only matters in blooms of producers: red tides. The toxin carries the producers' hue, and organisms of that hue are immune: producers, their kin, and look-alikes. Resistance ignores up to 90%. Eating toxic prey poisons the hunter.

**The water.** Gases, nutrients and toxin diffuse. Rotting detritus uses oxygen and releases CO₂ and minerals; without oxygen it rots more slowly and releases more minerals (sulfides). Vents release oxygen-free water, CO₂ and some nutrients. Deep water slowly relaxes toward its usual make-up (older water: less oxygen, more CO₂ and nutrients), standing in for ocean circulation. The result: oxygen-rich sunlit water, oxygen-poor sediment and vents where fermenters can thrive, and dead zones under heavy snowfall.

**The air.** Two global levels, oxygen and CO₂ (1.0 = today). The top layer of water trades gases with the air, and the air changes by what it gains or loses. It is large, so it changes over days, not minutes. CO₂ warms the climate (+3 °C per doubling). A world started with `initial_oxygen = 0.05` begins with anoxic air, and oxygen-producing life has to oxygenate it.

---

## 5. Life of an organism

### Size, mass, growth

- `mass = radius²`. Organisms are born small and **grow** toward their adult size (gene B0) by investing surplus energy, at up to 5% of their mass per second, costing **1.25 energy per unit mass** (more with heavy parts) and one unit of nutrients.
- Juveniles keep half their reserves as a buffer and invest the rest in growth.
- Only adults reproduce, so large organisms have long juvenile phases. Juveniles eat different things and are vulnerable to different predators — **ontogenetic niche shifts** come for free.

### Energy budget (per second)

```
basal     = 0.003 × mass^0.75 × U × temperature_stress
movement  = 0.003 × mass^0.75 × 3 × swimming × drag × throttle²   (walking is cheaper)
needs     = (basal + movement) × breathing/fermentation cost (§4)
+ photosynthesis, chemosynthesis, scavenging, hunting (below)
```

- Metabolism scales with `mass^0.75` (Kleiber): large organisms are cheaper per unit mass but need more in total.
- If reserves hit zero, the deficit comes out of HP (starvation).

### Food sources

| Diet | Mechanic | Scales with |
|------|----------|-------------|
| Photosynthesis | Share of light absorbed in the organism's light cell(s) (§1), limited by CO₂ or minerals (§4) | Width × frond span (big photosynthesizers need fronds) |
| Chemosynthesis | Uptake from the mineral field: `2 × c × radius × concentration`, depleting it | Perimeter (favors small) |
| Scavenging | Uptake from the detritus field, `12 × c × radius × (1 + filter) × concentration` (intercepting falling marine snow), plus a filter-feeding bonus when swimming; eating large carcasses on contact or within tentacle reach | Width; width × speed when swimming |
| Hunting | See below | Size, jaws |

**Hunting.** On contact with a non-kin organism (or within reach of its tentacles):

- **Engulf** if the prey's radius is ≤ 50% of the hunter's: the prey dies instantly, with the chance per second reduced by prey armor. The hunter's spikes don't matter, but the **prey's** spikes hurt the hunter.
- Otherwise **bite**: the hunter tears off `0.3 × c_hunt × (0.2 + 1.8 × jaws) × radius²` of the prey's mass per second, reduced by prey armor. Bitten prey shrink and lose HP, and must regrow. The prey's spikes reflect damage.
- The hunter assimilates **80% × c_hunt** of the prey's body energy (1 per unit mass, plus the prey's reserves), proportionally to damage dealt, and all of its nutrients. Anything it can't store becomes detritus (energy) or is excreted (nutrients).
- Toxic prey poisons the hunter: damage = 0.8 × prey toxin × mass eaten × (1 − resistance).

Energy moves up the food chain with losses, so predators are rarer than prey, and populations can oscillate.

### Health

- Max HP scales with mass and toughness (H1). Regeneration (H0) costs energy per HP.
- **Temperature stress**: `s = ((T − T_pref) / tolerance)²`. Metabolism × (1 + 0.5 s); beyond tolerance (s > 1), HP damage.

### Reproduction

When an organism is **adult** and its reserves are at least the patience threshold (R2), it produces up to `brood` offspring (R1) of the newborn size (R0). Each offspring costs `newborn_mass × 1.25` (more with heavy parts) plus the newborn's starting reserves (50% of its capacity), and the nutrients for its body plus a quarter-full store. It produces as many as it can afford. Offspring appear beside the parent and mutate independently.

- **r-strategists**: many tiny offspring, most of which die young.
- **K-strategists**: few large offspring.

### Death

| Cause | Result |
|-------|--------|
| HP reaches 0 (starvation, predation, temperature, spikes, suffocation, poison) | Corpse |
| Old age (past lifespan: 20% chance per second) | Corpse |
| Engulfed | No corpse — eaten whole |

Every corpse becomes a **carcass**: a grey, torn version of the organism (parts included) that sinks, can be eaten on contact (bites show), and decays into the **detritus field** (marine snow). Small carcasses decompose within about a minute; large ones last much longer, so a whale fall feeds a seafloor community.

Scavengers live two ways: eating carcasses on contact ("Eating a carcass"), and filtering marine snow from the water and the sediment that piles up on the seafloor ("Eating marine snow").

### Movement and physics

- Swimming: the organism turns toward its desired heading (limited by its turn rate) and thrusts at `throttle × max speed`. Small organisms stop instantly (low Reynolds number); large ones coast.
- Walking: organisms with legs that touch rock or the seafloor move at their walking speed (if faster than swimming), and grip resists the current and buoyancy.
- Anchoring: an organism with a holdfast that touches rock and isn't swimming off stays put. Anchored offspring settle next to their parents: reefs and kelp forests.
- **Buoyancy** gives a vertical drift proportional to √radius. **Brownian jitter** affects only tiny organisms. **Currents** carry everything: large slow gyres, plus eddies in the upper 450 units (curl noise, so the flow neither piles up nor empties out).
- Overlapping organisms push each other apart by mass. Rocks are solid.

---

## 6. Behavior: drives

Motile organisms (anything that can swim or walk) steer by a weighted sum of **drives** (chromosome N). Each drive is a signed weight: positive = move toward, negative = move away.

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
| N8 | Oxygen | Oxygen gradient |

- Food-related drives (light, minerals, carrion, smaller) are scaled by **hunger**, so fed organisms relax.
- When nothing is sensed, restlessness (N9) sets a cruising throttle and random turning.
- **Fields** (light, warmth, minerals, detritus, oxygen) are sensed without organs. **Other organisms and carcasses** are only perceived with eyes (within each eye's view cone) or antennae (all around). They only perceive individuals at least ~1/8 of their own size — a whale doesn't track single bacteria; it swims toward dense food fields instead.
- **Lures** count as light: anything with a positive light drive swims toward them.
- **Kin** = similar color (hue and saturation). Hunters never attack kin. This makes color matter for predators, so camouflage-by-mimicry is possible.

Examples of what the drives can produce: phototaxis (+Light), vent-seeking (+Minerals, +Warmth), hunting (+Smaller), fleeing (−Larger), schooling (+Kin), bottom-dwelling (−Rise).

---

## 7. Species

- Every organism carries a **species ID** inherited from its parent.
- Distance between genomes = weighted mean of per-gene differences. Color genes weigh 25%, part size and placement 30% (and only when both genomes have a part in that slot), everything else 100%. Each gene contributes at most 0.3 of its range, so one large jump can't make a species on its own: it takes changes in several genes. A different kind of part in a slot counts 0.5.
- If a newborn's distance from its species' **founder genome** exceeds the species threshold, it founds a **new species**. The new species records its parent species and founding time.
- A species is **established** (named, listed, charted) once it reaches 20 living members. Variants that die out before that are forgotten.
- **Names** are generated pseudo-Latin binomials. The genus is inherited unless the distance from the genus founder exceeds a larger threshold, so related species share a genus name (*Rubrovorax lentus*, *Rubrovorax magnus*).
- The resulting records form a **phylogeny**: parent species, founding and extinction times, peak population.

---

## 8. Environment dynamics and events

| Event | Timescale | Effect |
|-------|-----------|--------|
| Day/night, seasons | 10 min / 1 h | Pulses of light and temperature: reserves, and strategies that last the night, pay off |
| Climate | Days | Global temperature drifts ±8 °C: range shifts, temperature specialists rise and fall |
| Vent lifecycle | Hours | Vents flare, fade and die; new ones open elsewhere. Colonization and founder effects |
| **Upwelling** | ~every 3 h | Minerals and nutrients well up into the sunlit water for 30 min: blooms, and a surface bonanza for chemotrophs and mixotrophs |
| **Plague** | ~every 8 h | Strikes the most common species (only if it holds ≥35% of all life); ~70% of its members die over 10 min. Kill-the-winner keeps any one species from taking over |
| **Rock wall** | ~every 30 h | A wall rises from the seafloor to the surface over about 6 minutes and splits the ocean in two for ~20 h, then sinks. Separated populations drift apart genetically (allopatric speciation), then meet again |
| **Meteor impact** | ~every 48 h | Kills everything within 700 units, releases minerals, and dims the sun by up to 70% for ~4 days (an impact winter) |
| **Seed bank** | Always | If population falls below a minimum, the ancestor recolonizes so the world never goes dark |

## 9. Future extensions

- **Sexual reproduction**: whole-chromosome inheritance from each parent. Mating is only possible below a genetic-distance threshold, and color preference drives assortative mating. This gives real reproductive isolation. Ideally the capacity for sex itself evolves.
- Part kinds that can be combined into new ones (a leg with a fin becomes a flipper), and body parts on carcasses that scavengers can eat separately.
- Viewer interaction: chat commands wired to the existing event triggers (`World::trigger`, Ctrl+1–4 in the viewer).

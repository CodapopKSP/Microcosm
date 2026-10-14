# Future ideas

Big changes we want eventually, parked until the current systems are balanced. Nothing here
is implemented. Each section says what we're after, how it could work with what already
exists, and what to watch out for. The goal for all of them is the one in
[VISION.md](VISION.md): new ways of living should **evolve** from genes and selection, never
be switched on by hand.

Suggested order is at the end. Before the new ideas, a record of changes that were built on an
earlier branch and set aside.

---

## Set-aside changes

The game was rebuilt from commit 451e398, "Fine tuning, first good version", because the runs
after the last two commits on the old `rewrite` branch were much less diverse. Their simulation
changes are listed here so they can be brought back **one at a time**, each checked in a long
run before the next.

Already brought back: the tentacle rework (each tentacle grasps one thing within its full
length of its base and drags it to the mouth, `tentacle_reach` 1.0; the selection overlay shows
the grasps), the wide mineral halo around each vent (`vents.halo`), and the shelf and cliff
vents (in the generated ocean as `terrain.shelf_vents`; the Deep Ocean map has its own painted
permanent vents). The old "fewer vents" change no longer applies: painted maps decide where
vents can open.

### From 8a4e47c, "Update tentacles"

- **Jaws bite where they sit.** Jaws near the mouth made the mouth bite harder; jaws elsewhere
  became their own bite sites. Mouth jaws also cracked shells (`jaw_pierce` 0.6). Today jaws
  only strengthen the mouth, weighted by how far forward they face. The selection overlay
  showed each jaw's bite site.
- **A floating island** in mid-water with a vent chamber and tunnels (`island` under
  `[terrain]`).

### From 90344a1, "Large updates, sweeping. Mostly hunting and chem."

Its visual parts are already in: the mouth art, the subtler bite effect, the swallowing
animation, no feeding glow, and the star and speck fixes. Its simulation changes were:

- **Mouth-only eating:** no more eating through the body at 30%.
- **Hunters stop at 90% energy** (`full`): they leave prey and carcasses alone and stop
  chasing them.
- **Swallowing:** a bigger size limit (60% → 65% of the hunter's radius); smaller prey go down
  faster, first by the square of the size difference and later only by its log, scaled by
  hunting skill; several small prey per tick for skilled hunters (`gulps`); tiny prey worth
  less to a big hunter (`small_prey_value`). The first version produced giant, part-less
  predators that sat still and swallowed everything nearby.
- **Shells:** stronger (`armor_protection` 0.75 → 0.85), broader coverage, lighter and
  cheaper, less speed penalty, and armor makes prey count as bigger for swallowing
  (`shell_bulk`).
- **Stronger generalist penalty** (0.4 → 0.5).
- **Vents:** a smaller oxygen-free core, faster-spreading and longer-lasting minerals, and the
  shelf vent as a **chimney** that shoots minerals up to the sunlit water.
- **Chemosynthesis:** faster uptake, more energy per unit of minerals, lower upkeep. The
  numbers were on an older scale (uptake 2 → 3; today's `chemo_rate` is 20), so they need
  working out again before trying it.

---

## 1. Colonizing the depths

**Where it stands.** Deep life exists now, but it's thin and much of it lives at vents. Marine
snow that reaches the floor piles up there as sediment that scavengers filter, rotting without
oxygen releases extra minerals, and caves keep shedding detritus. In two saved worlds, 4% and
24% of organisms lived deeper than 2,500 units (1,225 and 3,938), and only some of those
(257 and 1,748) more than 600 units from a vent. The deep is still far poorer than the
surface: light is gone and vents are a handful of points.

**What real oceans do.** Deep life runs on four things: food falling from above, chemistry
seeping out of the sea floor, a thin layer of low-oxygen water at mid depth, and animals
that commute up and down every day.

**Ideas, roughly cheapest first:**

- **Stronger cold seeps.** Oxygen-poor rot already releases extra minerals wherever sediment
  lies. Making that a real, steady source along the floor (or at painted seep spots) would let
  chemotrophs live away from the few vents.
- **Twilight photosynthesis.** Make mineral-based (purple) photosynthesis work in much dimmer
  light than oxygen-producing (green) photosynthesis, as green sulfur bacteria do in the real
  ocean. The purple lineage then owns the twilight zone near vents and seeps, and green owns
  the sunlit surface: a gradient of niches rather than a wall.
- **An oxygen minimum zone.** At mid depth, rotting marine snow uses up oxygen faster than it
  mixes down. Fermenters win there, aerobes suffer. A distinct band with its own specialists,
  which also connects the anaerobic chemistry to the geography.
- **Daily migration.** Organisms that rise to feed at night and sink by day to hide from
  hunters that need light to see ([§4](#4-behaviours-that-depend-on-state) makes this
  evolvable). This carries food and oxygen downward, as it does in real oceans.
- **Eggs and cysts that sink** ([§5](#5-life-stages-eggs-and-metamorphosis)): a resting stage
  can fall through the zones and hatch deep, so a lineage can cross the moat without having
  to survive the whole trip.

**Chemistry rework (only if the above isn't enough).** Today minerals stand in for all
reduced chemistry. We could split out sulfide (from vents, seeps and anaerobic rot) and give
anaerobes a cheaper way to live on it. Keep the number of fields small: each field costs
memory, diffusion time and a map mode.

**Watch out for:** deep water getting *too* easy, so everything migrates down; and field
count and diffusion cost (headless timings are the check).

---

## 2. Sexual reproduction

**The goal.** Sex should appear because it pays, not because it's switched on. Its payoffs
in biology are combining good mutations from different lineages, clearing out bad ones, and
keeping up with parasites that adapt to whatever is common (the "Red Queen"). Its costs are
finding a partner and passing on only half your genes.

**How it could evolve here:**

- **A "sex" gene (R chromosome):** the chance that, when ready to breed, an organism waits
  for a partner instead of budding alone. At 0 it's exactly today's budding, so sex can
  creep in gradually. Many real protists do exactly this: bud when times are good, mate when
  stressed.
- **Mating:** two organisms of similar genome (within the species threshold), both ready to
  breed, touch. They make offspring whose genomes mix both parents: each chromosome from one
  parent or the other, with an occasional crossover inside a chromosome. Body-part slots swap
  as whole slots, so body plans stay coherent. Both parents pay half the endowment.
- **Finding each other:** the existing kin drive and senses already bring look-alikes
  together. A state-dependent kin drive ([§4](#4-behaviours-that-depend-on-state)) lets
  organisms gather only when ready to breed: spawning aggregations.
- **Species become real.** Today species are defined by genome distance. With sex, a
  species could be the group that actually interbreeds; hybrids of diverging groups can be
  less fit, which pushes the groups further apart. That's speciation by an actual mechanism,
  which is great for the edutainment side.
- **Mate choice gives colour a purpose.** A "preferred hue" gene plus "choosiness": the
  colour and pattern genes, which are cosmetic today, start to matter, and sexual selection
  can drive striking colours. Very watchable on stream.

**Simpler variants to consider first:** bacterial-style *conjugation* (on contact, copy one
chromosome from a kin partner, no offspring involved), or *mating types* (two
self-incompatible types, so each needs the other).

**Watch out for:** sparse populations where nobody finds a partner (keep budding as a
fallback); and the family tree, where offspring now have two parents (show the mother, and
record the father).

---

## 3. Endosymbiosis and parasitism

**The goal.** Occasionally something swallowed whole survives inside its hunter. From there
it can harm the host (a parasite), help it (a mutualist, as chloroplasts and mitochondria
once did), or drift between the two, depending on genes and on how it spreads.

Small hunters that hold on to a big organism with tentacles and bite it are already a kind of
external parasite; this is the internal version.

**The inside of a host, kept simple.** No spatial model: a host holds up to a few
*inclusions* (say 4), each a small record of genome, mass, energy and age. The inside is one
well-mixed place:

- **Light** is the light where the host is, dimmed by the host's body (a new "transparency"
  aspect: pale, weakly pigmented hosts let more through).
- **Oxygen, CO2, minerals** are the water's at the host's position.
- **Food** is the host's own energy and nutrient store.

**How it starts:** a gene for "resisting digestion" gives a small chance of surviving being
engulfed. A matching host gene digests inclusions over time. Two sides of an arms race.

**Living inside:** each inclusion runs its own diet on the host's interior:

- Photosynthesizers make energy from the light that reaches them.
- A **"share"** gene sets how much of what it makes goes to the host (a mutualist) versus
  how much it keeps and grows on.
- A **"take"** gene sets how fast it draws on the host's stores (a parasite).
- It can shelter the host too: an inclusion with toxin could poison the host's attackers.

**Reproduction and inheritance:** this is what decides mutualist versus parasite.

- **Vertical:** when the host buds, inclusions are split between parent and bud. The
  inclusion's success is tied to its host's, so selection favours helping.
- **Horizontal:** an inclusion that grows big enough bursts out as free-living offspring,
  damaging or killing the host. Selection favours taking.

This is a real result from evolutionary biology (the "virulence trade-off"), and it could
play out on screen: lineages that spread from host to host get nastier, lineages passed from
parent to child get friendlier.

**Eventually (stretch):** an inclusion lineage carried vertically for many generations could
become an *organelle*: it stops being a separate organism, its benefit becomes a permanent
trait of the host, and an event announces it.

**Showing it:** small cells drawn inside the host's body in the inclusion's colour; the
inspector lists them ("carries 2 green algae: +0.02 energy/s"); a chart for how many hosts
carry which inclusion species.

**Watch out for:**

- **Bookkeeping:** inclusions still belong to their species (population counts, extinction,
  the family tree).
- **Death:** when a host dies, inclusions escape into its carcass or die with it.
- **Cost:** a handful of cheap updates per host, only for hosts that have inclusions.

---

## 4. Behaviours that depend on state

**The goal.** Today each drive (toward light, warmth, prey, away from bigger organisms,
toward kin…) has one fixed strength for life. Real behaviour depends on state: hungry versus
full, young versus ready to breed, hurt, night versus day. A hard-coded "stop hunting when
full" rule (see [Set-aside changes](#set-aside-changes)) would be one example; these should
*evolve* instead.

**Modulators (a new chromosome).** A few slots, like the body-part slots, each saying
"*this state* changes *that drive* by *this much*":

- **Input (state):** hunger, readiness to breed, maturity (juvenile → adult), health, light
  level (day and night), age, recent attack.
- **Target:** any drive, or a threshold like "ignore food above X% energy" (the fullness
  rule, turned into a gene).
- **Gain:** signed. Optionally a threshold byte, so the effect switches on sharply instead
  of gradually.

Mutation changes gains, and sometimes rewires which state affects which drive, like the
part-type mutation does today. It's a tiny gene-regulation network.

**What it could produce** (none of these would be coded directly):

- **Breeding retreat:** ready to breed → avoid light → hide in dark caves to reproduce, come
  out to feed.
- **Daily migration:** darkness → rise; daylight → sink.
- **Growing up:** juveniles school with kin and flee everything; adults hunt alone.
- **Wounded:** hurt → flee more strongly.
- **Satiety:** each species evolves its own fullness threshold.

**Showing it:** the inspector translates each modulator into plain English ("seeks darkness
when ready to breed"), and the current mood shows next to the activity label.

**Cost:** a few multiply-adds per organism per steering decision; negligible.

---

## 5. Life stages: eggs and metamorphosis

**The goal.** Juveniles and adults that differ in more than size, with eggs and
metamorphosis as options that evolve gradually, not as fixed features.

**Stage-specific body parts.** The body-part type byte has a spare bit (bit 7). Use it
(plus a second flag) to mark a part as juvenile-only, adult-only or both. (Today any symmetry
value above 5 reads as 5, so a set bit 7 reads as a ring of 6: the decoder has to mask it off
first.) A larva with
flagella for spreading out can settle into a sessile adult with a holdfast and fronds: a
barnacle-style life cycle, from one genome.

**Metamorphosis.** A gene sets the size (or maturity) at which the stage switches. Switching
re-decodes the organism's traits for the new stage and costs energy. Optionally there's a
resting stage in between (still, maybe armoured) that's visible and vulnerable.

**Stage-specific diet or behaviour.** A diet-shift gene (a larva scavenging marine snow, an
adult hunting), and the maturity input to the modulators in
[§4](#4-behaviours-that-depend-on-state) for behaviour.

**Eggs.** A gene for "egg time": 0 keeps today's live budding; above 0, offspring start as
an egg that doesn't eat or move, drifts, sinks or floats (its own buoyancy gene), and hatches
after that time. Eggs make the trade-off between many small offspring and few large ones
visible. They can also be:

- **Resting cysts** that survive cold, darkness or low oxygen and wait for better
  conditions: lineages survive winters and cross the depth moat ([§1](#1-colonizing-the-depths)).
- **The product of sex** ([§2](#2-sexual-reproduction)): the zygote is a tough cyst, as in
  many real protists, which ties sex to hard times and dispersal.
- **Food:** egg clouds are food for scavengers and small hunters, a new niche.

**Showing it:** translucent eggs with a developing embryo inside; a short metamorphosis
animation; the inspector shows the stage ("larva, 60% to adult").

**Watch out for:** population counts (do eggs count as members?), the family tree (born at
laying or at hatching?), and eggs piling up (a cap per area, or they rot).

---

## 6. Species split on new abilities and new kinds of body part

**The problem.** A child starts a new species only when its genome as a whole has drifted
more than a threshold (0.035) from its species' founder. The distance averages every gene,
so a single new *kind* of part (a first tentacle, the first eye) moves it by about 0.01, and
so does gaining a whole new way of living. Lineages that look and live completely
differently can keep the same name, while a pile of small, invisible changes splits them.

**The idea: a body-plan signature.** Alongside genome distance, each species remembers its
founder's *signature*, a short set of yes/no features computed from traits at birth:

- **Kinds of part present:** one flag per kind (tentacles, eyes, jaws, fronds…). Only whether
  the kind is present, not how many: going from 2 to 3 tentacles changes nothing, 0 to 1
  does. Only parts big enough to do something count, so a trace part doesn't flip it.
- **Ways of living:** each diet (photosynthesis, chemosynthesis, scavenging, hunting), plus
  toxin, nitrogen fixation, a lure, breathing versus fermenting, and oxygen-producing versus
  mineral-based photosynthesis.

A birth whose signature differs from its species founder's starts a new candidate species at
once, and it's named and announced only once it reaches the usual breeding population (20
members), so short-lived oddballs never clutter the list. Genome distance keeps working as
today for gradual drift.

**Avoiding flicker.** Abilities use two thresholds, relative to the founder: a species
without hunting "gains" it only above 25% capability, and one with hunting "loses" it only
below 5%. A capability hovering near one value doesn't keep splitting lineages. Part kinds
can use a minimum size in the same way.

**Payoffs for viewers:**

- Names and events can say what's new: "A *Chlorophytina* lineage grows tentacles: new
  species *Chlorophytina tentaculata*". The existing epithets already describe traits, so the
  new feature can be preferred when choosing a name.
- The family tree becomes a record of innovations: each branch point can carry the feature
  that caused it.

**Watch out for:**

- **Species inflation:** many more candidates are created. The 20-member threshold hides
  most, but the registry's memory and the tree's size need checking over week-long runs.
- **Reversions:** a lineage that loses a new feature again founds yet another species, even
  if it ends up genetically identical to an ancestor. Species never rejoin their parent:
  arriving at the same form again is convergent evolution, and the family tree should show
  it as a separate branch.
- **Choosing the flags:** too many and every mutation is a new species, too few and the
  problem stays. Start with part kinds and diets, then add the others if needed.

**Cost:** a signature is a few bits computed once at birth from traits that are already
decoded; comparing two is one integer comparison.

---

## 7. Gradual exaptation: parts that turn into other parts

**The problem.** A body part's kind is 4 bits of its slot's type byte. When that byte mutates,
the part jumps straight to a neighbouring kind in the catalog (fin ↔ frond or tail fluke,
spine ↔ jaw, eye ↔ antenna). There are no in-between forms, so a new organ appears all at once, usually
useless at first, and is usually lost again. Real exaptation is gradual: a structure doing
one job slowly reshapes into something that does another, and every step on the way works a
little.

**The idea: a "form" byte per part.** Each part slot gets a fourth byte, so the P chromosome
grows from 8 × 3 to 8 × 4 bytes. It says how far the part has changed toward a related kind:

- **Paths between kinds** (a small graph, each kind with one to three neighbours), for
  example:
  - flagellum ↔ tentacle (a whip thickens into a muscular arm);
  - tentacle ↔ antenna (an arm thins into a feeler);
  - frond ↔ fin (a light-catching blade stiffens into a paddle);
  - fin ↔ leg (a paddle gains joints);
  - leg ↔ holdfast;
  - spine ↔ jaw;
  - spine ↔ shell plate;
  - stalk ↔ antenna;
  - eye ↔ lure (a light-sensing spot becomes a light-making one);
  - cilia band ↔ fin.
- **The byte:** 2 bits pick which neighbour it's heading toward, and 6 bits say how far along,
  from 0 (fully the part's own kind) to 63 (fully the neighbour). Mutation nudges the amount
  a step at a time. When it runs past the end, the part becomes the neighbour kind and the
  amount starts again from 0, so a lineage can walk from flagellum to tentacle over many
  generations.
- **Function blends.** A part partway along does both jobs, weighted by how far it has gone:
  a half-flagellum, half-tentacle gives half the thrust and half the grasp. A small penalty
  for being in between (say 10–20% of both) keeps pure forms worth having without making the
  path a dead end. Mass, upkeep and drag blend the same way.
- **Art blends.** The part shader already draws each kind from a few parameters. A blended
  part draws its two kinds' shapes and colours mixed by the amount, so viewers can watch a
  flagellum thicken and grow suckers over hours.

**Watch out for:**

- **Genome distance:** the form byte counts like any other gene, so gradual change splits
  species gradually. With [§6](#6-species-split-on-new-abilities-and-new-kinds-of-body-part),
  a part counts as its new kind once it's more than halfway along.
- **The old jump mutation:** keep a small chance of it, or remove it. Removing it makes every
  new kind of part come from a path, which is slower but more believable.
- **The genome grows** by 8 bytes, which needs a version bump (see the cross-cutting notes).

---

## 8. More varied body shapes (visual, from genes)

**The problem.** Every body is an ellipse stretched by the elongation gene and split into beads
by the segments gene, with one of a few cosmetic patterns. Species that differ a lot in their
genes still tend to look like variations of the same blob.

**The idea: a shape chromosome** of about four bytes, **visual only**: it doesn't change the
simulation, which keeps using the radius and the body shape it has today. Candidates:

- **Outline lobes:** round → a few soft lobes → a star-like or amoeboid outline, with a lobe
  depth.
- **Taper:** teardrop (wide front, thin tail), the reverse, or a waist pinched in the middle.
- **Margin:** smooth, scalloped, frilled, or finely spiky edge.
- **Membrane:** thin and see-through versus thick and opaque, and nucleus size.

**How it fits in:**

- **Genes drift freely** since they cost nothing, like the colour genes today. That gives
  each lineage its own look over time, which helps viewers tell species apart.
- **Genome distance:** shape genes should count at the reduced weight colour genes use, so
  neutral drift doesn't split species on its own.
- **Parts must sit on the drawn outline.** The outline function has to exist in both the Rust
  layout (`BodyShape::attach`) and the shader, like the body-shape constants today, so fins
  and flagella attach to the visible edge rather than an invisible ellipse.
- **Later**, some shape genes could gain a small real effect (a tapered body swims with less
  drag, a frilled margin catches more marine snow), turning cosmetic drift into raw material
  for adaptation, a small exaptation of its own.

---

## 9. Populating the cut-off basin: sea spray and flight

**The setup.** The built-in generated ocean (used when there's no map file, or by a `.toml`
map naming the `ocean` generator) has a mountain about 2,000 wide and 380 above sea level past
the ocean's right edge, and beyond it a basin of water that the mountain cuts off. It starts
empty: ancestors, random vents and events all stay in the ocean. It's a blank slate waiting
for a way in. The shipped Deep Ocean map has no basin (its land on the right is solid rock
with caves and tunnels), but a painted map could have one.

**The scale problem.** Top swimming speed is 20 × √radius at full swimming strength, so most
organisms (radius 1.4–3) manage 20–35 u/s and only rare giants go much faster. Leaping out of
the water at 40 u/s, with a sensible gravity (about 50 u/s²), reaches only about 16 above the
surface, and even a good glider (5 across for every 1 down) then covers only about 80. Leaping
and gliding alone will never cross this mountain; something has to give (see "Flight"
below).

**Two routes, in order:**

1. **Sea spray (microbes, early and rare).** Tiny organisms at the surface very rarely get blown
   across in spray and dropped into the basin, the way real microbes reach isolated lakes.
   - A small chance per tick for organisms below a size threshold, near the surface, to be
     carried over and land somewhere in the basin's sunlit water.
   - Simple to build, and it starts the basin's own story early: an occasional founder,
     then a lineage evolving in isolation (island biogeography, founder effects).
   - Keep it rare enough that the basin's life stays clearly separate from the ocean's.

2. **Flight (larger organisms, later and dramatic).** Flying fish evolved gliding to escape
   predators, not to travel; crossing the mountain would be a rare side effect of an escape
   behaviour, which is exactly the kind of exaptation the sim is about.
   - **Leaving the water:** today organisms are clamped at the surface. Instead, anything
     swimming upward fast enough at the surface breaches and becomes airborne. Fleeing near
     the surface makes that happen naturally.
   - **Fins as wings:** fins give lift in the air. Lift ramps up smoothly from about 80% fin
     size to full at 100% (size gene about E6 for 90%), rather than switching on at a hard
     threshold, so selection can push fins bigger step by step. Lift comes from each fin's
     size and angle across the body.
   - **In the air:** gravity, air drag and lift; energy drains fast, and the organism dries
     out after a while. Landing on the mountain kills it, or it slides back into the water;
     landing in water ends the flight.
   - **Getting over the mountain** needs one of:
     - **Updrafts (recommended):** wind off the sea rises up the mountain's seaward slope,
       like a gull riding a cliff, so a glider caught in it climbs and a lucky one rides it
       over the ridge. Crossings are possible but rare, and the mountain keeps its look.
     - **Powered flight:** fins at the very top of the size range can also flap in the air,
       giving sustained flight at a high energy cost, so only well-fed organisms with big
       reserves get far. A natural extension once gliding exists.
     - **A smaller barrier:** a lower, narrower ridge or a low pass (60–100 high instead of
       380). Simplest, but it changes the map.
   - **Hitchhiking:** small organisms held by a flyer's tentacle, or living inside it (§3),
     cross with it.

**What flight needs in the sim:**

- **Collision with the mountain above water:** the terrain already stores the mountain's
  height above the surface (the seafloor heightmap goes above it), so a simple height check
  works without extending the terrain grid upward.
- **Things that stop at the surface:** the spatial grid, chemistry fields and light all end at
  the water surface, so organisms in the air need clamped lookups or to skip those systems
  while airborne (no feeding, breathing air).
- **The viewer** can already draw organisms over the sky; airborne ones should spread their
  fins and maybe leave a splash when they leave and re-enter the water.

**Watch out for:** flight turning into an escape so good that predators can't hunt near the
surface; organisms in the air piling up at the ridge; and the basin filling from one lucky
crossing before others arrive (that's fine, it's what islands do, but it's worth watching).
Once something gets across, the basin's own conditions (no permanent vents, a fresh start)
decide what thrives there.

---

## 10. Small offspring

**The problem.** The Offspring size gene (R0) is meant to allow anything from splitting in two
to many tiny buds, but tiny offspring never appear. In two saved worlds (16,000 and 29,000
organisms, up to about 190 generations from the ancestor), 99.5–99.8% of newborns were 30–50%
of their parent's mass.

**Why:**

- **A floor on newborn size.** A newborn's mass is the adult mass × the gene's fraction, but
  never below 1 (`genes.min_radius` squared, in `reproduce()`). Most organisms are radius 1.4–3
  (mass 2–9), so their smallest possible newborn is already 11–50% of the parent. Below that
  the gene does nothing, so it drifts freely: in one save 7.8% of organisms carried R0 near `7x`
  (about 2% newborns) with no effect at all.
- **Large organisms rarely move the gene.** Only bodies of radius 8 or more can have small
  newborns, and they're rare (under 1.5% of organisms in both saves). R0 starts at `FF` and
  mutates in about 3% of births by a few steps; after 150–190 generations a lineage has had
  about five mutations of it, moving it by about ±15. Reaching `80` (about 3% newborns) needs
  sustained selection, and whether large organisms gain anything from small offspring hasn't
  been tested.
- **Small adults are sterile.** A parent keeps at least half its adult mass and a newborn needs
  mass 1, so an adult lighter than 2 (radius under 1.41, Size gene below `10`) can never
  reproduce. That's a hidden floor on body size.

**Options:**

- **Lower the newborn floor** (for example `min_radius` 0.5), which also removes the sterility
  trap. Tiny newborns can be swallowed by almost anything, and Brownian jitter and toxin hit them
  harder, so check that they can survive at all; more, smaller bodies also cost performance.
- **Fix only the sterility:** an adult under mass 2 buds a newborn of half its own mass.
- **Give small offspring a reason to pay:** eggs ([§5](#5-life-stages-eggs-and-metamorphosis))
  make the many-small versus few-large trade-off concrete.
- **Test the selection first:** large organisms with R0 swept from `FF` down, compared on
  population growth in a headless run.

---

## 11. Watching over the web, and performance

Today the viewer runs the simulation itself (`evo_app/src/sim.rs` calls `world.step()`). To let
people watch, and click organisms, from a URL, the simulation has to run on a server and the
viewers have to connect to it. `evo_core` already has no graphics code and already serializes
the world (rmp-serde for saves), so the split is realistic.

**The simulation runs on the CPU, not the GPU** (rayon across cores). The GPU only draws. A
server for this needs a fast CPU and good upload bandwidth; it doesn't need a big GPU unless it
also does the drawing.

### Three ways to do it

1. **Stream video, and interact through chat or a Twitch overlay (a few days).** Run `evo_app`
   as now and stream it with OBS. Viewers use chat commands (`!follow 1234`, `!species`) or
   click a Twitch video-overlay extension, which asks a small web service inside the app for
   that organism and shows its own inspector. Everyone shares one camera, and clicks have to be
   matched to where the camera is. Closest to the Twitch plan, and the least code.
2. **The server runs the simulation; each browser draws it (several weeks; the real version).**
   The server runs `evo_core` headless, around the clock, and sends world updates over a
   WebSocket. `evo_app` is built for the browser (WebAssembly + WebGPU, which Bevy supports;
   the WGSL shaders carry over) and becomes a pure viewer. Every viewer gets their own camera,
   zoom and selection. The inspector asks the server for an organism's full details only when
   it is clicked. New work:
   - an update format: only what's in each viewer's view, only what changed, compressed;
   - smoothing movement in the viewer between updates;
   - the background maps (chemicals, temperature) sent less often;
   - the viewer working without its own simulation.

   Bandwidth is the main cost. About 30,000 organisms × ~20 bytes × 15 updates/s is about
   9 MB/s per viewer for the whole world; sending only what's on screen cuts that a lot. The
   server's work doesn't grow with the number of viewers, only its bandwidth does.
3. **The server draws a separate video for each viewer, like cloud gaming.** Every viewer
   costs a server-side render and a GPU video encode. Expensive, and it tops out at a handful of
   viewers. Not recommended.

**Recommended:** start with 1, move to 2 if people should explore on their own. Either way,
give the simulation a clean boundary early: a "what's visible" update type and an "inspect
organism" request. The same boundary serves a Twitch extension's web service and a full browser
viewer, so 1 grows into 2 without throwing work away.

**Hosting.** Any of these can run at home behind something like a Cloudflare Tunnel (no open
ports); upload speed is the limit for option 2 with many viewers. A cheap CPU-heavy cloud
server works just as well, since options 1 and 2 need no server GPU.

### What viewers need for option 2

Not much. The browser only draws: the server does all the simulation, so the viewer's machine
does none of it. The world is tens of thousands of small shapes, a background shader and some
particles, which is light work for any GPU from the last several years, including laptop and
phone GPUs.

- **Browser:** WebGPU is in current Chrome, Edge and Safari, and in Firefox on most platforms.
  Bevy can fall back to WebGL2 for the rest, but WebGL2 can't run every shader feature, so the
  shaders need checking against it.
- **Weak machines:** the background shader is the heaviest part (noise, stars, chemistry
  particles at every pixel). A quality setting (fewer particles, a lower-resolution background)
  covers phones.
- **Download:** a Bevy WebAssembly build is several MB (less once compressed and with unused
  features off), a one-time load.
- **Connection:** the update stream above. Sending only the visible part of the world keeps
  it to a normal video stream's bandwidth or less.

### Performance, and the GPU

Measured with `evo_headless` on 12 cores, on two saved worlds (October 2026): a world of
29,000 organisms takes about 14 ms per tick, and one of 16,000 about 8 ms. Normal speed is 30
ticks per second (`dt` = 1/30), so they run about 2.4× and 4× faster than real time. Where the
time goes (ms per tick; grids and light run side by side):

| Phase | 29,000 | 16,000 | What it is |
|---|---|---|---|
| feed | 3.8 | 1.8 | eating, grabbing, tentacle pulls |
| think | 3.8 | 1.6 | senses and decisions for every organism |
| act | 2.1 | 1.0 | movement, collisions with rock and each other |
| grids | 1.4 | 1.0 | spatial lookup grids |
| life | 1.3 | 0.8 | births, deaths, ageing |
| staging | 0.8 | 0.9 | preparing the tick |
| environment | 0.7 | 0.7 | chemical diffusion, vents, temperature |
| light | 0.4 | 0.3 | shading from bodies and rock |

**A live stream at normal speed has 2–4× headroom today**, so performance only matters if the
world or the population grows a lot, or for faster-than-real-time headless runs when
balancing.

Cheaper wins come before the GPU:
- **Profile first** (for example `cargo flamegraph` on a headless run) to find the hot loops
  inside feed and think.
- **Memory layout:** organisms are big structs (genome, decoded traits, a `Vec` of grasps).
  Keeping the hot fields (position, velocity, energy) in tight arrays, and decoded traits
  cached, helps the CPU cache. This is likely the biggest CPU win.
- **Less work per tick:** senses could run every 3 ticks instead of 2, and diffusion less
  often, staggered across organisms.
- **Serial phases:** contact feeding, carcasses and births are still serial; checking that each
  phase really uses all cores is cheap.

**What suits the GPU:** the grid work — chemical and temperature diffusion, vent plumes, light
from above — is the same small sum at every cell. That's ideal for compute shaders and would
scale to much finer or larger maps. Senses could follow: each organism looks up its neighbours
in a grid, which is a known GPU pattern.

**What doesn't suit the GPU:** births, deaths, mutation, eating conflicts (two hunters, one
prey), species tracking and tentacle grasps are branchy, vary in size and need decisions made in
a fixed order. The GPU is bad at all of that, so it stays on the CPU.

**What it costs:**
- **Data moves between CPU and GPU every tick.** At 30 ticks a second that's affordable, but at
  hundreds of ticks a second (fast headless balancing) it can eat the gain unless the data stays
  on the GPU.
- **Results are harder to reproduce exactly.** GPU floating-point results can differ across
  vendors and drivers, so the same seed might not reproduce across machines. Saves and seeds
  are central to balancing here, so this matters.
- **Two copies of the logic** (Rust and WGSL), which have to be kept in step.
- **The server needs a GPU again**, which takes away some of the hosting simplicity above.

**Recommendation:** not needed for streaming at today's size. If more speed is wanted (bigger
world, faster balancing runs), profile and fix memory layout first; move the grids to the GPU
only if the maps need to get much finer.

**Already done** (about 1.4× faster overall): organisms re-sorted into spatial order every 100
ticks; the grid clears only occupied rows and the smallest bodies get their own fine level;
`best_move` uses precomputed direction tables; currents refreshed every 4 ticks per organism;
steering every 2 ticks; light every other tick, built alongside the grids; feeding's water
trade, field updates and bites in parallel; reproduction and death candidates found in parallel.
The profiling tools are `examples/bench.rs` (ms per phase plus a state fingerprint) and
`examples/profile.rs`.

### Longer ticks (`dt`)

`dt` is the length of one tick in sim-seconds (`[time] dt`, now 1/30). Nearly everything is
already scaled by `dt`, and the cost of a tick barely depends on its length. So a longer tick is
the biggest speed lever left: 1/20 is about 1.5× faster per sim-second, and 1/15 about 2×.

A few things are applied per tick rather than per second. They'd need rescaling so behaviour per
second stays the same:
- **Collision push** (`physics.push_stiffness`): overlapping bodies separate by a fixed
  fraction each tick.
- **Brownian jitter** (`physics.brownian`): its random walk would add up to more drift per
  second. It should scale with √dt.
- **Tick-counted intervals** in `world.rs`: `STEER_INTERVAL` (reaction time), `CURRENT_INTERVAL`,
  `LIGHT_INTERVAL`, `FIELD_INTERVAL`, `FX_THROTTLE`. Either express them in seconds or accept
  slightly slower reactions.
- **Per-tick damage terms:** `starvation_damage` is applied to the energy deficit each tick, so
  check it still scales.
- **Precision:** a fast microbe moves about 1.5 units per tick at 1/20 instead of 1 at 1/30.
  Most bodies are 3–4 units across, so collisions and chases get slightly coarser but should
  hold.

The viewer would step 20 (or 15) times per second at 1×. Drawing already interpolates between
ticks. Verify the way the CPU work was verified: 6-year runs on several seeds, comparing species
over time, diet mix, hunters, depth spread and body parts. Do it after the current tuning
settles, so a change in behaviour isn't mistaken for a tuning result.

## Cross-cutting notes

- **The genome grows.** Today it is 60 bytes. Modulators (about 18 bytes), sex (about 4),
  life cycle (about 4), symbiosis (about 4), part forms (8) and body shape (about 4) would
  take it to about 100. It needs a genome
  version bump and a new save format, and the genome display in the UI gets longer.
- **Each feature starts at "off".** New genes should default to values that reproduce
  today's behaviour exactly, so existing balance survives and the feature has to be
  *evolved into*.
- **Tests and runs.** Each feature gets unit tests for its mechanism, plus multi-hour
  headless runs compared against a baseline seed (ecology by depth, diet mix, parts carried).

## Suggested order

The set-aside changes come first, one at a time. The island and the vent and chemistry changes
are the least likely to hurt diversity, so try them before the jaw, swallowing and hunting
changes.

The species rework (§6) is small and independent of everything else, so it can be done at any
time. It's also worth doing early, because it makes the results of balancing easier to read.
Body shapes (§8) are visual only and independent too. Small offspring (§10) is small and
independent, but it changes the balance, so do it between tuning rounds, not during one. Gradual exaptation (§7) changes how
body plans evolve, so do it after the current balance is settled, and with §6, since the two
decide together when a changing part makes a new species.

1. **State-dependent behaviour (§4).** Smallest and most self-contained, and it makes the
   others richer (breeding aggregations, daily migration, stage-specific behaviour).
2. **Depth colonization (§1).** Sediment and seeps first, and daily migration once §4
   exists. Gives the other features more room to play out.
3. **Life stages and eggs (§5).** Needs the maturity input from §4; eggs help with §1.
4. **Sexual reproduction (§2).** Uses §4 for gathering and §5 for zygote cysts.
5. **Endosymbiosis and parasitism (§3).** The most new machinery. Parasites are also the
   strongest reason for sex to pay off, so it works best after §2.

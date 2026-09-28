# Future ideas

Big changes we want eventually, parked until the current systems are balanced. Nothing here
is implemented. Each section says what we're after, how it could work with what already
exists, and what to watch out for. The goal for all of them is the one in
[VISION.md](VISION.md): new ways of living should **evolve** from genes and selection, never
be switched on by hand.

Suggested order is at the end.

---

## 1. Colonizing the depths

**The problem.** Below the twilight zone almost nothing lives. The deep water has plenty of
nutrients, but no energy: light is gone, vents are rare points, and marine snow mostly rots
before it sinks that far. Deep life can't pay its way, so there's a moat between the surface
and the deep that lineages rarely cross, in either direction.

**What real oceans do.** Deep life runs on four things: food falling from above, chemistry
seeping out of the sea floor, a thin layer of low-oxygen water at mid depth, and animals
that commute up and down every day.

**Ideas, roughly cheapest first:**

- **A sediment layer on the floor.** Marine snow that reaches rock or the sea floor settles
  into a slow-rotting sediment field instead of vanishing. That makes a widespread, reliable
  food bank for bottom scavengers ("deposit feeders"), with legs and holdfasts finally
  having a big surface to work on.
- **Cold seeps.** Rotting sediment, where oxygen runs out, releases minerals (sulfide) back
  into the water: weak, spread-out chemosynthesis along the whole floor, not just at vents.
  Chemotrophs no longer need to find one of seven points.
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

Tentacle latchers chewing on a big organism are already a kind of external parasite; this is
the internal version.

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
trait of the host, and a milestone event celebrates it.

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
full, young versus ready to breed, hurt, night versus day. The 90% fullness rule for hunters
is a hard-coded example; more of these should *evolve*.

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
(plus a second flag) to mark a part as juvenile-only, adult-only or both. A larva with
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

## Cross-cutting notes

- **The genome grows.** Today it is 60 bytes. Modulators (about 18 bytes), sex (about 4),
  life cycle (about 4) and symbiosis (about 4) would take it to about 90. It needs a genome
  version bump and a new save format, and the genome display in the UI gets longer.
- **Each feature starts at "off".** New genes should default to values that reproduce
  today's behaviour exactly, so existing balance survives and the feature has to be
  *evolved into*.
- **Tests and runs.** Each feature gets unit tests for its mechanism, plus multi-hour
  headless runs compared against a baseline seed (ecology by depth, diet mix, parts carried).

## Suggested order

The species rework (§6) is small and independent of everything else, so it can be done at any
time. It's also worth doing early, because it makes the results of balancing easier to read.

1. **State-dependent behaviour (§4).** Smallest and most self-contained, and it makes the
   others richer (breeding aggregations, daily migration, stage-specific behaviour).
2. **Depth colonization (§1).** Sediment and seeps first, and daily migration once §4
   exists. Gives the other features more room to play out.
3. **Life stages and eggs (§5).** Needs the maturity input from §4; eggs help with §1.
4. **Sexual reproduction (§2).** Uses §4 for gathering and §5 for zygote cysts.
5. **Endosymbiosis and parasitism (§3).** The most new machinery. Parasites are also the
   strongest reason for sex to pay off, so it works best after §2.

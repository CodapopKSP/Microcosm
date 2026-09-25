# Vision

An evolution simulation meant to be **watched**: a side-on ocean you can leave running on a stream for weeks, where viewers come back to find new species, new niches, extinctions, and the occasional arms race. It is a game-like model, not a biology simulator. It should be complex enough that natural selection, mutation, speciation and exaptation visibly happen, and simple enough that a viewer can understand an organism by reading its genome.

## Goals

1. **Emergence from simple beginnings.** The world starts with a single sessile green microbe. Motility, senses, predation, scavenging, chemosynthesis, large bodies — all of it has to evolve.
2. **Visible speciation.** Lineages stay coherent long enough to be recognized. Species get names, founding dates, family trees, and extinctions.
3. **Readable genomes.** Ten lettered chromosomes, one byte per gene, each with a plain-English meaning.
4. **Pacing for a stream.** Change is slow enough to follow and fast enough that something new happens every session. Microbes turn over in minutes; macro-organisms take much longer.
5. **Scale and performance.** Tens of thousands of microbes, or fewer large organisms, at 60 fps, with fast-forward. Organism sizes span more than two orders of magnitude.
6. **Runs for weeks.** Deterministic from a seed, saveable and resumable, and it never goes extinct for good.

## Non-goals

- Real biochemistry, real genetics, or 3D.
- Player-directed evolution. Viewers may eventually trigger environmental events, but never edit genomes.
- Scientific accuracy where it would cost readability or performance.

## Principles

- **Energy is conserved.** Sunlight and vent minerals are the only inputs. Carrying capacity emerges from competition for them, not from caps.
- **Every capability costs upkeep.** No gene is free, and most genes matter in more than one context.
- **Config over constants.** Every tunable number lives in `configs/*.toml`.
- **The simulation core knows nothing about rendering.** The core is a plain Rust library; the viewer only reads it.

See [DESIGN.md](DESIGN.md) for the rules and [ARCHITECTURE.md](ARCHITECTURE.md) for the code.

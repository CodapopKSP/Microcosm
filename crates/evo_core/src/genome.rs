//! The genome: 68 bytes in nine lettered chromosomes, one byte per gene.
//!
//! See docs/DESIGN.md §2 for what each gene does, and `parts.rs` for the P chromosome.

use serde::{Deserialize, Serialize};

use crate::config::{MutationConfig, SpeciesConfig};
use crate::parts::{self, PartKind};
use crate::rng::Rng;

pub const GENOME_LEN: usize = 68;
/// Genomes were 60 bytes before part slots gained their form byte; those still load.
pub const OLD_GENOME_LEN: usize = 60;

/// A set of genes (bit `i` = gene `i`), such as the ones a mutation changed.
pub type GeneMask = u128;

/// Byte offsets of every gene.
pub mod gene {
    pub const SIZE: usize = 0;
    pub const BUOYANCY: usize = 1;
    pub const RESERVES: usize = 2;
    pub const LONGEVITY: usize = 3;
    pub const ELONGATION: usize = 4;
    pub const SEGMENTS: usize = 5;
    pub const HUE: usize = 6;
    pub const SATURATION: usize = 7;
    pub const PATTERN: usize = 8;
    pub const PHOTO: usize = 9;
    pub const CHEMO: usize = 10;
    pub const SCAVENGE: usize = 11;
    pub const HUNT: usize = 12;
    pub const RESPIRATION: usize = 13;
    pub const OXYGENIC: usize = 14;
    pub const UPTAKE: usize = 15;
    pub const FIXATION: usize = 16;
    pub const TOXIN: usize = 17;
    pub const RESISTANCE: usize = 18;
    pub const REGEN: usize = 19;
    pub const TOUGHNESS: usize = 20;
    /// First of the nine drive genes (N0..N8).
    pub const DRIVES: usize = 21;
    pub const RESTLESSNESS: usize = 30;
    /// First byte of the eight four-byte part slots (P chromosome).
    pub const PARTS: usize = 31;
    pub const OFFSPRING_SIZE: usize = 63;
    pub const BROOD: usize = 64;
    pub const PATIENCE: usize = 65;
    pub const TEMP_PREF: usize = 66;
    pub const TEMP_TOL: usize = 67;
}

/// Indices into the drive array (genes N0..N8).
pub mod drive {
    pub const LIGHT: usize = 0;
    pub const WARMTH: usize = 1;
    pub const MINERALS: usize = 2;
    pub const CARRION: usize = 3;
    pub const SMALLER: usize = 4;
    pub const LARGER: usize = 5;
    pub const KIN: usize = 6;
    /// Toward (+) or away from (−) the bodies packed right around it, felt by touch.
    pub const CROWDING: usize = 7;
    pub const OXYGEN: usize = 8;
    pub const COUNT: usize = 9;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneKind {
    /// 0..255 read as a fraction.
    Plain,
    /// Stores `value - 128` (so `80` means zero).
    Signed,
    /// Part slot: type + symmetry.
    PartKind,
    /// Part slot: size.
    PartSize,
    /// Part slot: attachment + angle.
    PartPlace,
    /// Part slot: path to a related kind + how far along it.
    PartForm,
}

#[derive(Debug, Clone, Copy)]
pub struct GeneInfo {
    /// Short name, e.g. "Size".
    pub name: &'static str,
    pub kind: GeneKind,
}

impl GeneInfo {
    pub fn signed(&self) -> bool {
        self.kind == GeneKind::Signed
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Chromosome {
    pub letter: char,
    pub name: &'static str,
    /// Offset of the first gene.
    pub start: usize,
    pub genes: &'static [GeneInfo],
}

impl Chromosome {
    pub fn range(&self) -> std::ops::Range<usize> {
        self.start..self.start + self.genes.len()
    }
}

const fn g(name: &'static str) -> GeneInfo {
    GeneInfo {
        name,
        kind: GeneKind::Plain,
    }
}

const fn s(name: &'static str) -> GeneInfo {
    GeneInfo {
        name,
        kind: GeneKind::Signed,
    }
}

const fn pk(name: &'static str) -> GeneInfo {
    GeneInfo {
        name,
        kind: GeneKind::PartKind,
    }
}

const fn ps(name: &'static str) -> GeneInfo {
    GeneInfo {
        name,
        kind: GeneKind::PartSize,
    }
}

const fn pp(name: &'static str) -> GeneInfo {
    GeneInfo {
        name,
        kind: GeneKind::PartPlace,
    }
}

const fn pf(name: &'static str) -> GeneInfo {
    GeneInfo {
        name,
        kind: GeneKind::PartForm,
    }
}

pub static CHROMOSOMES: [Chromosome; 9] = [
    Chromosome {
        letter: 'B',
        name: "Body",
        start: 0,
        genes: &[
            g("Size"),
            g("Buoyancy"),
            g("Reserves"),
            g("Longevity"),
            g("Elongation"),
            g("Segments"),
        ],
    },
    Chromosome {
        letter: 'C',
        name: "Color",
        start: 6,
        genes: &[g("Hue"), g("Saturation"), g("Pattern")],
    },
    Chromosome {
        letter: 'D',
        name: "Diet",
        start: 9,
        genes: &[
            g("Photosynthesis"),
            g("Chemosynthesis"),
            g("Scavenging"),
            g("Hunting"),
        ],
    },
    Chromosome {
        letter: 'G',
        name: "Chemistry",
        start: 13,
        genes: &[
            g("Respiration"),
            g("Oxygenic"),
            g("Nutrient uptake"),
            g("N-fixation"),
            g("Toxin"),
            g("Resistance"),
        ],
    },
    Chromosome {
        letter: 'H',
        name: "Health",
        start: 19,
        genes: &[g("Regeneration"), g("Toughness")],
    },
    Chromosome {
        letter: 'N',
        name: "Nature",
        start: 21,
        genes: &[
            s("Light"),
            s("Warmth"),
            s("Minerals"),
            s("Carrion"),
            s("Smaller"),
            s("Larger"),
            s("Kin"),
            s("Crowding"),
            s("Oxygen"),
            g("Restlessness"),
        ],
    },
    Chromosome {
        letter: 'P',
        name: "Parts",
        start: 31,
        genes: &[
            pk("Part 1 type"),
            ps("Part 1 size"),
            pp("Part 1 place"),
            pf("Part 1 form"),
            pk("Part 2 type"),
            ps("Part 2 size"),
            pp("Part 2 place"),
            pf("Part 2 form"),
            pk("Part 3 type"),
            ps("Part 3 size"),
            pp("Part 3 place"),
            pf("Part 3 form"),
            pk("Part 4 type"),
            ps("Part 4 size"),
            pp("Part 4 place"),
            pf("Part 4 form"),
            pk("Part 5 type"),
            ps("Part 5 size"),
            pp("Part 5 place"),
            pf("Part 5 form"),
            pk("Part 6 type"),
            ps("Part 6 size"),
            pp("Part 6 place"),
            pf("Part 6 form"),
            pk("Part 7 type"),
            ps("Part 7 size"),
            pp("Part 7 place"),
            pf("Part 7 form"),
            pk("Part 8 type"),
            ps("Part 8 size"),
            pp("Part 8 place"),
            pf("Part 8 form"),
        ],
    },
    Chromosome {
        letter: 'R',
        name: "Reproduction",
        start: 63,
        genes: &[g("Offspring size"), g("Brood size"), g("Patience")],
    },
    Chromosome {
        letter: 'T',
        name: "Temperature",
        start: 66,
        genes: &[g("Preferred"), g("Tolerance")],
    },
];

/// Chromosome containing gene `index`.
pub fn chromosome_of(index: usize) -> &'static Chromosome {
    CHROMOSOMES
        .iter()
        .find(|c| c.range().contains(&index))
        .expect("gene index out of range")
}

/// Code like `B0` for gene `index`.
pub fn gene_code(index: usize) -> String {
    let c = chromosome_of(index);
    format!("{}{}", c.letter, index - c.start)
}

pub fn gene_info(index: usize) -> GeneInfo {
    let c = chromosome_of(index);
    c.genes[index - c.start]
}

/// Where gene `index` of an old 60-byte genome is now: part slots grew from three bytes to
/// four, which moved every slot after the first and the R and T chromosomes.
pub fn from_old_index(index: usize) -> usize {
    const OLD_SLOT: usize = 3;
    const OLD_END: usize = gene::PARTS + OLD_SLOT * parts::SLOTS;
    if index < gene::PARTS {
        index
    } else if index < OLD_END {
        let k = index - gene::PARTS;
        parts::slot_offset(k / OLD_SLOT) + k % OLD_SLOT
    } else {
        index + (parts::SLOT_BYTES - OLD_SLOT) * parts::SLOTS
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Genome(pub [u8; GENOME_LEN]);

impl Default for Genome {
    fn default() -> Self {
        Genome([0; GENOME_LEN])
    }
}

/// Human-readable formats (JSON, TOML) get chromosome notation; binary formats get raw bytes.
impl Serialize for Genome {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() {
            s.serialize_str(&self.to_string())
        } else {
            s.serialize_bytes(&self.0)
        }
    }
}

impl<'de> Deserialize<'de> for Genome {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Genome;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "a genome ({GENOME_LEN} bytes or chromosome notation)")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Genome, E> {
                Genome::parse(v).map_err(E::custom)
            }
            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Genome, E> {
                Genome::from_bytes(v).ok_or_else(|| E::invalid_length(v.len(), &self))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Genome, A::Error> {
                let mut bytes = Vec::with_capacity(GENOME_LEN);
                while let Some(b) = seq.next_element()? {
                    bytes.push(b);
                }
                Genome::from_bytes(&bytes)
                    .ok_or_else(|| serde::de::Error::invalid_length(bytes.len(), &self))
            }
        }
        if d.is_human_readable() {
            d.deserialize_str(V)
        } else {
            d.deserialize_bytes(V)
        }
    }
}

impl std::fmt::Debug for Genome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Genome({self})")
    }
}

impl std::fmt::Display for Genome {
    /// `B:20A050200000 C:55C000 …`
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, c) in CHROMOSOMES.iter().enumerate() {
            if i > 0 {
                write!(f, " ")?;
            }
            write!(f, "{}:", c.letter)?;
            for (k, b) in self.0[c.range()].iter().enumerate() {
                // Part slots are grouped in fours for readability.
                if c.letter == 'P' && k > 0 && k % parts::SLOT_BYTES == 0 {
                    write!(f, " ")?;
                }
                write!(f, "{b:02X}")?;
            }
        }
        Ok(())
    }
}

impl Genome {
    /// A genome from its raw bytes: the current length, or an old 60-byte genome, whose part
    /// slots get a form byte of zero (each part wholly its own kind).
    pub fn from_bytes(bytes: &[u8]) -> Option<Genome> {
        if let Ok(bytes) = <[u8; GENOME_LEN]>::try_from(bytes) {
            return Some(Genome(bytes));
        }
        if bytes.len() != OLD_GENOME_LEN {
            return None;
        }
        let mut g = Genome::default();
        for (i, &b) in bytes.iter().enumerate() {
            g.0[from_old_index(i)] = b;
        }
        Some(g)
    }

    /// Parse chromosome notation: `B:20A050200000 C:55C000 …`. Every chromosome must appear once;
    /// spaces anywhere are ignored. An old P chromosome (three bytes per part slot) is read
    /// with each part wholly its own kind.
    pub fn parse(text: &str) -> Result<Genome, String> {
        let mut bytes = [0u8; GENOME_LEN];
        let mut seen = [false; CHROMOSOMES.len()];
        let mut current: Option<(usize, Vec<u8>)> = None;
        let mut pending_nibble: Option<u8> = None;

        let finish = |current: &mut Option<(usize, Vec<u8>)>,
                      bytes: &mut [u8; GENOME_LEN],
                      pending: &mut Option<u8>|
         -> Result<(), String> {
            if let Some((ci, mut data)) = current.take() {
                let c = &CHROMOSOMES[ci];
                if c.letter == 'P' && data.len() == 3 * parts::SLOTS {
                    data = data
                        .chunks(3)
                        .flat_map(|slot| [slot[0], slot[1], slot[2], 0])
                        .collect();
                }
                if pending.take().is_some() {
                    return Err(format!(
                        "chromosome {} has an odd number of hex digits",
                        c.letter
                    ));
                }
                if data.len() != c.genes.len() {
                    return Err(format!(
                        "chromosome {} needs {} bytes, got {}",
                        c.letter,
                        c.genes.len(),
                        data.len()
                    ));
                }
                bytes[c.range()].copy_from_slice(&data);
            }
            Ok(())
        };

        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let ch = chars[i];
            if ch.is_whitespace() {
                i += 1;
                continue;
            }
            if i + 1 < chars.len() && chars[i + 1] == ':' {
                finish(&mut current, &mut bytes, &mut pending_nibble)?;
                let letter = ch.to_ascii_uppercase();
                let ci = CHROMOSOMES
                    .iter()
                    .position(|c| c.letter == letter)
                    .ok_or_else(|| format!("unknown chromosome '{ch}'"))?;
                if seen[ci] {
                    return Err(format!("chromosome {letter} appears twice"));
                }
                seen[ci] = true;
                current = Some((ci, Vec::new()));
                i += 2;
                continue;
            }
            let digit =
                ch.to_digit(16)
                    .ok_or_else(|| format!("unexpected character '{ch}'"))? as u8;
            let Some((_, data)) = current.as_mut() else {
                return Err("hex digits before the first chromosome letter".into());
            };
            match pending_nibble.take() {
                Some(hi) => data.push(hi << 4 | digit),
                None => pending_nibble = Some(digit),
            }
            i += 1;
        }
        finish(&mut current, &mut bytes, &mut pending_nibble)?;
        if let Some(missing) = seen.iter().position(|s| !s) {
            return Err(format!(
                "chromosome {} is missing",
                CHROMOSOMES[missing].letter
            ));
        }
        Ok(Genome(bytes))
    }

    #[inline]
    pub fn get(&self, index: usize) -> u8 {
        self.0[index]
    }

    /// Gene as a fraction in `[0, 1]`.
    #[inline]
    pub fn frac(&self, index: usize) -> f32 {
        self.0[index] as f32 / 255.0
    }

    /// Signed gene in `[-1, 1]` (`80` = 0).
    #[inline]
    pub fn signed(&self, index: usize) -> f32 {
        ((self.0[index] as f32 - 128.0) / 127.0).clamp(-1.0, 1.0)
    }

    /// Mutate in place; returns the genes that changed.
    ///
    /// Every gene can take a small step (or rarely a re-roll). Part-type genes instead change
    /// symmetry, and part-form genes walk the part toward a related kind. Then, rarely, whole
    /// parts are duplicated into an empty slot, deleted, or grown new from an empty slot's
    /// leftover (cryptic) bytes.
    pub fn mutate(&mut self, rng: &mut Rng, cfg: &MutationConfig) -> GeneMask {
        let before = self.0;
        for i in 0..GENOME_LEN {
            let kind = gene_info(i).kind;
            let rate = if (gene::HUE..=gene::PATTERN).contains(&i) {
                cfg.rate * cfg.color_rate_multiplier
            } else if kind == GeneKind::PartKind {
                cfg.rate * cfg.part_type_rate_multiplier
            } else if kind == GeneKind::PartForm {
                cfg.rate * cfg.form_rate_multiplier
            } else {
                cfg.rate
            };
            if !rng.chance(rate) {
                continue;
            }
            if kind == GeneKind::PartKind {
                self.mutate_part_kind(i, rng);
                continue;
            }
            if kind == GeneKind::PartForm {
                self.mutate_part_form(i, rng, cfg);
                continue;
            }
            let old = self.0[i];
            self.0[i] = if rng.chance(cfg.reroll) {
                rng.below(256) as u8
            } else {
                // ±1 (50%), ±2 (25%), ±4 (12.5%), ±8 (12.5%)
                let roll = rng.f32();
                let step = if roll < 0.5 {
                    1
                } else if roll < 0.75 {
                    2
                } else if roll < 0.875 {
                    4
                } else {
                    8
                };
                let step = ((step as f32 * cfg.step_scale).round() as i32).max(1);
                let delta = if rng.chance(0.5) { step } else { -step };
                (old as i32 + delta).clamp(0, 255) as u8
            };
        }
        self.mutate_diet_shift(rng, cfg);
        self.mutate_structure(rng, cfg);
        let mut mask: GeneMask = 0;
        for (i, (now, was)) in self.0.iter().zip(before).enumerate() {
            if *now != was {
                mask |= 1 << i;
            }
        }
        mask
    }

    /// Change a part's symmetry. Empty slots stay empty. (A part's kind changes only by walking
    /// its form to a related kind.)
    fn mutate_part_kind(&mut self, i: usize, rng: &mut Rng) {
        let byte = self.0[i];
        let kind = byte & 0x0F;
        if kind == 0 {
            return;
        }
        let sym = (byte >> 4).min(5);
        let new_sym = (sym as i32 + if rng.chance(0.5) { 1 } else { -1 }).clamp(0, 5) as u8;
        self.0[i] = new_sym << 4 | kind;
    }

    /// Walk a part along its path to a related kind (the form byte at `i`), a step either way.
    /// Only a part that hasn't started changing picks a new path, so it never jumps from one
    /// half-made form to another. Past the end of its path it becomes the other kind, wholly,
    /// and its path then leads back.
    fn mutate_part_form(&mut self, i: usize, rng: &mut Rng, cfg: &MutationConfig) {
        let at = i - (parts::SLOT_BYTES - 1);
        let Some(kind) = PartKind::from_code(self.0[at]) else {
            return;
        };
        let path = self.0[i] >> 6;
        let amount = (self.0[i] % parts::FORM_STEPS) as i32;
        if amount == 0 && rng.chance(0.5) {
            self.0[i] = (rng.below(4) as u8) << 6;
            return;
        }
        let Some(other) = kind.path(path) else {
            return;
        };
        let roll = rng.f32();
        let scale = if roll < 0.4 {
            0.5
        } else if roll < 0.8 {
            1.0
        } else {
            2.0
        };
        let step = ((cfg.form_step * scale).round() as i32).max(1);
        let amount = amount + if rng.chance(0.5) { step } else { -step };
        if amount >= parts::FORM_STEPS as i32 {
            self.0[at] = self.0[at] & 0xF0 | other.code();
            self.0[i] = other.path_to(kind).unwrap_or(0) << 6;
        } else {
            self.0[i] = path << 6 | amount.max(0) as u8;
        }
    }

    /// Rarely, a big jump in diet: 30–60% of one diet gene's investment moves to another in a
    /// single step, so a lineage can cross the dip between two ways of living (for example from
    /// photosynthesis plus scavenging to photosynthesis plus hunting) instead of having to walk
    /// through the worse mixes in between.
    fn mutate_diet_shift(&mut self, rng: &mut Rng, cfg: &MutationConfig) {
        if !rng.chance(cfg.diet_shift) {
            return;
        }
        const DIETS: [usize; 4] = [gene::PHOTO, gene::CHEMO, gene::SCAVENGE, gene::HUNT];
        let invested: Vec<usize> = DIETS.into_iter().filter(|&g| self.0[g] >= 0x20).collect();
        if invested.is_empty() {
            return;
        }
        let from = invested[rng.below(invested.len() as u32) as usize];
        let others: Vec<usize> = DIETS.into_iter().filter(|&g| g != from).collect();
        let to = others[rng.below(others.len() as u32) as usize];
        let amount = (self.0[from] as f32 * rng.range(0.3, 0.6)) as u8;
        self.0[from] -= amount;
        self.0[to] = self.0[to].saturating_add(amount);
    }

    /// Whole-part duplication, deletion, and new parts.
    fn mutate_structure(&mut self, rng: &mut Rng, cfg: &MutationConfig) {
        let occupied: Vec<usize> = (0..parts::SLOTS)
            .filter(|&k| self.0[parts::slot_offset(k)] & 0x0F != 0)
            .collect();
        let empty: Vec<usize> = (0..parts::SLOTS)
            .filter(|&k| self.0[parts::slot_offset(k)] & 0x0F == 0)
            .collect();
        if !occupied.is_empty() && !empty.is_empty() && rng.chance(cfg.part_duplicate) {
            // Copy a part into an empty slot; the copy is then free to diverge.
            let from = parts::slot_offset(occupied[rng.below(occupied.len() as u32) as usize]);
            let to = parts::slot_offset(empty[rng.below(empty.len() as u32) as usize]);
            for b in 0..parts::SLOT_BYTES {
                self.0[to + b] = self.0[from + b];
            }
            // Often the copy lands somewhere else on the body.
            if rng.chance(0.5) {
                self.0[to + 2] = rng.below(256) as u8;
            }
        } else if !occupied.is_empty() && rng.chance(cfg.part_delete) {
            // Delete: only the type is cleared; size and place remain as cryptic bytes.
            let at = parts::slot_offset(occupied[rng.below(occupied.len() as u32) as usize]);
            self.0[at] = 0;
        } else if !empty.is_empty() && rng.chance(cfg.part_new) {
            // A new part grows from an empty slot, reusing whatever its leftover bytes say.
            let at = parts::slot_offset(empty[rng.below(empty.len() as u32) as usize]);
            let kind = 1 + rng.below(PartKind::ALL.len() as u32) as u8;
            let sym = if rng.chance(0.5) { 1 } else { 0 };
            self.0[at] = sym << 4 | kind;
            self.0[at + 1] = self.0[at + 1].clamp(0x20, 0x60);
            // It starts as wholly its own kind (the leftover path stays, unused until it grows).
            self.0[at + 3] &= 0xC0;
            // A slot that never held a part has a zero placement (the head, pointing straight
            // forward), so it starts somewhere random; one that did may reuse where that part was.
            if self.0[at + 2] == 0 || rng.chance(0.5) {
                self.0[at + 2] = rng.below(256) as u8;
            }
        }
    }

    /// Weighted mean per-gene difference, in `[0, 1]`. Each gene contributes at most `gene_cap`,
    /// so one large jump in a single gene can't make a new species on its own. Color genes weigh
    /// `color_weight`. A part slot's type and form count together, up to 0.5 (a new kind of part
    /// is a big step, reached a little at a time as the form changes); its size and placement
    /// count only when both genomes actually have a part there.
    pub fn distance(&self, other: &Genome, sc: &SpeciesConfig) -> f32 {
        let mut sum = 0.0;
        let mut weight_total = 0.0;
        let byte_diff = |a: u8, b: u8| (a as i32 - b as i32).unsigned_abs() as f32 / 255.0;
        for i in 0..GENOME_LEN {
            let (w, d) = match gene_info(i).kind {
                GeneKind::PartKind => {
                    let (a, b) = (self.0[i], other.0[i]);
                    let form = parts::SLOT_BYTES - 1;
                    let kinds = slot_kinds(a, self.0[i + form])
                        .difference(&slot_kinds(b, other.0[i + form]));
                    let symmetry = if a & 0x0F == 0 || b & 0x0F == 0 {
                        0.0
                    } else {
                        ((a >> 4).min(5) as i32 - (b >> 4).min(5) as i32).unsigned_abs() as f32
                            / 5.0
                    };
                    // A new kind of part is a big change, so it isn't held to `gene_cap`.
                    sum += (0.5 * kinds + symmetry).min(0.5);
                    weight_total += 1.0;
                    continue;
                }
                // Counted with the type.
                GeneKind::PartForm => continue,
                GeneKind::PartSize | GeneKind::PartPlace => {
                    let slot = parts::slot_offset((i - gene::PARTS) / parts::SLOT_BYTES);
                    let both = self.0[slot] & 0x0F != 0 && other.0[slot] & 0x0F != 0;
                    (
                        0.3,
                        if both {
                            byte_diff(self.0[i], other.0[i])
                        } else {
                            0.0
                        },
                    )
                }
                _ if (gene::HUE..=gene::PATTERN).contains(&i) => {
                    let d = if i == gene::HUE {
                        // Hue is circular.
                        let d = (self.0[i] as i32 - other.0[i] as i32).unsigned_abs();
                        d.min(256 - d) as f32 / 128.0
                    } else {
                        byte_diff(self.0[i], other.0[i])
                    };
                    sum += (sc.color_weight * d).min(sc.color_cap);
                    weight_total += sc.color_weight;
                    continue;
                }
                _ if i == gene::SIZE => {
                    sum += byte_diff(self.0[i], other.0[i]).min(sc.size_cap);
                    weight_total += 1.0;
                    continue;
                }
                _ => (1.0, byte_diff(self.0[i], other.0[i])),
            };
            sum += w * d.min(sc.gene_cap);
            weight_total += w;
        }
        sum / weight_total
    }
}

/// Saving a [`GeneMask`]: as 16 bytes. Saves from before the genome grew hold a 64-bit number
/// numbering the old 60-byte genome; its genes are moved to where they are now.
pub mod gene_mask_serde {
    use super::{GeneMask, OLD_GENOME_LEN, from_old_index};

    pub fn serialize<S: serde::Serializer>(mask: &GeneMask, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(&mask.to_le_bytes())
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<GeneMask, D::Error> {
        struct V;
        impl serde::de::Visitor<'_> for V {
            type Value = GeneMask;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "a gene mask")
            }
            fn visit_u64<E: serde::de::Error>(self, old: u64) -> Result<GeneMask, E> {
                Ok((0..OLD_GENOME_LEN)
                    .filter(|&i| old & (1 << i) != 0)
                    .fold(0, |mask, i| mask | 1 << from_old_index(i)))
            }
            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<GeneMask, E> {
                let bytes: [u8; 16] = v.try_into().map_err(|_| E::invalid_length(v.len(), &self))?;
                Ok(GeneMask::from_le_bytes(bytes))
            }
        }
        d.deserialize_any(V)
    }
}

/// What a part slot holds, as shares of part kinds (kind code 0 = empty).
struct SlotKinds([(u8, f32); 2]);

fn slot_kinds(kind_byte: u8, form: u8) -> SlotKinds {
    let code = kind_byte & 0x0F;
    match PartKind::from_code(code).and_then(|k| parts::decode_form(k, form)) {
        Some((other, t)) => SlotKinds([(code, 1.0 - t), (other.code(), t)]),
        None => SlotKinds([(code, 1.0), (code, 0.0)]),
    }
}

impl SlotKinds {
    /// How different two slots are, from 0 (the same) to 1 (nothing in common): the share
    /// of one that would have to change kind to match the other.
    fn difference(&self, other: &SlotKinds) -> f32 {
        let share = |s: &SlotKinds, code: u8| -> f32 {
            s.0.iter().filter(|p| p.0 == code).map(|p| p.1).sum()
        };
        let mut codes: Vec<u8> = self.0.iter().chain(&other.0).map(|p| p.0).collect();
        codes.sort_unstable();
        codes.dedup();
        0.5 * codes
            .into_iter()
            .map(|c| (share(self, c) - share(other, c)).abs())
            .sum::<f32>()
    }
}

/// Distance between two colors given as (hue, saturation) gene bytes, in `[0, ~1.5]`.
#[inline]
pub fn color_distance(hue_a: u8, sat_a: u8, hue_b: u8, sat_b: u8) -> f32 {
    let dh = hue_a.wrapping_sub(hue_b).min(hue_b.wrapping_sub(hue_a)) as f32 / 128.0;
    let ds = (sat_a as f32 - sat_b as f32).abs() / 255.0;
    dh + 0.5 * ds
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANCESTOR: &str = "B:20A050200000 C:55C000 D:FF000000 G:80FF80000000 H:2030 N:80808080808080808000 \
        P:00000000 00000000 00000000 00000000 00000000 00000000 00000000 00000000 R:FF00B0 T:80A0";

    #[test]
    fn chromosomes_cover_every_byte_once() {
        let mut covered = [0; GENOME_LEN];
        for c in &CHROMOSOMES {
            for i in c.range() {
                covered[i] += 1;
            }
        }
        assert!(covered.iter().all(|&n| n == 1));
        assert_eq!(chromosome_of(gene::DRIVES).letter, 'N');
        assert_eq!(chromosome_of(gene::RESTLESSNESS).letter, 'N');
        assert_eq!(chromosome_of(gene::PARTS).letter, 'P');
        assert_eq!(chromosome_of(gene::TEMP_TOL).letter, 'T');
        assert_eq!(gene_code(gene::SIZE), "B0");
        assert_eq!(gene_code(gene::HUNT), "D3");
        assert_eq!(gene_code(gene::TOXIN), "G4");
        assert_eq!(
            gene_info(parts::slot_offset(2) + 1).kind,
            GeneKind::PartSize
        );
    }

    #[test]
    fn parse_and_format_roundtrip() {
        let g = Genome::parse(ANCESTOR).unwrap();
        let text = g.to_string();
        assert_eq!(Genome::parse(&text).unwrap(), g);
        assert!(text.contains("P:00000000 00000000"), "{text}");
        assert_eq!(g.get(gene::SIZE), 0x20);
        assert_eq!(g.get(gene::PHOTO), 0xFF);
        assert_eq!(g.get(gene::OXYGENIC), 0xFF);
        assert_eq!(g.signed(gene::DRIVES), 0.0);
        // Spaces inside chromosomes and lowercase are fine.
        let spaced = text.replace("B:20A050200000", "b: 20 a0 50 20 00 00");
        assert_eq!(Genome::parse(&spaced).unwrap(), g);
    }

    #[test]
    fn parse_rejects_bad_input() {
        assert!(Genome::parse("B:00").is_err());
        assert!(Genome::parse(&ANCESTOR.replace("T:80A0", "")).is_err());
        assert!(Genome::parse(&ANCESTOR.replace("T:80A0", "T:80A")).is_err());
        assert!(Genome::parse(&ANCESTOR.replace("T:80A0", "T:80A0 C:55C000")).is_err());
        assert!(Genome::parse(&ANCESTOR.replace("T:80A0", "X:80A0")).is_err());
    }

    #[test]
    fn mutation_rate_is_low_and_mask_is_accurate() {
        let base = Genome::parse(ANCESTOR).unwrap();
        let cfg = MutationConfig::default();
        let mut rng = Rng::new(7);
        let mut unchanged = 0;
        let trials = 10_000;
        for _ in 0..trials {
            let mut g = base;
            let mask = g.mutate(&mut rng, &cfg);
            for i in 0..GENOME_LEN {
                assert_eq!(mask & (1 << i) != 0, g.0[i] != base.0[i]);
            }
            if mask == 0 {
                unchanged += 1;
            }
        }
        let frac = unchanged as f32 / trials as f32;
        assert!((0.15..0.45).contains(&frac), "exact-copy fraction {frac}");
    }

    #[test]
    fn parts_appear_duplicate_and_shift() {
        let mut cfg = MutationConfig::default();
        cfg.part_new = 1.0;
        let mut rng = Rng::new(3);
        let mut g = Genome::parse(ANCESTOR).unwrap();
        g.mutate(&mut rng, &cfg);
        assert_eq!(parts::decode_parts(&g).len(), 1, "a new part grows");
        cfg.part_new = 0.0;
        cfg.part_duplicate = 1.0;
        g.mutate(&mut rng, &cfg);
        let p = parts::decode_parts(&g);
        assert_eq!(p.len(), 2, "the part is duplicated");
        assert_eq!(p[0].kind, p[1].kind);
        // Type mutations change only the symmetry: never the kind, and never empty a slot.
        let mut shifted = g;
        let at = parts::slot_offset(p[0].slot);
        for _ in 0..200 {
            shifted.mutate_part_kind(at, &mut rng);
            assert_eq!(shifted.0[at] & 0x0F, g.0[at] & 0x0F);
        }
    }

    #[test]
    fn old_genomes_still_load() {
        let old = "B:20A050200000 C:55C000 D:FF000000 G:80FF80000000 H:2030 N:80808080808080808000 \
            P:0180C0 000000 000000 000000 000000 000000 000000 23FF10 R:FF00B0 T:80A0";
        let g = Genome::parse(old).unwrap();
        let p = parts::decode_parts(&g);
        assert_eq!(p.len(), 2);
        assert_eq!((p[0].kind, p[0].size, p[0].toward), (PartKind::Flagellum, 0x80 as f32 / 255.0, None));
        assert_eq!((p[1].slot, p[1].kind), (7, PartKind::Tentacle));
        assert_eq!(g.get(gene::TEMP_TOL), 0xA0);
        // The raw bytes of an old save, as well as its text.
        let mut bytes = [0u8; OLD_GENOME_LEN];
        for i in 0..OLD_GENOME_LEN {
            let at = from_old_index(i);
            assert!(gene_info(at).kind != GeneKind::PartForm);
            bytes[i] = g.0[at];
        }
        assert_eq!(Genome::from_bytes(&bytes), Some(g));
        assert_eq!(Genome::from_bytes(&bytes[..59]), None);
    }

    #[test]
    fn forms_walk_to_related_kinds_a_step_at_a_time() {
        let cfg = MutationConfig::default();
        let mut rng = Rng::new(11);
        let mut g = Genome::parse(ANCESTOR).unwrap();
        let at = parts::slot_offset(0);
        g.0[at] = 0x10 | PartKind::Flagellum.code();
        let form = at + parts::SLOT_BYTES - 1;
        let mut became = std::collections::HashSet::new();
        for _ in 0..20_000 {
            let (kind, before) = (g.0[at], g.0[form]);
            g.mutate_part_form(form, &mut rng, &cfg);
            assert_eq!(g.0[at] >> 4, 1, "the symmetry stays");
            if g.0[at] != kind {
                // A new kind: wholly itself, with the way back as its path.
                let (was, now) = (PartKind::from_code(kind).unwrap(), PartKind::from_code(g.0[at]).unwrap());
                assert_eq!(was.path(before >> 6), Some(now));
                assert_eq!(g.0[form], now.path_to(was).unwrap() << 6);
                became.insert(now);
            } else if before % parts::FORM_STEPS != 0 {
                assert_eq!(g.0[form] >> 6, before >> 6, "a part partway along keeps its path");
            }
        }
        assert!(became.contains(&PartKind::Tentacle) && became.contains(&PartKind::Cilia));
        assert!(!became.contains(&PartKind::Spine), "never to an unrelated kind");
    }

    #[test]
    fn a_changing_part_drifts_away_a_little_at_a_time() {
        let sc = SpeciesConfig::default();
        let mut base = Genome::parse(ANCESTOR).unwrap();
        let at = parts::slot_offset(0);
        base.0[at] = PartKind::Flagellum.code();
        let mut last = 0.0;
        for amount in 1..parts::FORM_STEPS {
            let mut g = base;
            g.0[at + 3] = amount; // path 0: toward a tentacle
            let d = base.distance(&g, &sc);
            assert!(d > last, "{amount}");
            last = d;
        }
        // All the way along is as far as a tentacle (the old one-step jump).
        let mut tentacle = base;
        tentacle.0[at] = PartKind::Tentacle.code();
        let full = base.distance(&tentacle, &sc);
        assert!((full - last).abs() < full / 32.0, "{full} vs {last}");
    }

    #[test]
    fn diet_shifts_move_investment_between_diets() {
        let mut cfg = MutationConfig {
            rate: 0.0,
            part_duplicate: 0.0,
            part_delete: 0.0,
            part_new: 0.0,
            diet_shift: 1.0,
            ..MutationConfig::default()
        };
        let mut rng = Rng::new(5);
        let ancestor = Genome::parse(ANCESTOR).unwrap();
        let diets = [gene::PHOTO, gene::CHEMO, gene::SCAVENGE, gene::HUNT];
        let total = |g: &Genome| diets.iter().map(|&d| g.0[d] as u32).sum::<u32>();
        let mut targets: std::collections::HashSet<usize> = Default::default();
        for _ in 0..200 {
            let mut g = ancestor;
            g.mutate(&mut rng, &cfg);
            assert_eq!(total(&g), total(&ancestor), "investment is moved, not made");
            let moved = ancestor.0[gene::PHOTO] - g.0[gene::PHOTO];
            let share = moved as f32 / ancestor.0[gene::PHOTO] as f32;
            assert!((0.29..0.61).contains(&share), "moved {share}");
            targets.extend(diets.iter().skip(1).copied().filter(|&d| g.0[d] > 0));
        }
        assert_eq!(targets.len(), 3, "it can go to any other diet");
        cfg.diet_shift = 0.0;
        let mut g = ancestor;
        g.mutate(&mut rng, &cfg);
        assert_eq!(g, ancestor);
    }

    #[test]
    fn new_parts_start_anywhere_on_the_body() {
        // The ancestor's empty slots are all zeros (the head, pointing straight forward); new
        // parts must not all start there.
        let mut cfg = MutationConfig::default();
        cfg.part_new = 1.0;
        let mut rng = Rng::new(11);
        let ancestor = Genome::parse(ANCESTOR).unwrap();
        let (mut front, mut angles) = (0, std::collections::HashSet::new());
        for _ in 0..200 {
            let mut g = ancestor;
            g.mutate(&mut rng, &cfg);
            let p = parts::decode_parts(&g);
            let place = g.0[parts::slot_offset(p[0].slot) + 2];
            if place == 0 {
                front += 1;
            }
            angles.insert(place & 31);
        }
        assert!(front < 10, "{front} of 200 new parts at the very front");
        assert!(angles.len() > 20, "angles vary: {}", angles.len());
    }

    #[test]
    fn distance_properties() {
        let caps = |gene_cap: f32| SpeciesConfig {
            color_weight: 0.25,
            gene_cap,
            color_cap: 10.0,
            size_cap: 1.0,
            ..Default::default()
        };
        let a = Genome::parse(ANCESTOR).unwrap();
        assert_eq!(a.distance(&a, &caps(1.0)), 0.0);
        let mut b = a;
        b.0[gene::LONGEVITY] = 0xFF;
        let d = a.distance(&b, &caps(1.0));
        assert!(a.distance(&b, &caps(0.3)) < d);
        assert!(d > 0.0 && d < 1.0);
        assert_eq!(d, b.distance(&a, &caps(1.0)));
        // Hue wraps around.
        let mut c = a;
        c.0[gene::HUE] = a.0[gene::HUE].wrapping_add(250);
        let mut d2 = a;
        d2.0[gene::HUE] = a.0[gene::HUE].wrapping_add(6);
        assert!((a.distance(&c, &caps(1.0)) - a.distance(&d2, &caps(1.0))).abs() < 1e-6);
        // Leftover bytes in empty part slots don't count.
        let mut e = a;
        e.0[parts::slot_offset(0) + 1] = 0xFF;
        assert_eq!(a.distance(&e, &caps(1.0)), 0.0);
    }

    #[test]
    fn color_and_size_have_their_own_caps() {
        let sc = SpeciesConfig::default();
        let a = Genome::parse(ANCESTOR).unwrap();
        let total = a.distance(&a, &sc);
        assert_eq!(total, 0.0);
        let weight_total = {
            // Any one ordinary gene changed fully counts gene_cap / weight_total.
            let mut b = a;
            b.0[gene::LONGEVITY] = a.0[gene::LONGEVITY] ^ 0xFF;
            let lb = (a.0[gene::LONGEVITY] as f32 - b.0[gene::LONGEVITY] as f32).abs() / 255.0;
            lb.min(sc.gene_cap) / a.distance(&b, &sc)
        };
        // The opposite hue counts the colour cap.
        let mut c = a;
        c.0[gene::HUE] = a.0[gene::HUE].wrapping_add(128);
        assert!((a.distance(&c, &sc) * weight_total - sc.color_cap).abs() < 1e-4);
        // The largest size change counts the size cap.
        let mut s = a;
        s.0[gene::SIZE] = 0xFF;
        let ds = (0xFF - a.0[gene::SIZE]) as f32 / 255.0;
        assert!((a.distance(&s, &sc) * weight_total - ds.min(sc.size_cap)).abs() < 1e-4);
    }

    #[test]
    fn color_distance_wraps() {
        assert_eq!(color_distance(0, 100, 0, 100), 0.0);
        assert!((color_distance(250, 100, 4, 100) - 10.0 / 128.0).abs() < 1e-6);
    }
}

//! The genome: 60 bytes in nine lettered chromosomes, one byte per gene.
//!
//! See docs/DESIGN.md §2 for what each gene does, and `parts.rs` for the P chromosome.

use serde::{Deserialize, Serialize};

use crate::config::{MutationConfig, SpeciesConfig};
use crate::parts::{self, PartKind};
use crate::rng::Rng;

pub const GENOME_LEN: usize = 60;

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
    /// First byte of the eight three-byte part slots (P chromosome).
    pub const PARTS: usize = 31;
    pub const OFFSPRING_SIZE: usize = 55;
    pub const BROOD: usize = 56;
    pub const PATIENCE: usize = 57;
    pub const TEMP_PREF: usize = 58;
    pub const TEMP_TOL: usize = 59;
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
            pk("Part 2 type"),
            ps("Part 2 size"),
            pp("Part 2 place"),
            pk("Part 3 type"),
            ps("Part 3 size"),
            pp("Part 3 place"),
            pk("Part 4 type"),
            ps("Part 4 size"),
            pp("Part 4 place"),
            pk("Part 5 type"),
            ps("Part 5 size"),
            pp("Part 5 place"),
            pk("Part 6 type"),
            ps("Part 6 size"),
            pp("Part 6 place"),
            pk("Part 7 type"),
            ps("Part 7 size"),
            pp("Part 7 place"),
            pk("Part 8 type"),
            ps("Part 8 size"),
            pp("Part 8 place"),
        ],
    },
    Chromosome {
        letter: 'R',
        name: "Reproduction",
        start: 55,
        genes: &[g("Offspring size"), g("Brood size"), g("Patience")],
    },
    Chromosome {
        letter: 'T',
        name: "Temperature",
        start: 58,
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
                let bytes: [u8; GENOME_LEN] = v
                    .try_into()
                    .map_err(|_| E::invalid_length(v.len(), &self))?;
                Ok(Genome(bytes))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Genome, A::Error> {
                let mut bytes = [0u8; GENOME_LEN];
                for (i, b) in bytes.iter_mut().enumerate() {
                    *b = seq
                        .next_element()?
                        .ok_or_else(|| serde::de::Error::invalid_length(i, &self))?;
                }
                Ok(Genome(bytes))
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
                // Part slots are grouped in threes for readability.
                if c.letter == 'P' && k > 0 && k % 3 == 0 {
                    write!(f, " ")?;
                }
                write!(f, "{b:02X}")?;
            }
        }
        Ok(())
    }
}

impl Genome {
    /// Parse chromosome notation: `B:20A050200000 C:55C000 …`. Every chromosome must appear once;
    /// spaces anywhere are ignored.
    pub fn parse(text: &str) -> Result<Genome, String> {
        let mut bytes = [0u8; GENOME_LEN];
        let mut seen = [false; CHROMOSOMES.len()];
        let mut current: Option<(usize, Vec<u8>)> = None;
        let mut pending_nibble: Option<u8> = None;

        let finish = |current: &mut Option<(usize, Vec<u8>)>,
                      bytes: &mut [u8; GENOME_LEN],
                      pending: &mut Option<u8>|
         -> Result<(), String> {
            if let Some((ci, data)) = current.take() {
                let c = &CHROMOSOMES[ci];
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

    /// Mutate in place; returns a bitmask of the genes that changed.
    ///
    /// Every gene can take a small step (or rarely a re-roll). Part-type genes instead shift to a
    /// neighboring part type or change symmetry. Then, rarely, whole parts are duplicated into an
    /// empty slot, deleted, or grown new from an empty slot's leftover (cryptic) bytes.
    pub fn mutate(&mut self, rng: &mut Rng, cfg: &MutationConfig) -> u64 {
        let before = self.0;
        for i in 0..GENOME_LEN {
            let rate = if (gene::HUE..=gene::PATTERN).contains(&i) {
                cfg.rate * cfg.color_rate_multiplier
            } else if gene_info(i).kind == GeneKind::PartKind {
                cfg.rate * cfg.part_type_rate_multiplier
            } else {
                cfg.rate
            };
            if !rng.chance(rate) {
                continue;
            }
            if gene_info(i).kind == GeneKind::PartKind {
                self.mutate_part_kind(i, rng);
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
        let mut mask = 0u64;
        for (i, (now, was)) in self.0.iter().zip(before).enumerate() {
            if *now != was {
                mask |= 1 << i;
            }
        }
        mask
    }

    /// Shift a part to a neighboring type, or change its symmetry. Empty slots stay empty.
    fn mutate_part_kind(&mut self, i: usize, rng: &mut Rng) {
        let byte = self.0[i];
        let kind = byte & 0x0F;
        if kind == 0 {
            return;
        }
        let sym = (byte >> 4).min(5);
        if rng.chance(0.5) {
            // Neighboring types in the catalog are related forms (fin ↔ leg ↔ …).
            let n = PartKind::ALL.len() as i32;
            let shifted =
                (kind as i32 - 1 + if rng.chance(0.5) { 1 } else { -1 }).rem_euclid(n) + 1;
            self.0[i] = sym << 4 | shifted as u8;
        } else {
            let new_sym = (sym as i32 + if rng.chance(0.5) { 1 } else { -1 }).clamp(0, 5) as u8;
            self.0[i] = new_sym << 4 | kind;
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
            for b in 0..3 {
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
            // A slot that never held a part has a zero placement (the head, pointing straight
            // forward), so it starts somewhere random; one that did may reuse where that part was.
            if self.0[at + 2] == 0 || rng.chance(0.5) {
                self.0[at + 2] = rng.below(256) as u8;
            }
        }
    }

    /// Weighted mean per-gene difference, in `[0, 1]`. Each gene contributes at most `gene_cap`,
    /// so one large jump in a single gene can't make a new species on its own. Color genes weigh
    /// `color_weight`. A part slot's type counts up to 0.5 (a new kind of part is a big step); its
    /// size and placement count only when both genomes actually have a part there.
    pub fn distance(&self, other: &Genome, sc: &SpeciesConfig) -> f32 {
        let mut sum = 0.0;
        let mut weight_total = 0.0;
        let byte_diff = |a: u8, b: u8| (a as i32 - b as i32).unsigned_abs() as f32 / 255.0;
        for i in 0..GENOME_LEN {
            let (w, d) = match gene_info(i).kind {
                GeneKind::PartKind => {
                    let (a, b) = (self.0[i], other.0[i]);
                    let d = if a & 0x0F != b & 0x0F {
                        1.0
                    } else if a & 0x0F == 0 {
                        0.0
                    } else {
                        ((a >> 4).min(5) as i32 - (b >> 4).min(5) as i32).unsigned_abs() as f32
                            / 5.0
                    };
                    // A new kind of part is a big change, so it isn't held to `gene_cap`.
                    sum += d.min(0.5);
                    weight_total += 1.0;
                    continue;
                }
                GeneKind::PartSize | GeneKind::PartPlace => {
                    let slot = parts::slot_offset((i - gene::PARTS) / 3);
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
        P:000000 000000 000000 000000 000000 000000 000000 000000 R:FF00B0 T:80A0";

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
        assert!(text.contains("P:000000 000000"), "{text}");
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
        // Type shifts move to a neighbor in the catalog and never empty a slot.
        let mut shifted = g;
        let at = parts::slot_offset(p[0].slot);
        for _ in 0..200 {
            shifted.mutate_part_kind(at, &mut rng);
            assert_ne!(shifted.0[at] & 0x0F, 0);
        }
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

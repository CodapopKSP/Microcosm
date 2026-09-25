//! The genome: 34 bytes in ten lettered chromosomes, one byte per gene.
//!
//! See docs/DESIGN.md §2 for what each gene does.

use serde::{Deserialize, Serialize};

use crate::config::MutationConfig;
use crate::rng::Rng;

pub const GENOME_LEN: usize = 34;

/// Byte offsets of every gene.
pub mod gene {
    pub const ARMOR: usize = 0;
    pub const SPIKES: usize = 1;
    pub const JAWS: usize = 2;
    pub const SIZE: usize = 3;
    pub const BUOYANCY: usize = 4;
    pub const RESERVES: usize = 5;
    pub const LONGEVITY: usize = 6;
    pub const HUE: usize = 7;
    pub const SATURATION: usize = 8;
    pub const PATTERN: usize = 9;
    pub const PHOTO: usize = 10;
    pub const CHEMO: usize = 11;
    pub const SCAVENGE: usize = 12;
    pub const HUNT: usize = 13;
    pub const REGEN: usize = 14;
    pub const TOUGHNESS: usize = 15;
    pub const SPEED: usize = 16;
    pub const AGILITY: usize = 17;
    pub const RESTLESSNESS: usize = 18;
    /// First of the eight drive genes (N0..N7).
    pub const DRIVES: usize = 19;
    pub const OFFSPRING_SIZE: usize = 27;
    pub const BROOD: usize = 28;
    pub const PATIENCE: usize = 29;
    pub const SENSE_RANGE: usize = 30;
    pub const FOV: usize = 31;
    pub const TEMP_PREF: usize = 32;
    pub const TEMP_TOL: usize = 33;
}

/// Indices into the drive array (gene N0..N7).
pub mod drive {
    pub const LIGHT: usize = 0;
    pub const WARMTH: usize = 1;
    pub const MINERALS: usize = 2;
    pub const CARRION: usize = 3;
    pub const SMALLER: usize = 4;
    pub const LARGER: usize = 5;
    pub const KIN: usize = 6;
    pub const RISE: usize = 7;
    pub const COUNT: usize = 8;
}

#[derive(Debug, Clone, Copy)]
pub struct GeneInfo {
    /// Short name, e.g. "Size".
    pub name: &'static str,
    /// Signed genes store `value - 128` (so `80` means zero).
    pub signed: bool,
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
        signed: false,
    }
}

const fn s(name: &'static str) -> GeneInfo {
    GeneInfo { name, signed: true }
}

pub static CHROMOSOMES: [Chromosome; 10] = [
    Chromosome {
        letter: 'A',
        name: "Arms",
        start: 0,
        genes: &[g("Armor"), g("Spikes"), g("Jaws")],
    },
    Chromosome {
        letter: 'B',
        name: "Body",
        start: 3,
        genes: &[g("Size"), g("Buoyancy"), g("Reserves"), g("Longevity")],
    },
    Chromosome {
        letter: 'C',
        name: "Color",
        start: 7,
        genes: &[g("Hue"), g("Saturation"), g("Pattern")],
    },
    Chromosome {
        letter: 'D',
        name: "Diet",
        start: 10,
        genes: &[
            g("Photosynthesis"),
            g("Chemosynthesis"),
            g("Scavenging"),
            g("Hunting"),
        ],
    },
    Chromosome {
        letter: 'H',
        name: "Health",
        start: 14,
        genes: &[g("Regeneration"), g("Toughness")],
    },
    Chromosome {
        letter: 'M',
        name: "Motion",
        start: 16,
        genes: &[g("Speed"), g("Agility"), g("Restlessness")],
    },
    Chromosome {
        letter: 'N',
        name: "Nature",
        start: 19,
        genes: &[
            s("Light"),
            s("Warmth"),
            s("Minerals"),
            s("Carrion"),
            s("Smaller"),
            s("Larger"),
            s("Kin"),
            s("Rise"),
        ],
    },
    Chromosome {
        letter: 'R',
        name: "Reproduction",
        start: 27,
        genes: &[g("Offspring size"), g("Brood size"), g("Patience")],
    },
    Chromosome {
        letter: 'S',
        name: "Senses",
        start: 30,
        genes: &[g("Range"), g("Field of view")],
    },
    Chromosome {
        letter: 'T',
        name: "Temperature",
        start: 32,
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
    /// `A:000000 B:20A05020 …`
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, c) in CHROMOSOMES.iter().enumerate() {
            if i > 0 {
                write!(f, " ")?;
            }
            write!(f, "{}:", c.letter)?;
            for b in &self.0[c.range()] {
                write!(f, "{b:02X}")?;
            }
        }
        Ok(())
    }
}

impl Genome {
    /// Parse chromosome notation: `A:000000 B:20 A0 50 20 …`. Every chromosome must appear once.
    pub fn parse(text: &str) -> Result<Genome, String> {
        let mut bytes = [0u8; GENOME_LEN];
        let mut seen = [false; 10];
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
    pub fn mutate(&mut self, rng: &mut Rng, cfg: &MutationConfig) -> u64 {
        let mut mask = 0u64;
        for i in 0..GENOME_LEN {
            let rate = if (gene::HUE..=gene::PATTERN).contains(&i) {
                cfg.rate * cfg.color_rate_multiplier
            } else {
                cfg.rate
            };
            if !rng.chance(rate) {
                continue;
            }
            let old = self.0[i];
            let new = if rng.chance(cfg.reroll) {
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
                let delta = if rng.chance(0.5) { step } else { -step };
                (old as i32 + delta).clamp(0, 255) as u8
            };
            if new != old {
                self.0[i] = new;
                mask |= 1 << i;
            }
        }
        mask
    }

    /// Weighted mean per-gene difference, in `[0, 1]`. Each gene contributes at most `gene_cap`,
    /// so one large jump in a single gene can't make a new species on its own.
    pub fn distance(&self, other: &Genome, color_weight: f32, gene_cap: f32) -> f32 {
        let mut sum = 0.0;
        let mut weight_total = 0.0;
        for i in 0..GENOME_LEN {
            let w = if (gene::HUE..=gene::PATTERN).contains(&i) {
                color_weight
            } else {
                1.0
            };
            let d = if i == gene::HUE {
                // Hue is circular.
                let d = (self.0[i] as i32 - other.0[i] as i32).unsigned_abs();
                d.min(256 - d) as f32 / 128.0
            } else {
                (self.0[i] as i32 - other.0[i] as i32).unsigned_abs() as f32 / 255.0
            };
            sum += w * d.min(gene_cap);
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

    const ANCESTOR: &str = "A:000000 B:20A05020 C:55C000 D:FF000000 H:2030 M:000000 N:8080808080808080 R:FF00B0 S:0000 T:9080";

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
        assert_eq!(chromosome_of(gene::TEMP_TOL).letter, 'T');
        assert_eq!(gene_code(gene::SIZE), "B0");
        assert_eq!(gene_code(gene::HUNT), "D3");
    }

    #[test]
    fn parse_and_format_roundtrip() {
        let g = Genome::parse(ANCESTOR).unwrap();
        assert_eq!(g.to_string(), ANCESTOR);
        assert_eq!(g.get(gene::SIZE), 0x20);
        assert_eq!(g.get(gene::PHOTO), 0xFF);
        assert_eq!(g.signed(gene::DRIVES), 0.0);
        // Spaces inside chromosomes and lowercase are fine.
        let spaced = ANCESTOR.replace("B:20A05020", "b: 20 a0 50 20");
        assert_eq!(Genome::parse(&spaced).unwrap(), g);
    }

    #[test]
    fn parse_rejects_bad_input() {
        assert!(Genome::parse("A:00").is_err());
        assert!(Genome::parse(&ANCESTOR.replace("T:9080", "")).is_err());
        assert!(Genome::parse(&ANCESTOR.replace("T:9080", "T:908")).is_err());
        assert!(Genome::parse(&ANCESTOR.replace("T:9080", "T:9080 A:000000")).is_err());
        assert!(Genome::parse(&ANCESTOR.replace("T:9080", "X:9080")).is_err());
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
        assert!((0.6..0.75).contains(&frac), "exact-copy fraction {frac}");
    }

    #[test]
    fn distance_properties() {
        let a = Genome::parse(ANCESTOR).unwrap();
        assert_eq!(a.distance(&a, 0.25, 1.0), 0.0);
        let mut b = a;
        b.0[gene::SIZE] = 0xFF;
        let d = a.distance(&b, 0.25, 1.0);
        assert!(a.distance(&b, 0.25, 0.3) < d);
        assert!(d > 0.0 && d < 1.0);
        assert_eq!(d, b.distance(&a, 0.25, 1.0));
        // Hue wraps around.
        let mut c = a;
        c.0[gene::HUE] = a.0[gene::HUE].wrapping_add(250);
        let mut d2 = a;
        d2.0[gene::HUE] = a.0[gene::HUE].wrapping_add(6);
        assert!((a.distance(&c, 1.0, 1.0) - a.distance(&d2, 1.0, 1.0)).abs() < 1e-6);
    }

    #[test]
    fn color_distance_wraps() {
        assert_eq!(color_distance(0, 100, 0, 100), 0.0);
        assert!((color_distance(250, 100, 4, 100) - 10.0 / 128.0).abs() < 1e-6);
    }
}

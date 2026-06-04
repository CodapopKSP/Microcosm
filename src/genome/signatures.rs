//! Pursue/avoid signature slots, type registry, and fuzzy matching ([Genome_Definition.md](../../docs/Genome_Definition.md)).

use crate::genome::encoding::GenomeError;
use crate::genome::encoding_v2::GenomeV2;

pub const SIMPLE_SLOT_LEN: usize = 6;
pub const AND_TYPE: u16 = 0x0100;
pub const MAX_AND_ARITY: u8 = 4;

pub const SIG_COLOR: u16 = 0x0000;
pub const SIG_BODY_RADIUS: u16 = 0x0001;
pub const SIG_ARMOR: u16 = 0x0002;
pub const SIG_HEALING_RATE: u16 = 0x0003;
pub const SIG_ENERGY_HEALTH_RATIO: u16 = 0x0004;
pub const SIG_REPRO_THRESHOLD: u16 = 0x0005;
pub const SIG_OFFSPRING_ENDOWMENT: u16 = 0x0006;
pub const SIG_MOVEMENT_DESIRE: u16 = 0x0007;
pub const SIG_TURNING_DESIRE: u16 = 0x0008;
pub const SIG_MAX_SPEED: u16 = 0x0009;
pub const SIG_MOVEMENT_EFFICIENCY: u16 = 0x000A;
pub const SIG_PHOTO_EFFICIENCY: u16 = 0x000B;
pub const SIG_BIOMASS_EFFICIENCY: u16 = 0x000C;
pub const SIG_MINERALS_EFFICIENCY: u16 = 0x000D;
pub const SIG_DEAD_ORGANISM: u16 = 0x0200;
pub const SIG_MINERALS: u16 = 0x0201;
pub const SIG_BRIGHTER_SUNLIGHT: u16 = 0x0202;
pub const SIG_DIMMER_SUNLIGHT: u16 = 0x0203;

/// One `(type u16, value u32)` atom in a pursue/avoid slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignatureAtom {
    pub ty: u16,
    pub value: u32,
}

/// Parsed OR slot: simple atom or AND group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureSlot {
    Simple(SignatureAtom),
    And(Vec<SignatureAtom>),
}

/// Environment samples for chemical signature types (ray hit or endpoint).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ChemicalObservation {
    pub dead_organism: bool,
    pub minerals_raw: u16,
    /// Light at sample minus observer `LightExposure`.
    pub light_delta: f32,
}

/// Byte length of one OR slot starting at `offset` in the genome byte slice.
pub fn slot_payload_len(bytes: &[u8], offset: usize) -> Result<usize, GenomeError> {
    if offset + 2 > bytes.len() {
        return Err(GenomeError::LocusOutOfBounds {
            offset,
            width: 2,
        });
    }
    let ty = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);
    if ty == AND_TYPE {
        if offset + 4 > bytes.len() {
            return Err(GenomeError::LocusOutOfBounds {
                offset,
                width: 4,
            });
        }
        let arity = bytes[offset + 2];
        if arity == 0 || arity > MAX_AND_ARITY {
            return Err(GenomeError::InvalidAndArity { arity });
        }
        Ok(4 + usize::from(arity) * SIMPLE_SLOT_LEN)
    } else {
        Ok(SIMPLE_SLOT_LEN)
    }
}

fn read_atom(bytes: &[u8], offset: usize) -> Result<SignatureAtom, GenomeError> {
    if offset + SIMPLE_SLOT_LEN > bytes.len() {
        return Err(GenomeError::LocusOutOfBounds {
            offset,
            width: SIMPLE_SLOT_LEN,
        });
    }
    Ok(SignatureAtom {
        ty: u16::from_be_bytes([bytes[offset], bytes[offset + 1]]),
        value: u32::from_be_bytes([
            bytes[offset + 2],
            bytes[offset + 3],
            bytes[offset + 4],
            bytes[offset + 5],
        ]),
    })
}

/// Parse `count` consecutive OR slots starting at `offset`; returns next offset.
pub fn parse_slot_list(
    bytes: &[u8],
    mut offset: usize,
    count: u8,
) -> Result<(Vec<SignatureSlot>, usize), GenomeError> {
    let mut slots = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let len = slot_payload_len(bytes, offset)?;
        if offset + len > bytes.len() {
            return Err(GenomeError::SlotPayloadOverflow {
                need: offset + len,
                have: bytes.len(),
            });
        }
        let ty = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);
        if ty == AND_TYPE {
            let arity = bytes[offset + 2];
            let mut atoms = Vec::with_capacity(arity as usize);
            let mut atom_off = offset + 4;
            for _ in 0..arity {
                atoms.push(read_atom(bytes, atom_off)?);
                atom_off += SIMPLE_SLOT_LEN;
            }
            slots.push(SignatureSlot::And(atoms));
        } else {
            slots.push(SignatureSlot::Simple(read_atom(bytes, offset)?));
        }
        offset += len;
    }
    Ok((slots, offset))
}

/// Pursue then avoid slot lists from variable payload.
pub fn parse_pursue_avoid_lists(
    genome: &GenomeV2,
) -> Result<(Vec<SignatureSlot>, Vec<SignatureSlot>), GenomeError> {
    let pursue_n = genome.pursue_slot_count()?;
    let avoid_n = genome.avoid_slot_count()?;
    let core_start =
        crate::genome::encoding_v2::HEADER_LEN + crate::genome::encoding_v2::CORE_FIXED_LEN;
    let (pursue, offset) = parse_slot_list(&genome.bytes, core_start, pursue_n)?;
    let (avoid, _) = parse_slot_list(&genome.bytes, offset, avoid_n)?;
    Ok((pursue, avoid))
}

/// Total bytes consumed by pursue then avoid slots after the 24-byte fixed core.
pub fn variable_payload_len(genome: &GenomeV2) -> Result<usize, GenomeError> {
    let pursue = genome.pursue_slot_count()? as usize;
    let avoid = genome.avoid_slot_count()? as usize;
    let core_start =
        crate::genome::encoding_v2::HEADER_LEN + crate::genome::encoding_v2::CORE_FIXED_LEN;
    let mut offset = core_start;
    for _ in 0..pursue + avoid {
        let len = slot_payload_len(&genome.bytes, offset)?;
        offset = offset.saturating_add(len);
        if offset > genome.bytes.len() {
            return Err(GenomeError::SlotPayloadOverflow {
                need: offset,
                have: genome.bytes.len(),
            });
        }
    }
    Ok(offset - core_start)
}

/// Expected total genome length from header + core + slot payload.
pub fn expected_genome_len(genome: &GenomeV2) -> Result<usize, GenomeError> {
    Ok(crate::genome::encoding_v2::HEADER_LEN
        + crate::genome::encoding_v2::CORE_FIXED_LEN
        + variable_payload_len(genome)?)
}

pub fn is_known_signature_type(ty: u16) -> bool {
    matches!(
        ty,
        SIG_COLOR
            | SIG_BODY_RADIUS
            | SIG_ARMOR
            | SIG_HEALING_RATE
            | SIG_ENERGY_HEALTH_RATIO
            | SIG_REPRO_THRESHOLD
            | SIG_OFFSPRING_ENDOWMENT
            | SIG_MOVEMENT_DESIRE
            | SIG_TURNING_DESIRE
            | SIG_MAX_SPEED
            | SIG_MOVEMENT_EFFICIENCY
            | SIG_PHOTO_EFFICIENCY
            | SIG_BIOMASS_EFFICIENCY
            | SIG_MINERALS_EFFICIENCY
            | SIG_DEAD_ORGANISM
            | SIG_MINERALS
            | SIG_BRIGHTER_SUNLIGHT
            | SIG_DIMMER_SUNLIGHT
    )
}

fn nibble_close(observed: u8, target: u8, delta: u8) -> bool {
    let diff = (observed as i16 - target as i16).abs();
    diff <= delta as i16
}

/// Per-nibble ±`delta` on one byte (two nibbles).
pub fn byte_fuzzy_match(observed: u8, target: u8, delta: u8) -> bool {
    nibble_close(observed >> 4, target >> 4, delta)
        && nibble_close(observed & 0x0F, target & 0x0F, delta)
}

/// Per-nibble ±`delta` on low 16 bits.
pub fn u16_fuzzy_match(observed: u16, target: u16, delta: u8) -> bool {
    for shift in [12_u8, 8, 4, 0] {
        let o = ((observed >> shift) & 0xF) as u8;
        let t = ((target >> shift) & 0xF) as u8;
        if !nibble_close(o, t, delta) {
            return false;
        }
    }
    true
}

/// Per-channel byte fuzzy match on low 24 bits of `value`.
pub fn rgb_fuzzy_match(observed: [u8; 3], value: u32, delta: u8) -> bool {
    let r = ((value >> 16) & 0xFF) as u8;
    let g = ((value >> 8) & 0xFF) as u8;
    let b = (value & 0xFF) as u8;
    byte_fuzzy_match(observed[0], r, delta)
        && byte_fuzzy_match(observed[1], g, delta)
        && byte_fuzzy_match(observed[2], b, delta)
}

fn light_delta_to_u16(delta: f32) -> u16 {
    (delta.max(0.0).min(1.0) * 65535.0).round() as u16
}

fn is_chemical_signature(ty: u16) -> bool {
    matches!(
        ty,
        SIG_DEAD_ORGANISM | SIG_MINERALS | SIG_BRIGHTER_SUNLIGHT | SIG_DIMMER_SUNLIGHT
    )
}

/// Match one atom against a smelled agent genome and optional chemical sample.
pub fn atom_matches(
    atom: &SignatureAtom,
    target: Option<&GenomeV2>,
    chemical: Option<ChemicalObservation>,
    fuzzy_delta: u8,
) -> bool {
    if !is_known_signature_type(atom.ty) {
        return false;
    }
    if is_chemical_signature(atom.ty) {
        return match atom.ty {
            SIG_DEAD_ORGANISM => chemical
                .map(|chem| {
                    if atom.value == 0 {
                        chem.dead_organism
                    } else {
                        chem.dead_organism && u16_fuzzy_match(0, atom.value as u16, fuzzy_delta)
                    }
                })
                .unwrap_or(false),
            SIG_MINERALS => chemical
                .map(|chem| u16_fuzzy_match(chem.minerals_raw, atom.value as u16, fuzzy_delta))
                .unwrap_or(false),
            SIG_BRIGHTER_SUNLIGHT => chemical
                .map(|chem| {
                    if chem.light_delta <= 0.0 {
                        false
                    } else {
                        u16_fuzzy_match(
                            light_delta_to_u16(chem.light_delta),
                            atom.value as u16,
                            fuzzy_delta,
                        )
                    }
                })
                .unwrap_or(false),
            SIG_DIMMER_SUNLIGHT => chemical
                .map(|chem| {
                    if chem.light_delta >= 0.0 {
                        false
                    } else {
                        u16_fuzzy_match(
                            light_delta_to_u16(-chem.light_delta),
                            atom.value as u16,
                            fuzzy_delta,
                        )
                    }
                })
                .unwrap_or(false),
            _ => false,
        };
    }
    let Some(target) = target else {
        return false;
    };
    match atom.ty {
        SIG_COLOR => match (target.color_r(), target.color_g(), target.color_b()) {
            (Ok(r), Ok(g), Ok(b)) => rgb_fuzzy_match([r, g, b], atom.value, fuzzy_delta),
            _ => false,
        },
        SIG_BODY_RADIUS => target
            .body_radius_raw()
            .map(|obs| u16_fuzzy_match(obs, atom.value as u16, fuzzy_delta))
            .unwrap_or(false),
        SIG_ARMOR => target
            .armor_raw()
            .map(|obs| u16_fuzzy_match(obs, atom.value as u16, fuzzy_delta))
            .unwrap_or(false),
        SIG_HEALING_RATE => target
            .healing_rate_raw()
            .map(|obs| u16_fuzzy_match(obs, atom.value as u16, fuzzy_delta))
            .unwrap_or(false),
        SIG_ENERGY_HEALTH_RATIO => target
            .energy_health_ratio_raw()
            .map(|obs| u16_fuzzy_match(obs, atom.value as u16, fuzzy_delta))
            .unwrap_or(false),
        SIG_REPRO_THRESHOLD => target
            .repro_threshold_raw()
            .map(|obs| byte_fuzzy_match(obs, atom.value as u8, fuzzy_delta))
            .unwrap_or(false),
        SIG_OFFSPRING_ENDOWMENT => target
            .offspring_endowment_raw()
            .map(|obs| byte_fuzzy_match(obs, atom.value as u8, fuzzy_delta))
            .unwrap_or(false),
        SIG_MOVEMENT_DESIRE => target
            .movement_desire_raw()
            .map(|obs| byte_fuzzy_match(obs, atom.value as u8, fuzzy_delta))
            .unwrap_or(false),
        SIG_TURNING_DESIRE => target
            .turning_desire_raw()
            .map(|obs| byte_fuzzy_match(obs, atom.value as u8, fuzzy_delta))
            .unwrap_or(false),
        SIG_MAX_SPEED => target
            .max_speed_raw()
            .map(|obs| byte_fuzzy_match(obs, atom.value as u8, fuzzy_delta))
            .unwrap_or(false),
        SIG_MOVEMENT_EFFICIENCY => target
            .movement_efficiency_raw()
            .map(|obs| byte_fuzzy_match(obs, atom.value as u8, fuzzy_delta))
            .unwrap_or(false),
        SIG_PHOTO_EFFICIENCY => target
            .photo_efficiency_raw()
            .map(|obs| byte_fuzzy_match(obs, atom.value as u8, fuzzy_delta))
            .unwrap_or(false),
        SIG_BIOMASS_EFFICIENCY => target
            .biomass_efficiency_raw()
            .map(|obs| byte_fuzzy_match(obs, atom.value as u8, fuzzy_delta))
            .unwrap_or(false),
        SIG_MINERALS_EFFICIENCY => target
            .minerals_efficiency_raw()
            .map(|obs| byte_fuzzy_match(obs, atom.value as u8, fuzzy_delta))
            .unwrap_or(false),
        _ => false,
    }
}

/// Whether one OR slot matches the observation.
pub fn slot_matches(
    slot: &SignatureSlot,
    target: Option<&GenomeV2>,
    chemical: Option<ChemicalObservation>,
    fuzzy_delta: u8,
) -> bool {
    match slot {
        SignatureSlot::Simple(atom) => atom_matches(atom, target, chemical, fuzzy_delta),
        SignatureSlot::And(atoms) => atoms
            .iter()
            .all(|a| atom_matches(a, target, chemical, fuzzy_delta)),
    }
}

/// True if any slot in the list matches.
pub fn slot_list_matches(
    slots: &[SignatureSlot],
    target: Option<&GenomeV2>,
    chemical: Option<ChemicalObservation>,
    fuzzy_delta: u8,
) -> bool {
    slots
        .iter()
        .any(|s| slot_matches(s, target, chemical, fuzzy_delta))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::genome::{GenomeV2, V2_HEADER_LEN};
    use crate::genome::v2_core;

    fn v2_config() -> SimConfig {
        SimConfig::load_from_path(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs/genome_v2.example.toml"),
        )
        .unwrap()
    }

    #[test]
    fn ancestor_has_zero_slot_payload() {
        let config = v2_config();
        let genome = GenomeV2::ancestor_from_config(&config).unwrap();
        assert_eq!(variable_payload_len(&genome).unwrap(), 0);
        assert_eq!(expected_genome_len(&genome).unwrap(), 28);
    }

    #[test]
    fn and_slot_length_arity_two() {
        let bytes = [
            0x01, 0x00, 0x02, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x00,
        ];
        assert_eq!(slot_payload_len(&bytes, 0).unwrap(), 16);
    }

    #[test]
    fn fuzzy_u8_0x0a_matches_neighbors_not_far() {
        assert!(byte_fuzzy_match(0x0A, 0x0A, 1));
        assert!(byte_fuzzy_match(0x09, 0x0A, 1));
        assert!(byte_fuzzy_match(0x0B, 0x0A, 1));
        assert!(!byte_fuzzy_match(0x07, 0x0A, 1));
    }

    #[test]
    fn pursue_color_signature_matches_greenish_genome() {
        let config = v2_config();
        let mut genetics = config.genetics.clone();
        let mut bytes = genetics.parse_ancestor_bytes().unwrap();
        bytes[V2_HEADER_LEN + v2_core::PURSUE_SLOT_COUNT] = 1;
        bytes.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0xFF, 0x05]);
        genetics.core_length = bytes.len();
        let genome = GenomeV2::from_bytes(bytes, &genetics).unwrap();
        let (pursue, avoid) = parse_pursue_avoid_lists(&genome).unwrap();
        assert_eq!(avoid.len(), 0);
        assert_eq!(pursue.len(), 1);
        let target = GenomeV2::ancestor_from_config(&config).unwrap();
        assert!(slot_list_matches(&pursue, Some(&target), None, 1));
    }

    #[test]
    fn invalid_signature_type_never_matches() {
        let atom = SignatureAtom {
            ty: 0x00EE,
            value: 0,
        };
        let config = v2_config();
        let target = GenomeV2::ancestor_from_config(&config).unwrap();
        assert!(!atom_matches(&atom, Some(&target), None, 1));
    }

    #[test]
    fn and_slot_requires_all_atoms() {
        let bytes = [
            0x01, 0x00, 0x02, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x00,
        ];
        let (slots, _) = parse_slot_list(&bytes, 0, 1).unwrap();
        let config = v2_config();
        let mut g = GenomeV2::ancestor_from_config(&config).unwrap();
        let idx = V2_HEADER_LEN + v2_core::BODY_RADIUS;
        g.bytes[idx] = 0;
        g.bytes[idx + 1] = 0x0A;
        assert!(slot_list_matches(&slots, Some(&g), None, 0));
        g.bytes[idx + 1] = 0xFF;
        assert!(!slot_list_matches(&slots, Some(&g), None, 0));
    }
}

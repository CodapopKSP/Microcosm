//! Genome v1 byte layout, validation, and core locus accessors.

use std::fmt;

use crate::config::{ConfigError, GeneticsConfig, SimConfig};

pub const MAGIC: &[u8; 4] = b"EVO1";
pub const HEADER_LEN: usize = 4;
pub const CORE_SEGMENT_LEN: usize = 28;
pub const CORE_GENOME_LEN: usize = HEADER_LEN + CORE_SEGMENT_LEN;

/// Core-segment byte offsets (relative to first byte after the header).
pub mod core {
    pub const COLOR_R: usize = 0;
    pub const COLOR_G: usize = 1;
    pub const COLOR_B: usize = 2;
    pub const BODY_RADIUS: usize = 3;
    pub const PHOTO_STRENGTH: usize = 4;
    pub const METABOLISM_BASE: usize = 6;
    pub const LOCOMOTION_INVESTMENT: usize = 8;
    pub const REPRO_THRESHOLD: usize = 10;
    pub const REPRO_COST: usize = 12;
    pub const OFFSPRING_ENDOWMENT: usize = 14;
    /// Per-tick movement intent: `raw / 255` = probability of moving (0 = never, 255 = always).
    pub const DESIRE_TO_MOVE: usize = 16;
    /// Legacy alias (was `repro_cooldown`; config-only cooldown since genome revision #4).
    pub const RESERVED_16: usize = DESIRE_TO_MOVE;
    /// Reserved (was `repro_risk_factor`; removed in genome revision #5).
    pub const RESERVED_17: usize = 17;
    pub const SKIN_HARDNESS: usize = 18;
    pub const REPAIR_RATE: usize = 20;
    /// Legacy alias for [`REPAIR_RATE`] (was `repair_efficiency`).
    pub const REPAIR_EFFICIENCY: usize = REPAIR_RATE;
    pub const CHEMO_POTENTIAL: usize = 22;
    pub const MOVEMENT_PACE: usize = 24;
    /// Legacy alias for [`MOVEMENT_PACE`].
    pub const MAX_SPEED: usize = MOVEMENT_PACE;
    pub const MAX_AGE: usize = 26;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Genome {
    pub bytes: Vec<u8>,
    pub version: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenomeError {
    InvalidMagic,
    VersionMismatch { expected: u8, stored: u8 },
    LengthMismatch { expected: usize, actual: usize },
    CoreSegmentTooShort { need: usize, have: usize },
    LocusOutOfBounds { offset: usize, width: usize },
    Config(String),
}

impl fmt::Display for GenomeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GenomeError::InvalidMagic => write!(f, "genome header magic is not EVO1"),
            GenomeError::VersionMismatch { expected, stored } => {
                write!(
                    f,
                    "genome version mismatch (expected {expected}, stored {stored})"
                )
            }
            GenomeError::LengthMismatch { expected, actual } => write!(
                f,
                "genome length {actual} does not match expected {expected}"
            ),
            GenomeError::CoreSegmentTooShort { need, have } => write!(
                f,
                "genome core segment too short (need {need} bytes after header, have {have})"
            ),
            GenomeError::LocusOutOfBounds { offset, width } => write!(
                f,
                "locus at core offset {offset} with width {width} is out of bounds"
            ),
            GenomeError::Config(msg) => write!(f, "genome config error: {msg}"),
        }
    }
}

impl std::error::Error for GenomeError {}

impl From<ConfigError> for GenomeError {
    fn from(err: ConfigError) -> Self {
        GenomeError::Config(err.to_string())
    }
}

impl Genome {
    pub fn from_bytes(
        bytes: Vec<u8>,
        expected_version: u8,
        expected_len: usize,
    ) -> Result<Self, GenomeError> {
        let genome = Self {
            version: expected_version,
            bytes,
        };
        genome.validate(expected_version, expected_len)?;
        Ok(genome)
    }

    /// Build and validate the configured ancestor template.
    pub fn ancestor_from_config(config: &SimConfig) -> Result<Self, GenomeError> {
        let bytes = config.genetics.parse_ancestor_bytes()?;
        Self::from_bytes(
            bytes,
            config.genetics.genome_version,
            config.genetics.core_length,
        )
    }

    pub fn ancestor_from_genetics(genetics: &GeneticsConfig) -> Result<Self, GenomeError> {
        let bytes = genetics.parse_ancestor_bytes()?;
        Self::from_bytes(bytes, genetics.genome_version, genetics.core_length)
    }

    pub fn clone_genome(&self) -> Self {
        Self {
            bytes: self.bytes.clone(),
            version: self.version,
        }
    }

    /// Fast check for spawn pipeline (stillborn when false).
    pub fn is_valid(&self, expected_version: u8, expected_len: usize) -> bool {
        self.validate(expected_version, expected_len).is_ok()
    }

    pub fn validate(&self, expected_version: u8, expected_len: usize) -> Result<(), GenomeError> {
        if self.bytes.len() != expected_len {
            return Err(GenomeError::LengthMismatch {
                expected: expected_len,
                actual: self.bytes.len(),
            });
        }

        if self.bytes.len() < HEADER_LEN {
            return Err(GenomeError::CoreSegmentTooShort {
                need: CORE_SEGMENT_LEN,
                have: self.bytes.len().saturating_sub(HEADER_LEN),
            });
        }

        if &self.bytes[0..HEADER_LEN] != MAGIC {
            return Err(GenomeError::InvalidMagic);
        }

        if self.version != expected_version {
            return Err(GenomeError::VersionMismatch {
                expected: expected_version,
                stored: self.version,
            });
        }

        let core_len = self.bytes.len() - HEADER_LEN;
        if core_len < CORE_SEGMENT_LEN {
            return Err(GenomeError::CoreSegmentTooShort {
                need: CORE_SEGMENT_LEN,
                have: core_len,
            });
        }

        Ok(())
    }

    pub fn to_hex(&self) -> String {
        self.bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn absolute_core_index(&self, core_offset: usize) -> Result<usize, GenomeError> {
        if core_offset >= CORE_SEGMENT_LEN {
            return Err(GenomeError::LocusOutOfBounds {
                offset: core_offset,
                width: 1,
            });
        }
        Ok(HEADER_LEN + core_offset)
    }

    pub fn read_u8_at_core(&self, core_offset: usize) -> Result<u8, GenomeError> {
        let idx = self.absolute_core_index(core_offset)?;
        Ok(self.bytes[idx])
    }

    pub fn read_u16_be_at_core(&self, core_offset: usize) -> Result<u16, GenomeError> {
        if core_offset + 1 >= CORE_SEGMENT_LEN {
            return Err(GenomeError::LocusOutOfBounds {
                offset: core_offset,
                width: 2,
            });
        }
        let idx = self.absolute_core_index(core_offset)?;
        Ok(u16::from_be_bytes([self.bytes[idx], self.bytes[idx + 1]]))
    }

    pub fn color_r(&self) -> Result<u8, GenomeError> {
        self.read_u8_at_core(core::COLOR_R)
    }

    pub fn color_g(&self) -> Result<u8, GenomeError> {
        self.read_u8_at_core(core::COLOR_G)
    }

    pub fn color_b(&self) -> Result<u8, GenomeError> {
        self.read_u8_at_core(core::COLOR_B)
    }

    pub fn body_radius_raw(&self) -> Result<u8, GenomeError> {
        self.read_u8_at_core(core::BODY_RADIUS)
    }

    pub fn photo_strength_raw(&self) -> Result<u16, GenomeError> {
        self.read_u16_be_at_core(core::PHOTO_STRENGTH)
    }

    pub fn metabolism_base_raw(&self) -> Result<u16, GenomeError> {
        self.read_u16_be_at_core(core::METABOLISM_BASE)
    }

    pub fn locomotion_investment_raw(&self) -> Result<u16, GenomeError> {
        self.read_u16_be_at_core(core::LOCOMOTION_INVESTMENT)
    }

    pub fn repro_threshold_raw(&self) -> Result<u16, GenomeError> {
        self.read_u16_be_at_core(core::REPRO_THRESHOLD)
    }

    pub fn repro_cost_raw(&self) -> Result<u16, GenomeError> {
        self.read_u16_be_at_core(core::REPRO_COST)
    }

    pub fn offspring_endowment_raw(&self) -> Result<u16, GenomeError> {
        self.read_u16_be_at_core(core::OFFSPRING_ENDOWMENT)
    }

    pub fn desire_to_move_raw(&self) -> Result<u8, GenomeError> {
        self.read_u8_at_core(core::DESIRE_TO_MOVE)
    }

    pub fn skin_hardness_raw(&self) -> Result<u16, GenomeError> {
        self.read_u16_be_at_core(core::SKIN_HARDNESS)
    }

    pub fn repair_rate_raw(&self) -> Result<u16, GenomeError> {
        self.read_u16_be_at_core(core::REPAIR_RATE)
    }

    /// Legacy alias for [`repair_rate_raw`].
    pub fn repair_efficiency_raw(&self) -> Result<u16, GenomeError> {
        self.repair_rate_raw()
    }

    pub fn chemo_potential_raw(&self) -> Result<u16, GenomeError> {
        self.read_u16_be_at_core(core::CHEMO_POTENTIAL)
    }

    pub fn movement_pace_raw(&self) -> Result<u16, GenomeError> {
        self.read_u16_be_at_core(core::MOVEMENT_PACE)
    }

    /// Legacy alias for [`movement_pace_raw`].
    pub fn max_speed_raw(&self) -> Result<u16, GenomeError> {
        self.movement_pace_raw()
    }

    pub fn max_age_raw(&self) -> Result<u16, GenomeError> {
        self.read_u16_be_at_core(core::MAX_AGE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;

    fn default_ancestor() -> Genome {
        Genome::ancestor_from_config(&SimConfig::load_default().unwrap()).unwrap()
    }

    #[test]
    fn ancestor_from_default_config_is_valid() {
        let config = SimConfig::load_default().unwrap();
        let genome = Genome::ancestor_from_config(&config).unwrap();
        assert!(genome.is_valid(config.genetics.genome_version, config.genetics.core_length));
        assert_eq!(&genome.bytes[0..4], MAGIC);
        assert_eq!(genome.bytes.len(), CORE_GENOME_LEN);
    }

    #[test]
    fn default_ancestor_loci_match_genome_md() {
        let g = default_ancestor();
        assert_eq!(g.color_r().unwrap(), 0x88);
        assert_eq!(g.color_g().unwrap(), 0xCC);
        assert_eq!(g.color_b().unwrap(), 0x44);
        assert_eq!(g.body_radius_raw().unwrap(), 109);
        assert_eq!(g.photo_strength_raw().unwrap(), 0xFFFF);
        assert_eq!(g.metabolism_base_raw().unwrap(), 0x3000);
        assert_eq!(g.locomotion_investment_raw().unwrap(), 0x2800);
        assert_eq!(g.repro_threshold_raw().unwrap(), 0x0AF6);
        assert_eq!(g.repro_cost_raw().unwrap(), 0x4000);
        assert_eq!(g.offspring_endowment_raw().unwrap(), 0x0AF6);
        assert_eq!(g.desire_to_move_raw().unwrap(), 0);
        assert_eq!(g.read_u8_at_core(core::RESERVED_17).unwrap(), 0x80);
        assert_eq!(g.skin_hardness_raw().unwrap(), 0);
        assert_eq!(g.repair_rate_raw().unwrap(), 0x5555);
        assert_eq!(g.chemo_potential_raw().unwrap(), 0);
        assert_eq!(g.movement_pace_raw().unwrap(), 0x5555);
        assert_eq!(g.max_age_raw().unwrap(), 0x8000);
    }

    #[test]
    fn rejects_invalid_magic() {
        let config = SimConfig::load_default().unwrap();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        bytes[0] = b'X';
        let err = Genome::from_bytes(bytes, config.genetics.genome_version, config.genetics.core_length)
            .unwrap_err();
        assert_eq!(err, GenomeError::InvalidMagic);
    }

    #[test]
    fn rejects_length_mismatch() {
        let config = SimConfig::load_default().unwrap();
        let bytes = vec![0u8; 16];
        let err = Genome::from_bytes(bytes, config.genetics.genome_version, config.genetics.core_length)
            .unwrap_err();
        assert!(matches!(err, GenomeError::LengthMismatch { .. }));
    }

    #[test]
    fn rejects_version_mismatch_on_validate() {
        let config = SimConfig::load_default().unwrap();
        let mut genome = default_ancestor();
        genome.version = 99;
        let err = genome
            .validate(config.genetics.genome_version, config.genetics.core_length)
            .unwrap_err();
        assert!(matches!(err, GenomeError::VersionMismatch { .. }));
    }

    #[test]
    fn to_hex_roundtrip_length() {
        let g = default_ancestor();
        assert_eq!(g.to_hex().len(), CORE_GENOME_LEN * 2);
    }

    #[test]
    fn u16_accessor_is_big_endian() {
        let config = SimConfig::load_default().unwrap();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        let core = HEADER_LEN + core::PHOTO_STRENGTH;
        bytes[core] = 0x12;
        bytes[core + 1] = 0x34;
        let g = Genome::from_bytes(bytes, config.genetics.genome_version, config.genetics.core_length)
            .unwrap();
        assert_eq!(g.photo_strength_raw().unwrap(), 0x1234);
    }
}

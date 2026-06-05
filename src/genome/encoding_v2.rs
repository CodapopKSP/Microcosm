//! Genome v2 (`EVO2`) byte layout, validation, and core locus accessors.

use crate::config::{GeneticsConfig, SimConfig};
use crate::genome::encoding::GenomeError;
use crate::genome::signatures::{expected_genome_len, slot_payload_len};

pub const MAGIC: &[u8; 4] = b"EVO2";
pub const HEADER_LEN: usize = 4;
pub const CORE_FIXED_LEN: usize = 23;
pub const MIN_GENOME_LEN: usize = HEADER_LEN + CORE_FIXED_LEN;

/// Core-segment byte offsets (relative to first byte after the header).
pub mod core {
    pub const COLOR_R: usize = 0;
    pub const COLOR_G: usize = 1;
    pub const COLOR_B: usize = 2;
    pub const BODY_RADIUS: usize = 3;
    pub const ARMOR: usize = 5;
    pub const HEALING_RATE: usize = 7;
    pub const ENERGY_HEALTH_RATIO: usize = 9;
    pub const REPRO_THRESHOLD: usize = 10;
    pub const OFFSPRING_ENDOWMENT: usize = 11;
    pub const MOVEMENT_DESIRE: usize = 12;
    pub const TURNING_DESIRE: usize = 13;
    pub const MAX_SPEED: usize = 14;
    pub const MOVEMENT_EFFICIENCY: usize = 15;
    pub const PHOTO_EFFICIENCY: usize = 16;
    pub const BIOMASS_EFFICIENCY: usize = 17;
    pub const MINERALS_EFFICIENCY: usize = 18;
    pub const SENSOR_MASK: usize = 19;
    pub const SENSOR_STRENGTH: usize = 20;
    pub const PURSUE_SLOT_COUNT: usize = 21;
    pub const AVOID_SLOT_COUNT: usize = 22;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenomeV2 {
    pub bytes: Vec<u8>,
    pub version: u8,
}

impl GenomeV2 {
    pub fn from_bytes(
        bytes: Vec<u8>,
        genetics: &GeneticsConfig,
    ) -> Result<Self, GenomeError> {
        let genome = Self {
            version: genetics.genome_version,
            bytes,
        };
        genome.validate(genetics)?;
        Ok(genome)
    }

    pub fn ancestor_from_config(config: &SimConfig) -> Result<Self, GenomeError> {
        Self::ancestor_from_genetics(&config.genetics)
    }

    pub fn ancestor_from_genetics(genetics: &GeneticsConfig) -> Result<Self, GenomeError> {
        let bytes = genetics
            .parse_ancestor_bytes()
            .map_err(GenomeError::from)?;
        Self::from_bytes(bytes, genetics)
    }

    pub fn clone_genome(&self) -> Self {
        Self {
            bytes: self.bytes.clone(),
            version: self.version,
        }
    }

    pub fn is_valid(&self, genetics: &GeneticsConfig) -> bool {
        self.validate(genetics).is_ok()
    }

    pub fn validate(&self, genetics: &GeneticsConfig) -> Result<(), GenomeError> {
        if self.bytes.len() < MIN_GENOME_LEN {
            return Err(GenomeError::CoreSegmentTooShort {
                need: CORE_FIXED_LEN,
                have: self.bytes.len().saturating_sub(HEADER_LEN),
            });
        }

        if self.bytes.len() > genetics.max_genome_bytes {
            return Err(GenomeError::LengthMismatch {
                expected: genetics.max_genome_bytes,
                actual: self.bytes.len(),
            });
        }

        if &self.bytes[0..HEADER_LEN] != MAGIC {
            return Err(GenomeError::InvalidMagic);
        }

        if self.version != genetics.genome_version {
            return Err(GenomeError::VersionMismatch {
                expected: genetics.genome_version,
                stored: self.version,
            });
        }

        let pursue = self.pursue_slot_count()?;
        let avoid = self.avoid_slot_count()?;
        if pursue > genetics.max_pursue_slots || avoid > genetics.max_avoid_slots {
            return Err(GenomeError::SlotCountExceedsConfig { pursue, avoid });
        }

        let payload_expected = expected_genome_len(self)?;
        if self.bytes.len() != genetics.core_length {
            return Err(GenomeError::LengthMismatch {
                expected: genetics.core_length,
                actual: self.bytes.len(),
            });
        }
        if payload_expected != genetics.core_length {
            return Err(GenomeError::LengthMismatch {
                expected: payload_expected,
                actual: genetics.core_length,
            });
        }

        let core_start = HEADER_LEN + CORE_FIXED_LEN;
        let mut offset = core_start;
        for _ in 0..usize::from(pursue) + usize::from(avoid) {
            let len = slot_payload_len(&self.bytes, offset)?;
            offset = offset.saturating_add(len);
        }

        Ok(())
    }

    pub fn to_hex(&self) -> String {
        self.bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Fixed-core display groups ([Genome_Definition.md](../../../docs/Genome_Definition.md) example layout).
    pub fn core_chromosome_groups() -> &'static [(&'static str, usize)] {
        &[
            ("C", 3), // Color
            ("B", 2), // Body
            ("A", 2), // Armor
            ("H", 3), // Healing rate (2 B) + energy/health ratio (1 B)
            ("R", 2), // Reproduction threshold + offspring endowment
            ("M", 4), // Movement desire, turning, max speed, move efficiency
            ("E", 3), // Photo / biomass / mineral efficiency
            ("S", 2), // Sensors
            ("I", 2), // Pursue + avoid slot counts
        ]
    }

    fn hex_chunk(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Core bytes grouped by chromosome label (`C B A H R M E S I`).
    pub fn core_chromosomes_hex(&self) -> Vec<(String, String)> {
        let core = self
            .bytes
            .get(HEADER_LEN..HEADER_LEN + CORE_FIXED_LEN)
            .unwrap_or(&[]);
        let mut offset = 0;
        Self::core_chromosome_groups()
            .iter()
            .map(|(label, len)| {
                let chunk = &core[offset..offset + len];
                offset += len;
                (label.to_string(), Self::hex_chunk(chunk))
            })
            .collect()
    }

    /// Header + grouped core + trailing pursue/avoid payload (if any).
    pub fn chromosomes_display_lines(&self) -> Vec<(String, String)> {
        let mut lines = vec![("Hdr".to_string(), Self::hex_chunk(&self.bytes[..HEADER_LEN]))];
        lines.extend(self.core_chromosomes_hex());
        let payload_start = HEADER_LEN + CORE_FIXED_LEN;
        if self.bytes.len() > payload_start {
            lines.push((
                "Slots".to_string(),
                Self::hex_chunk(&self.bytes[payload_start..]),
            ));
        }
        lines
    }

    pub fn chromosomes_display_text(&self) -> String {
        self.chromosomes_display_lines()
            .into_iter()
            .map(|(label, hex)| format!("{label} {hex}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn absolute_core_index(&self, core_offset: usize) -> Result<usize, GenomeError> {
        if core_offset >= CORE_FIXED_LEN {
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
        if core_offset + 1 >= CORE_FIXED_LEN {
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

    pub fn body_radius_raw(&self) -> Result<u16, GenomeError> {
        self.read_u16_be_at_core(core::BODY_RADIUS)
    }

    pub fn armor_raw(&self) -> Result<u16, GenomeError> {
        self.read_u16_be_at_core(core::ARMOR)
    }

    pub fn healing_rate_raw(&self) -> Result<u16, GenomeError> {
        self.read_u16_be_at_core(core::HEALING_RATE)
    }

    pub fn energy_health_ratio_raw(&self) -> Result<u8, GenomeError> {
        self.read_u8_at_core(core::ENERGY_HEALTH_RATIO)
    }

    pub fn repro_threshold_raw(&self) -> Result<u8, GenomeError> {
        self.read_u8_at_core(core::REPRO_THRESHOLD)
    }

    pub fn offspring_endowment_raw(&self) -> Result<u8, GenomeError> {
        self.read_u8_at_core(core::OFFSPRING_ENDOWMENT)
    }

    pub fn movement_desire_raw(&self) -> Result<u8, GenomeError> {
        self.read_u8_at_core(core::MOVEMENT_DESIRE)
    }

    pub fn turning_desire_raw(&self) -> Result<u8, GenomeError> {
        self.read_u8_at_core(core::TURNING_DESIRE)
    }

    pub fn max_speed_raw(&self) -> Result<u8, GenomeError> {
        self.read_u8_at_core(core::MAX_SPEED)
    }

    pub fn movement_efficiency_raw(&self) -> Result<u8, GenomeError> {
        self.read_u8_at_core(core::MOVEMENT_EFFICIENCY)
    }

    pub fn photo_efficiency_raw(&self) -> Result<u8, GenomeError> {
        self.read_u8_at_core(core::PHOTO_EFFICIENCY)
    }

    pub fn biomass_efficiency_raw(&self) -> Result<u8, GenomeError> {
        self.read_u8_at_core(core::BIOMASS_EFFICIENCY)
    }

    pub fn minerals_efficiency_raw(&self) -> Result<u8, GenomeError> {
        self.read_u8_at_core(core::MINERALS_EFFICIENCY)
    }

    pub fn sensor_mask_raw(&self) -> Result<u8, GenomeError> {
        self.read_u8_at_core(core::SENSOR_MASK)
    }

    pub fn sensor_strength_raw(&self) -> Result<u8, GenomeError> {
        self.read_u8_at_core(core::SENSOR_STRENGTH)
    }

    pub fn pursue_slot_count(&self) -> Result<u8, GenomeError> {
        self.read_u8_at_core(core::PURSUE_SLOT_COUNT)
    }

    pub fn avoid_slot_count(&self) -> Result<u8, GenomeError> {
        self.read_u8_at_core(core::AVOID_SLOT_COUNT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;

    fn v2_config() -> SimConfig {
        SimConfig::load_from_path(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs/genome_v2.example.toml"),
        )
        .unwrap()
    }

    #[test]
    fn ancestor_is_valid_evo2() {
        let config = v2_config();
        let genome = GenomeV2::ancestor_from_config(&config).unwrap();
        assert!(genome.is_valid(&config.genetics));
        assert_eq!(&genome.bytes[0..4], MAGIC);
        assert_eq!(genome.bytes.len(), 27);
    }

    #[test]
    fn ancestor_core_loci_match_spec() {
        let genome = GenomeV2::ancestor_from_config(&v2_config()).unwrap();
        assert_eq!(genome.color_r().unwrap(), 0x00);
        assert_eq!(genome.color_g().unwrap(), 0xFF);
        assert_eq!(genome.color_b().unwrap(), 0x05);
        assert_eq!(genome.body_radius_raw().unwrap(), 0x000A);
        assert_eq!(genome.armor_raw().unwrap(), 0);
        assert_eq!(genome.healing_rate_raw().unwrap(), 0);
        assert_eq!(genome.energy_health_ratio_raw().unwrap(), 0x80);
        assert_eq!(genome.repro_threshold_raw().unwrap(), 0xFF);
        assert_eq!(genome.offspring_endowment_raw().unwrap(), 0x0F);
        assert_eq!(genome.max_speed_raw().unwrap(), 0);
        assert_eq!(genome.photo_efficiency_raw().unwrap(), 0xFF);
        assert_eq!(genome.biomass_efficiency_raw().unwrap(), 0);
        assert_eq!(genome.minerals_efficiency_raw().unwrap(), 0);
        assert_eq!(genome.pursue_slot_count().unwrap(), 0);
        assert_eq!(genome.avoid_slot_count().unwrap(), 0);
    }

    #[test]
    fn core_chromosomes_match_doc_grouping() {
        let genome = GenomeV2::ancestor_from_config(&v2_config()).unwrap();
        let groups: Vec<_> = genome
            .core_chromosomes_hex()
            .into_iter()
            .map(|(label, hex)| format!("{label} {hex}"))
            .collect();
        assert_eq!(
            groups,
            [
                "C 00ff05",
                "B 000a",
                "A 0000",
                "H 000080",
                "R ff0f",
                "M 00000000",
                "E ff0000",
                "S 0000",
                "I 0000",
            ]
        );
    }
}

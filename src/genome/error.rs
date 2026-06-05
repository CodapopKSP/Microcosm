//! Genome validation and parse errors.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenomeError {
    InvalidMagic,
    VersionMismatch { expected: u8, stored: u8 },
    LengthMismatch { expected: usize, actual: usize },
    CoreSegmentTooShort { need: usize, have: usize },
    LocusOutOfBounds { offset: usize, width: usize },
    InvalidAndArity { arity: u8 },
    SlotPayloadOverflow { need: usize, have: usize },
    SlotCountExceedsConfig { pursue: u8, avoid: u8 },
    Config(String),
}

impl fmt::Display for GenomeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GenomeError::InvalidMagic => write!(f, "genome header magic is invalid"),
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
            GenomeError::InvalidAndArity { arity } => {
                write!(f, "AND signature arity {arity} is out of range (1..=4)")
            }
            GenomeError::SlotPayloadOverflow { need, have } => write!(
                f,
                "pursue/avoid slot payload needs {need} bytes but genome has {have}"
            ),
            GenomeError::SlotCountExceedsConfig { pursue, avoid } => write!(
                f,
                "pursue/avoid slot counts ({pursue}, {avoid}) exceed config limits"
            ),
            GenomeError::Config(msg) => write!(f, "genome config error: {msg}"),
        }
    }
}

impl std::error::Error for GenomeError {}

impl From<crate::config::ConfigError> for GenomeError {
    fn from(err: crate::config::ConfigError) -> Self {
        GenomeError::Config(err.to_string())
    }
}

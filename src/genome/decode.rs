//! Dispatch genome / phenotype decode by `genetics.genome_version`.

use crate::config::{GENOME_VERSION_V1, GENOME_VERSION_V2, SimConfig};
use crate::genome::encoding::{Genome, GenomeError};
use crate::genome::encoding_v2::GenomeV2;
use crate::genome::phenotype::{DecodeError, Phenotype};
use crate::genome::phenotype_v2::{DecodeErrorV2, PhenotypeV2};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnyGenome {
    V1(Genome),
    V2(GenomeV2),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AnyPhenotype {
    V1(Phenotype),
    V2(PhenotypeV2),
}

impl AnyGenome {
    pub fn clone_genome(&self) -> Self {
        match self {
            AnyGenome::V1(g) => AnyGenome::V1(g.clone_genome()),
            AnyGenome::V2(g) => AnyGenome::V2(g.clone_genome()),
        }
    }

    pub fn v1(&self) -> Option<&Genome> {
        match self {
            AnyGenome::V1(g) => Some(g),
            AnyGenome::V2(_) => None,
        }
    }

    pub fn v2(&self) -> Option<&GenomeV2> {
        match self {
            AnyGenome::V2(g) => Some(g),
            AnyGenome::V1(_) => None,
        }
    }
}

impl AnyPhenotype {
    pub fn v1(&self) -> Option<&Phenotype> {
        match self {
            AnyPhenotype::V1(p) => Some(p),
            AnyPhenotype::V2(_) => None,
        }
    }

    pub fn v2(&self) -> Option<&PhenotypeV2> {
        match self {
            AnyPhenotype::V2(p) => Some(p),
            AnyPhenotype::V1(_) => None,
        }
    }

    pub fn body_radius(&self) -> f32 {
        match self {
            AnyPhenotype::V1(p) => p.body_radius,
            AnyPhenotype::V2(p) => p.body_radius,
        }
    }

    pub fn max_speed(&self) -> f32 {
        match self {
            AnyPhenotype::V1(p) => p.max_speed,
            AnyPhenotype::V2(p) => p.max_speed_effective,
        }
    }

    pub fn color(&self) -> [f32; 3] {
        match self {
            AnyPhenotype::V1(p) => p.color,
            AnyPhenotype::V2(p) => p.color,
        }
    }

    /// Energy required before reproduction can trigger (v2: fraction of max energy).
    pub fn repro_threshold_energy(&self) -> f32 {
        match self {
            AnyPhenotype::V1(p) => p.repro_threshold,
            AnyPhenotype::V2(p) => p.max_energy * p.repro_threshold_frac,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnyDecodeError {
    V1(DecodeError),
    V2(DecodeErrorV2),
    UnsupportedVersion(u8),
}

impl std::fmt::Display for AnyDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnyDecodeError::V1(e) => write!(f, "{e}"),
            AnyDecodeError::V2(e) => write!(f, "{e}"),
            AnyDecodeError::UnsupportedVersion(v) => {
                write!(f, "unsupported genetics.genome_version {v}")
            }
        }
    }
}

impl std::error::Error for AnyDecodeError {}

pub fn genome_from_bytes(bytes: Vec<u8>, config: &SimConfig) -> Result<AnyGenome, GenomeError> {
    match config.genetics.genome_version {
        GENOME_VERSION_V1 => Ok(AnyGenome::V1(Genome::from_bytes(
            bytes,
            config.genetics.genome_version,
            config.genetics.core_length,
        )?)),
        GENOME_VERSION_V2 => Ok(AnyGenome::V2(GenomeV2::from_bytes(
            bytes,
            &config.genetics,
        )?)),
        v => Err(GenomeError::Config(format!(
            "unsupported genetics.genome_version {v}"
        ))),
    }
}

pub fn ancestor_from_config(config: &SimConfig) -> Result<AnyGenome, GenomeError> {
    match config.genetics.genome_version {
        GENOME_VERSION_V1 => Ok(AnyGenome::V1(Genome::ancestor_from_config(config)?)),
        GENOME_VERSION_V2 => Ok(AnyGenome::V2(GenomeV2::ancestor_from_config(config)?)),
        v => Err(GenomeError::Config(format!(
            "unsupported genetics.genome_version {v}"
        ))),
    }
}

pub fn decode_phenotype(genome: &AnyGenome, config: &SimConfig) -> Result<AnyPhenotype, AnyDecodeError> {
    match genome {
        AnyGenome::V1(g) => crate::genome::phenotype::decode(g, config)
            .map(AnyPhenotype::V1)
            .map_err(AnyDecodeError::V1),
        AnyGenome::V2(g) => crate::genome::phenotype_v2::decode(g, config)
            .map(AnyPhenotype::V2)
            .map_err(AnyDecodeError::V2),
    }
}

/// Decode v1 genome (explicit entry for existing call sites).
pub fn decode(genome: &Genome, config: &SimConfig) -> Result<Phenotype, DecodeError> {
    crate::genome::phenotype::decode(genome, config)
}

/// Decode v2 genome.
pub fn decode_v2(genome: &GenomeV2, config: &SimConfig) -> Result<PhenotypeV2, DecodeErrorV2> {
    crate::genome::phenotype_v2::decode(genome, config)
}

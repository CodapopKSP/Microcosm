//! Genome v2 decode and load helpers.

use crate::config::{GENOME_VERSION_V2, SimConfig};
use crate::genome::encoding_v2::GenomeV2;
use crate::genome::error::GenomeError;
use crate::genome::phenotype_v2::{DecodeErrorV2, PhenotypeV2};

pub type AnyGenome = GenomeV2;
pub type AnyPhenotype = PhenotypeV2;
pub type AnyDecodeError = DecodeErrorV2;

pub fn genome_from_bytes(bytes: Vec<u8>, config: &SimConfig) -> Result<AnyGenome, GenomeError> {
    if config.genetics.genome_version != GENOME_VERSION_V2 {
        return Err(GenomeError::Config(format!(
            "unsupported genetics.genome_version {} (only v2 supported)",
            config.genetics.genome_version
        )));
    }
    GenomeV2::from_bytes(bytes, &config.genetics)
}

pub fn ancestor_from_config(config: &SimConfig) -> Result<AnyGenome, GenomeError> {
    if config.genetics.genome_version != GENOME_VERSION_V2 {
        return Err(GenomeError::Config(format!(
            "unsupported genetics.genome_version {} (only v2 supported)",
            config.genetics.genome_version
        )));
    }
    GenomeV2::ancestor_from_config(config)
}

pub fn decode_phenotype(
    genome: &AnyGenome,
    config: &SimConfig,
) -> Result<AnyPhenotype, AnyDecodeError> {
    crate::genome::phenotype_v2::decode(genome, config)
}

pub fn decode_v2(genome: &GenomeV2, config: &SimConfig) -> Result<PhenotypeV2, DecodeErrorV2> {
    crate::genome::phenotype_v2::decode(genome, config)
}

pub fn phenotype_body_radius(p: &AnyPhenotype) -> f32 {
    p.body_radius
}

pub fn phenotype_max_speed(p: &AnyPhenotype) -> f32 {
    p.max_speed_effective
}

pub fn phenotype_color(p: &AnyPhenotype) -> [f32; 3] {
    p.color
}

pub fn phenotype_repro_threshold_energy(p: &AnyPhenotype) -> f32 {
    p.max_energy * p.repro_threshold_frac
}

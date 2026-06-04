//! Genome v2 mutation operators (core + variable payload bytes).

use rand::Rng;

use crate::config::GeneticsConfig;
use crate::genome::encoding_v2::{GenomeV2, HEADER_LEN};
use crate::genome::mutation::nibble_step_byte;

/// Clone parent and apply `mutations_per_offspring` independent passes.
pub fn mutate_offspring_v2(genome: &GenomeV2, genetics: &GeneticsConfig, rng: &mut impl Rng) -> GenomeV2 {
    let mut child = genome.clone_genome();
    for _ in 0..genetics.mutations_per_offspring {
        apply_one_mutation_pass_v2(&mut child, genetics, rng);
    }
    child
}

fn apply_one_mutation_pass_v2(genome: &mut GenomeV2, genetics: &GeneticsConfig, rng: &mut impl Rng) {
    let roll = rng.gen::<f32>();
    let mut threshold = 0.0;

    threshold += genetics.p_bit_flip;
    if roll < threshold {
        bit_flip_v2(genome, rng);
        return;
    }

    threshold += genetics.p_point;
    if roll < threshold {
        nibble_step_v2(genome, rng);
        return;
    }

    threshold += genetics.p_resample;
    if roll < threshold {
        resample_byte_v2(genome, rng);
    }
}

fn mutable_byte_range(genome: &GenomeV2) -> std::ops::Range<usize> {
    HEADER_LEN..genome.bytes.len()
}

fn bit_flip_v2(genome: &mut GenomeV2, rng: &mut impl Rng) {
    let range = mutable_byte_range(genome);
    if range.is_empty() {
        return;
    }
    let idx = rng.gen_range(range);
    let bit = 1_u8 << rng.gen_range(0..8);
    genome.bytes[idx] ^= bit;
}

fn nibble_step_v2(genome: &mut GenomeV2, rng: &mut impl Rng) {
    let range = mutable_byte_range(genome);
    if range.is_empty() {
        return;
    }
    let idx = rng.gen_range(range);
    genome.bytes[idx] = nibble_step_byte(genome.bytes[idx], rng);
}

fn resample_byte_v2(genome: &mut GenomeV2, rng: &mut impl Rng) {
    let range = mutable_byte_range(genome);
    if range.is_empty() {
        return;
    }
    let idx = rng.gen_range(range);
    genome.bytes[idx] = rng.gen::<u8>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use crate::config::SimConfig;
    use crate::genome::encoding_v2::MIN_GENOME_LEN;
    use crate::genome::GenomeV2;

    fn v2_config() -> SimConfig {
        SimConfig::load_from_path(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs/genome_v2.example.toml"),
        )
        .unwrap()
    }

    #[test]
    fn mutate_offspring_v2_changes_bytes() {
        let config = v2_config();
        let parent = GenomeV2::ancestor_from_config(&config).unwrap();
        let mut rng = StdRng::seed_from_u64(42);
        let child = mutate_offspring_v2(&parent, &config.genetics, &mut rng);
        assert_ne!(parent.bytes, child.bytes);
        assert!(child.is_valid(&config.genetics));
    }

    #[test]
    fn mutated_child_often_stays_valid_for_ancestor() {
        let config = v2_config();
        let parent = GenomeV2::ancestor_from_config(&config).unwrap();
        let mut rng = StdRng::seed_from_u64(99);
        let mut valid = 0;
        for _ in 0..30 {
            let child = mutate_offspring_v2(&parent, &config.genetics, &mut rng);
            assert!(child.bytes.len() >= MIN_GENOME_LEN);
            if child.is_valid(&config.genetics) {
                valid += 1;
            }
        }
        assert!(
            valid > 0,
            "at least some offspring mutations should remain valid for fixed-core ancestor"
        );
    }
}

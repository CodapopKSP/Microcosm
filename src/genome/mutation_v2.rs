//! Genome v2 gene-level ±increment mutation (no bit flips or resample).

use rand::Rng;

use crate::config::{GeneticsConfig, MutationV2Config};
use crate::genome::encoding_v2::{core, GenomeV2, HEADER_LEN};
use crate::genome::error::GenomeError;
use crate::genome::signatures::slot_payload_len;

/// Clone parent and apply `passes_min..=passes_max` independent gene increments.
pub fn mutate_offspring_v2(genome: &GenomeV2, genetics: &GeneticsConfig, rng: &mut impl Rng) -> GenomeV2 {
    let mut child = genome.clone_genome();
    let cfg = genetics
        .mutation_v2
        .as_ref()
        .expect("mutation_v2 required for v2 offspring");
    let passes = if cfg.passes_min == cfg.passes_max {
        cfg.passes_min
    } else {
        rng.gen_range(cfg.passes_min..=cfg.passes_max)
    };
    for _ in 0..passes {
        apply_one_gene_increment(&mut child, genetics, cfg, rng);
    }
    child
}

fn apply_one_gene_increment(
    genome: &mut GenomeV2,
    genetics: &GeneticsConfig,
    cfg: &MutationV2Config,
    rng: &mut impl Rng,
) {
    let genes = match build_gene_registry(genome, genetics) {
        Ok(g) if !g.is_empty() => g,
        _ => return,
    };
    let gene = &genes[rng.gen_range(0..genes.len())];
    let direction = if rng.gen_bool(0.5) { 1 } else { -1 };
    let magnitude = if rng.gen::<f32>() < cfg.p_large_step {
        rng.gen_range(cfg.large_step_min..=cfg.large_step_max) as i32
    } else {
        1
    };
    let delta = direction * magnitude;
    apply_gene_delta(&mut genome.bytes, gene, delta);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MutableGene {
    U8Core(usize),
    U16Core(usize),
    PursueSlotType(usize),
    PursueSlotValue(usize),
    AvoidSlotType(usize),
    AvoidSlotValue(usize),
}

fn build_gene_registry(
    genome: &GenomeV2,
    genetics: &GeneticsConfig,
) -> Result<Vec<MutableGene>, GenomeError> {
    let mut genes = Vec::new();
    for off in [
        core::COLOR_R,
        core::COLOR_G,
        core::COLOR_B,
        core::ENERGY_HEALTH_RATIO,
        core::REPRO_THRESHOLD,
        core::OFFSPRING_ENDOWMENT,
        core::MOVEMENT_DESIRE,
        core::TURNING_DESIRE,
        core::MAX_SPEED,
        core::MOVEMENT_EFFICIENCY,
        core::PHOTO_EFFICIENCY,
        core::BIOMASS_EFFICIENCY,
        core::MINERALS_EFFICIENCY,
        core::SENSOR_MASK,
        core::SENSOR_STRENGTH,
        core::PURSUE_SLOT_COUNT,
        core::AVOID_SLOT_COUNT,
    ] {
        genes.push(MutableGene::U8Core(off));
    }
    for off in [core::BODY_RADIUS, core::ARMOR, core::HEALING_RATE] {
        genes.push(MutableGene::U16Core(off));
    }

    let pursue_n = genome.pursue_slot_count()?;
    let avoid_n = genome.avoid_slot_count()?;
    let mut offset = HEADER_LEN + crate::genome::encoding_v2::CORE_FIXED_LEN;
    for i in 0..pursue_n {
        genes.push(MutableGene::PursueSlotType(usize::from(i)));
        genes.push(MutableGene::PursueSlotValue(usize::from(i)));
        let len = slot_payload_len(&genome.bytes, offset)?;
        offset += len;
    }
    for i in 0..avoid_n {
        genes.push(MutableGene::AvoidSlotType(usize::from(i)));
        genes.push(MutableGene::AvoidSlotValue(usize::from(i)));
        let len = slot_payload_len(&genome.bytes, offset)?;
        offset += len;
    }

    let _ = genetics;
    Ok(genes)
}

fn resolve_slot_offsets(
    bytes: &[u8],
    pursue_count: u8,
    avoid_count: u8,
    is_pursue: bool,
    slot_index: usize,
) -> Result<(usize, usize), GenomeError> {
    let core_start = HEADER_LEN + crate::genome::encoding_v2::CORE_FIXED_LEN;
    let mut offset = core_start;
    let pursue = pursue_count as usize;
    if is_pursue {
        for i in 0..pursue {
            if i == slot_index {
                let type_off = offset;
                let value_off = offset + 2;
                return Ok((type_off, value_off));
            }
            offset += slot_payload_len(bytes, offset)?;
        }
    } else {
        for _ in 0..pursue {
            offset += slot_payload_len(bytes, offset)?;
        }
        let avoid = avoid_count as usize;
        for i in 0..avoid {
            if i == slot_index {
                let type_off = offset;
                let value_off = offset + 2;
                return Ok((type_off, value_off));
            }
            offset += slot_payload_len(bytes, offset)?;
        }
    }
    Err(GenomeError::LocusOutOfBounds {
        offset: slot_index,
        width: 1,
    })
}

fn apply_gene_delta(bytes: &mut [u8], gene: &MutableGene, delta: i32) {
    match gene {
        MutableGene::U8Core(off) => {
            let idx = HEADER_LEN + off;
            if idx < bytes.len() {
                step_u8(bytes, idx, delta);
            }
        }
        MutableGene::U16Core(off) => {
            let idx = HEADER_LEN + off;
            if idx + 1 < bytes.len() {
                step_u16_be(bytes, idx, delta);
            }
        }
        MutableGene::PursueSlotType(idx) | MutableGene::AvoidSlotType(idx) => {
            let pursue = bytes.get(HEADER_LEN + core::PURSUE_SLOT_COUNT).copied().unwrap_or(0);
            let avoid = bytes.get(HEADER_LEN + core::AVOID_SLOT_COUNT).copied().unwrap_or(0);
            let is_pursue = matches!(gene, MutableGene::PursueSlotType(_));
            if let Ok((type_off, _)) =
                resolve_slot_offsets(bytes, pursue, avoid, is_pursue, *idx)
            {
                step_u16_be(bytes, type_off, delta);
            }
        }
        MutableGene::PursueSlotValue(idx) | MutableGene::AvoidSlotValue(idx) => {
            let pursue = bytes.get(HEADER_LEN + core::PURSUE_SLOT_COUNT).copied().unwrap_or(0);
            let avoid = bytes.get(HEADER_LEN + core::AVOID_SLOT_COUNT).copied().unwrap_or(0);
            let is_pursue = matches!(gene, MutableGene::PursueSlotValue(_));
            if let Ok((_, value_off)) =
                resolve_slot_offsets(bytes, pursue, avoid, is_pursue, *idx)
            {
                step_u32_be(bytes, value_off, delta);
            }
        }
    }
}

fn step_u8(bytes: &mut [u8], idx: usize, delta: i32) {
    let v = bytes[idx] as i32;
    bytes[idx] = (v + delta).clamp(0, 255) as u8;
}

fn step_u16_be(bytes: &mut [u8], idx: usize, delta: i32) {
    let v = u16::from_be_bytes([bytes[idx], bytes[idx + 1]]) as i32;
    let new = (v + delta).clamp(0, 65535) as u16;
    bytes[idx..idx + 2].copy_from_slice(&new.to_be_bytes());
}

fn step_u32_be(bytes: &mut [u8], idx: usize, delta: i32) {
    let v = u32::from_be_bytes([
        bytes[idx],
        bytes[idx + 1],
        bytes[idx + 2],
        bytes[idx + 3],
    ]) as i64;
    let new = (v + i64::from(delta)).clamp(0, u32::MAX as i64) as u32;
    bytes[idx..idx + 4].copy_from_slice(&new.to_be_bytes());
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

    fn no_mutation_cfg(config: &mut SimConfig) {
        config.genetics.mutation_v2 = Some(MutationV2Config {
            passes_min: 0,
            passes_max: 0,
            p_large_step: 0.0,
            large_step_min: 3,
            large_step_max: 4,
        });
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
    fn pass_count_within_config_bounds() {
        let mut config = v2_config();
        config.genetics.mutation_v2 = Some(MutationV2Config {
            passes_min: 0,
            passes_max: 4,
            p_large_step: 0.0,
            large_step_min: 3,
            large_step_max: 4,
        });
        let parent = GenomeV2::ancestor_from_config(&config).unwrap();
        let mut rng = StdRng::seed_from_u64(7);
        for seed in 0..50_u64 {
            let mut r = StdRng::seed_from_u64(seed);
            let before = parent.bytes.clone();
            let child = mutate_offspring_v2(&parent, &config.genetics, &mut r);
            let diff_count = before
                .iter()
                .zip(child.bytes.iter())
                .filter(|(a, b)| a != b)
                .count();
            assert!(diff_count <= 4, "at most one gene region per pass");
            let _ = rng.gen::<u32>();
        }
    }

    #[test]
    fn u8_gene_clamps_at_bounds() {
        let mut bytes = vec![0u8; HEADER_LEN + core::COLOR_R + 1];
        bytes[HEADER_LEN + core::COLOR_R] = 255;
        step_u8(&mut bytes, HEADER_LEN + core::COLOR_R, 5);
        assert_eq!(bytes[HEADER_LEN + core::COLOR_R], 255);
        step_u8(&mut bytes, HEADER_LEN + core::COLOR_R, -300);
        assert_eq!(bytes[HEADER_LEN + core::COLOR_R], 0);
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

    #[test]
    fn zero_passes_leaves_genome_unchanged() {
        let mut config = v2_config();
        no_mutation_cfg(&mut config);
        let parent = GenomeV2::ancestor_from_config(&config).unwrap();
        let mut rng = StdRng::seed_from_u64(1);
        let child = mutate_offspring_v2(&parent, &config.genetics, &mut rng);
        assert_eq!(parent.bytes, child.bytes);
    }

    /// EMR-F4: green sessile ancestor can gain movement genes via mutation (color does not cap).
    #[test]
    fn mutation_can_raise_movement_from_sedentary_green_ancestor() {
        use crate::genome::v2_core;

        let config = SimConfig::load_default().unwrap();
        let parent = GenomeV2::ancestor_from_config(&config).unwrap();
        assert_eq!(parent.movement_desire_raw().unwrap(), 0);
        assert_eq!(parent.max_speed_raw().unwrap(), 0);
        assert_eq!(parent.read_u8_at_core(v2_core::COLOR_G).unwrap(), 0xFF);

        let mut raised = false;
        for seed in 0..500_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let child = mutate_offspring_v2(&parent, &config.genetics, &mut rng);
            if child.movement_desire_raw().unwrap() > 0
                || child.max_speed_raw().unwrap() > 0
                || child.movement_efficiency_raw().unwrap() > 0
            {
                raised = true;
                break;
            }
        }
        assert!(
            raised,
            "mutation_v2 should raise movement genes from default green sessile ancestor"
        );
    }
}

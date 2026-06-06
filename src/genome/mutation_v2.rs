//! Genome v2 gene-level ±increment mutation (no bit flips or resample).

use rand::Rng;

use crate::config::{GeneticsConfig, MutationV2Config};
use crate::genome::encoding_v2::{core, GenomeV2, EFFICIENCY_TRIAD_BUDGET, HEADER_LEN};
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
    let mut genes = match build_gene_registry(&child, genetics) {
        Ok(g) => g,
        Err(_) => Vec::new(),
    };
    let mut slot_counts = slot_count_pair(&child);
    for _ in 0..passes {
        if genes.is_empty() {
            break;
        }
        apply_one_gene_increment(&mut child, &genes, cfg, rng);
        let updated = slot_count_pair(&child);
        if updated != slot_counts {
            slot_counts = updated;
            genes = match build_gene_registry(&child, genetics) {
                Ok(g) => g,
                Err(_) => Vec::new(),
            };
        }
    }
    let _ = child.sync_color_from_efficiency();
    child
}

fn slot_count_pair(genome: &GenomeV2) -> (u8, u8) {
    (
        genome.pursue_slot_count().unwrap_or(0),
        genome.avoid_slot_count().unwrap_or(0),
    )
}

fn apply_one_gene_increment(
    genome: &mut GenomeV2,
    genes: &[MutableGene],
    cfg: &MutationV2Config,
    rng: &mut impl Rng,
) {
    if genes.is_empty() {
        return;
    }
    let gene = &genes[rng.gen_range(0..genes.len())];
    let direction = if rng.gen_bool(0.5) { 1 } else { -1 };
    let magnitude = if rng.gen::<f32>() < cfg.p_large_step {
        rng.gen_range(cfg.large_step_min..=cfg.large_step_max) as i32
    } else {
        1
    };
    let delta = flip_delta_if_at_floor(&genome.bytes, gene, direction * magnitude);
    apply_gene_delta(&mut genome.bytes, gene, delta, rng);
}

/// Genes at minimum (0) cannot take a downward step — flip to upward so floor hits always move.
fn flip_delta_if_at_floor(bytes: &[u8], gene: &MutableGene, delta: i32) -> i32 {
    if delta < 0 && gene_at_floor(bytes, gene) {
        -delta
    } else {
        delta
    }
}

fn gene_at_floor(bytes: &[u8], gene: &MutableGene) -> bool {
    match gene {
        MutableGene::U8Core(off) => {
            let idx = HEADER_LEN + off;
            bytes.get(idx).is_some_and(|&b| b == 0)
        }
        MutableGene::U16Core(off) => {
            let idx = HEADER_LEN + off;
            (idx + 1 < bytes.len())
                .then(|| u16::from_be_bytes([bytes[idx], bytes[idx + 1]]))
                .is_some_and(|v| v == 0)
        }
        MutableGene::PursueSlotType(idx) | MutableGene::AvoidSlotType(idx) => {
            slot_type_at_floor(bytes, *idx, matches!(gene, MutableGene::PursueSlotType(_)))
        }
        MutableGene::PursueSlotValue(idx) | MutableGene::AvoidSlotValue(idx) => {
            slot_value_at_floor(bytes, *idx, matches!(gene, MutableGene::PursueSlotValue(_)))
        }
    }
}

fn slot_type_at_floor(bytes: &[u8], slot_index: usize, is_pursue: bool) -> bool {
    let pursue = bytes.get(HEADER_LEN + core::PURSUE_SLOT_COUNT).copied().unwrap_or(0);
    let avoid = bytes.get(HEADER_LEN + core::AVOID_SLOT_COUNT).copied().unwrap_or(0);
    resolve_slot_offsets(bytes, pursue, avoid, is_pursue, slot_index)
        .ok()
        .and_then(|(type_off, _)| {
            (type_off + 1 < bytes.len())
                .then(|| u16::from_be_bytes([bytes[type_off], bytes[type_off + 1]]))
        })
        .is_some_and(|v| v == 0)
}

fn slot_value_at_floor(bytes: &[u8], slot_index: usize, is_pursue: bool) -> bool {
    let pursue = bytes.get(HEADER_LEN + core::PURSUE_SLOT_COUNT).copied().unwrap_or(0);
    let avoid = bytes.get(HEADER_LEN + core::AVOID_SLOT_COUNT).copied().unwrap_or(0);
    resolve_slot_offsets(bytes, pursue, avoid, is_pursue, slot_index)
        .ok()
        .and_then(|(_, value_off)| {
            (value_off + 3 < bytes.len()).then(|| {
                u32::from_be_bytes([
                    bytes[value_off],
                    bytes[value_off + 1],
                    bytes[value_off + 2],
                    bytes[value_off + 3],
                ])
            })
        })
        .is_some_and(|v| v == 0)
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

const EFFICIENCY_TRIAD_CORE: [usize; 3] = [
    core::PHOTO_EFFICIENCY,
    core::BIOMASS_EFFICIENCY,
    core::MINERALS_EFFICIENCY,
];

fn efficiency_triad_leg(core_off: usize) -> Option<usize> {
    EFFICIENCY_TRIAD_CORE
        .iter()
        .position(|&off| off == core_off)
}

fn apply_gene_delta(bytes: &mut [u8], gene: &MutableGene, delta: i32, rng: &mut impl Rng) {
    match gene {
        MutableGene::U8Core(off) => {
            if efficiency_triad_leg(*off).is_some() {
                step_efficiency_triad(bytes, *off, delta, rng);
            } else {
                let idx = HEADER_LEN + off;
                if idx < bytes.len() {
                    step_u8(bytes, idx, delta);
                }
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

/// Shift budget between photo / biomass / minerals while keeping their sum at 255.
fn step_efficiency_triad(bytes: &mut [u8], core_off: usize, delta: i32, rng: &mut impl Rng) {
    let Some(target) = efficiency_triad_leg(core_off) else {
        return;
    };
    let idxs: [usize; 3] = EFFICIENCY_TRIAD_CORE.map(|off| HEADER_LEN + off);
    let mut vals = [bytes[idxs[0]], bytes[idxs[1]], bytes[idxs[2]]];
    debug_assert_eq!(
        u16::from(vals[0]) + u16::from(vals[1]) + u16::from(vals[2]),
        EFFICIENCY_TRIAD_BUDGET
    );

    if delta > 0 {
        let others_sum: u16 = (0..3)
            .filter(|&i| i != target)
            .map(|i| u16::from(vals[i]))
            .sum();
        let max_up = (u16::from(255u8) - u16::from(vals[target]))
            .min(others_sum)
            .min(delta as u16);
        if max_up == 0 {
            return;
        }
        vals[target] += max_up as u8;
        take_from_others(&mut vals, target, max_up, rng);
    } else if delta < 0 {
        let max_down = u16::from(vals[target]).min((-delta) as u16);
        if max_down == 0 {
            return;
        }
        vals[target] -= max_down as u8;
        give_to_others(&mut vals, target, max_down, rng);
    }

    debug_assert_eq!(
        u16::from(vals[0]) + u16::from(vals[1]) + u16::from(vals[2]),
        EFFICIENCY_TRIAD_BUDGET
    );
    for (i, &idx) in idxs.iter().enumerate() {
        bytes[idx] = vals[i];
    }
}

fn take_from_others(vals: &mut [u8; 3], exclude: usize, amount: u16, rng: &mut impl Rng) {
    let mut remaining = amount;
    while remaining > 0 {
        let donors: Vec<usize> = (0..3)
            .filter(|&i| i != exclude && vals[i] > 0)
            .collect();
        if donors.is_empty() {
            break;
        }
        let pick = donors[rng.gen_range(0..donors.len())];
        let take = remaining.min(u16::from(vals[pick]));
        vals[pick] -= take as u8;
        remaining -= take;
    }
}

fn give_to_others(vals: &mut [u8; 3], exclude: usize, amount: u16, rng: &mut impl Rng) {
    let mut remaining = amount;
    while remaining > 0 {
        let receivers: Vec<usize> = (0..3)
            .filter(|&i| i != exclude && vals[i] < 255)
            .collect();
        if receivers.is_empty() {
            break;
        }
        let pick = receivers[rng.gen_range(0..receivers.len())];
        let room = u16::from(255u8) - u16::from(vals[pick]);
        let give = remaining.min(room);
        vals[pick] += give as u8;
        remaining -= give;
    }
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
    use crate::genome::error::GenomeError;
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
            // Triad / u16 / u32 genes can touch multiple bytes in a single pass.
            let max_passes = config.genetics.mutation_v2.as_ref().unwrap().passes_max as usize;
            assert!(
                diff_count <= max_passes * 4,
                "bounded byte diffs per offspring (got {diff_count})"
            );
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

    fn efficiency_triad_sum(bytes: &[u8]) -> u16 {
        EFFICIENCY_TRIAD_CORE
            .iter()
            .map(|&off| u16::from(bytes[HEADER_LEN + off]))
            .sum()
    }

    #[test]
    fn efficiency_triad_mutation_preserves_budget() {
        let config = v2_config();
        let parent = GenomeV2::ancestor_from_config(&config).unwrap();
        assert_eq!(efficiency_triad_sum(&parent.bytes), EFFICIENCY_TRIAD_BUDGET);
        let mut rng = StdRng::seed_from_u64(123);
        for _ in 0..200 {
            let child = mutate_offspring_v2(&parent, &config.genetics, &mut rng);
            assert_eq!(
                efficiency_triad_sum(&child.bytes),
                EFFICIENCY_TRIAD_BUDGET,
                "E chromosome must always sum to FF"
            );
        }
    }

    #[test]
    fn efficiency_triad_increase_steals_from_others() {
        let config = v2_config();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        let mut rng = StdRng::seed_from_u64(1);
        step_efficiency_triad(
            &mut bytes,
            core::BIOMASS_EFFICIENCY,
            10,
            &mut rng,
        );
        assert_eq!(bytes[HEADER_LEN + core::PHOTO_EFFICIENCY], 245);
        assert_eq!(bytes[HEADER_LEN + core::BIOMASS_EFFICIENCY], 10);
        assert_eq!(bytes[HEADER_LEN + core::MINERALS_EFFICIENCY], 0);
        assert_eq!(efficiency_triad_sum(&bytes), EFFICIENCY_TRIAD_BUDGET);
    }

    #[test]
    fn invalid_efficiency_triad_sum_rejected() {
        let config = v2_config();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        bytes[HEADER_LEN + core::PHOTO_EFFICIENCY] = 0xFF;
        bytes[HEADER_LEN + core::BIOMASS_EFFICIENCY] = 0x05;
        bytes[HEADER_LEN + core::MINERALS_EFFICIENCY] = 0x10;
        let genome = GenomeV2::from_bytes(bytes, &config.genetics);
        assert!(matches!(
            genome,
            Err(GenomeError::EfficiencyTriadSumInvalid { sum: 276 })
        ));
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

    /// Default-config mutation rates: B/M are hit less often than E (triad reshuffles every photo− step).
    #[test]
    fn default_config_body_and_movement_mutation_rates() {
        let config = SimConfig::load_default().unwrap();
        let parent = GenomeV2::ancestor_from_config(&config).unwrap();
        let parent_body = parent.body_radius_raw().unwrap();
        let parent_move = (
            parent.movement_desire_raw().unwrap(),
            parent.turning_desire_raw().unwrap(),
            parent.max_speed_raw().unwrap(),
            parent.movement_efficiency_raw().unwrap(),
        );
        let parent_e = (
            parent.photo_efficiency_raw().unwrap(),
            parent.biomass_efficiency_raw().unwrap(),
            parent.minerals_efficiency_raw().unwrap(),
        );

        let n = 10_000_u32;
        let mut body_changed = 0_u32;
        let mut move_changed = 0_u32;
        let mut e_changed = 0_u32;
        for seed in 0..n {
            let mut rng = StdRng::seed_from_u64(u64::from(seed));
            let child = mutate_offspring_v2(&parent, &config.genetics, &mut rng);
            if child.body_radius_raw().unwrap() != parent_body {
                body_changed += 1;
            }
            let cm = (
                child.movement_desire_raw().unwrap(),
                child.turning_desire_raw().unwrap(),
                child.max_speed_raw().unwrap(),
                child.movement_efficiency_raw().unwrap(),
            );
            if cm != parent_move {
                move_changed += 1;
            }
            let ce = (
                child.photo_efficiency_raw().unwrap(),
                child.biomass_efficiency_raw().unwrap(),
                child.minerals_efficiency_raw().unwrap(),
            );
            if ce != parent_e {
                e_changed += 1;
            }
        }

        let max_speed_positive = (0..n)
            .filter(|seed| {
                let mut rng = StdRng::seed_from_u64(u64::from(*seed));
                mutate_offspring_v2(&parent, &config.genetics, &mut rng)
                    .max_speed_raw()
                    .unwrap()
                    > 0
            })
            .count();

        assert!(
            body_changed > n / 50,
            "body chromosome should change often (got {body_changed}/{n})"
        );
        assert!(
            move_changed > n / 100,
            "movement chromosome should change sometimes (got {move_changed}/{n})"
        );
        assert!(
            e_changed > n / 5,
            "E triad should change very frequently (got {e_changed}/{n})"
        );
        assert!(
            max_speed_positive > 0,
            "max_speed>0 should appear in mutations (got {max_speed_positive}/{n}); \
             if sim pop stays sessile, selection is filtering locomotion"
        );
    }

    #[test]
    fn floor_flips_negative_step_on_zero_byte() {
        let mut bytes = vec![0u8; HEADER_LEN + core::MAX_SPEED + 1];
        let gene = MutableGene::U8Core(core::MAX_SPEED);
        assert!(gene_at_floor(&bytes, &gene));
        let delta = flip_delta_if_at_floor(&bytes, &gene, -10);
        assert_eq!(delta, 10);
        apply_gene_delta(&mut bytes, &gene, delta, &mut StdRng::seed_from_u64(1));
        assert_eq!(bytes[HEADER_LEN + core::MAX_SPEED], 10);
    }

    #[test]
    fn floor_flips_negative_triad_step_at_zero() {
        let config = v2_config();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        bytes[HEADER_LEN + core::BIOMASS_EFFICIENCY] = 0;
        let gene = MutableGene::U8Core(core::BIOMASS_EFFICIENCY);
        let delta = flip_delta_if_at_floor(&bytes, &gene, -5);
        assert_eq!(delta, 5);
        let mut rng = StdRng::seed_from_u64(1);
        apply_gene_delta(&mut bytes, &gene, delta, &mut rng);
        assert!(bytes[HEADER_LEN + core::BIOMASS_EFFICIENCY] > 0);
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
        for seed in 0..50_u64 {
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

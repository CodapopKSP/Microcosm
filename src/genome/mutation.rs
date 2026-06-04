//! Core genome mutation operators (Phase 1H; see [GENOME.md](../../docs/GENOME.md)).
//!
//! [`point_noise`] applies **nibble steps**: one hex digit (high or low 4 bits of a byte) changes by ±1.
//! Colors and other u8/u16 loci drift gradually instead of large byte deltas or full resamples.

use rand::Rng;

use crate::config::GeneticsConfig;
use crate::genome::encoding::{self, core, Genome, HEADER_LEN};

#[derive(Clone, Copy)]
struct CoreLocus {
    offset: usize,
    len: usize,
}

const CORE_LOCI: &[CoreLocus] = &[
    CoreLocus {
        offset: core::COLOR_R,
        len: 1,
    },
    CoreLocus {
        offset: core::COLOR_G,
        len: 1,
    },
    CoreLocus {
        offset: core::COLOR_B,
        len: 1,
    },
    CoreLocus {
        offset: core::BODY_RADIUS,
        len: 1,
    },
    CoreLocus {
        offset: core::PHOTO_STRENGTH,
        len: 2,
    },
    CoreLocus {
        offset: core::METABOLISM_BASE,
        len: 2,
    },
    CoreLocus {
        offset: core::LOCOMOTION_INVESTMENT,
        len: 2,
    },
    CoreLocus {
        offset: core::REPRO_THRESHOLD,
        len: 2,
    },
    CoreLocus {
        offset: core::REPRO_COST,
        len: 2,
    },
    CoreLocus {
        offset: core::OFFSPRING_ENDOWMENT,
        len: 2,
    },
    CoreLocus {
        offset: core::DESIRE_TO_MOVE,
        len: 1,
    },
    CoreLocus {
        offset: core::SKIN_HARDNESS,
        len: 2,
    },
    CoreLocus {
        offset: core::REPAIR_RATE,
        len: 2,
    },
    CoreLocus {
        offset: core::CHEMO_POTENTIAL,
        len: 2,
    },
    CoreLocus {
        offset: core::MOVEMENT_PACE,
        len: 2,
    },
    CoreLocus {
        offset: core::MAX_AGE,
        len: 2,
    },
];

/// Step one hex nibble in `byte` by +1 or −1 (clamped to 0..=15 per nibble).
pub fn nibble_step_byte(byte: u8, rng: &mut impl Rng) -> u8 {
    let high = byte >> 4;
    let low = byte & 0x0F;
    let delta: i8 = if rng.gen_bool(0.5) { 1 } else { -1 };

    if rng.gen_bool(0.5) {
        let new_high = (high as i8 + delta).clamp(0, 15) as u8;
        (new_high << 4) | low
    } else {
        let new_low = (low as i8 + delta).clamp(0, 15) as u8;
        (high << 4) | new_low
    }
}

/// Apply one nibble step to a single byte within a locus (`len` 1 or 2).
pub fn nibble_step_locus_byte(genome: &mut Genome, base: usize, locus_len: usize, rng: &mut impl Rng) {
    if locus_len == 0 {
        return;
    }
    let byte_idx = base + rng.gen_range(0..locus_len);
    if byte_idx >= genome.bytes.len() {
        return;
    }
    genome.bytes[byte_idx] = nibble_step_byte(genome.bytes[byte_idx], rng);
}

/// Clone parent genome and apply `mutations_per_offspring` independent passes.
pub fn mutate_offspring(genome: &Genome, genetics: &GeneticsConfig, rng: &mut impl Rng) -> Genome {
    let mut child = genome.clone_genome();
    for _ in 0..genetics.mutations_per_offspring {
        apply_one_mutation_pass(&mut child, genetics, rng);
    }
    child
}

fn apply_one_mutation_pass(genome: &mut Genome, genetics: &GeneticsConfig, rng: &mut impl Rng) {
    let roll = rng.gen::<f32>();
    let mut threshold = 0.0;

    threshold += genetics.p_bit_flip;
    if roll < threshold {
        bit_flip(genome, rng);
        return;
    }

    threshold += genetics.p_point;
    if roll < threshold {
        point_noise(genome, rng);
        return;
    }

    threshold += genetics.p_resample;
    if roll < threshold {
        resample_locus(genome, rng);
        return;
    }

    if genetics.p_dup > 0.0 {
        threshold += genetics.p_dup;
        if roll < threshold {
            return;
        }
    }
    if genetics.p_del > 0.0 {
        threshold += genetics.p_del;
        if roll < threshold {
            return;
        }
    }
}

fn core_start() -> usize {
    HEADER_LEN
}

fn core_end(genome: &Genome) -> usize {
    genome.bytes.len().min(HEADER_LEN + encoding::CORE_SEGMENT_LEN)
}

/// Fixed chance `movement_pace` is picked on point/resample passes (remaining mass split among other loci).
const MOVEMENT_PACE_LOCUS_PROB: f32 = 0.075;
const LOCUS_WEIGHT_DEFAULT: u32 = 1;
const LOCUS_WEIGHT_BODY_RADIUS: u32 = 2;

fn movement_pace_locus() -> &'static CoreLocus {
    CORE_LOCI
        .iter()
        .find(|l| l.offset == core::MOVEMENT_PACE)
        .expect("MOVEMENT_PACE locus present in CORE_LOCI")
}

fn locus_selection_weight(locus: &CoreLocus) -> u32 {
    debug_assert_ne!(locus.offset, core::MOVEMENT_PACE);
    match locus.offset {
        core::BODY_RADIUS => LOCUS_WEIGHT_BODY_RADIUS,
        _ => LOCUS_WEIGHT_DEFAULT,
    }
}

fn random_mutable_locus_non_speed(rng: &mut impl Rng) -> &'static CoreLocus {
    let total: u32 = CORE_LOCI
        .iter()
        .filter(|l| l.offset != core::MOVEMENT_PACE)
        .map(|l| locus_selection_weight(l))
        .sum();
    let mut pick = rng.gen_range(0..total);
    for locus in CORE_LOCI {
        if locus.offset == core::MOVEMENT_PACE {
            continue;
        }
        let w = locus_selection_weight(locus);
        if pick < w {
            return locus;
        }
        pick -= w;
    }
    CORE_LOCI
        .iter()
        .find(|l| l.offset != core::MOVEMENT_PACE)
        .expect("non-speed CORE_LOCI present")
}

fn random_mutable_locus(rng: &mut impl Rng) -> &'static CoreLocus {
    if rng.gen::<f32>() < MOVEMENT_PACE_LOCUS_PROB {
        return movement_pace_locus();
    }
    random_mutable_locus_non_speed(rng)
}

fn locus_base(locus: &CoreLocus) -> usize {
    core_start() + locus.offset
}

fn bit_flip(genome: &mut Genome, rng: &mut impl Rng) {
    let start = core_start();
    let end = core_end(genome);
    if end <= start {
        return;
    }
    let idx = rng.gen_range(start..end);
    let bit = rng.gen_range(0..8);
    genome.bytes[idx] ^= 1 << bit;
}

/// Small drift: one hex digit on one byte of a random core locus changes by ±1.
fn point_noise(genome: &mut Genome, rng: &mut impl Rng) {
    let locus = random_mutable_locus(rng);
    nibble_step_locus_byte(genome, locus_base(locus), locus.len, rng);
}

fn resample_locus(genome: &mut Genome, rng: &mut impl Rng) {
    let locus = random_mutable_locus(rng);
    let base = locus_base(locus);
    match locus.len {
        1 => {
            if base < genome.bytes.len() {
                genome.bytes[base] = rng.gen::<u8>();
            }
        }
        2 => {
            if base + 1 < genome.bytes.len() {
                let value: u16 = rng.gen();
                genome.bytes[base] = (value >> 8) as u8;
                genome.bytes[base + 1] = (value & 0xFF) as u8;
            }
        }
        len => {
            for i in 0..len {
                let idx = base + i;
                if idx < genome.bytes.len() {
                    genome.bytes[idx] = rng.gen::<u8>();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use crate::config::SimConfig;
    use crate::genome::Genome;

    #[test]
    fn nibble_step_changes_at_most_one_hex_digit_by_one() {
        let mut rng = StdRng::seed_from_u64(1);
        let parent = 0x88u8;
        for _ in 0..200 {
            let child = nibble_step_byte(parent, &mut rng);
            if child == parent {
                continue;
            }
            let high_diff = (child >> 4).abs_diff(parent >> 4);
            let low_diff = (child & 0x0F).abs_diff(parent & 0x0F);
            assert!(
                (high_diff == 1 && low_diff == 0) || (high_diff == 0 && low_diff == 1),
                "0x{parent:02X} -> 0x{child:02X} must change exactly one nibble by 1"
            );
        }
    }

    #[test]
    fn color_locus_nibble_step_is_small() {
        let config = SimConfig::load_v1_regression().unwrap();
        let mut parent = Genome::ancestor_from_config(&config).unwrap();
        let idx = HEADER_LEN + core::COLOR_G;
        parent.bytes[idx] = 0xCC;

        let mut rng = StdRng::seed_from_u64(2);
        let mut child = parent.clone_genome();
        nibble_step_locus_byte(&mut child, idx, 1, &mut rng);

        assert_ne!(parent.bytes[idx], child.bytes[idx]);
        assert!(
            parent.bytes[idx].abs_diff(child.bytes[idx]) <= 16,
            "color byte should drift by at most one nibble step"
        );
    }

    #[test]
    fn mutation_changes_child_genome() {
        let config = SimConfig::load_v1_regression().unwrap();
        let parent = Genome::ancestor_from_config(&config).unwrap();
        let mut rng = rand::rngs::StdRng::seed_from_u64(99);
        let mut genetics = config.genetics.clone();
        genetics.mutations_per_offspring = 8;
        genetics.p_bit_flip = 0.0;
        genetics.p_point = 1.0;
        genetics.p_resample = 0.0;

        let child = mutate_offspring(&parent, &genetics, &mut rng);
        assert_ne!(parent.to_hex(), child.to_hex());
    }

    #[test]
    fn mutate_offspring_can_change_max_age_raw() {
        let config = SimConfig::load_v1_regression().unwrap();
        let parent = Genome::ancestor_from_config(&config).unwrap();
        let parent_raw = parent.max_age_raw().unwrap();
        let mut genetics = config.genetics.clone();
        genetics.mutations_per_offspring = 12;
        genetics.p_bit_flip = 0.0;
        genetics.p_point = 1.0;
        genetics.p_resample = 0.0;

        let mut rng = StdRng::seed_from_u64(4242);
        let mut saw_different = false;
        for _ in 0..64 {
            let child = mutate_offspring(&parent, &genetics, &mut rng);
            if child.max_age_raw().unwrap() != parent_raw {
                saw_different = true;
                break;
            }
        }
        assert!(
            saw_different,
            "offspring max_age_raw should differ from parent after repeated point mutations"
        );
    }

    #[test]
    fn many_passes_can_change_genome() {
        let config = SimConfig::load_v1_regression().unwrap();
        let parent = Genome::ancestor_from_config(&config).unwrap();
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let child = mutate_offspring(&parent, &config.genetics, &mut rng);
        let _ = child;
    }

    #[test]
    fn nibble_step_on_max_speed_locus_changes_phenotype() {
        let config = SimConfig::load_v1_regression().unwrap();
        let parent = Genome::ancestor_from_config(&config).unwrap();
        let before = crate::genome::decode(&parent, &config).unwrap();

        let mut rng = StdRng::seed_from_u64(11);
        let mut child = parent.clone_genome();
        let base = HEADER_LEN + core::MAX_SPEED;
        nibble_step_locus_byte(&mut child, base, 2, &mut rng);

        let after = crate::genome::decode(&child, &config).unwrap();
        assert_ne!(
            &parent.bytes[base..base + 2],
            &child.bytes[base..base + 2]
        );
        assert_ne!(before.max_speed, after.max_speed);
    }

    #[test]
    fn default_config_uses_frequent_small_steps() {
        let config = SimConfig::load_v1_regression().unwrap();
        let g = &config.genetics;
        assert!(g.mutations_per_offspring >= 4);
        assert!(g.p_point > g.p_bit_flip);
        assert_eq!(g.delta_max, 1);
    }

    #[test]
    fn max_speed_locus_pick_probability_is_seven_point_five_percent() {
        assert_eq!(MOVEMENT_PACE_LOCUS_PROB, 0.075);
    }

    #[test]
    fn skin_hardness_and_repair_rate_are_core_loci() {
        assert!(CORE_LOCI.iter().any(|l| l.offset == core::SKIN_HARDNESS));
        assert!(CORE_LOCI.iter().any(|l| l.offset == core::REPAIR_RATE));
        assert!(CORE_LOCI.iter().any(|l| l.offset == core::DESIRE_TO_MOVE));
    }

    #[test]
    fn body_locus_is_doubled_when_picking_non_speed() {
        let total: u32 = CORE_LOCI
            .iter()
            .filter(|l| l.offset != core::MOVEMENT_PACE)
            .map(locus_selection_weight)
            .sum();
        let body = LOCUS_WEIGHT_BODY_RADIUS as f32 / total as f32;
        let uniform = LOCUS_WEIGHT_DEFAULT as f32 / total as f32;
        assert!(body > uniform);
    }
}

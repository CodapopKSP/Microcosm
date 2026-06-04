//! Phenotype decode from genome v1 core loci.

use std::fmt;

use crate::config::{ColorPhotosynthesisConfig, SimConfig};
use crate::genome::encoding::{Genome, GenomeError};

const CHEMO_UPKEEP_SCALE: f32 = 0.12;

#[derive(Debug, Clone, PartialEq)]
pub struct Phenotype {
    pub color: [f32; 3],
    /// Fitness-linked green excess from color loci ([PHASE_4.md](../../docs/PHASE_4.md) §4.7).
    pub greenness: f32,
    /// Photosynthesis multiplier: `1 + k_green_photo * greenness`.
    pub green_photo_mult: f32,
    /// Extra idle metabolism per tick: `k_green_meta * greenness`.
    pub green_metabolism_extra: f32,
    pub body_radius: f32,
    pub photo_strength: f32,
    /// Photosynthetic surface vs `r_ref` (1.0 at reference radius; scales with body area).
    pub photo_area_scale: f32,
    pub photo_upkeep: f32,
    /// Idle metabolism per tick (radius-scaled, config floor applied).
    pub metabolism_idle: f32,
    /// Decoded metabolism gene before radius scale (for moving-metabolism formula).
    pub metabolism_base: f32,
    pub locomotion_investment: f32,
    /// Per-tick probability of attempting movement (`raw / 255`; 0 = never, 1 = always).
    pub desire_to_move: f32,
    /// Speed cap at `movement_pace = 1` before pace multiplier.
    pub max_speed_base: f32,
    /// Evolvable pace: scales `max_speed` and distance-indexed move energy together.
    pub movement_pace: f32,
    pub max_speed: f32,
    /// Energy per world unit at `movement_pace = 1` (`1 / (10 * body_radius)`).
    pub move_cost: f32,
    pub move_penalty: f32,
    pub repro_threshold: f32,
    pub repro_cost: f32,
    pub offspring_endowment: f32,
    pub skin_hardness: f32,
    /// `skin_hardness / (skin_hardness + k_mitigate)` — reduces incoming damage (Phase 4B).
    pub damage_mitigation: f32,
    pub armor_upkeep: f32,
    /// HP throughput cap per tick scales with this; energy per HP is config `repair_energy_per_hp`.
    pub repair_rate: f32,
    pub chemo_potential: f32,
    pub chemo_upkeep: f32,
    /// Vent/soup sampling quality when above `chemo_cliff` ([PHASE_5.md](../../docs/PHASE_5.md) §5.3).
    pub chemo_sensitivity: f32,
    pub core_max_hp: f32,
    /// Scales logarithmic age metabolism (`max_age` locus; see [GENOME.md](../../docs/GENOME.md)).
    pub age_metabolism_scale: f32,
    k_move_meta: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    Genome(GenomeError),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Genome(err) => write!(f, "phenotype decode failed: {err}"),
        }
    }
}

impl std::error::Error for DecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DecodeError::Genome(err) => Some(err),
        }
    }
}

impl From<GenomeError> for DecodeError {
    fn from(err: GenomeError) -> Self {
        DecodeError::Genome(err)
    }
}

/// Decode byte 16: `0` = never move, `255` (`0xFF`) = move every tick.
pub fn desire_to_move_from_raw(raw: u8) -> f32 {
    raw as f32 / 255.0
}

/// Whether an agent attempts movement this tick (`roll` in `[0, 1)`).
pub fn wants_to_move_this_tick(desire_to_move: f32, roll: f32) -> bool {
    if desire_to_move <= 0.0 {
        false
    } else if desire_to_move >= 1.0 {
        true
    } else {
        roll < desire_to_move
    }
}

/// Distance-indexed movement energy ([GENOME_REVISIONS.md](../../docs/GENOME_REVISIONS.md)).
///
/// At `movement_pace = 1`, traveling `10 * body_radius` world units costs 1 energy.
pub fn movement_energy_for_distance(
    distance: f32,
    body_radius: f32,
    movement_pace: f32,
) -> f32 {
    let r = body_radius.max(1e-6);
    distance.max(0.0) / (10.0 * r) * movement_pace.max(0.0)
}

impl Phenotype {
    /// Speed ratio for current velocity.
    pub fn speed_ratio(&self, speed: f32) -> f32 {
        let max_speed = self.max_speed.max(1e-6);
        (speed / max_speed).clamp(0.0, 1.0)
    }

    /// Moving metabolism for the current speed (idle metabolism charged separately).
    pub fn metabolism_moving(&self, speed: f32) -> f32 {
        let speed_ratio = self.speed_ratio(speed);
        self.metabolism_idle * (1.0 + self.k_move_meta * speed_ratio)
    }

    pub fn is_chemo_above_cliff(&self, cliff: f32) -> bool {
        self.chemo_potential > cliff
    }
}

/// Sensitivity derived from `chemo_potential` once above the cliff (Phase 5B).
pub fn chemo_sensitivity_from_potential(chemo_potential: f32, chemo_cliff: f32) -> f32 {
    if chemo_potential > chemo_cliff && chemo_cliff > 0.0 {
        (chemo_potential / chemo_cliff).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Chemosynthetic gain at a vent sample ([ENERGY_AND_ENVIRONMENT.md](../../docs/ENERGY_AND_ENVIRONMENT.md)).
pub fn chemo_gain_per_tick(
    phenotype: &Phenotype,
    vent_flux: f32,
    chemo_cliff: f32,
    k_chemo: f32,
) -> f32 {
    if vent_flux <= 0.0 || !phenotype.is_chemo_above_cliff(chemo_cliff) {
        return 0.0;
    }
    phenotype.chemo_potential
        * vent_flux
        * phenotype.chemo_sensitivity
        * k_chemo.max(0.0)
}

/// Armor mitigation from skin hardness ([PHASE_4.md](../../docs/PHASE_4.md) §4.2).
pub fn damage_mitigation_from_hardness(hardness: f32, k_mitigate: f32) -> f32 {
    if hardness <= 0.0 {
        return 0.0;
    }
    let k = k_mitigate.max(0.0);
    if k <= 0.0 {
        return 1.0;
    }
    hardness / (hardness + k)
}

/// Mitigated damage: `raw * (1 - mitigation)`.
pub fn mitigated_damage(raw: f32, mitigation: f32) -> f32 {
    raw.max(0.0) * (1.0 - mitigation.clamp(0.0, 1.0))
}

/// Green excess from decoded RGB in [0, 1].
pub fn green_excess_from_rgb(r: f32, g: f32, b: f32) -> f32 {
    g - 0.5 * (r + b)
}

/// Clamp greenness from color channels and config scale.
pub fn greenness_from_rgb(
    r: f32,
    g: f32,
    b: f32,
    color_cfg: &ColorPhotosynthesisConfig,
) -> f32 {
    if color_cfg.greenness_scale <= 0.0 {
        return 0.0;
    }
    let excess = green_excess_from_rgb(r, g, b);
    (excess / color_cfg.greenness_scale).clamp(0.0, color_cfg.greenness_max)
}

/// Extra photosynthetic gain per tick from greenness at a given light level (before area scale).
pub fn green_photo_buff_per_tick(
    greenness: f32,
    photo_strength: f32,
    photo_area_scale: f32,
    light: f32,
    solar_constant: f32,
    color_cfg: &ColorPhotosynthesisConfig,
) -> f32 {
    if color_cfg.k_green_photo <= 0.0 || greenness <= 0.0 {
        return 0.0;
    }
    let base = photo_strength * photo_area_scale * light * solar_constant;
    base * color_cfg.k_green_photo * greenness
}

/// Extra metabolism drain per tick from greenness.
pub fn green_metabolism_penalty_per_tick(
    greenness: f32,
    color_cfg: &ColorPhotosynthesisConfig,
) -> f32 {
    color_cfg.k_green_meta * greenness.max(0.0)
}

/// §4.7 inequality at `light_ref` with `photo_strength_ref = 1.0` (unit tests).
pub fn green_tradeoff_holds_at_light_ref(
    color_cfg: &ColorPhotosynthesisConfig,
    solar_constant: f32,
) -> bool {
    let buff = color_cfg.k_green_photo * color_cfg.light_ref * 1.0 * solar_constant;
    green_metabolism_penalty_per_tick(1.0, color_cfg) < buff
}

/// Decode core loci and derived traits. Caller should pass a validated [`Genome`].
pub fn decode(genome: &Genome, config: &SimConfig) -> Result<Phenotype, DecodeError> {
    genome.validate(config.genetics.genome_version, config.genetics.core_length)?;

    let dr = &config.genetics.decode_ranges;
    let economy = &config.economy;

    let color = [
        lerp_u8(genome.color_r()?, 0.0, 1.0),
        lerp_u8(genome.color_g()?, 0.0, 1.0),
        lerp_u8(genome.color_b()?, 0.0, 1.0),
    ];
    let color_cfg = &config.genetics.color_photosynthesis;
    let greenness = greenness_from_rgb(color[0], color[1], color[2], color_cfg);
    let green_photo_mult = 1.0 + color_cfg.k_green_photo * greenness;
    let green_metabolism_extra = color_cfg.k_green_meta * greenness;

    let body_radius = lerp_u8(
        genome.body_radius_raw()?,
        dr.body_radius_min,
        dr.body_radius_max,
    );

    let photo_strength = lerp_u16(genome.photo_strength_raw()?, 0.0, dr.photo_max);
    let photo_area_scale = (body_radius / dr.r_ref).powi(2);
    let photo_upkeep = dr.k_photo * photo_strength;

    let metabolism_base = lerp_u16(
        genome.metabolism_base_raw()?,
        dr.meta_min,
        dr.meta_max,
    );
    let radius_scale = (body_radius / dr.r_ref).powi(2);
    let metabolism_scaled = metabolism_base * radius_scale;
    let metabolism_idle = metabolism_scaled.max(economy.metabolism_floor);

    let locomotion_investment =
        lerp_u16(genome.locomotion_investment_raw()?, 0.0, dr.loco_max);
    let desire_to_move = desire_to_move_from_raw(genome.desire_to_move_raw()?);

    let skin_hardness = lerp_u16(genome.skin_hardness_raw()?, 0.0, 1.0);
    let damage_mitigation = damage_mitigation_from_hardness(skin_hardness, dr.k_mitigate);
    let move_penalty = dr.k_move_hardness * skin_hardness;
    let armor_upkeep = dr.k_armor * skin_hardness;

    let size_ratio = (body_radius / dr.r_ref).max(0.0);
    let max_speed_base = ((dr.v_base + dr.k_v * locomotion_investment) * size_ratio
        - move_penalty)
        .max(0.0);
    let movement_pace = lerp_u16(genome.movement_pace_raw()?, dr.pace_min, dr.pace_max);
    let max_speed = (max_speed_base * movement_pace).max(0.0);
    let move_cost = movement_energy_for_distance(1.0, body_radius, 1.0);

    let repro_threshold = lerp_u16(genome.repro_threshold_raw()?, dr.e_min, dr.e_max);
    let repro_cost = lerp_u16(genome.repro_cost_raw()?, dr.e_min, dr.e_max);
    let offspring_endowment = lerp_u16(genome.offspring_endowment_raw()?, dr.e_min, dr.e_max);

    let repair_rate = lerp_u16(genome.repair_rate_raw()?, dr.repair_rate_min, dr.repair_rate_max);

    let chemo_raw = genome.chemo_potential_raw()?;
    let chemo_potential = chemo_raw as f32 / 65535.0;
    let chemo_upkeep = if chemo_raw > 0 {
        CHEMO_UPKEEP_SCALE * chemo_potential
    } else {
        0.0
    };
    let chemo_sensitivity = chemo_sensitivity_from_potential(chemo_potential, dr.chemo_cliff);

    let age_metabolism_scale = lerp_u16(
        genome.max_age_raw()?,
        dr.age_metabolism_scale_min,
        dr.age_metabolism_scale_max,
    );

    Ok(Phenotype {
        color,
        greenness,
        green_photo_mult,
        green_metabolism_extra,
        body_radius,
        photo_strength,
        photo_area_scale,
        photo_upkeep,
        metabolism_idle,
        metabolism_base,
        locomotion_investment,
        desire_to_move,
        max_speed_base,
        movement_pace,
        max_speed,
        move_cost,
        move_penalty,
        repro_threshold,
        repro_cost,
        offspring_endowment,
        skin_hardness,
        damage_mitigation,
        armor_upkeep,
        repair_rate,
        chemo_potential,
        chemo_upkeep,
        chemo_sensitivity,
        core_max_hp: economy.core_max_hp,
        age_metabolism_scale,
        k_move_meta: dr.k_move_meta,
    })
}

pub fn lerp_u8(raw: u8, min: f32, max: f32) -> f32 {
    min + (raw as f32 / 255.0) * (max - min)
}

pub fn lerp_u16(raw: u16, min: f32, max: f32) -> f32 {
    min + (raw as f32 / 65535.0) * (max - min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::genome::encoding::{self, Genome};

    fn default_config() -> SimConfig {
        SimConfig::load_default().unwrap()
    }

    fn default_genome() -> Genome {
        Genome::ancestor_from_config(&default_config()).unwrap()
    }

    fn decode_default() -> Phenotype {
        decode(&default_genome(), &default_config()).unwrap()
    }

    fn genome_with_photo_raw(raw: u16) -> Genome {
        let config = default_config();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        let i = encoding::HEADER_LEN + encoding::core::PHOTO_STRENGTH;
        bytes[i] = (raw >> 8) as u8;
        bytes[i + 1] = (raw & 0xFF) as u8;
        Genome::from_bytes(bytes, config.genetics.genome_version, config.genetics.core_length)
            .unwrap()
    }

    fn genome_with_u16_locus(core_offset: usize, raw: u16) -> Genome {
        genome_with_u16_loci(&[(core_offset, raw)])
    }

    fn genome_with_u16_loci(loci: &[(usize, u16)]) -> Genome {
        let config = default_config();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        for &(core_offset, raw) in loci {
            let i = encoding::HEADER_LEN + core_offset;
            bytes[i] = (raw >> 8) as u8;
            bytes[i + 1] = (raw & 0xFF) as u8;
        }
        Genome::from_bytes(bytes, config.genetics.genome_version, config.genetics.core_length)
            .unwrap()
    }

    fn genome_with_color_bytes(r: u8, g: u8, b: u8) -> Genome {
        let config = default_config();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        let base = encoding::HEADER_LEN;
        bytes[base + encoding::core::COLOR_R] = r;
        bytes[base + encoding::core::COLOR_G] = g;
        bytes[base + encoding::core::COLOR_B] = b;
        Genome::from_bytes(bytes, config.genetics.genome_version, config.genetics.core_length)
            .unwrap()
    }

    #[test]
    fn move_cost_is_one_over_ten_radii_at_pace_one() {
        let p = decode_default();
        let expected = 1.0 / (10.0 * p.body_radius);
        assert!((p.move_cost - expected).abs() < 1e-5);
    }

    #[test]
    fn max_speed_scales_with_locomotion_and_movement_pace() {
        let config = default_config();
        let dr = &config.genetics.decode_ranges;
        let p = decode_default();
        let expected_base = ((dr.v_base + dr.k_v * p.locomotion_investment)
            * (p.body_radius / dr.r_ref)
            - p.move_penalty)
            .max(0.0);
        assert!((p.max_speed_base - expected_base).abs() < 0.05);
        assert!((p.max_speed - expected_base * p.movement_pace).abs() < 0.05);
        assert!(p.max_speed > 0.0, "default ancestor should move at pace 1");
    }

    #[test]
    fn ancestor_phenotype_within_expected_bounds() {
        let config = default_config();
        let dr = &config.genetics.decode_ranges;
        let p = decode_default();

        assert!((p.color[0] - 136.0 / 255.0).abs() < 0.01);
        assert!((p.color[1] - 204.0 / 255.0).abs() < 0.01);
        assert!((p.color[2] - 68.0 / 255.0).abs() < 0.01);

        assert!(p.body_radius >= dr.body_radius_min && p.body_radius <= dr.body_radius_max);
        assert!(p.photo_strength >= 0.0 && p.photo_strength <= dr.photo_max);
        assert!(
            p.photo_area_scale > 0.0,
            "photo surface scales with body radius vs r_ref"
        );
        assert!(p.metabolism_base >= dr.meta_min && p.metabolism_base <= dr.meta_max);
        assert!(p.locomotion_investment >= 0.0 && p.locomotion_investment <= dr.loco_max);
        assert!(p.repro_threshold >= dr.e_min && p.repro_threshold <= dr.e_max);
        assert!((p.repro_threshold - 100.0).abs() < 1.0);
        assert!((p.offspring_endowment - 100.0).abs() < 1.0);
        assert!(p.max_speed > 0.0);
        assert!((p.movement_pace - 1.0).abs() < 0.02);
        assert_eq!(p.desire_to_move, 0.0);
        assert!(dr.e_max > 100.0, "e_max should allow thresholds above ancestor default");
        assert!(p.core_max_hp == config.economy.core_max_hp);
        assert!(p.metabolism_idle >= config.economy.metabolism_floor);
    }

    #[test]
    fn extreme_u16_alleles_respect_config_max() {
        let config = default_config();
        let dr = &config.genetics.decode_ranges;

        let photo = decode(
            &genome_with_u16_locus(encoding::core::PHOTO_STRENGTH, 0xFFFF),
            &config,
        )
        .unwrap();
        assert!((photo.photo_strength - dr.photo_max).abs() < 1e-4);

        let meta = decode(
            &genome_with_u16_locus(encoding::core::METABOLISM_BASE, 0xFFFF),
            &config,
        )
        .unwrap();
        assert!((meta.metabolism_base - dr.meta_max).abs() < 1e-3);

        let loco = decode(
            &genome_with_u16_locus(encoding::core::LOCOMOTION_INVESTMENT, 0xFFFF),
            &config,
        )
        .unwrap();
        assert!((loco.locomotion_investment - dr.loco_max).abs() < 1e-4);

        let fast_pace = decode(
            &genome_with_u16_locus(encoding::core::MOVEMENT_PACE, 0xFFFF),
            &config,
        )
        .unwrap();
        let slow_pace = decode(
            &genome_with_u16_locus(encoding::core::MOVEMENT_PACE, 0x0000),
            &config,
        )
        .unwrap();
        assert!(fast_pace.max_speed > slow_pace.max_speed);
        assert!(fast_pace.movement_pace > slow_pace.movement_pace);
    }

    #[test]
    fn repro_threshold_decodes_above_100_at_max_allele() {
        let config = default_config();
        let dr = &config.genetics.decode_ranges;
        let p = decode(
            &genome_with_u16_locus(encoding::core::REPRO_THRESHOLD, 0xFFFF),
            &config,
        )
        .unwrap();
        assert!(p.repro_threshold > 100.0);
        assert!((p.repro_threshold - dr.e_max).abs() < 1e-3);
    }

    #[test]
    fn offspring_endowment_decodes_above_100_at_max_allele() {
        let config = default_config();
        let dr = &config.genetics.decode_ranges;
        let p = decode(
            &genome_with_u16_locus(encoding::core::OFFSPRING_ENDOWMENT, 0xFFFF),
            &config,
        )
        .unwrap();
        assert!(p.offspring_endowment > 100.0);
        assert!((p.offspring_endowment - dr.e_max).abs() < 1e-3);
    }

    #[test]
    fn movement_pace_and_locomotion_affect_max_speed() {
        let config = default_config();
        let slow_pace = decode(
            &genome_with_u16_locus(encoding::core::MOVEMENT_PACE, 0x1000),
            &config,
        )
        .unwrap();
        let fast_pace = decode(
            &genome_with_u16_locus(encoding::core::MOVEMENT_PACE, 0xE000),
            &config,
        )
        .unwrap();
        assert!(fast_pace.max_speed > slow_pace.max_speed);

        let cheap_loco = decode(
            &genome_with_u16_loci(&[
                (encoding::core::LOCOMOTION_INVESTMENT, 0x1000),
                (encoding::core::MOVEMENT_PACE, 0x5555),
            ]),
            &config,
        )
        .unwrap();
        let costly_loco = decode(
            &genome_with_u16_loci(&[
                (encoding::core::LOCOMOTION_INVESTMENT, 0xE000),
                (encoding::core::MOVEMENT_PACE, 0x5555),
            ]),
            &config,
        )
        .unwrap();
        assert!(costly_loco.max_speed > cheap_loco.max_speed);
    }

    #[test]
    fn movement_energy_scales_linearly_with_pace() {
        let config = default_config();
        let pace_one = decode_default();
        let pace_two = decode(
            &genome_with_u16_locus(encoding::core::MOVEMENT_PACE, 0xFFFF),
            &config,
        )
        .unwrap();
        let d = 30.0;
        let e1 = movement_energy_for_distance(d, pace_one.body_radius, pace_one.movement_pace);
        let e2 = movement_energy_for_distance(d, pace_two.body_radius, pace_two.movement_pace);
        assert!((e2 / e1 - pace_two.movement_pace / pace_one.movement_pace).abs() < 0.05);
    }

    #[test]
    fn photo_upkeep_increases_with_photo_strength() {
        let config = default_config();
        let low = decode(&genome_with_photo_raw(0x1000), &config).unwrap();
        let high = decode(&genome_with_photo_raw(0xC000), &config).unwrap();
        assert!(high.photo_strength > low.photo_strength);
        assert!(high.photo_upkeep > low.photo_upkeep);
    }

    #[test]
    fn invalid_genome_fails_decode() {
        let config = default_config();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        bytes[0] = b'X';
        let genome = Genome {
            version: config.genetics.genome_version,
            bytes,
        };
        assert!(decode(&genome, &config).is_err());
    }

    #[test]
    fn metabolism_moving_scales_with_speed_when_configured() {
        let config = default_config();
        let p = decode(
            &genome_with_u16_locus(encoding::core::MOVEMENT_PACE, 0x5555),
            &config,
        )
        .unwrap();
        assert!(p.max_speed > 0.0);
        if p.k_move_meta > 0.0 {
            assert!(p.metabolism_moving(p.max_speed) > p.metabolism_idle);
        } else {
            assert_eq!(p.metabolism_moving(0.0), p.metabolism_idle);
            assert_eq!(p.metabolism_moving(p.max_speed), p.metabolism_idle);
        }
    }

    #[test]
    fn mitigation_increases_with_hardness() {
        let config = default_config();
        let dr = &config.genetics.decode_ranges;
        let soft = decode_default();
        assert_eq!(soft.damage_mitigation, 0.0);
        assert_eq!(
            mitigated_damage(10.0, soft.damage_mitigation),
            10.0,
            "zero hardness should take full raw damage"
        );

        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        let i = encoding::HEADER_LEN + encoding::core::SKIN_HARDNESS;
        bytes[i] = 0xFF;
        bytes[i + 1] = 0xFF;
        let hard_genome =
            Genome::from_bytes(bytes, config.genetics.genome_version, config.genetics.core_length)
                .unwrap();
        let hard = decode(&hard_genome, &config).unwrap();
        assert!(hard.damage_mitigation > soft.damage_mitigation);
        let expected = hard.skin_hardness / (hard.skin_hardness + dr.k_mitigate);
        assert!((hard.damage_mitigation - expected).abs() < 1e-5);
        assert!(mitigated_damage(10.0, hard.damage_mitigation) < 10.0);
    }

    #[test]
    fn armor_upkeep_scales_with_hardness() {
        let config = default_config();
        let soft = decode_default();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        let i = encoding::HEADER_LEN + encoding::core::SKIN_HARDNESS;
        bytes[i] = 0xFF;
        bytes[i + 1] = 0xFF;
        let hard_genome =
            Genome::from_bytes(bytes, config.genetics.genome_version, config.genetics.core_length)
                .unwrap();
        let hard = decode(&hard_genome, &config).unwrap();
        assert!(hard.armor_upkeep > soft.armor_upkeep);
        assert!(hard.move_penalty > soft.move_penalty);
    }

    #[test]
    fn age_metabolism_scale_locus_decodes_in_range() {
        let config = default_config();
        let dr = &config.genetics.decode_ranges;

        let min_p = decode(
            &genome_with_u16_locus(encoding::core::MAX_AGE, 0x0000),
            &config,
        )
        .unwrap();
        assert!((min_p.age_metabolism_scale - dr.age_metabolism_scale_min).abs() < 1e-3);

        let max_p = decode(
            &genome_with_u16_locus(encoding::core::MAX_AGE, 0xFFFF),
            &config,
        )
        .unwrap();
        assert!((max_p.age_metabolism_scale - dr.age_metabolism_scale_max).abs() < 1e-3);

        let mid = decode_default();
        assert!(
            mid.age_metabolism_scale > dr.age_metabolism_scale_min
                && mid.age_metabolism_scale < dr.age_metabolism_scale_max
        );
    }

    #[test]
    fn age_metabolism_rises_with_ticks() {
        let config = default_config();
        let p = decode_default();
        let young =
            crate::sim::age_metabolism_per_tick(3600, p.age_metabolism_scale, &config.economy);
        let old = crate::sim::age_metabolism_per_tick(
            config.economy.age_metabolism_cap_ticks as u32,
            p.age_metabolism_scale,
            &config.economy,
        );
        assert!(old > young);
        assert!(
            (old * 60.0 - config.economy.age_metabolism_cap_per_sec * p.age_metabolism_scale).abs()
                < 0.05
        );
    }

    #[test]
    fn ancestor_has_no_armor_upkeep() {
        let p = decode_default();
        assert_eq!(p.skin_hardness, 0.0);
        assert_eq!(p.armor_upkeep, 0.0);
        assert_eq!(p.damage_mitigation, 0.0);
    }

    #[test]
    fn repair_rate_decodes_in_config_range() {
        let config = default_config();
        let dr = &config.genetics.decode_ranges;
        let p = decode_default();
        assert!((p.repair_rate - 1.0).abs() < 0.02);

        let slow = decode(
            &genome_with_u16_locus(encoding::core::REPAIR_RATE, 0x0000),
            &config,
        )
        .unwrap();
        let fast = decode(
            &genome_with_u16_locus(encoding::core::REPAIR_RATE, 0xFFFF),
            &config,
        )
        .unwrap();
        assert!((slow.repair_rate - dr.repair_rate_min).abs() < 1e-3);
        assert!((fast.repair_rate - dr.repair_rate_max).abs() < 1e-3);
        assert!(fast.repair_rate > slow.repair_rate);
    }

    #[test]
    fn chemo_sensitivity_zero_below_cliff() {
        let config = default_config();
        let cliff = config.genetics.decode_ranges.chemo_cliff;
        let g = genome_with_u16_locus(encoding::core::CHEMO_POTENTIAL, 0x5709);
        let p = decode(&g, &config).unwrap();
        assert!(!p.is_chemo_above_cliff(cliff));
        assert_eq!(p.chemo_sensitivity, 0.0);
        assert_eq!(chemo_gain_per_tick(&p, 0.1, cliff, 1.0), 0.0);
    }

    #[test]
    fn chemo_sensitivity_positive_above_cliff() {
        let config = default_config();
        let cliff = config.genetics.decode_ranges.chemo_cliff;
        let g = genome_with_u16_locus(encoding::core::CHEMO_POTENTIAL, 0x8CCC);
        let p = decode(&g, &config).unwrap();
        assert!(p.is_chemo_above_cliff(cliff));
        assert!(p.chemo_sensitivity > 0.0);
        assert!(chemo_gain_per_tick(&p, 0.05, cliff, 1.0) > 0.0);
    }

    #[test]
    fn greenness_increases_with_g_over_rb() {
        let config = default_config();
        let mid = decode(&genome_with_color_bytes(0x88, 0x88, 0x88), &config).unwrap();
        let greener = decode(&genome_with_color_bytes(0x20, 0xFF, 0x20), &config).unwrap();
        let less_green = decode(&genome_with_color_bytes(0xFF, 0x30, 0xFF), &config).unwrap();

        assert!(greener.greenness > mid.greenness);
        assert!(greener.greenness > less_green.greenness);
        assert_eq!(less_green.greenness, 0.0, "low G with high R/B should clamp to zero");
        assert!(
            greener.greenness > 0.5,
            "strong green should yield high greenness, got {}",
            greener.greenness
        );
    }

    #[test]
    fn desire_to_move_decodes_from_byte() {
        let config = default_config();
        assert_eq!(decode_default().desire_to_move, 0.0);

        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        bytes[encoding::HEADER_LEN + encoding::core::DESIRE_TO_MOVE] = 0xFF;
        let restless = decode(
            &Genome::from_bytes(
                bytes,
                config.genetics.genome_version,
                config.genetics.core_length,
            )
            .unwrap(),
            &config,
        )
        .unwrap();
        assert!((restless.desire_to_move - 1.0).abs() < 1e-5);
    }

    #[test]
    fn wants_to_move_probability_edges() {
        assert!(!wants_to_move_this_tick(0.0, 0.0));
        assert!(wants_to_move_this_tick(1.0, 0.99));
        assert!(wants_to_move_this_tick(0.5, 0.25));
        assert!(!wants_to_move_this_tick(0.5, 0.75));
    }

    #[test]
    fn green_penalty_less_than_buff_at_light_ref() {
        let config = default_config();
        let cp = &config.genetics.color_photosynthesis;
        assert!(
            green_tradeoff_holds_at_light_ref(cp, config.environment.solar_constant),
            "§4.7 invariant: photo buff at light_ref must exceed metabolism penalty \
             (k_green_photo={}, k_green_meta={}, solar={})",
            cp.k_green_photo,
            cp.k_green_meta,
            config.environment.solar_constant
        );
        let buff = cp.k_green_photo * cp.light_ref * 1.0 * config.environment.solar_constant;
        let penalty = green_metabolism_penalty_per_tick(1.0, cp);
        assert!(penalty < buff, "penalty {penalty} should be < buff {buff}");
    }
}

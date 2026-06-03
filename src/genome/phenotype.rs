//! Phenotype decode from genome v1 core loci.

use std::fmt;

use crate::config::{EconomyConfig, SimConfig};
use crate::genome::encoding::{Genome, GenomeError};

const REPAIR_EFFICIENCY_MIN: f32 = 0.1;
const REPAIR_EFFICIENCY_MAX: f32 = 2.0;
const CHEMO_UPKEEP_SCALE: f32 = 0.12;

#[derive(Debug, Clone, PartialEq)]
pub struct Phenotype {
    pub color: [f32; 3],
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
    pub max_speed: f32,
    pub move_cost: f32,
    pub move_penalty: f32,
    pub repro_threshold: f32,
    pub repro_cost: f32,
    pub offspring_endowment: f32,
    pub repro_cooldown_ticks: u32,
    pub repro_risk_factor: f32,
    pub skin_hardness: f32,
    /// `skin_hardness / (skin_hardness + k_mitigate)` — reduces incoming damage (Phase 4B).
    pub damage_mitigation: f32,
    pub armor_upkeep: f32,
    pub repair_efficiency: f32,
    pub chemo_potential: f32,
    pub chemo_upkeep: f32,
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

/// Movement energy per second: scales with `(body_radius/r_ref)²` and speed ratio.
///
/// At reference size, `speed_ratio = move_energy_half_speed_ratio` costs
/// `move_energy_per_sec_at_half_speed` (default 1 J/s).
pub fn movement_energy_per_sec(
    photo_area_scale: f32,
    speed_ratio: f32,
    economy: &EconomyConfig,
) -> f32 {
    let anchor = economy.move_energy_half_speed_ratio.max(0.01);
    let ratio = speed_ratio.clamp(0.0, 1.0);
    economy.move_energy_per_sec_at_half_speed * photo_area_scale.max(0.0) * (ratio / anchor)
}

/// Movement energy per tick (60 Hz).
pub fn movement_energy_per_tick(
    photo_area_scale: f32,
    speed_ratio: f32,
    economy: &EconomyConfig,
) -> f32 {
    movement_energy_per_sec(photo_area_scale, speed_ratio, economy) / 60.0
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

    let skin_hardness = lerp_u16(genome.skin_hardness_raw()?, 0.0, 1.0);
    let damage_mitigation = damage_mitigation_from_hardness(skin_hardness, dr.k_mitigate);
    let move_penalty = dr.k_move_hardness * skin_hardness;
    let armor_upkeep = dr.k_armor * skin_hardness;

    let size_ratio = (body_radius / dr.r_ref).max(0.0);
    let speed_mult = lerp_u16(genome.max_speed_raw()?, dr.speed_min, dr.speed_max);
    let max_speed = ((dr.v_base + dr.k_v * locomotion_investment) * size_ratio * speed_mult
        - move_penalty)
        .max(0.0);

    let move_cost = movement_energy_per_tick(
        photo_area_scale,
        economy.move_energy_half_speed_ratio,
        economy,
    );

    let repro_threshold = lerp_u16(genome.repro_threshold_raw()?, dr.e_min, dr.e_max);
    let repro_cost = lerp_u16(genome.repro_cost_raw()?, dr.e_min, dr.e_max);
    let offspring_endowment = lerp_u16(genome.offspring_endowment_raw()?, dr.e_min, dr.e_max);

    let repro_cooldown_ticks = lerp_u8(
        genome.repro_cooldown_raw()?,
        dr.cd_min,
        dr.cd_max,
    )
    .round()
    .max(0.0) as u32;

    let repro_risk_factor = lerp_u8(genome.repro_risk_factor_raw()?, 0.0, 1.0);

    let repair_efficiency = lerp_u16(
        genome.repair_efficiency_raw()?,
        REPAIR_EFFICIENCY_MIN,
        REPAIR_EFFICIENCY_MAX,
    );

    let chemo_raw = genome.chemo_potential_raw()?;
    let chemo_potential = chemo_raw as f32 / 65535.0;
    let chemo_upkeep = if chemo_raw > 0 {
        CHEMO_UPKEEP_SCALE * chemo_potential
    } else {
        0.0
    };

    let age_metabolism_scale = lerp_u16(
        genome.max_age_raw()?,
        dr.age_metabolism_scale_min,
        dr.age_metabolism_scale_max,
    );

    Ok(Phenotype {
        color,
        body_radius,
        photo_strength,
        photo_area_scale,
        photo_upkeep,
        metabolism_idle,
        metabolism_base,
        locomotion_investment,
        max_speed,
        move_cost,
        move_penalty,
        repro_threshold,
        repro_cost,
        offspring_endowment,
        repro_cooldown_ticks,
        repro_risk_factor,
        skin_hardness,
        damage_mitigation,
        armor_upkeep,
        repair_efficiency,
        chemo_potential,
        chemo_upkeep,
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
        let config = default_config();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        let i = encoding::HEADER_LEN + core_offset;
        bytes[i] = (raw >> 8) as u8;
        bytes[i + 1] = (raw & 0xFF) as u8;
        Genome::from_bytes(bytes, config.genetics.genome_version, config.genetics.core_length)
            .unwrap()
    }

    #[test]
    fn move_cost_matches_half_speed_anchor() {
        let config = default_config();
        let p = decode_default();
        let expected = movement_energy_per_tick(p.photo_area_scale, 0.5, &config.economy);
        assert!(
            (p.move_cost - expected).abs() < 1e-5,
            "move_cost {} should match half-speed movement tick ({expected})",
            p.move_cost
        );
    }

    #[test]
    fn max_speed_scales_with_body_size_and_locus() {
        let config = default_config();
        let dr = &config.genetics.decode_ranges;
        let p = decode_default();
        let speed_mult = lerp_u16(0, dr.speed_min, dr.speed_max);
        let expected = ((dr.v_base + dr.k_v * p.locomotion_investment)
            * (p.body_radius / dr.r_ref)
            * speed_mult
            - p.move_penalty)
            .max(0.0);
        assert!(
            (p.max_speed - expected).abs() < 0.05,
            "max_speed {} vs body-scaled expected {expected}",
            p.max_speed
        );
        assert!(p.max_speed > 0.0, "ancestor should be mobile");
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
        assert!((p.repro_threshold - 100.0).abs() < 0.01);
        assert!((p.offspring_endowment - 100.0).abs() < 0.01);
        assert!(p.max_speed > 0.0);
        assert!((p.move_cost * 60.0 - config.economy.move_energy_per_sec_at_half_speed * p.photo_area_scale).abs() < 0.1);
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

        let fast = decode(
            &genome_with_u16_locus(encoding::core::MAX_SPEED, 0xFFFF),
            &config,
        )
        .unwrap();
        let slow = decode(
            &genome_with_u16_locus(encoding::core::MAX_SPEED, 0x0000),
            &config,
        )
        .unwrap();
        assert!(fast.max_speed > slow.max_speed);
    }

    #[test]
    fn max_speed_locus_and_locomotion_affect_body_derived_speed() {
        let config = default_config();
        let slow_locus = decode(
            &genome_with_u16_locus(encoding::core::MAX_SPEED, 0x1000),
            &config,
        )
        .unwrap();
        let fast_locus = decode(
            &genome_with_u16_locus(encoding::core::MAX_SPEED, 0xE000),
            &config,
        )
        .unwrap();
        assert!(fast_locus.max_speed > slow_locus.max_speed);

        let cheap_loco = decode(
            &genome_with_u16_locus(encoding::core::LOCOMOTION_INVESTMENT, 0x1000),
            &config,
        )
        .unwrap();
        let costly_loco = decode(
            &genome_with_u16_locus(encoding::core::LOCOMOTION_INVESTMENT, 0xE000),
            &config,
        )
        .unwrap();
        assert!(costly_loco.max_speed > cheap_loco.max_speed);
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
            &genome_with_u16_locus(encoding::core::MAX_SPEED, 0x8000),
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
}

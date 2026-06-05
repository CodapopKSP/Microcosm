//! Phenotype decode from genome v2 (`EVO2`) core loci.

use std::fmt;

use crate::config::{GeneticsDecodeV2, GeneticsTriadConfig, SimConfig, TriadCouplingMode};
use crate::genome::encoding_v2::GenomeV2;
use crate::genome::error::GenomeError;

#[derive(Debug, Clone, PartialEq)]
pub struct PhenotypeV2 {
    pub color: [f32; 3],
    pub color_rgb: [u8; 3],
    pub body_radius: f32,
    pub radius_metabolism_bonus: f32,
    pub vitality_budget: f32,
    pub max_energy: f32,
    pub max_health: f32,
    pub energy_cap_fraction: f32,
    pub armor_stroke: f32,
    pub armor_metabolism_factor: f32,
    pub armor_speed_penalty: f32,
    pub heal_hp_per_sec: f32,
    pub heal_cost_multiplier: f32,
    pub repro_threshold_frac: f32,
    pub offspring_endowment_frac: f32,
    pub movement_desire: f32,
    pub turn_chance_per_sec: f32,
    pub turn_degrees_max: f32,
    pub max_speed_decoded: f32,
    pub max_speed_effective: f32,
    pub movement_cost_multiplier: f32,
    pub photo_gene_eff: f32,
    pub biomass_gene_eff: f32,
    pub minerals_gene_eff: f32,
    pub photo_effective: f32,
    pub biomass_effective: f32,
    pub minerals_effective: f32,
    pub metabolism_demand: f32,
    pub sensor_mask: u8,
    pub sensor_eye_count: u32,
    pub sensor_reach_per_eye: f32,
    pub sensor_width_deg_per_eye: f32,
    pub pursue_slot_count: u8,
    pub avoid_slot_count: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeErrorV2 {
    Genome(GenomeError),
}

impl fmt::Display for DecodeErrorV2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeErrorV2::Genome(err) => write!(f, "phenotype v2 decode failed: {err}"),
        }
    }
}

impl std::error::Error for DecodeErrorV2 {}

impl From<GenomeError> for DecodeErrorV2 {
    fn from(err: GenomeError) -> Self {
        DecodeErrorV2::Genome(err)
    }
}

pub fn u16_fraction(raw: u16) -> f32 {
    raw as f32 / 65535.0
}

pub fn byte_fraction(raw: u8, scale: f32) -> f32 {
    raw as f32 / scale
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

pub fn triad_coupling(others: &[f32], mode: TriadCouplingMode) -> f32 {
    if others.is_empty() {
        return 0.0;
    }
    match mode {
        TriadCouplingMode::MaxOther => others.iter().copied().fold(0.0_f32, f32::max),
        TriadCouplingMode::MeanOther => others.iter().sum::<f32>() / others.len() as f32,
        TriadCouplingMode::ProductOther => others.iter().product(),
    }
}

pub fn apply_triad(
    gene_eff: f32,
    others: &[f32],
    triad: &GeneticsTriadConfig,
) -> f32 {
    let cross = triad_coupling(others, triad.coupling_mode);
    (gene_eff * (1.0 - triad.coupling_strength * cross)).clamp(0.0, 1.0)
}

pub fn sensor_eye_count(mask: u8) -> u32 {
    mask.count_ones()
}

pub fn decode(genome: &GenomeV2, config: &SimConfig) -> Result<PhenotypeV2, DecodeErrorV2> {
    genome.validate(&config.genetics)?;

    let d = config.genetics.require_decode_v2().map_err(|e| {
        DecodeErrorV2::Genome(GenomeError::Config(e.to_string()))
    })?;
    let triad = config.genetics.require_triad().map_err(|e| {
        DecodeErrorV2::Genome(GenomeError::Config(e.to_string()))
    })?;

    let r = genome.color_r()?;
    let g = genome.color_g()?;
    let b = genome.color_b()?;
    let color_rgb = [r, g, b];
    let color = [
        r as f32 / d.channel_max,
        g as f32 / d.channel_max,
        b as f32 / d.channel_max,
    ];

    let body_t = u16_fraction(genome.body_radius_raw()?);
    let body_radius = lerp(d.radius_at_zero, d.radius_at_max, body_t);
    let radius_energy_bonus = body_t * d.energy_bonus_at_max;
    let radius_metabolism_bonus = body_t * d.metabolism_bonus_at_max;

    let armor_t = u16_fraction(genome.armor_raw()?);
    let armor_stroke = armor_t * d.stroke_max;
    let armor_metabolism_factor = 1.0 + armor_t * d.metabolism_pct_at_max;
    let armor_speed_penalty = armor_t * d.speed_penalty_at_max;

    let heal_t = u16_fraction(genome.healing_rate_raw()?);
    let heal_hp_per_sec =
        d.heal_base_hp_per_sec * lerp(1.0, d.heal_speed_mult_at_max, heal_t);
    let heal_cost_multiplier = lerp(1.0, d.heal_cost_mult_at_max, heal_t);

    let ratio_byte = genome.energy_health_ratio_raw()?;
    let energy_cap_fraction = lerp(
        d.energy_cap_frac_at_byte0,
        d.energy_cap_frac_at_byte255,
        byte_fraction(ratio_byte, d.byte_scale),
    );

    let vitality_budget = if d.vitality_budget_from_radius {
        radius_energy_bonus
    } else {
        d.energy_bonus_at_max
    };
    let max_energy = vitality_budget * energy_cap_fraction;
    let max_health = vitality_budget * (1.0 - energy_cap_fraction);

    let repro_threshold_frac = byte_fraction(genome.repro_threshold_raw()?, d.byte_scale);
    let offspring_endowment_frac =
        byte_fraction(genome.offspring_endowment_raw()?, d.byte_scale);
    let movement_desire = byte_fraction(genome.movement_desire_raw()?, d.byte_scale);

    let turn_raw = genome.turning_desire_raw()?;
    let turn_high = turn_raw >> 4;
    let turn_low = turn_raw & 0x0F;
    let nibble_denom = (d.nibble_levels - 1).max(1) as f32;
    let turn_chance_per_sec = (turn_high as f32 / nibble_denom) * d.turn_chance_max_per_sec;
    let turn_degrees_max = (turn_low as f32 / nibble_denom) * d.turn_degrees_max;

    let max_speed_decoded =
        byte_fraction(genome.max_speed_raw()?, d.byte_scale) * d.max_speed_at_max_byte;
    let max_speed_effective =
        (max_speed_decoded * (1.0 - armor_speed_penalty)).max(0.0);

    let move_eff = byte_fraction(genome.movement_efficiency_raw()?, d.byte_scale);
    let movement_cost_multiplier = 1.0 - move_eff * d.movement_cost_reduction_at_max;

    let photo_gene_eff = byte_fraction(genome.photo_efficiency_raw()?, d.byte_scale);
    let biomass_gene_eff = byte_fraction(genome.biomass_efficiency_raw()?, d.byte_scale);
    let minerals_gene_eff = byte_fraction(genome.minerals_efficiency_raw()?, d.byte_scale);

    let photo_effective = apply_triad(
        photo_gene_eff,
        &[biomass_gene_eff, minerals_gene_eff],
        triad,
    );
    let biomass_effective = apply_triad(
        biomass_gene_eff,
        &[photo_gene_eff, minerals_gene_eff],
        triad,
    );
    let minerals_effective = apply_triad(
        minerals_gene_eff,
        &[photo_gene_eff, biomass_gene_eff],
        triad,
    );

    let metabolism_demand = radius_metabolism_bonus * armor_metabolism_factor;

    let sensor_mask = genome.sensor_mask_raw()?;
    let sensor_strength = genome.sensor_strength_raw()?;
    let sensor_eye_count = sensor_eye_count(sensor_mask);
    let strength_high = sensor_strength >> 4;
    let strength_low = sensor_strength & 0x0F;
    let eyes = sensor_eye_count.max(1) as f32;
    let sensor_reach_per_eye =
        (strength_high as f32 / nibble_denom) * d.sensor_distance_at_high_nibble_f / eyes;
    let sensor_width_deg_per_eye =
        (strength_low as f32 / nibble_denom) * d.sensor_width_deg_at_low_nibble_f / eyes;

    Ok(PhenotypeV2 {
        color,
        color_rgb,
        body_radius,
        radius_metabolism_bonus,
        vitality_budget,
        max_energy,
        max_health,
        energy_cap_fraction,
        armor_stroke,
        armor_metabolism_factor,
        armor_speed_penalty,
        heal_hp_per_sec,
        heal_cost_multiplier,
        repro_threshold_frac,
        offspring_endowment_frac,
        movement_desire,
        turn_chance_per_sec,
        turn_degrees_max,
        max_speed_decoded,
        max_speed_effective,
        movement_cost_multiplier,
        photo_gene_eff,
        biomass_gene_eff,
        minerals_gene_eff,
        photo_effective,
        biomass_effective,
        minerals_effective,
        metabolism_demand,
        sensor_mask,
        sensor_eye_count,
        sensor_reach_per_eye,
        sensor_width_deg_per_eye,
        pursue_slot_count: genome.pursue_slot_count()?,
        avoid_slot_count: genome.avoid_slot_count()?,
    })
}

impl PhenotypeV2 {
    /// Photosynthesis gain per second before light and tick scaling.
    pub fn photo_gain_per_sec(&self, decode: &GeneticsDecodeV2) -> f32 {
        decode.photo_rate_per_sec_per_radius * self.body_radius * self.photo_effective
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::genome::encoding_v2::GenomeV2;

    fn v2_config() -> SimConfig {
        SimConfig::load_from_path(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs/genome_v2.example.toml"),
        )
        .unwrap()
    }

    #[test]
    fn ancestor_golden_decode() {
        let config = v2_config();
        let genome = GenomeV2::ancestor_from_config(&config).unwrap();
        let p = decode(&genome, &config).unwrap();
        assert_eq!(p.color_rgb, [0x00, 0xFF, 0x05]);
        assert!(p.body_radius >= 4.0 && p.body_radius < 5.0);
        assert_eq!(p.max_speed_effective, 0.0);
        assert!((p.repro_threshold_frac - 1.0).abs() < 1e-6);
        assert!((p.offspring_endowment_frac - 15.0 / 255.0).abs() < 0.01);
        assert!(p.photo_effective > 0.9);
        assert_eq!(p.biomass_effective, 0.0);
        assert_eq!(p.minerals_effective, 0.0);
        assert!((p.energy_cap_fraction - 0.5).abs() < 0.05);
        assert_eq!(p.sensor_eye_count, 0);
        assert_eq!(p.pursue_slot_count, 0);
        assert_eq!(p.avoid_slot_count, 0);
    }

    #[test]
    fn ratio_byte_ff_shifts_caps_toward_health() {
        let config = v2_config();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        let idx = crate::genome::encoding_v2::HEADER_LEN + crate::genome::encoding_v2::core::ENERGY_HEALTH_RATIO;
        bytes[idx] = 0x00;
        let energy_heavy = decode(
            &GenomeV2::from_bytes(bytes.clone(), &config.genetics).unwrap(),
            &config,
        )
        .unwrap();
        bytes[idx] = 0xFF;
        let health_heavy = decode(&GenomeV2::from_bytes(bytes, &config.genetics).unwrap(), &config)
            .unwrap();
        assert!(energy_heavy.max_energy > health_heavy.max_energy);
        assert!(health_heavy.max_health > energy_heavy.max_health);
    }

    #[test]
    fn triad_max_other_reduces_photo_when_biomass_high() {
        let config = v2_config();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        let photo_idx =
            crate::genome::encoding_v2::HEADER_LEN + crate::genome::encoding_v2::core::PHOTO_EFFICIENCY;
        let biomass_idx =
            crate::genome::encoding_v2::HEADER_LEN + crate::genome::encoding_v2::core::BIOMASS_EFFICIENCY;
        bytes[photo_idx] = 0xFF;
        bytes[biomass_idx] = 0x00;
        let solo = decode(&GenomeV2::from_bytes(bytes.clone(), &config.genetics).unwrap(), &config)
            .unwrap();
        bytes[biomass_idx] = 0xFF;
        let coupled = decode(&GenomeV2::from_bytes(bytes, &config.genetics).unwrap(), &config).unwrap();
        assert!(coupled.photo_effective < solo.photo_effective);
    }

    #[test]
    fn color_does_not_cap_photo_gene_efficiency() {
        let config = v2_config();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        let g_idx =
            crate::genome::encoding_v2::HEADER_LEN + crate::genome::encoding_v2::core::COLOR_G;
        let photo_idx =
            crate::genome::encoding_v2::HEADER_LEN + crate::genome::encoding_v2::core::PHOTO_EFFICIENCY;
        bytes[g_idx] = 0x00;
        bytes[photo_idx] = 0xFF;
        let p = decode(&GenomeV2::from_bytes(bytes, &config.genetics).unwrap(), &config).unwrap();
        assert!(
            (p.photo_gene_eff - 1.0).abs() < 1e-5,
            "photo gene eff should be full range regardless of color_g nibble"
        );
    }
}

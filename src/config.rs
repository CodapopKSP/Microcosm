//! Simulation configuration loaded from TOML (`configs/*.toml`).
#![allow(dead_code)] // Most fields are consumed in Phase 1B+ systems.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Supported genome encoder version (see [GENOME.md](../docs/GENOME.md)).
pub const SUPPORTED_GENOME_VERSION: u8 = 1;

/// Default config path relative to the crate manifest (works with `cargo run` from any cwd).
pub fn default_config_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs/default.toml")
}

#[derive(Debug, Clone, Deserialize)]
pub struct SimConfig {
    pub world: WorldConfig,
    pub performance: PerformanceConfig,
    pub genetics: GeneticsConfig,
    pub economy: EconomyConfig,
    pub environment: EnvironmentConfig,
    pub run: RunConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorldConfig {
    pub width: f32,
    pub height: f32,
    pub boundary: BoundaryMode,
    pub initial_population: usize,
    /// PoC agent radius until genome-driven `body_radius` (Phase 1E).
    #[serde(default = "default_agent_radius")]
    pub default_agent_radius: f32,
    #[serde(default = "default_min_speed")]
    pub min_speed: f32,
    #[serde(default = "default_max_speed")]
    pub max_speed: f32,
}

fn default_agent_radius() -> f32 {
    12.0
}
fn default_min_speed() -> f32 {
    10.0
}
fn default_max_speed() -> f32 {
    30.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryMode {
    Bounce,
    Wrap,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PerformanceConfig {
    pub max_entities: usize,
    pub failsafe_policy: FailsafePolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailsafePolicy {
    StopSpawns,
    DespawnLowestEnergy,
    Pause,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GeneticsConfig {
    pub genome_version: u8,
    pub core_length: usize,
    pub ancestor_hex: String,
    #[serde(default)]
    pub p_bit_flip: f32,
    #[serde(default)]
    pub p_point: f32,
    #[serde(default)]
    pub p_resample: f32,
    #[serde(default)]
    pub p_dup: f32,
    #[serde(default)]
    pub p_del: f32,
    #[serde(default = "default_mutations_per_offspring")]
    pub mutations_per_offspring: u32,
    #[serde(default = "default_delta_max")]
    pub delta_max: u8,
    pub decode_ranges: DecodeRanges,
    /// Greenness niche: photosynthesis buff + metabolism penalty ([PHASE_4.md](../docs/PHASE_4.md) §4.7).
    #[serde(default)]
    pub color_photosynthesis: ColorPhotosynthesisConfig,
}

/// Color-driven photosynthesis tradeoff (Phase 4G).
#[derive(Debug, Clone, Deserialize)]
pub struct ColorPhotosynthesisConfig {
    #[serde(default = "default_greenness_scale")]
    pub greenness_scale: f32,
    #[serde(default = "default_greenness_max")]
    pub greenness_max: f32,
    #[serde(default)]
    pub k_green_photo: f32,
    #[serde(default)]
    pub k_green_meta: f32,
    #[serde(default = "default_light_ref")]
    pub light_ref: f32,
}

fn default_greenness_scale() -> f32 {
    0.5
}

fn default_greenness_max() -> f32 {
    1.0
}

fn default_light_ref() -> f32 {
    1.0
}

impl Default for ColorPhotosynthesisConfig {
    fn default() -> Self {
        Self {
            greenness_scale: default_greenness_scale(),
            greenness_max: default_greenness_max(),
            k_green_photo: 0.0,
            k_green_meta: 0.0,
            light_ref: default_light_ref(),
        }
    }
}

impl ColorPhotosynthesisConfig {
    /// §4.7 tradeoff: at `light_ref`, buff from `photo_strength_ref = 1.0` exceeds metabolism penalty.
    pub fn validate_tradeoff(&self, solar_constant: f32) -> Result<(), ConfigError> {
        if self.k_green_photo <= 0.0 && self.k_green_meta <= 0.0 {
            return Ok(());
        }
        if !self.greenness_scale.is_finite() || self.greenness_scale <= 0.0 {
            return Err(ConfigError::Validation(
                "genetics.color_photosynthesis.greenness_scale must be positive and finite".into(),
            ));
        }
        if !self.greenness_max.is_finite() || self.greenness_max < 0.0 {
            return Err(ConfigError::Validation(
                "genetics.color_photosynthesis.greenness_max must be non-negative and finite".into(),
            ));
        }
        if self.k_green_photo < 0.0
            || self.k_green_meta < 0.0
            || !self.k_green_photo.is_finite()
            || !self.k_green_meta.is_finite()
        {
            return Err(ConfigError::Validation(
                "genetics.color_photosynthesis k_green_photo and k_green_meta must be non-negative and finite"
                    .into(),
            ));
        }
        if !self.light_ref.is_finite() || self.light_ref < 0.0 {
            return Err(ConfigError::Validation(
                "genetics.color_photosynthesis.light_ref must be non-negative and finite".into(),
            ));
        }
        if self.k_green_meta > 0.0 {
            let buff = self.k_green_photo * self.light_ref * 1.0 * solar_constant;
            if buff <= self.k_green_meta {
                return Err(ConfigError::Validation(format!(
                    "genetics.color_photosynthesis: k_green_photo * light_ref * solar_constant ({buff:.6}) \
                     must exceed k_green_meta ({})",
                    self.k_green_meta
                )));
            }
        }
        Ok(())
    }
}

fn default_mutations_per_offspring() -> u32 {
    2
}
fn default_delta_max() -> u8 {
    8
}

#[derive(Debug, Clone, Deserialize)]
pub struct DecodeRanges {
    pub body_radius_min: f32,
    pub body_radius_max: f32,
    pub photo_max: f32,
    pub k_photo: f32,
    pub meta_min: f32,
    pub meta_max: f32,
    pub loco_max: f32,
    /// `movement_pace` locus decode range (bytes 24–25).
    #[serde(alias = "speed_min")]
    pub pace_min: f32,
    #[serde(alias = "speed_max")]
    pub pace_max: f32,
    /// `repair_rate` locus decode range (bytes 20–21).
    pub repair_rate_min: f32,
    pub repair_rate_max: f32,
    pub e_min: f32,
    pub e_max: f32,
    pub k_armor: f32,
    pub k_move_hardness: f32,
    pub chemo_cliff: f32,
    pub v_base: f32,
    pub k_v: f32,
    pub m_base: f32,
    pub k_m: f32,
    pub k_drag: f32,
    pub r_ref: f32,
    pub k_move_meta: f32,
    /// Gene locus scales logarithmic age metabolism (`age_metabolism_scale_min`..`max`).
    pub age_metabolism_scale_min: f32,
    pub age_metabolism_scale_max: f32,
    /// Armor mitigation constant: `h / (h + k_mitigate)` (Phase 4B).
    pub k_mitigate: f32,
    /// Scales chemosynthetic vent gain (Phase 5B).
    #[serde(default = "default_k_chemo")]
    pub k_chemo: f32,
}

fn default_k_chemo() -> f32 {
    1.0
}

/// HP repair ordering when energy is available ([PHASE_4.md](../docs/PHASE_4.md) §4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairPolicy {
    CoreFirst,
    Cheapest,
    Symmetric,
}

fn default_repair_policy() -> RepairPolicy {
    RepairPolicy::CoreFirst
}

#[derive(Debug, Clone, Deserialize)]
pub struct EconomyConfig {
    pub metabolism_floor: f32,
    pub spawn_energy: f32,
    pub flat_energy_grant_per_tick: f32,
    pub core_max_hp: f32,
    pub repro_safety_margin: f32,
    /// Ticks before another reproduction attempt after success or stillbirth (config-only; byte 16 reserved).
    pub repro_cooldown_ticks: u32,
    pub p_death_on_repro_base: f32,
    pub p_damage_on_repro_base: f32,
    /// Fraction of [`Phenotype::repro_threshold`] applied when a repro damage roll succeeds.
    pub repro_damage_fraction_of_max_hp: f32,
    pub repair_energy_per_hp: f32,
    /// Max HP restored per tick at `repair_rate = 1.0` (throughput cap; energy still `hp * repair_energy_per_hp`).
    #[serde(default = "default_repair_hp_per_tick_at_rate_1")]
    pub repair_hp_per_tick_at_rate_1: f32,
    /// Log-age metabolism reaches this rate (energy/s) at `age_metabolism_cap_ticks` (60 Hz).
    pub age_metabolism_cap_per_sec: f32,
    pub age_metabolism_cap_ticks: f32,
    pub age_metabolism_ref_ticks: f32,
    #[serde(default = "default_repair_policy")]
    pub repair_policy: RepairPolicy,
    /// Collision damage scale: `raw = collision_damage_scale * relative_speed` (Phase 4B).
    pub collision_damage_scale: f32,
    /// Movement energy (J/s) at reference body size when `speed_ratio = move_energy_half_speed_ratio`.
    #[serde(default = "default_move_energy_per_sec_at_half_speed")]
    pub move_energy_per_sec_at_half_speed: f32,
    /// Speed ratio anchor for `move_energy_per_sec_at_half_speed` (default 0.5 = half max speed).
    #[serde(default = "default_move_energy_half_speed_ratio")]
    pub move_energy_half_speed_ratio: f32,
}

fn default_move_energy_per_sec_at_half_speed() -> f32 {
    1.0
}

fn default_move_energy_half_speed_ratio() -> f32 {
    0.5
}

fn default_repair_hp_per_tick_at_rate_1() -> f32 {
    50.0
}

/// Axis-aligned circular hazard zone ([PHASE_4.md](../docs/PHASE_4.md) §4.1).
#[derive(Debug, Clone, Deserialize)]
pub struct HazardZone {
    pub center: [f32; 2],
    pub radius: f32,
    pub damage_per_tick: f32,
}

/// Axis-aligned circular thermal vent ([PHASE_5.md](../docs/PHASE_5.md) §5.1).
#[derive(Debug, Clone, Deserialize)]
pub struct VentZone {
    pub center: [f32; 2],
    pub radius: f32,
    pub vent_flux: f32,
}

/// World energy and light ([PHASE_2.md](../docs/PHASE_2.md) sub-phase 2A).
#[derive(Debug, Clone, Deserialize)]
pub struct EnvironmentConfig {
    pub energy_model: EnergyModel,
    pub solar_constant: f32,
    pub light_mode: LightMode,
    #[serde(default = "default_gradient_axis")]
    pub gradient_axis: GradientAxis,
    pub gradient_min: f32,
    pub gradient_max: f32,
    pub gradient_bright_at: GradientBrightAt,
    #[serde(default)]
    pub crowding_blocks_photosynthesis: bool,
    /// Grade photosynthesis by overlap area and cluster size (one sun per patch).
    #[serde(default = "default_overlap_shades_photosynthesis")]
    pub overlap_shades_photosynthesis: bool,
    /// Fraction of self-area overlap treated as lost light before cluster split.
    #[serde(default = "default_overlap_shade_factor")]
    pub overlap_shade_factor: f32,
    /// Circular hazard bands; empty ⇒ no zone damage (Phase 4A).
    #[serde(default)]
    pub hazards: Vec<HazardZone>,
    /// Circular thermal vents; empty ⇒ no chemo flux sample (Phase 5A).
    #[serde(default)]
    pub vents: Vec<VentZone>,
}

fn default_overlap_shades_photosynthesis() -> bool {
    true
}

fn default_overlap_shade_factor() -> f32 {
    0.5
}

fn default_gradient_axis() -> GradientAxis {
    GradientAxis::Y
}

/// Per-entity irradiance model ([ENERGY_AND_ENVIRONMENT.md](../docs/ENERGY_AND_ENVIRONMENT.md)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnergyModel {
    /// `photo_strength * light(pos) * solar_constant` (Phase 2+).
    Local,
    /// Phase 1 flat grant per tick (`economy.flat_energy_grant_per_tick`).
    LegacyFlat,
    /// Global solar pool — not implemented until Phase 6.
    Pool,
    /// Occlusion shading — not implemented until Phase 6.
    Spatial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightMode {
    Uniform,
    GradientY,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GradientAxis {
    Y,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GradientBrightAt {
    Top,
    Bottom,
}

impl EnvironmentConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        match self.energy_model {
            EnergyModel::Local | EnergyModel::LegacyFlat => {}
            EnergyModel::Pool => {
                return Err(ConfigError::Validation(
                    "environment.energy_model \"pool\" is not implemented yet (planned Phase 6); \
                     use \"local\" or \"legacy_flat\""
                        .into(),
                ));
            }
            EnergyModel::Spatial => {
                return Err(ConfigError::Validation(
                    "environment.energy_model \"spatial\" is not implemented yet (planned Phase 6); \
                     use \"local\" or \"legacy_flat\""
                        .into(),
                ));
            }
        }

        if !self.solar_constant.is_finite() || self.solar_constant < 0.0 {
            return Err(ConfigError::Validation(
                "environment.solar_constant must be a finite number >= 0".into(),
            ));
        }

        if !self.gradient_min.is_finite() || !self.gradient_max.is_finite() {
            return Err(ConfigError::Validation(
                "environment.gradient_min and gradient_max must be finite".into(),
            ));
        }

        if self.gradient_max < self.gradient_min {
            return Err(ConfigError::Validation(
                "environment.gradient_max must be >= gradient_min".into(),
            ));
        }

        if !self.overlap_shade_factor.is_finite()
            || !(0.0..=1.0).contains(&self.overlap_shade_factor)
        {
            return Err(ConfigError::Validation(
                "environment.overlap_shade_factor must be in [0, 1]".into(),
            ));
        }

        for (i, zone) in self.hazards.iter().enumerate() {
            if !zone.center[0].is_finite() || !zone.center[1].is_finite() {
                return Err(ConfigError::Validation(format!(
                    "environment.hazards[{i}].center must be finite"
                )));
            }
            if !zone.radius.is_finite() || zone.radius <= 0.0 {
                return Err(ConfigError::Validation(format!(
                    "environment.hazards[{i}].radius must be positive and finite"
                )));
            }
            if !zone.damage_per_tick.is_finite() || zone.damage_per_tick < 0.0 {
                return Err(ConfigError::Validation(format!(
                    "environment.hazards[{i}].damage_per_tick must be finite and >= 0"
                )));
            }
        }

        for (i, zone) in self.vents.iter().enumerate() {
            if !zone.center[0].is_finite() || !zone.center[1].is_finite() {
                return Err(ConfigError::Validation(format!(
                    "environment.vents[{i}].center must be finite"
                )));
            }
            if !zone.radius.is_finite() || zone.radius <= 0.0 {
                return Err(ConfigError::Validation(format!(
                    "environment.vents[{i}].radius must be positive and finite"
                )));
            }
            if !zone.vent_flux.is_finite() || zone.vent_flux < 0.0 {
                return Err(ConfigError::Validation(format!(
                    "environment.vents[{i}].vent_flux must be finite and >= 0"
                )));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunConfig {
    pub master_seed: Option<u64>,
    #[serde(default)]
    pub max_ticks: u64,
    #[serde(default = "default_snapshot_interval")]
    pub snapshot_interval: u64,
    #[serde(default = "default_metrics_path")]
    pub metrics_path: String,
}

fn default_snapshot_interval() -> u64 {
    60
}
fn default_metrics_path() -> String {
    "metrics.jsonl".into()
}

/// Errors from loading or validating configuration.
#[derive(Debug)]
pub enum ConfigError {
    Io { path: PathBuf, source: std::io::Error },
    Parse { path: PathBuf, source: toml::de::Error },
    Validation(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io { path, source } => {
                write!(f, "failed to read config '{}': {}", path.display(), source)
            }
            ConfigError::Parse { path, source } => {
                write!(f, "invalid TOML in '{}': {}", path.display(), source)
            }
            ConfigError::Validation(msg) => write!(f, "config validation failed: {msg}"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::Io { source, .. } => Some(source),
            ConfigError::Parse { source, .. } => Some(source),
            ConfigError::Validation(_) => None,
        }
    }
}

impl SimConfig {
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref().to_path_buf();
        let contents = fs::read_to_string(&path).map_err(|source| ConfigError::Io {
            path: path.clone(),
            source,
        })?;
        let config: SimConfig = toml::from_str(&contents).map_err(|source| ConfigError::Parse {
            path: path.clone(),
            source,
        })?;
        config.validate()?;
        Ok(config)
    }

    pub fn load_default() -> Result<Self, ConfigError> {
        Self::load_from_path(default_config_path())
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.genetics.genome_version != SUPPORTED_GENOME_VERSION {
            return Err(ConfigError::Validation(format!(
                "unsupported genetics.genome_version {} (expected {SUPPORTED_GENOME_VERSION})",
                self.genetics.genome_version
            )));
        }

        if self.genetics.core_length == 0 {
            return Err(ConfigError::Validation(
                "genetics.core_length must be greater than 0".into(),
            ));
        }

        let ancestor = self.genetics.parse_ancestor_bytes()?;
        if ancestor.len() != self.genetics.core_length {
            return Err(ConfigError::Validation(format!(
                "genetics.ancestor_hex decodes to {} bytes, but genetics.core_length is {}",
                ancestor.len(),
                self.genetics.core_length
            )));
        }

        crate::genome::Genome::ancestor_from_genetics(&self.genetics)
            .map_err(|e| ConfigError::Validation(e.to_string()))?;

        if self.world.width <= 0.0 || self.world.height <= 0.0 {
            return Err(ConfigError::Validation(
                "world.width and world.height must be positive".into(),
            ));
        }

        if self.world.initial_population == 0 {
            return Err(ConfigError::Validation(
                "world.initial_population must be greater than 0".into(),
            ));
        }

        if self.world.default_agent_radius <= 0.0 {
            return Err(ConfigError::Validation(
                "world.default_agent_radius must be positive".into(),
            ));
        }

        if self.world.min_speed < 0.0 || self.world.max_speed <= self.world.min_speed {
            return Err(ConfigError::Validation(
                "world.min_speed and world.max_speed must satisfy 0 <= min < max".into(),
            ));
        }

        let dr = &self.genetics.decode_ranges;
        if dr.body_radius_max <= dr.body_radius_min {
            return Err(ConfigError::Validation(
                "genetics.decode_ranges.body_radius_max must exceed body_radius_min".into(),
            ));
        }

        if dr.pace_max <= dr.pace_min {
            return Err(ConfigError::Validation(
                "genetics.decode_ranges.pace_max must exceed pace_min".into(),
            ));
        }

        if dr.repair_rate_max <= dr.repair_rate_min {
            return Err(ConfigError::Validation(
                "genetics.decode_ranges.repair_rate_max must exceed repair_rate_min".into(),
            ));
        }

        if self.economy.repair_hp_per_tick_at_rate_1 <= 0.0 {
            return Err(ConfigError::Validation(
                "economy.repair_hp_per_tick_at_rate_1 must be positive".into(),
            ));
        }

        if dr.meta_max <= dr.meta_min {
            return Err(ConfigError::Validation(
                "genetics.decode_ranges.meta_max must exceed meta_min".into(),
            ));
        }

        if dr.e_max <= dr.e_min {
            return Err(ConfigError::Validation(
                "genetics.decode_ranges.e_max must exceed e_min".into(),
            ));
        }

        if self.economy.repro_cooldown_ticks == 0 {
            return Err(ConfigError::Validation(
                "economy.repro_cooldown_ticks must be positive".into(),
            ));
        }

        if dr.age_metabolism_scale_max <= dr.age_metabolism_scale_min {
            return Err(ConfigError::Validation(
                "genetics.decode_ranges.age_metabolism_scale_max must exceed age_metabolism_scale_min"
                    .into(),
            ));
        }

        if self.economy.age_metabolism_cap_ticks <= 0.0
            || !self.economy.age_metabolism_cap_ticks.is_finite()
        {
            return Err(ConfigError::Validation(
                "economy.age_metabolism_cap_ticks must be positive and finite".into(),
            ));
        }

        if self.economy.age_metabolism_ref_ticks <= 0.0
            || !self.economy.age_metabolism_ref_ticks.is_finite()
        {
            return Err(ConfigError::Validation(
                "economy.age_metabolism_ref_ticks must be positive and finite".into(),
            ));
        }

        if self.economy.age_metabolism_cap_per_sec < 0.0
            || !self.economy.age_metabolism_cap_per_sec.is_finite()
        {
            return Err(ConfigError::Validation(
                "economy.age_metabolism_cap_per_sec must be non-negative and finite".into(),
            ));
        }

        if self.economy.p_damage_on_repro_base < 0.0
            || self.economy.p_damage_on_repro_base > 1.0
            || !self.economy.p_damage_on_repro_base.is_finite()
        {
            return Err(ConfigError::Validation(
                "economy.p_damage_on_repro_base must be in [0, 1]".into(),
            ));
        }

        if self.economy.repro_damage_fraction_of_max_hp < 0.0
            || !self.economy.repro_damage_fraction_of_max_hp.is_finite()
        {
            return Err(ConfigError::Validation(
                "economy.repro_damage_fraction_of_max_hp must be non-negative and finite".into(),
            ));
        }

        if dr.k_mitigate < 0.0 || !dr.k_mitigate.is_finite() {
            return Err(ConfigError::Validation(
                "genetics.decode_ranges.k_mitigate must be non-negative and finite".into(),
            ));
        }

        if dr.k_chemo < 0.0 || !dr.k_chemo.is_finite() {
            return Err(ConfigError::Validation(
                "genetics.decode_ranges.k_chemo must be non-negative and finite".into(),
            ));
        }

        if self.economy.collision_damage_scale < 0.0
            || !self.economy.collision_damage_scale.is_finite()
        {
            return Err(ConfigError::Validation(
                "economy.collision_damage_scale must be non-negative and finite".into(),
            ));
        }

        if self.economy.move_energy_per_sec_at_half_speed < 0.0
            || !self.economy.move_energy_per_sec_at_half_speed.is_finite()
        {
            return Err(ConfigError::Validation(
                "economy.move_energy_per_sec_at_half_speed must be non-negative and finite".into(),
            ));
        }

        if self.economy.move_energy_half_speed_ratio <= 0.0
            || self.economy.move_energy_half_speed_ratio > 1.0
            || !self.economy.move_energy_half_speed_ratio.is_finite()
        {
            return Err(ConfigError::Validation(
                "economy.move_energy_half_speed_ratio must be in (0, 1]".into(),
            ));
        }

        self.environment.validate()?;
        self.genetics
            .color_photosynthesis
            .validate_tradeoff(self.environment.solar_constant)?;

        Ok(())
    }

    pub fn is_seeded(&self) -> bool {
        self.run.master_seed.is_some()
    }
}

impl GeneticsConfig {
    /// Parse `ancestor_hex` (optional `0x` prefix, whitespace ignored).
    pub fn parse_ancestor_bytes(&self) -> Result<Vec<u8>, ConfigError> {
        parse_hex_bytes(&self.ancestor_hex)
    }
}

fn parse_hex_bytes(hex: &str) -> Result<Vec<u8>, ConfigError> {
    let stripped: String = hex
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    let stripped = stripped.strip_prefix("0x").unwrap_or(&stripped);
    if stripped.is_empty() {
        return Err(ConfigError::Validation(
            "genetics.ancestor_hex is empty".into(),
        ));
    }
    if !stripped.len().is_multiple_of(2) {
        return Err(ConfigError::Validation(
            "genetics.ancestor_hex must have an even number of hex digits".into(),
        ));
    }
    if !stripped.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ConfigError::Validation(
            "genetics.ancestor_hex contains non-hex characters".into(),
        ));
    }

    (0..stripped.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&stripped[i..i + 2], 16).map_err(|_| {
                ConfigError::Validation("genetics.ancestor_hex invalid hex byte".into())
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hazards_default_empty_when_key_missing() {
        let toml = r#"
            energy_model = "local"
            solar_constant = 1.0
            light_mode = "uniform"
            gradient_min = 0.0
            gradient_max = 1.0
            gradient_bright_at = "top"
        "#;
        let env: EnvironmentConfig = toml::from_str(toml).unwrap();
        assert!(env.hazards.is_empty());
    }

    #[test]
    fn vents_default_empty_when_key_missing() {
        let toml = r#"
            energy_model = "local"
            solar_constant = 1.0
            light_mode = "uniform"
            gradient_min = 0.0
            gradient_max = 1.0
            gradient_bright_at = "top"
        "#;
        let env: EnvironmentConfig = toml::from_str(toml).unwrap();
        assert!(env.vents.is_empty());
    }

    #[test]
    fn crowding_flag_off_by_default() {
        let config = SimConfig::load_default().expect("default.toml should load");
        assert!(
            !config.environment.crowding_blocks_photosynthesis,
            "binary overlap blocking must be off in default config (Phase 2 canonical)"
        );
    }

    #[test]
    fn repair_policy_defaults_to_core_first() {
        let toml = r#"
            metabolism_floor = 0.1
            spawn_energy = 50.0
            flat_energy_grant_per_tick = 0.0
            core_max_hp = 100.0
            repro_safety_margin = 0.25
            repro_cooldown_ticks = 192
            p_death_on_repro_base = 0.15
            p_damage_on_repro_base = 0.2
            repro_damage_fraction_of_max_hp = 0.1
            repair_energy_per_hp = 1.0
            collision_damage_scale = 0.15
            age_metabolism_cap_per_sec = 1.0
            age_metabolism_cap_ticks = 18000.0
            age_metabolism_ref_ticks = 3600.0
        "#;
        let economy: EconomyConfig = toml::from_str(toml).unwrap();
        assert_eq!(economy.repair_policy, RepairPolicy::CoreFirst);
    }

    #[test]
    fn color_photosynthesis_defaults_when_section_missing() {
        let toml = r#"
            genome_version = 1
            core_length = 32
            ancestor_hex = "45564f3188cc446dffff300028000af640000af6008000005555000055558000"
            [decode_ranges]
            body_radius_min = 6.0
            body_radius_max = 20.0
            photo_max = 3.0
            k_photo = 0.008
            meta_min = 0.001
            meta_max = 0.03
            loco_max = 1.0
            pace_min = 0.5
            pace_max = 2.0
            repair_rate_min = 0.5
            repair_rate_max = 2.0
            e_min = 15.0
            e_max = 2000.0
            k_armor = 0.08
            k_move_hardness = 0.02
            chemo_cliff = 0.35
            v_base = 10.0
            k_v = 18.0
            m_base = 0.0
            k_m = 0.0
            k_drag = 0.0
            r_ref = 12.0
            k_move_meta = 0.0
            age_metabolism_scale_min = 0.5
            age_metabolism_scale_max = 2.0
            k_mitigate = 0.5
        "#;
        let genetics: GeneticsConfig = toml::from_str(toml).unwrap();
        assert_eq!(genetics.color_photosynthesis.k_green_photo, 0.0);
        assert_eq!(genetics.color_photosynthesis.k_green_meta, 0.0);
        assert_eq!(genetics.color_photosynthesis.greenness_scale, 0.5);
    }

    #[test]
    fn hazard_lab_toml_loads() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs/hazard_lab.toml");
        let config = SimConfig::load_from_path(&path).expect("hazard_lab.toml should load");
        assert_eq!(config.environment.hazards.len(), 1);
        assert!(config.environment.hazards[0].damage_per_tick > 0.01);
    }

    #[test]
    fn vent_lab_toml_loads() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs/vent_lab.toml");
        let config = SimConfig::load_from_path(&path).expect("vent_lab.toml should load");
        assert_eq!(config.environment.vents.len(), 1);
        assert!(config.environment.vents[0].vent_flux > 0.01);
        let genome = crate::genome::Genome::ancestor_from_config(&config).unwrap();
        let phenotype = crate::genome::decode(&genome, &config).unwrap();
        assert!(
            phenotype.is_chemo_above_cliff(config.genetics.decode_ranges.chemo_cliff),
            "vent_lab ancestor should be above chemo cliff"
        );
    }

    #[test]
    fn load_default_toml() {
        let config = SimConfig::load_default().expect("default.toml should load");
        assert_eq!(config.genetics.genome_version, SUPPORTED_GENOME_VERSION);
        assert_eq!(config.genetics.core_length, 32);
        assert_eq!(config.world.initial_population, 80);
        assert!(config.is_seeded());
        assert_eq!(config.environment.energy_model, EnergyModel::Local);
        assert_eq!(config.economy.flat_energy_grant_per_tick, 0.0);
        assert_eq!(config.economy.repair_policy, RepairPolicy::CoreFirst);
        assert_eq!(config.environment.light_mode, LightMode::GradientY);
        assert!(!config.environment.crowding_blocks_photosynthesis);
        assert!(config.genetics.color_photosynthesis.k_green_photo > 0.0);
        assert!(
            config.genetics.color_photosynthesis.k_green_meta
                < config.genetics.color_photosynthesis.k_green_photo
                    * config.genetics.color_photosynthesis.light_ref
                    * config.environment.solar_constant
        );
    }

    #[test]
    fn rejects_unimplemented_energy_model_pool() {
        let err = load_config_with_environment(|e| e.energy_model = EnergyModel::Pool)
            .expect_err("pool should be rejected at validate");
        assert!(matches!(err, ConfigError::Validation(_)));
        assert!(err.to_string().contains("pool"));
    }

    #[test]
    fn accepts_energy_model_local() {
        let config = load_config_with_environment(|e| e.energy_model = EnergyModel::Local)
            .expect("local should validate");
        assert_eq!(config.environment.energy_model, EnergyModel::Local);
    }

    #[test]
    fn rejects_unknown_light_mode_at_parse() {
        let path = default_config_path();
        let mut contents = fs::read_to_string(&path).unwrap();
        contents = contents.replace("light_mode = \"gradient_y\"", "light_mode = \"map\"");
        let err = toml::from_str::<SimConfig>(&contents).unwrap_err();
        assert!(err.to_string().contains("light_mode"));
    }

    #[test]
    fn rejects_invalid_gradient_range() {
        let err = load_config_with_environment(|e| {
            e.gradient_min = 1.0;
            e.gradient_max = 0.2;
        })
        .expect_err("gradient_max < gradient_min");
        assert!(matches!(err, ConfigError::Validation(_)));
    }

    fn load_config_with_environment(
        patch: impl FnOnce(&mut EnvironmentConfig),
    ) -> Result<SimConfig, ConfigError> {
        let mut config = SimConfig::load_default()?;
        patch(&mut config.environment);
        config.validate()?;
        Ok(config)
    }

    #[test]
    fn rejects_unsupported_genome_version() {
        let path = default_config_path();
        let mut contents = fs::read_to_string(&path).unwrap();
        contents = contents.replace("genome_version = 1", "genome_version = 99");
        let config: SimConfig = toml::from_str(&contents).unwrap();
        let err = config.validate().unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)));
        assert!(err.to_string().contains("genome_version"));
    }

    #[test]
    fn rejects_invalid_hex() {
        let path = default_config_path();
        let mut contents = fs::read_to_string(&path).unwrap();
        contents = contents.replace(
            &contents
                .lines()
                .find(|l| l.starts_with("ancestor_hex"))
                .unwrap()
                .to_string(),
            "ancestor_hex = \"ZZ\"",
        );
        let config: SimConfig = toml::from_str(&contents).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn ancestor_matches_core_length() {
        let config = SimConfig::load_default().unwrap();
        let bytes = config.genetics.parse_ancestor_bytes().unwrap();
        assert_eq!(bytes.len(), config.genetics.core_length);
        assert_eq!(&bytes[0..4], b"EVO1");
    }
}

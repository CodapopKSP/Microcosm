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
    pub speed_min: f32,
    pub speed_max: f32,
    pub e_min: f32,
    pub e_max: f32,
    pub cd_min: f32,
    pub cd_max: f32,
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
}

#[derive(Debug, Clone, Deserialize)]
pub struct EconomyConfig {
    pub metabolism_floor: f32,
    pub spawn_energy: f32,
    pub flat_energy_grant_per_tick: f32,
    pub core_max_hp: f32,
    pub repro_safety_margin: f32,
    pub p_death_on_repro_base: f32,
    pub repair_energy_per_hp: f32,
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
    /// Phase 1 flat grant per tick (until Model A gain is wired).
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

        if dr.speed_max <= dr.speed_min {
            return Err(ConfigError::Validation(
                "genetics.decode_ranges.speed_max must exceed speed_min".into(),
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

        if dr.cd_max <= dr.cd_min {
            return Err(ConfigError::Validation(
                "genetics.decode_ranges.cd_max must exceed cd_min".into(),
            ));
        }

        self.environment.validate()?;

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
    fn crowding_flag_off_by_default() {
        let config = SimConfig::load_default().expect("default.toml should load");
        assert!(
            !config.environment.crowding_blocks_photosynthesis,
            "binary overlap blocking must be off in default config (Phase 2 canonical)"
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
        assert_eq!(config.environment.light_mode, LightMode::GradientY);
        assert!(!config.environment.crowding_blocks_photosynthesis);
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

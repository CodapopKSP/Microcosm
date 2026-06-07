//! Simulation configuration loaded from TOML (`configs/*.toml`).
#![allow(dead_code)] // Most fields are consumed in Phase 1B+ systems.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Genome encoder ([Genome_Definition.md](../docs/Genome_Definition.md)).
pub const GENOME_VERSION_V2: u8 = 2;

/// Default `genetics.genome_version` for [`SimConfig::load_default`] (EVO2).
pub const SUPPORTED_GENOME_VERSION: u8 = GENOME_VERSION_V2;

pub fn is_supported_genome_version(version: u8) -> bool {
    version == GENOME_VERSION_V2
}

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
    #[serde(default)]
    pub physics: WorldPhysicsConfig,
}

/// Passive forces applied each tick from body size (Brownian vs gravity).
#[derive(Debug, Clone, Deserialize)]
pub struct WorldPhysicsConfig {
    /// Bodies below this radius jitter; at or above sink downward.
    #[serde(default = "default_physics_radius_threshold")]
    pub radius_threshold: f32,
    /// Max random drift speed for small bodies (world units / sec).
    #[serde(default = "default_brownian_speed")]
    pub brownian_speed: f32,
    /// Downward drift speed for large bodies (world units / sec).
    #[serde(default = "default_gravity_speed")]
    pub gravity_speed: f32,
}

impl Default for WorldPhysicsConfig {
    fn default() -> Self {
        Self {
            radius_threshold: default_physics_radius_threshold(),
            brownian_speed: default_brownian_speed(),
            gravity_speed: default_gravity_speed(),
        }
    }
}

impl WorldPhysicsConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !self.radius_threshold.is_finite() || self.radius_threshold <= 0.0 {
            return Err(ConfigError::Validation(
                "world.physics.radius_threshold must be positive and finite".into(),
            ));
        }
        if !self.brownian_speed.is_finite() || self.brownian_speed < 0.0 {
            return Err(ConfigError::Validation(
                "world.physics.brownian_speed must be non-negative and finite".into(),
            ));
        }
        if !self.gravity_speed.is_finite() || self.gravity_speed < 0.0 {
            return Err(ConfigError::Validation(
                "world.physics.gravity_speed must be non-negative and finite".into(),
            ));
        }
        Ok(())
    }
}

fn default_physics_radius_threshold() -> f32 {
    5.0
}

fn default_brownian_speed() -> f32 {
    1.2
}

fn default_gravity_speed() -> f32 {
    10.0
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
    /// Agents removed per failsafe pass when [`FailsafePolicy::DespawnLowestEnergy`] is active.
    #[serde(default = "default_failsafe_cull_count")]
    pub failsafe_cull_count: usize,
    /// Emergency corpse cap; when exceeded, [`corpse_cull_count`](Self::corpse_cull_count) despawn.
    #[serde(default = "default_corpse_max")]
    pub corpse_max: usize,
    /// Corpses removed per pass when count exceeds [`corpse_max`](Self::corpse_max) (lowest biomass first).
    #[serde(default = "default_corpse_cull_count")]
    pub corpse_cull_count: usize,
}

fn default_failsafe_cull_count() -> usize {
    3000
}

fn default_corpse_max() -> usize {
    1000
}

fn default_corpse_cull_count() -> usize {
    500
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
    /// Required when `genome_version = 2` ([CONFIG.md](../docs/CONFIG.md) `[genetics.decode_v2]`).
    #[serde(default)]
    pub decode_v2: Option<GeneticsDecodeV2>,
    #[serde(default)]
    pub triad: Option<GeneticsTriadConfig>,
    #[serde(default)]
    pub behavior: Option<GeneticsBehaviorConfig>,
    #[serde(default = "default_max_genome_bytes")]
    pub max_genome_bytes: usize,
    #[serde(default = "default_max_pursue_slots")]
    pub max_pursue_slots: u8,
    #[serde(default = "default_max_avoid_slots")]
    pub max_avoid_slots: u8,
    /// Required when `genome_version = 2` ([genetics.mutation_v2]).
    #[serde(default)]
    pub mutation_v2: Option<MutationV2Config>,
}

/// Gene-level ±increment mutation for EVO2 offspring.
#[derive(Debug, Clone, Deserialize)]
pub struct MutationV2Config {
    pub passes_min: u32,
    pub passes_max: u32,
    pub p_large_step: f32,
    pub large_step_min: u32,
    pub large_step_max: u32,
    /// Scales ±increment on the body-radius u16 locus only (decode span unchanged).
    #[serde(default = "default_body_radius_step_multiplier")]
    pub body_radius_step_multiplier: f32,
}

fn default_body_radius_step_multiplier() -> f32 {
    1.0
}

impl MutationV2Config {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.passes_min > self.passes_max {
            return Err(ConfigError::Validation(
                "genetics.mutation_v2.passes_min must not exceed passes_max".into(),
            ));
        }
        if self.p_large_step < 0.0 || self.p_large_step > 1.0 || !self.p_large_step.is_finite() {
            return Err(ConfigError::Validation(
                "genetics.mutation_v2.p_large_step must be in [0, 1]".into(),
            ));
        }
        if self.large_step_min < 1 || self.large_step_max < self.large_step_min {
            return Err(ConfigError::Validation(
                "genetics.mutation_v2.large_step_min must be >= 1 and <= large_step_max".into(),
            ));
        }
        if self.body_radius_step_multiplier <= 0.0 || !self.body_radius_step_multiplier.is_finite()
        {
            return Err(ConfigError::Validation(
                "genetics.mutation_v2.body_radius_step_multiplier must be positive and finite"
                    .into(),
            ));
        }
        Ok(())
    }
}

fn default_max_genome_bytes() -> usize {
    512
}
fn default_max_pursue_slots() -> u8 {
    1
}
fn default_max_avoid_slots() -> u8 {
    1
}

/// Decode scales for EVO2 ([CONFIG.md](../docs/CONFIG.md) `[genetics.decode_v2]`).
#[derive(Debug, Clone, Deserialize)]
pub struct GeneticsDecodeV2 {
    pub nibble_levels: u32,
    pub channel_max: f32,
    pub radius_at_zero: f32,
    pub radius_at_max: f32,
    pub energy_bonus_at_max: f32,
    pub metabolism_bonus_at_max: f32,
    pub stroke_max: f32,
    pub metabolism_pct_at_max: f32,
    pub speed_penalty_at_max: f32,
    pub heal_base_hp_per_sec: f32,
    pub heal_speed_mult_at_max: f32,
    pub heal_cost_mult_at_max: f32,
    pub energy_cap_frac_at_byte0: f32,
    pub energy_cap_frac_at_byte255: f32,
    pub vitality_budget_from_radius: bool,
    pub byte_scale: f32,
    pub turn_chance_max_per_sec: f32,
    pub turn_degrees_max: f32,
    pub max_speed_at_max_byte: f32,
    pub movement_cost_reduction_at_max: f32,
    pub photo_rate_per_sec_per_radius: f32,
    pub sensor_distance_at_high_nibble_f: f32,
    pub sensor_width_deg_at_low_nibble_f: f32,
    pub sensor_ray_count: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriadCouplingMode {
    MaxOther,
    MeanOther,
    ProductOther,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GeneticsTriadConfig {
    pub coupling_mode: TriadCouplingMode,
    pub coupling_strength: f32,
}

fn default_sensor_ray_interval_ticks() -> u32 {
    60
}

fn default_sensor_ray_phase_ticks() -> u32 {
    0
}

#[derive(Debug, Clone, Deserialize)]
pub struct GeneticsBehaviorConfig {
    pub fuzzy_nibble_delta: u8,
    pub avoid_flee_heading_offset_deg: f32,
    pub sensor_upkeep_per_eye_per_tick: f32,
    pub sensor_upkeep_strength_scale: f32,
    /// How often [`sense_rays`](crate::sim::systems::sense_rays) runs; 1 = every tick. Behavior reuses the last scan between updates.
    #[serde(default = "default_sensor_ray_interval_ticks")]
    pub sensor_ray_interval_ticks: u32,
    /// Tick offset within the interval (default 0 — stagger from biomass feeding via its phase).
    #[serde(default = "default_sensor_ray_phase_ticks")]
    pub sensor_ray_phase_ticks: u32,
}

impl GeneticsBehaviorConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.sensor_ray_interval_ticks == 0 {
            return Err(ConfigError::Validation(
                "genetics.behavior.sensor_ray_interval_ticks must be at least 1".into(),
            ));
        }
        if self.sensor_ray_phase_ticks >= self.sensor_ray_interval_ticks {
            return Err(ConfigError::Validation(
                "genetics.behavior.sensor_ray_phase_ticks must be < sensor_ray_interval_ticks"
                    .into(),
            ));
        }
        if self.sensor_upkeep_per_eye_per_tick < 0.0 || !self.sensor_upkeep_per_eye_per_tick.is_finite()
        {
            return Err(ConfigError::Validation(
                "genetics.behavior.sensor_upkeep_per_eye_per_tick must be finite and >= 0".into(),
            ));
        }
        Ok(())
    }
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

/// Order parents pay reproduction endowment from current vitality pools (v2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReproPaymentOrder {
    HealthFirst,
    #[default]
    EnergyFirst,
    Proportional,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EconomyConfig {
    pub metabolism_floor: f32,
    pub spawn_energy: f32,
    /// Fraction of decoded max health at spawn (v2); v1 uses [`core_max_hp`](EconomyConfig::core_max_hp) from phenotype decode.
    #[serde(default = "default_spawn_health_frac")]
    pub spawn_health_frac: f32,
    pub flat_energy_grant_per_tick: f32,
    pub core_max_hp: f32,
    pub repro_safety_margin: f32,
    /// Ticks before another reproduction attempt after success or stillbirth (config-only; byte 16 reserved).
    pub repro_cooldown_ticks: u32,
    pub p_death_on_repro_base: f32,
    pub p_damage_on_repro_base: f32,
    /// Fraction of [`Phenotype::repro_threshold`] applied when a repro damage roll succeeds.
    pub repro_damage_fraction_of_max_hp: f32,
    /// v2: which parent pool is spent first when paying offspring endowment.
    #[serde(default)]
    pub repro_payment_order: ReproPaymentOrder,
    pub repair_energy_per_hp: f32,
    /// Max HP restored per tick at `repair_rate = 1.0` (throughput cap; energy still `hp * repair_energy_per_hp`).
    #[serde(default = "default_repair_hp_per_tick_at_rate_1")]
    pub repair_hp_per_tick_at_rate_1: f32,
    /// Log-age metabolism multiplier at `age_metabolism_cap_ticks` (1.0 at birth; e.g. 2.0 = double demand).
    pub age_metabolism_cap_multiplier: f32,
    pub age_metabolism_cap_ticks: f32,
    pub age_metabolism_ref_ticks: f32,
    #[serde(default = "default_repair_policy")]
    pub repair_policy: RepairPolicy,
    /// Movement energy (J/s) at reference body size when `speed_ratio = move_energy_half_speed_ratio`.
    #[serde(default = "default_move_energy_per_sec_at_half_speed")]
    pub move_energy_per_sec_at_half_speed: f32,
    /// Speed ratio anchor for `move_energy_per_sec_at_half_speed` (default 0.5 = half max speed).
    #[serde(default = "default_move_energy_half_speed_ratio")]
    pub move_energy_half_speed_ratio: f32,
    /// v2 move calibration: energy per `distance / (this × body_radius)` at efficiency multiplier 1.0.
    #[serde(default = "default_move_energy_units_per_radius")]
    pub move_energy_units_per_radius: f32,
}

fn default_move_energy_per_sec_at_half_speed() -> f32 {
    1.0
}

fn default_move_energy_half_speed_ratio() -> f32 {
    0.5
}

fn default_move_energy_units_per_radius() -> f32 {
    10.0
}

fn default_repair_hp_per_tick_at_rate_1() -> f32 {
    50.0
}

fn default_spawn_health_frac() -> f32 {
    1.0
}

/// Axis-aligned circular hazard zone ([PHASE_4.md](../docs/PHASE_4.md) §4.1).
#[derive(Debug, Clone, Deserialize)]
pub struct HazardZone {
    pub center: [f32; 2],
    pub radius: f32,
    pub damage_per_tick: f32,
    /// Local chemosynthetic flux while inside the zone (summed like vent flux; default 0).
    #[serde(default)]
    pub chem_flux: f32,
}

/// Small hazard patches that spawn and despawn randomly in a world-Y band (typically near the top).
#[derive(Debug, Clone, Deserialize)]
pub struct DynamicHazardsConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Minimum world Y for hazard centers.
    pub band_y_min: f32,
    /// Maximum world Y for hazard centers.
    pub band_y_max: f32,
    pub radius: f32,
    pub damage_per_tick: f32,
    #[serde(default)]
    pub chem_flux: f32,
    pub max_active: u32,
    /// Sim ticks between spawn attempts when below [`max_active`](Self::max_active).
    pub spawn_interval_ticks: u32,
    pub lifetime_min_ticks: u32,
    pub lifetime_max_ticks: u32,
}

fn default_dynamic_hazards() -> DynamicHazardsConfig {
    DynamicHazardsConfig {
        enabled: false,
        band_y_min: 0.0,
        band_y_max: 0.0,
        radius: 40.0,
        damage_per_tick: 0.0,
        chem_flux: 0.0,
        max_active: 0,
        spawn_interval_ticks: 120,
        lifetime_min_ticks: 180,
        lifetime_max_ticks: 600,
    }
}

impl DynamicHazardsConfig {
    pub fn validate(&self, half_x: f32, half_y: f32) -> Result<(), ConfigError> {
        if !self.enabled {
            return Ok(());
        }
        if !self.band_y_min.is_finite() || !self.band_y_max.is_finite() {
            return Err(ConfigError::Validation(
                "environment.dynamic_hazards.band_y_min and band_y_max must be finite".into(),
            ));
        }
        if self.band_y_max < self.band_y_min {
            return Err(ConfigError::Validation(
                "environment.dynamic_hazards.band_y_max must be >= band_y_min".into(),
            ));
        }
        if !self.radius.is_finite() || self.radius <= 0.0 {
            return Err(ConfigError::Validation(
                "environment.dynamic_hazards.radius must be positive and finite".into(),
            ));
        }
        if !self.damage_per_tick.is_finite() || self.damage_per_tick < 0.0 {
            return Err(ConfigError::Validation(
                "environment.dynamic_hazards.damage_per_tick must be finite and >= 0".into(),
            ));
        }
        if !self.chem_flux.is_finite() || self.chem_flux < 0.0 {
            return Err(ConfigError::Validation(
                "environment.dynamic_hazards.chem_flux must be finite and >= 0".into(),
            ));
        }
        if self.max_active == 0 {
            return Err(ConfigError::Validation(
                "environment.dynamic_hazards.max_active must be at least 1 when enabled".into(),
            ));
        }
        if self.spawn_interval_ticks == 0 {
            return Err(ConfigError::Validation(
                "environment.dynamic_hazards.spawn_interval_ticks must be at least 1".into(),
            ));
        }
        if self.lifetime_min_ticks == 0 || self.lifetime_max_ticks < self.lifetime_min_ticks {
            return Err(ConfigError::Validation(
                "environment.dynamic_hazards lifetime_min_ticks must be >= 1 and \
                 lifetime_max_ticks must be >= lifetime_min_ticks"
                    .into(),
            ));
        }
        let margin = self.radius;
        if self.band_y_min < -half_y + margin || self.band_y_max > half_y - margin {
            return Err(ConfigError::Validation(format!(
                "environment.dynamic_hazards band must stay inside world Y \
                 [{}, {}] with radius margin",
                -half_y + margin,
                half_y - margin
            )));
        }
        if margin >= half_x {
            return Err(ConfigError::Validation(format!(
                "environment.dynamic_hazards.radius ({}) must be smaller than world half-width ({half_x})",
                self.radius
            )));
        }
        Ok(())
    }
}

/// Axis-aligned circular thermal vent ([PHASE_5.md](../docs/PHASE_5.md) §5.1).
#[derive(Debug, Clone, Deserialize)]
pub struct VentZone {
    pub center: [f32; 2],
    pub radius: f32,
    pub vent_flux: f32,
}

/// v2 ingestion and environment sampling ([Genome_Definition.md](../docs/Genome_Definition.md), G7).
#[derive(Debug, Clone, Deserialize)]
pub struct FeedingConfig {
    /// Maps summed vent flux at a point to mineral concentration in `[0, 1]`.
    #[serde(default = "default_minerals_vent_flux_scale")]
    pub minerals_vent_flux_scale: f32,
    /// Energy gained per tick at full minerals digestion and concentration `1.0`.
    #[serde(default = "default_minerals_uptake_per_tick")]
    pub minerals_uptake_per_tick: f32,
    /// Health-energy units per second per body-radius unit at 100% biomass digestion
    /// (`uptake/sec = body_radius × biomass_effective × this value`).
    #[serde(default = "default_biomass_uptake_per_radius_per_sec")]
    pub biomass_uptake_per_radius_per_sec: f32,
    /// How often corpse/prey biomass feeding runs; default 60 = once per second at 60 Hz.
    #[serde(default = "default_biomass_feeding_interval_ticks")]
    pub biomass_feeding_interval_ticks: u32,
    /// Tick offset within the interval (default 30 — half second after sensor rays at 60 Hz).
    #[serde(default = "default_biomass_feeding_phase_ticks")]
    pub biomass_feeding_phase_ticks: u32,
    #[serde(default = "default_corpse_ttl_ticks")]
    pub corpse_ttl_ticks: u32,
    /// Fraction of the dead agent's max health stored as corpse consumable energy (1.0 = full pool).
    #[serde(default = "default_corpse_energy_fraction_of_max_health")]
    pub corpse_energy_fraction_of_max_health: f32,
    /// Global soup pool (Phase 6 minimal scalar field).
    #[serde(default)]
    pub soup_enabled: bool,
    #[serde(default = "default_soup_regen_per_tick")]
    pub soup_regen_per_tick: f32,
    #[serde(default = "default_soup_max_concentration")]
    pub soup_max_concentration: f32,
    #[serde(default = "default_soup_initial_concentration")]
    pub soup_initial_concentration: f32,
    /// Config-only uptake multiplier (transporter loci are future).
    #[serde(default = "default_soup_uptake_rate")]
    pub soup_uptake_rate: f32,
}

fn default_minerals_vent_flux_scale() -> f32 {
    25.0
}
fn default_minerals_uptake_per_tick() -> f32 {
    0.08
}
fn default_biomass_uptake_per_radius_per_sec() -> f32 {
    10.0
}
fn default_biomass_feeding_interval_ticks() -> u32 {
    60
}

fn default_biomass_feeding_phase_ticks() -> u32 {
    30
}
fn default_corpse_ttl_ticks() -> u32 {
    3600
}
fn default_corpse_energy_fraction_of_max_health() -> f32 {
    0.25
}
fn default_soup_regen_per_tick() -> f32 {
    0.0005
}
fn default_soup_max_concentration() -> f32 {
    1.0
}
fn default_soup_initial_concentration() -> f32 {
    0.0
}
fn default_soup_uptake_rate() -> f32 {
    0.04
}

fn default_feeding_config() -> FeedingConfig {
    FeedingConfig {
        minerals_vent_flux_scale: default_minerals_vent_flux_scale(),
        minerals_uptake_per_tick: default_minerals_uptake_per_tick(),
        biomass_uptake_per_radius_per_sec: default_biomass_uptake_per_radius_per_sec(),
        biomass_feeding_interval_ticks: default_biomass_feeding_interval_ticks(),
        biomass_feeding_phase_ticks: default_biomass_feeding_phase_ticks(),
        corpse_ttl_ticks: default_corpse_ttl_ticks(),
        corpse_energy_fraction_of_max_health: default_corpse_energy_fraction_of_max_health(),
        soup_enabled: false,
        soup_regen_per_tick: default_soup_regen_per_tick(),
        soup_max_concentration: default_soup_max_concentration(),
        soup_initial_concentration: default_soup_initial_concentration(),
        soup_uptake_rate: default_soup_uptake_rate(),
    }
}

impl FeedingConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        for (name, v) in [
            ("minerals_vent_flux_scale", self.minerals_vent_flux_scale),
            ("minerals_uptake_per_tick", self.minerals_uptake_per_tick),
            ("biomass_uptake_per_radius_per_sec", self.biomass_uptake_per_radius_per_sec),
            (
                "corpse_energy_fraction_of_max_health",
                self.corpse_energy_fraction_of_max_health,
            ),
            ("soup_regen_per_tick", self.soup_regen_per_tick),
            ("soup_max_concentration", self.soup_max_concentration),
            ("soup_initial_concentration", self.soup_initial_concentration),
            ("soup_uptake_rate", self.soup_uptake_rate),
        ] {
            if !v.is_finite() || v < 0.0 {
                return Err(ConfigError::Validation(format!(
                    "environment.feeding.{name} must be finite and >= 0"
                )));
            }
        }
        if self.soup_max_concentration <= 0.0 {
            return Err(ConfigError::Validation(
                "environment.feeding.soup_max_concentration must be positive".into(),
            ));
        }
        if self.soup_initial_concentration > self.soup_max_concentration {
            return Err(ConfigError::Validation(
                "environment.feeding.soup_initial_concentration must be <= soup_max_concentration"
                    .into(),
            ));
        }
        if self.corpse_energy_fraction_of_max_health > 1.0 {
            return Err(ConfigError::Validation(
                "environment.feeding.corpse_energy_fraction_of_max_health must be <= 1".into(),
            ));
        }
        if self.biomass_feeding_interval_ticks == 0 {
            return Err(ConfigError::Validation(
                "environment.feeding.biomass_feeding_interval_ticks must be at least 1".into(),
            ));
        }
        if self.biomass_feeding_phase_ticks >= self.biomass_feeding_interval_ticks {
            return Err(ConfigError::Validation(
                "environment.feeding.biomass_feeding_phase_ticks must be < biomass_feeding_interval_ticks"
                    .into(),
            ));
        }
        Ok(())
    }
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
    /// World Y where light equals [`gradient_min`](Self::gradient_min). Omitted ⇒ shaded world edge.
    #[serde(default)]
    pub gradient_min_y: Option<f32>,
    pub gradient_bright_at: GradientBrightAt,
    /// Circular hazard bands; empty ⇒ no zone damage (Phase 4A).
    #[serde(default)]
    pub hazards: Vec<HazardZone>,
    /// Ephemeral hazard patches spawned randomly in a Y band (typically near the map top).
    #[serde(default = "default_dynamic_hazards")]
    pub dynamic_hazards: DynamicHazardsConfig,
    /// Circular thermal vents; empty ⇒ no chemo flux sample (Phase 5A).
    #[serde(default)]
    pub vents: Vec<VentZone>,
    /// Mineral field for signature `0x0201` (G7).
    #[serde(default)]
    pub minerals_enabled: bool,
    /// Corpse markers for signature `0x0200` (G7).
    #[serde(default)]
    pub corpses_enabled: bool,
    #[serde(default = "default_feeding_config")]
    pub feeding: FeedingConfig,
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
    pub fn validate(&self, half_x: f32, half_y: f32) -> Result<(), ConfigError> {
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

        if let Some(y) = self.gradient_min_y {
            if !y.is_finite() {
                return Err(ConfigError::Validation(
                    "environment.gradient_min_y must be finite".into(),
                ));
            }
            match self.gradient_bright_at {
                GradientBrightAt::Top if y >= half_y => {
                    return Err(ConfigError::Validation(format!(
                        "environment.gradient_min_y must be below the bright top edge \
                         (y < {half_y}) when gradient_bright_at = \"top\""
                    )));
                }
                GradientBrightAt::Bottom if y <= -half_y => {
                    return Err(ConfigError::Validation(format!(
                        "environment.gradient_min_y must be above the bright bottom edge \
                         (y > {}) when gradient_bright_at = \"bottom\"",
                        -half_y
                    )));
                }
                _ => {}
            }
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
            if !zone.chem_flux.is_finite() || zone.chem_flux < 0.0 {
                return Err(ConfigError::Validation(format!(
                    "environment.hazards[{i}].chem_flux must be finite and >= 0"
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

        self.feeding.validate()?;
        self.dynamic_hazards.validate(half_x, half_y)?;

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunConfig {
    pub master_seed: Option<u64>,
    #[serde(default)]
    pub max_ticks: u64,
    /// Cadence for non-movement simulation logic (metabolism, behavior, feeding except biomass, …).
    /// Movement and passive physics still run every tick.
    #[serde(default = "default_logic_interval_ticks")]
    pub logic_interval_ticks: u32,
    /// Tick offset within [`logic_interval_ticks`](Self::logic_interval_ticks).
    #[serde(default)]
    pub logic_interval_phase_ticks: u32,
    #[serde(default = "default_snapshot_interval")]
    pub snapshot_interval: u64,
    #[serde(default = "default_metrics_path")]
    pub metrics_path: String,
}

fn default_logic_interval_ticks() -> u32 {
    6
}

fn default_snapshot_interval() -> u64 {
    60
}
fn default_metrics_path() -> String {
    "metrics.jsonl".into()
}

impl RunConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.logic_interval_ticks == 0 {
            return Err(ConfigError::Validation(
                "run.logic_interval_ticks must be at least 1".into(),
            ));
        }
        if self.logic_interval_phase_ticks >= self.logic_interval_ticks {
            return Err(ConfigError::Validation(
                "run.logic_interval_phase_ticks must be < run.logic_interval_ticks".into(),
            ));
        }
        Ok(())
    }
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
        if !is_supported_genome_version(self.genetics.genome_version) {
            return Err(ConfigError::Validation(format!(
                "unsupported genetics.genome_version {} (only v2 supported)",
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

        let d2 = self.genetics.require_decode_v2()?;
        self.genetics.require_triad()?;
        self.genetics.require_behavior()?.validate()?;
        let mutation_v2 = self.genetics.require_mutation_v2()?;
        d2.validate()?;
        mutation_v2.validate()?;
        crate::genome::GenomeV2::ancestor_from_genetics(&self.genetics)
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

        self.world.physics.validate()?;

        if self.economy.repair_hp_per_tick_at_rate_1 <= 0.0 {
            return Err(ConfigError::Validation(
                "economy.repair_hp_per_tick_at_rate_1 must be positive".into(),
            ));
        }

        if self.economy.move_energy_units_per_radius <= 0.0
            || !self.economy.move_energy_units_per_radius.is_finite()
        {
            return Err(ConfigError::Validation(
                "economy.move_energy_units_per_radius must be positive and finite".into(),
            ));
        }

        if self.economy.repro_cooldown_ticks == 0 {
            return Err(ConfigError::Validation(
                "economy.repro_cooldown_ticks must be positive".into(),
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

        if self.economy.age_metabolism_cap_multiplier < 1.0
            || !self.economy.age_metabolism_cap_multiplier.is_finite()
        {
            return Err(ConfigError::Validation(
                "economy.age_metabolism_cap_multiplier must be finite and >= 1".into(),
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

        self.environment
            .validate(self.world.width * 0.5, self.world.height * 0.5)?;
        self.run.validate()?;

        if self.performance.max_entities == 0 {
            return Err(ConfigError::Validation(
                "performance.max_entities must be at least 1".into(),
            ));
        }
        if self.performance.failsafe_policy == FailsafePolicy::DespawnLowestEnergy
            && self.performance.failsafe_cull_count == 0
        {
            return Err(ConfigError::Validation(
                "performance.failsafe_cull_count must be at least 1 when failsafe_policy is \
                 \"despawn_lowest_energy\""
                    .into(),
            ));
        }
        if self.performance.corpse_max > 0 && self.performance.corpse_cull_count == 0 {
            return Err(ConfigError::Validation(
                "performance.corpse_cull_count must be at least 1 when performance.corpse_max is set"
                    .into(),
            ));
        }

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

    pub fn require_decode_v2(&self) -> Result<&GeneticsDecodeV2, ConfigError> {
        self.decode_v2.as_ref().ok_or_else(|| {
            ConfigError::Validation(
                "genetics.decode_v2 is required when genetics.genome_version = 2".into(),
            )
        })
    }

    pub fn require_triad(&self) -> Result<&GeneticsTriadConfig, ConfigError> {
        self.triad.as_ref().ok_or_else(|| {
            ConfigError::Validation(
                "genetics.triad is required when genetics.genome_version = 2".into(),
            )
        })
    }

    pub fn require_behavior(&self) -> Result<&GeneticsBehaviorConfig, ConfigError> {
        self.behavior.as_ref().ok_or_else(|| {
            ConfigError::Validation(
                "genetics.behavior is required when genetics.genome_version = 2".into(),
            )
        })
    }

    pub fn require_mutation_v2(&self) -> Result<&MutationV2Config, ConfigError> {
        self.mutation_v2.as_ref().ok_or_else(|| {
            ConfigError::Validation(
                "genetics.mutation_v2 is required when genetics.genome_version = 2".into(),
            )
        })
    }
}

impl GeneticsDecodeV2 {
    pub fn validate(&self) -> Result<(), ConfigError> {
        let positive = |name: &str, v: f32| -> Result<(), ConfigError> {
            if !v.is_finite() || v <= 0.0 {
                return Err(ConfigError::Validation(format!(
                    "genetics.decode_v2.{name} must be positive and finite"
                )));
            }
            Ok(())
        };
        if self.nibble_levels < 2 {
            return Err(ConfigError::Validation(
                "genetics.decode_v2.nibble_levels must be at least 2".into(),
            ));
        }
        positive("channel_max", self.channel_max)?;
        positive("radius_at_max", self.radius_at_max)?;
        if self.radius_at_max <= self.radius_at_zero {
            return Err(ConfigError::Validation(
                "genetics.decode_v2.radius_at_max must exceed radius_at_zero".into(),
            ));
        }
        positive("energy_bonus_at_max", self.energy_bonus_at_max)?;
        positive("metabolism_bonus_at_max", self.metabolism_bonus_at_max)?;
        positive("byte_scale", self.byte_scale)?;
        if self.energy_cap_frac_at_byte0 < 0.0
            || self.energy_cap_frac_at_byte0 > 1.0
            || self.energy_cap_frac_at_byte255 < 0.0
            || self.energy_cap_frac_at_byte255 > 1.0
        {
            return Err(ConfigError::Validation(
                "genetics.decode_v2 energy cap fractions must be in [0, 1]".into(),
            ));
        }
        if self.sensor_ray_count == 0 {
            return Err(ConfigError::Validation(
                "genetics.decode_v2.sensor_ray_count must be positive".into(),
            ));
        }
        Ok(())
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
            age_metabolism_cap_multiplier = 1.5
            age_metabolism_cap_ticks = 18000.0
            age_metabolism_ref_ticks = 3600.0
        "#;
        let economy: EconomyConfig = toml::from_str(toml).unwrap();
        assert_eq!(economy.repair_policy, RepairPolicy::CoreFirst);
    }

    #[test]
    fn mineral_vent_v2_toml_loads() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs/mineral_vent_v2.toml");
        let config = SimConfig::load_from_path(&path).expect("mineral_vent_v2.toml should load");
        assert!(config.environment.minerals_enabled);
        assert!(config.environment.corpses_enabled);
        assert!(config.environment.feeding.soup_enabled);
        let genome = crate::genome::GenomeV2::ancestor_from_config(&config).unwrap();
        let phenotype = crate::genome::decode_v2(&genome, &config).unwrap();
        assert!(phenotype.minerals_effective > 0.0);
    }

    #[test]
    fn genome_v2_example_toml_loads() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs/genome_v2.example.toml");
        let config = SimConfig::load_from_path(&path).expect("genome_v2.example.toml should load");
        assert_eq!(config.genetics.genome_version, GENOME_VERSION_V2);
        assert_eq!(config.genetics.core_length, 39);
        let genome = crate::genome::GenomeV2::ancestor_from_config(&config).unwrap();
        let phenotype = crate::genome::decode_v2(&genome, &config).unwrap();
        assert_eq!(phenotype.max_speed_effective, 0.0);
        assert!((phenotype.repro_threshold_frac - 1.0).abs() < 1e-6);
        assert!((phenotype.offspring_endowment_frac - 15.0 / 255.0).abs() < 0.02);
        assert!(phenotype.photo_effective > 0.0);
    }

    #[test]
    fn load_default_toml() {
        let config = SimConfig::load_default().expect("default.toml should load");
        assert_eq!(config.genetics.genome_version, GENOME_VERSION_V2);
        assert_eq!(config.genetics.core_length, 39);
        assert_eq!(config.world.initial_population, 10);
        assert!(config.is_seeded());
        assert_eq!(config.environment.energy_model, EnergyModel::Local);
        assert_eq!(config.economy.flat_energy_grant_per_tick, 0.0);
        assert_eq!(config.economy.repair_policy, RepairPolicy::CoreFirst);
        assert_eq!(config.environment.light_mode, LightMode::GradientY);
        assert!(config.genetics.decode_v2.is_some());
        assert!(config.genetics.triad.is_some());
        assert!(config.genetics.behavior.is_some());
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
        contents = contents.replace("genome_version = 2", "genome_version = 99");
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
    fn v2_default_ancestor_matches_core_length() {
        let config = SimConfig::load_default().unwrap();
        let bytes = config.genetics.parse_ancestor_bytes().unwrap();
        assert_eq!(bytes.len(), config.genetics.core_length);
        assert_eq!(&bytes[0..4], b"EVO2");
    }
}

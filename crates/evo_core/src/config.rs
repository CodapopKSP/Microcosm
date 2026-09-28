//! Simulation configuration. Every tunable number lives here.
//!
//! Defaults in this file must match `configs/default.toml` (enforced by a test).
//! TOML files may omit any key; missing keys take the default.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub world: WorldConfig,
    pub terrain: TerrainConfig,
    pub time: TimeConfig,
    pub light: LightConfig,
    pub temperature: TemperatureConfig,
    pub minerals: MineralsConfig,
    pub detritus: DetritusConfig,
    pub vents: VentsConfig,
    pub genes: GenesConfig,
    pub parts: PartsConfig,
    pub upkeep: UpkeepConfig,
    pub chemistry: ChemistryConfig,
    pub atmosphere: AtmosphereConfig,
    pub metabolism: MetabolismConfig,
    pub feeding: FeedingConfig,
    pub physics: PhysicsConfig,
    pub currents: CurrentsConfig,
    pub reproduction: ReproductionConfig,
    pub mutation: MutationConfig,
    pub species: SpeciesConfig,
    pub population: PopulationConfig,
    pub events: EventsConfig,
    pub stats: StatsConfig,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        Self::from_toml(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn from_toml(text: &str) -> Result<Self, String> {
        let config: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        let w = &self.world;
        if w.width < 1000.0 || w.height < 1000.0 {
            return Err("world width/height must be at least 1000".into());
        }
        for (name, cell) in [
            ("terrain.cell", self.terrain.cell),
            ("light.cell", self.light.cell),
            ("temperature.cell", self.temperature.cell),
            ("minerals.cell", self.minerals.cell),
            ("detritus.cell", self.detritus.cell),
        ] {
            if !(4.0..=512.0).contains(&cell) {
                return Err(format!("{name} must be between 4 and 512"));
            }
        }
        if self.time.dt <= 0.0 || self.time.dt > 0.2 {
            return Err("time.dt must be in (0, 0.2]".into());
        }
        crate::genome::Genome::parse(&self.population.ancestor)
            .map_err(|e| format!("population.ancestor: {e}"))?;
        if !self.population.vent_ancestor.trim().is_empty() {
            crate::genome::Genome::parse(&self.population.vent_ancestor)
                .map_err(|e| format!("population.vent_ancestor: {e}"))?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorldConfig {
    /// World width in units.
    pub width: f32,
    /// World height in units; the water surface is at the top.
    pub height: f32,
    /// Master seed: same seed + config ⇒ same run.
    pub seed: u64,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            width: 16000.0,
            height: 6000.0,
            seed: 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TerrainConfig {
    pub cell: f32,
    /// Mean seafloor height above the bottom of the world.
    pub floor_height: f32,
    /// Amplitude of seafloor bumps.
    pub floor_roughness: f32,
    /// The shallow shelf starts at this fraction of the width and extends to the right edge.
    pub shelf_start: f32,
    /// Depth of the shelf top below the surface.
    pub shelf_depth: f32,
    /// Trench center as a fraction of the width.
    pub trench_center: f32,
    /// Trench half-width as a fraction of the width.
    pub trench_width: f32,
    /// How far below the regular floor the trench reaches.
    pub trench_depth: f32,
    /// Number of rock outcrops (pillars, arches, boulders).
    pub rocks: u32,
    /// Typical rock outcrop size.
    pub rock_size: f32,
    /// Number of caves: hollow rock chambers with a narrow entrance, dark inside.
    pub caves: u32,
    /// Typical cave size.
    pub cave_size: f32,
    /// A vent system under the shelf (chamber, shaft, tunnel to the cliff) and a vent down the
    /// cliff, both permanent.
    pub shelf_vents: bool,
    /// A floating island in mid-water with tunnels and a permanent vent inside.
    pub island: bool,
    /// Island center as a fraction of the width, its center's depth, and its size.
    pub island_x: f32,
    pub island_depth: f32,
    pub island_width: f32,
    pub island_height: f32,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            cell: 8.0,
            floor_height: 1400.0,
            floor_roughness: 350.0,
            shelf_start: 0.78,
            shelf_depth: 900.0,
            trench_center: 0.3,
            trench_width: 0.06,
            trench_depth: 1100.0,
            rocks: 16,
            rock_size: 260.0,
            caves: 7,
            cave_size: 320.0,
            shelf_vents: true,
            island: true,
            island_x: 0.55,
            island_depth: 1500.0,
            island_width: 2400.0,
            island_height: 950.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TimeConfig {
    /// Sim seconds per tick. 1× playback runs `1/dt` ticks per real second.
    pub dt: f32,
    /// Length of a day/night cycle in sim seconds.
    pub day_length: f32,
    /// Days per year (one full cycle of seasons).
    pub year_length: f32,
}

impl Default for TimeConfig {
    fn default() -> Self {
        Self {
            dt: 1.0 / 30.0,
            day_length: 600.0,
            year_length: 6.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LightConfig {
    pub cell: f32,
    /// Energy per second per unit of width at the surface at noon (before seasons).
    pub surface_intensity: f32,
    /// Light level at midnight as a fraction of noon.
    pub night_level: f32,
    /// Seasonal swing of surface light (±fraction).
    pub season_amplitude: f32,
    /// Depth over which light halves in clear water.
    pub half_depth: f32,
    /// Light absorbed by any body regardless of pigment (shading).
    pub body_absorptance: f32,
    /// Extra absorptance at full photosynthetic capability.
    pub pigment_absorptance: f32,
    /// Energy gained per unit of light absorbed by pigment.
    pub photo_yield: f32,
}

impl Default for LightConfig {
    fn default() -> Self {
        Self {
            cell: 16.0,
            surface_intensity: 0.1,
            night_level: 0.2,
            season_amplitude: 0.3,
            half_depth: 400.0,
            body_absorptance: 0.1,
            pigment_absorptance: 0.8,
            photo_yield: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TemperatureConfig {
    pub cell: f32,
    /// Mean surface temperature (°C).
    pub surface: f32,
    /// Seasonal swing of surface temperature (±°C).
    pub season_amplitude: f32,
    /// Temperature of deep water (°C).
    pub deep: f32,
    /// Depth scale of the transition from surface to deep temperature.
    pub thermocline_depth: f32,
    /// Maximum climate drift (±°C) of the slow random walk.
    pub climate_amplitude: f32,
    /// Typical time for the climate to wander across its range, in days.
    pub climate_timescale: f32,
    /// Extra temperature at the center of a full-strength vent (°C).
    pub vent_heat: f32,
    /// Distance over which vent heat fades.
    pub vent_heat_radius: f32,
    /// Seconds between temperature field refreshes.
    pub update_interval: f32,
}

impl Default for TemperatureConfig {
    fn default() -> Self {
        Self {
            cell: 64.0,
            surface: 20.0,
            season_amplitude: 6.0,
            deep: 4.0,
            thermocline_depth: 700.0,
            climate_amplitude: 8.0,
            climate_timescale: 40.0,
            vent_heat: 45.0,
            vent_heat_radius: 260.0,
            update_interval: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MineralsConfig {
    pub cell: f32,
    /// Diffusion coefficient (units²/s).
    pub diffusion: f32,
    /// Fraction lost per second (precipitation).
    pub decay: f32,
    /// Upward drift inside vent plumes (units/s).
    pub plume_rise: f32,
    /// Half-width of a vent plume.
    pub plume_width: f32,
    /// Upward drift above a vent under a shaft open to the water (a chimney), units/s: its
    /// minerals shoot up the shaft and on to the sunlit water.
    pub chimney_rise: f32,
    /// Width of that fast-rising column.
    pub chimney_width: f32,
}

impl Default for MineralsConfig {
    fn default() -> Self {
        Self {
            cell: 32.0,
            diffusion: 250.0,
            decay: 0.0025,
            plume_rise: 4.0,
            plume_width: 160.0,
            chimney_rise: 30.0,
            chimney_width: 48.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DetritusConfig {
    pub cell: f32,
    /// Sinking speed of marine snow (units/s).
    pub sink_speed: f32,
    /// Fraction decomposed per second.
    pub decay: f32,
    /// Fraction of decomposed detritus released as minerals.
    pub to_minerals: f32,
    /// Diffusion coefficient (units²/s).
    pub diffusion: f32,
}

impl Default for DetritusConfig {
    fn default() -> Self {
        Self {
            cell: 32.0,
            sink_speed: 3.0,
            decay: 0.0015,
            to_minerals: 0.5,
            diffusion: 100.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VentsConfig {
    /// Target number of active vents (besides the permanent ones the terrain places).
    pub count: u32,
    /// Mineral output of a full-strength vent (energy/s).
    pub output: f32,
    /// Radius over which a vent releases minerals.
    pub radius: f32,
    /// Share of the output that seeps out over `halo_radius` around the vent instead, so
    /// dilute minerals reach oxygenated water nearby.
    pub halo: f32,
    pub halo_radius: f32,
    /// Vent water strips the oxygen from the water out to this distance.
    pub dead_zone: f32,
    /// Vent lifetime range in sim hours.
    pub lifetime_min: f32,
    pub lifetime_max: f32,
    /// Vents only open at least this deep below the surface.
    pub min_depth: f32,
    /// How many vents are kept shallow (between `min_depth` and `shallow_max_depth`): stepping
    /// stones between the sunlit water and the deep.
    pub shallow: u32,
    pub shallow_max_depth: f32,
    /// The other vents open at least this deep.
    pub deep_min_depth: f32,
}

impl Default for VentsConfig {
    fn default() -> Self {
        Self {
            count: 4,
            output: 30.0,
            radius: 48.0,
            halo: 0.35,
            halo_radius: 240.0,
            dead_zone: 60.0,
            lifetime_min: 2.0,
            lifetime_max: 8.0,
            min_depth: 700.0,
            shallow: 1,
            shallow_max_depth: 1800.0,
            deep_min_depth: 3000.0,
        }
    }
}

/// Constants that turn gene bytes into traits (see docs/DESIGN.md §2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GenesConfig {
    /// Radius at size gene 00.
    pub min_radius: f32,
    /// Size gene steps per doubling of radius.
    pub size_doubling: f32,
    /// Max speed = swimming strength (0..1, from parts) × speed_scale × √radius.
    pub speed_scale: f32,
    /// Lifespan base in seconds (× √radius × longevity factor).
    pub lifespan_base: f32,
    /// Maximum lifespan multiplier from longevity.
    pub longevity_max: f32,
    pub reserves_min: f32,
    pub reserves_max: f32,
    pub toughness_min: f32,
    pub toughness_max: f32,
    pub regen_min: f32,
    pub regen_max: f32,
    pub turn_min: f32,
    pub turn_max: f32,
    /// Absolute cap on sense range.
    pub sense_cap: f32,
    pub temp_max: f32,
    pub tolerance_min: f32,
    pub tolerance_max: f32,
    /// Extra cost of building body mass per additional body segment (fraction).
    pub segment_cost: f32,
    /// Diet generalist penalty: each diet loses this × the investment in the others
    /// (0.6 ⇒ two full diets give 40% each).
    pub generalist_penalty: f32,
}

impl Default for GenesConfig {
    fn default() -> Self {
        Self {
            min_radius: 1.0,
            size_doubling: 32.0,
            speed_scale: 20.0,
            lifespan_base: 300.0,
            longevity_max: 4.0,
            reserves_min: 0.3,
            reserves_max: 3.0,
            toughness_min: 0.3,
            toughness_max: 2.0,
            regen_min: 0.002,
            regen_max: 0.05,
            turn_min: 0.3,
            turn_max: 6.0,
            sense_cap: 1200.0,
            temp_max: 40.0,
            tolerance_min: 1.0,
            tolerance_max: 15.0,
            segment_cost: 0.1,
            generalist_penalty: 0.5,
        }
    }
}

/// How body parts work (per-kind details are in `parts.rs` and `traits.rs`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PartsConfig {
    /// Radius at which flagella and cilia lose most of their push (they suit microbes).
    pub small_swimmer_radius: f32,
    /// Radius around which fins and flukes become efficient (they suit big bodies).
    pub large_swimmer_radius: f32,
    /// Walking speed at full leg strength (× √radius), on rock or seafloor.
    pub crawl_speed: f32,
    /// Sight range of a full-size eye, in body radii.
    pub eye_range: f32,
    /// Smell range of full-strength antennae, in body radii (every direction).
    pub smell_range: f32,
    /// A tentacle grasps anything within this fraction of its length of its base (it bends).
    pub tentacle_reach: f32,
    /// Extra light-catching width per unit of frond length (in body radii).
    pub frond_span: f32,
    /// Extra nutrient-uptake surface per unit of frond length.
    pub frond_uptake: f32,
    /// Parts on the tip of a stalk work this much better (raised eyes, fronds, lures).
    pub stalk_bonus: f32,
    /// Light of a full-strength lure, as a fraction of surface noon light.
    pub lure_brightness: f32,
    /// Radius of a full-strength lure's glow.
    pub lure_radius: f32,
    /// Distance from rock (beyond the body) that still counts as touching it.
    pub contact_margin: f32,
    /// Every organism smells others this close (in body radii), even without antennae.
    pub base_smell: f32,
    /// Eyes need light: in darkness they see this fraction of their range…
    pub eye_dark: f32,
    /// …reaching full range at this light level (fraction of surface noon light).
    pub eye_light: f32,
}

impl Default for PartsConfig {
    fn default() -> Self {
        Self {
            small_swimmer_radius: 8.0,
            large_swimmer_radius: 6.0,
            crawl_speed: 12.0,
            eye_range: 40.0,
            smell_range: 16.0,
            tentacle_reach: 0.8,
            frond_span: 0.45,
            frond_uptake: 0.3,
            stalk_bonus: 1.3,
            lure_brightness: 0.3,
            lure_radius: 160.0,
            contact_margin: 2.0,
            base_smell: 6.0,
            eye_dark: 0.15,
            eye_light: 0.1,
        }
    }
}

/// Upkeep of each capability as a fraction of basal metabolism at full strength.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UpkeepConfig {
    pub buoyancy: f32,
    pub reserves: f32,
    pub longevity: f32,
    pub photo: f32,
    pub chemo: f32,
    pub scavenge: f32,
    pub hunt: f32,
    pub regen: f32,
    pub toughness: f32,
    /// Temperature tolerance.
    pub tolerance: f32,
    pub uptake: f32,
    pub fixation: f32,
    pub toxin: f32,
    pub resistance: f32,
    /// Per body segment beyond the first.
    pub segments: f32,
    /// Multiplier on the upkeep of body parts (each part kind has its own, see `parts.rs`).
    pub parts: f32,
}

impl Default for UpkeepConfig {
    fn default() -> Self {
        Self {
            buoyancy: 0.3,
            reserves: 0.3,
            longevity: 0.4,
            photo: 0.3,
            chemo: 0.2,
            scavenge: 0.3,
            hunt: 0.3,
            regen: 0.2,
            toughness: 0.3,
            tolerance: 0.4,
            uptake: 0.2,
            fixation: 0.2,
            toxin: 0.3,
            resistance: 0.3,
            segments: 0.05,
            parts: 1.5,
        }
    }
}

/// What organisms take from and give to the water (docs/DESIGN.md §4).
/// Oxygen and CO₂ are measured so that 1.0 is water in equilibrium with today's air.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ChemistryConfig {
    /// Oxygen released by photosynthesis or used by respiration, per unit of energy.
    pub oxygen_per_energy: f32,
    /// CO₂ used by photosynthesis or released by respiration, per unit of energy.
    pub co2_per_energy: f32,
    /// Below this oxygen level, aerobic respiration can't get enough.
    pub oxygen_half: f32,
    /// Extra cost of fermenting instead of breathing, per unit of work.
    pub ferment_penalty: f32,
    /// Fermentation capacity every organism has, however aerobic.
    pub ferment_floor: f32,
    /// Share of fermentation's extra cost that leaks out as organic waste (food for scavengers).
    pub ferment_waste: f32,
    /// Fraction of max HP lost per second when an organism can't meet its needs at all.
    pub suffocation_damage: f32,
    /// Fraction of max HP lost per second by strict fermenters per unit of oxygen above 0.5.
    pub oxygen_toxicity: f32,
    /// CO₂ level at which oxygen-producing photosynthesis slows (it runs at 1× at level 1).
    pub co2_half: f32,
    /// Minerals used per unit of energy by sulfur-based photosynthesis.
    pub sulfur_minerals: f32,
    /// Mineral concentration at which sulfur-based photosynthesis runs at half speed.
    pub sulfur_half: f32,
    /// Nutrients needed per unit of body mass built.
    pub nutrient_per_mass: f32,
    /// Nutrient store capacity per unit of body mass.
    pub nutrient_store: f32,
    /// Uptake per second = rate × gene × radius × surface × concentration.
    pub uptake_rate: f32,
    /// Uptake multiplier while anchored by a holdfast (it draws from the rock).
    pub anchor_uptake: f32,
    /// Nutrients fixed per second per unit of mass at full fixation.
    pub fixation_rate: f32,
    /// Energy per unit of nutrient fixed.
    pub fixation_cost: f32,
    /// Toxin leaked per second = rate × gene × radius.
    pub toxin_rate: f32,
    /// Fraction of max HP lost per second per unit of toxin concentration (÷ (1 + radius/8)).
    pub toxin_damage: f32,
    /// Fraction of toxin that breaks down per second.
    pub toxin_decay: f32,
    /// Damage to a hunter per unit of mass eaten from fully toxic prey (× its mass-to-HP).
    pub poison_damage: f32,
    /// Diffusion of dissolved gases, nutrients and toxins (units²/s).
    pub diffusion: f32,
    /// Fraction per second by which deep water relaxes toward its usual make-up
    /// (ocean circulation that the simulation doesn't model).
    pub ventilation: f32,
    /// Nutrient concentration of surface and deep water, and the depth scale between them.
    pub nutrient_surface: f32,
    pub nutrient_deep: f32,
    pub nutricline_depth: f32,
    /// Oxygen of deep water relative to the surface (it's older water).
    pub deep_oxygen: f32,
    /// CO₂ and nutrients released by a full-strength vent per second.
    pub vent_co2: f32,
    pub vent_nutrients: f32,
    /// Vents release oxygen-free water: fraction of oxygen removed per second near a vent.
    pub vent_oxygen_sink: f32,
    /// Decomposition without oxygen runs this much slower…
    pub anoxic_decay: f32,
    /// …and releases this fraction as minerals (sulfides).
    pub anoxic_minerals: f32,
    /// Oxygen used by decomposing marine snow, per unit of energy decomposed (× oxygen_per_energy).
    pub decomposition_oxygen: f32,
    /// Nutrients released by upwelling water per unit of its mineral energy.
    pub upwelling_nutrients: f32,
}

impl Default for ChemistryConfig {
    fn default() -> Self {
        Self {
            oxygen_per_energy: 2.0,
            co2_per_energy: 2.0,
            oxygen_half: 0.25,
            ferment_penalty: 0.6,
            ferment_floor: 0.25,
            ferment_waste: 0.5,
            suffocation_damage: 0.2,
            oxygen_toxicity: 0.05,
            co2_half: 0.3,
            sulfur_minerals: 0.3,
            sulfur_half: 0.05,
            nutrient_per_mass: 1.0,
            nutrient_store: 1.0,
            uptake_rate: 1.0,
            anchor_uptake: 3.0,
            fixation_rate: 0.01,
            fixation_cost: 0.5,
            toxin_rate: 0.05,
            toxin_damage: 2.0,
            toxin_decay: 0.02,
            poison_damage: 0.8,
            diffusion: 600.0,
            ventilation: 0.0003,
            nutrient_surface: 0.05,
            nutrient_deep: 0.5,
            nutricline_depth: 1200.0,
            deep_oxygen: 0.7,
            vent_co2: 4.0,
            vent_nutrients: 2.0,
            vent_oxygen_sink: 0.025,
            anoxic_decay: 0.5,
            anoxic_minerals: 0.9,
            decomposition_oxygen: 1.0,
            upwelling_nutrients: 0.5,
        }
    }
}

/// The air above the ocean: two global levels (1.0 = today) that the surface trades with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AtmosphereConfig {
    /// Fraction per second by which surface water moves toward equilibrium with the air.
    pub surface_exchange: f32,
    /// Size of the atmosphere, in fully saturated water columns per unit of surface width.
    /// Larger = the air changes more slowly.
    pub size: f32,
    /// Degrees of warming per doubling of CO₂ in the air.
    pub greenhouse: f32,
    /// Air at the start (1.0 = today). Try `initial_oxygen = 0.05` for an anoxic early Earth.
    pub initial_oxygen: f32,
    pub initial_co2: f32,
}

impl Default for AtmosphereConfig {
    fn default() -> Self {
        Self {
            surface_exchange: 0.02,
            size: 40.0,
            greenhouse: 3.0,
            initial_oxygen: 1.0,
            initial_co2: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MetabolismConfig {
    /// Basal metabolism = basal × mass^0.75 × upkeep multiplier (energy/s).
    pub basal: f32,
    /// Swimming at full throttle with full speed gene costs this × basal.
    pub move_cost: f32,
    /// Energy to build one unit of body mass.
    pub build_cost: f32,
    /// Energy content of one unit of body mass when eaten or decomposed.
    pub body_energy: f32,
    /// Max growth per second as a fraction of current mass.
    pub growth_rate: f32,
    /// Juveniles keep this fraction of their reserves before investing in growth.
    pub growth_buffer: f32,
    /// Energy per HP regenerated.
    pub regen_cost: f32,
    /// HP lost per unit of energy deficit when starving.
    pub starvation_damage: f32,
    /// Metabolism multiplier per unit of temperature stress.
    pub temp_stress_cost: f32,
    /// Fraction of max HP lost per second per unit of stress beyond tolerance.
    pub temp_damage: f32,
    /// Chance per second of dying once past lifespan.
    pub old_age_death_rate: f32,
}

impl Default for MetabolismConfig {
    fn default() -> Self {
        Self {
            basal: 0.003,
            move_cost: 3.0,
            build_cost: 1.25,
            body_energy: 1.0,
            growth_rate: 0.05,
            growth_buffer: 0.5,
            regen_cost: 1.5,
            starvation_damage: 1.0,
            temp_stress_cost: 0.5,
            temp_damage: 0.05,
            old_age_death_rate: 0.2,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FeedingConfig {
    /// Mineral uptake per second = chemo_rate × capability × radius × concentration.
    pub chemo_rate: f32,
    pub chemo_yield: f32,
    /// Detritus uptake per second = scavenge_rate × capability × radius × concentration.
    pub scavenge_rate: f32,
    /// Extra detritus uptake when swimming: filter_rate × capability × width × speed × concentration.
    pub filter_rate: f32,
    /// Carcass eating rate (mass per second per radius²).
    pub carcass_rate: f32,
    /// Fraction of eaten energy actually assimilated.
    pub assimilation: f32,
    /// Prey radius ≤ this × hunter radius can be engulfed whole.
    pub engulf_ratio: f32,
    /// Engulf rate (per second of contact at the mouth, at full hunting capability) for prey at
    /// the size limit; smaller prey go down faster, by (limit / size ratio)².
    pub engulf_rate: f32,
    /// Most small prey swallowed at the mouth in one tick (× hunting capability²).
    pub gulps: u32,
    /// Share of their energy that tiny prey (5% of the hunter's radius or less) are worth to it,
    /// rising to full value at 30%.
    pub small_prey_value: f32,
    /// A shell makes prey harder to swallow whole: on its armored side it counts as up to this
    /// much bigger.
    pub shell_bulk: f32,
    /// Hunters spare look-alikes (kin) unless hungrier than this (0 = fed, 1 = starving).
    pub kin_hunger: f32,
    /// Hunting capability below this does nothing (trace genes from drift don't bite).
    pub min_hunt: f32,
    /// Speed at which full-strength tentacles drag what they hold toward the mouth
    /// (× √hunter radius), shared between the two bodies by mass.
    pub tentacle_pull: f32,
    /// Fraction of a shell's protection that full-strength jaws bite through.
    pub jaw_pierce: f32,
    /// Hunters and scavengers ignore food while their energy is above this fraction of full.
    pub full: f32,
    /// Bite: mass removed per second per hunter radius² at full capability and jaws.
    pub bite_rate: f32,
    /// Damage reduction at full armor.
    pub armor_protection: f32,
    /// Damage to an attacker per unit of prey mass taken, at full spikes (× hunter's mass-to-HP).
    pub spike_damage: f32,
    /// Food drives are scaled by hunger between these factors (full → starving).
    pub hunger_min: f32,
    pub hunger_max: f32,
}

impl Default for FeedingConfig {
    fn default() -> Self {
        Self {
            chemo_rate: 3.0,
            chemo_yield: 2.0,
            scavenge_rate: 12.0,
            filter_rate: 0.05,
            carcass_rate: 0.5,
            assimilation: 0.8,
            engulf_ratio: 0.65,
            engulf_rate: 3.0,
            gulps: 4,
            small_prey_value: 0.6,
            shell_bulk: 0.4,
            kin_hunger: 0.5,
            min_hunt: 0.05,
            tentacle_pull: 12.0,
            jaw_pierce: 0.5,
            full: 0.9,
            bite_rate: 0.3,
            armor_protection: 0.85,
            spike_damage: 0.6,
            hunger_min: 0.3,
            hunger_max: 1.7,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PhysicsConfig {
    /// Vertical drift at full buoyancy (× √radius), units/s.
    pub buoyancy_speed: f32,
    /// Brownian jitter speed at radius 1 (falls off as 1/radius).
    pub brownian: f32,
    /// How quickly velocity follows thrust (/s, divided by √radius).
    pub response: f32,
    /// Fraction of overlap resolved per tick when bodies push apart.
    pub push_stiffness: f32,
    /// Max speed penalty at full armor.
    pub armor_speed_penalty: f32,
    /// Sinking per unit of part mass (as a fraction of body mass), in buoyancy units.
    pub part_density: f32,
    /// Carcass sinking speed (× √radius), units/s.
    pub carcass_sink: f32,
    /// Corpses at least this large become carcasses; smaller ones become detritus.
    pub carcass_min_radius: f32,
    /// Fraction of a carcass that decomposes into detritus per second.
    pub carcass_decay: f32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            buoyancy_speed: 3.0,
            brownian: 8.0,
            response: 20.0,
            push_stiffness: 0.5,
            armor_speed_penalty: 0.2,
            part_density: 1.5,
            carcass_sink: 2.0,
            carcass_min_radius: 0.0,
            carcass_decay: 0.01,
        }
    }
}

/// Ocean currents (curl-noise flow): slow gyres everywhere, eddies in the surface mixed layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CurrentsConfig {
    /// Size of the large gyres.
    pub gyre_scale: f32,
    /// Typical gyre speed (units/s).
    pub gyre_speed: f32,
    /// Seconds for the gyre pattern to change substantially.
    pub gyre_period: f32,
    pub eddy_scale: f32,
    pub eddy_speed: f32,
    pub eddy_period: f32,
    /// Depth of the turbulent surface layer where eddies act.
    pub mixing_depth: f32,
}

impl Default for CurrentsConfig {
    fn default() -> Self {
        Self {
            gyre_scale: 3000.0,
            gyre_speed: 3.0,
            gyre_period: 900.0,
            eddy_scale: 450.0,
            eddy_speed: 8.0,
            eddy_period: 180.0,
            mixing_depth: 450.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ReproductionConfig {
    /// Newborns start with this fraction of their reserve capacity (paid by the parent).
    pub newborn_reserves: f32,
    /// Minimum seconds between reproduction events.
    pub cooldown: f32,
}

impl Default for ReproductionConfig {
    fn default() -> Self {
        Self {
            newborn_reserves: 0.5,
            cooldown: 2.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MutationConfig {
    /// Chance per gene per offspring of mutating.
    pub rate: f32,
    /// Of mutations, the fraction that re-roll the gene to a random value.
    pub reroll: f32,
    /// Multiplier on the size of a mutation's step (±1, 2, 4 or 8 before scaling).
    pub step_scale: f32,
    /// Part-type genes mutate this many times as often as other genes (type shifts and
    /// symmetry changes are big steps).
    pub part_type_rate_multiplier: f32,
    /// Color genes mutate this many times as often (a fast "molecular clock", so lineages
    /// visibly diverge in color).
    pub color_rate_multiplier: f32,
    /// Chance per offspring that a whole part is copied into an empty slot.
    pub part_duplicate: f32,
    /// Chance per offspring that a part is lost.
    pub part_delete: f32,
    /// Chance per offspring that a new part grows in an empty slot.
    pub part_new: f32,
}

impl Default for MutationConfig {
    fn default() -> Self {
        Self {
            rate: 0.03,
            reroll: 0.05,
            step_scale: 2.0,
            part_type_rate_multiplier: 0.3,
            color_rate_multiplier: 3.0,
            part_duplicate: 0.003,
            part_delete: 0.004,
            part_new: 0.005,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SpeciesConfig {
    /// Genetic distance from the species founder that starts a new species.
    pub threshold: f32,
    /// Genetic distance from the genus founder that starts a new genus.
    pub genus_threshold: f32,
    /// Weight of color genes in genetic distance (others weigh 1).
    pub color_weight: f32,
    /// Most any single gene can add to genetic distance (as a fraction of its range).
    pub gene_cap: f32,
    /// Living members needed before a species is named and listed.
    pub established: u32,
    /// Color distance under which organisms recognize each other as kin.
    pub kin_threshold: f32,
}

impl Default for SpeciesConfig {
    fn default() -> Self {
        Self {
            threshold: 0.035,
            genus_threshold: 0.07,
            color_weight: 0.25,
            gene_cap: 0.3,
            established: 20,
            kin_threshold: 0.08,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PopulationConfig {
    /// The ancestor genome, in chromosome notation (see docs/DESIGN.md).
    pub ancestor: String,
    /// Organisms spawned at the start.
    pub initial: u32,
    /// Initial organisms spawn within this depth below the surface.
    pub spawn_depth: f32,
    /// If the population falls below this, the seed bank recolonizes.
    pub min_population: u32,
    /// Organisms added by one recolonization.
    pub reseed_count: u32,
    /// Hard safety cap: no births above this population.
    pub max_population: u32,
    /// A second founding lineage that lives at the vents (empty = none).
    pub vent_ancestor: String,
    /// Vent ancestors spawned around each vent at the start.
    pub vent_initial: u32,
    /// Vent ancestors that settle at a newly opened vent when no vent life lives nearby.
    pub vent_seed: u32,
}

impl Default for PopulationConfig {
    fn default() -> Self {
        Self {
            ancestor: "B:20A050200000 C:55C000 D:FF000000 G:E0FF80000000 H:2030 \
                       N:80808080808080808000 \
                       P:000000 000000 000000 000000 000000 000000 000000 000000 R:FF00B0 T:80A0"
                .into(),
            initial: 2000,
            spawn_depth: 300.0,
            min_population: 100,
            reseed_count: 300,
            max_population: 250_000,
            vent_ancestor: String::new(),
            vent_initial: 40,
            vent_seed: 20,
        }
    }
}

/// Random world events. Intervals are mean sim hours between occurrences (0 = never).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EventsConfig {
    pub enabled: bool,
    pub plague_every: f32,
    /// A plague only strikes a species holding at least this share of all organisms.
    pub plague_min_share: f32,
    /// Seconds a plague lasts.
    pub plague_duration: f32,
    /// Chance per second for each member of the stricken species to die.
    pub plague_kill_rate: f32,
    pub upwelling_every: f32,
    pub upwelling_duration: f32,
    /// Mineral energy per second welling up into the sunlit water.
    pub upwelling_rate: f32,
    pub upwelling_width: f32,
    pub impact_every: f32,
    /// Everything within this radius of the impact dies.
    pub impact_radius: f32,
    /// Fraction of sunlight blocked at the height of the impact winter.
    pub impact_winter: f32,
    /// Length of the impact winter in days.
    pub impact_winter_days: f32,
    /// Mineral energy the impact releases.
    pub impact_minerals: f32,
    pub barrier_every: f32,
    /// Hours the wall stands at full height.
    pub barrier_duration: f32,
    /// Hours to rise (and to sink again).
    pub barrier_rise: f32,
    pub barrier_width: f32,
}

impl Default for EventsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            plague_every: 8.0,
            plague_min_share: 0.35,
            plague_duration: 600.0,
            plague_kill_rate: 0.002,
            upwelling_every: 3.0,
            upwelling_duration: 1800.0,
            upwelling_rate: 40.0,
            upwelling_width: 1500.0,
            impact_every: 48.0,
            impact_radius: 700.0,
            impact_winter: 0.7,
            impact_winter_days: 4.0,
            impact_minerals: 5000.0,
            barrier_every: 30.0,
            barrier_duration: 20.0,
            barrier_rise: 0.1,
            barrier_width: 200.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StatsConfig {
    /// Seconds between statistics samples.
    pub sample_interval: f32,
    /// Maximum samples kept (older ones are thinned out).
    pub max_samples: usize,
    /// Maximum events kept in the log.
    pub max_events: usize,
}

impl Default for StatsConfig {
    fn default() -> Self {
        Self {
            sample_interval: 10.0,
            max_samples: 4000,
            max_events: 500,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_toml_matches_rust_defaults() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../configs/default.toml");
        let loaded = Config::load(&path).expect("configs/default.toml should load");
        assert_eq!(
            loaded,
            Config::default(),
            "configs/default.toml and Config::default() have drifted apart"
        );
    }

    #[test]
    fn every_shipped_config_loads() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../configs");
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "toml") {
                Config::load(&path).unwrap_or_else(|e| panic!("{e}"));
            }
        }
    }

    #[test]
    fn empty_toml_is_default() {
        assert_eq!(Config::from_toml("").unwrap(), Config::default());
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(Config::from_toml("[world]\nwidht = 10\n").is_err());
    }
}

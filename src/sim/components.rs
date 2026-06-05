//! ECS components for simulation entities (see [ENTITIES.md](../../docs/ENTITIES.md)).

use bevy::prelude::*;

use crate::config::SimConfig;
use crate::genome::{
    decode_phenotype, genome_from_bytes, phenotype_body_radius, phenotype_color,
    phenotype_max_speed, phenotype_repro_threshold_energy, AnyGenome, AnyPhenotype,
};

/// Scalar light multiplier at entity position (updated each tick by [`super::systems::sense_exposure`](crate::sim::sense_exposure)).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct LightExposure(pub f32);

/// Raw hazard damage per tick from overlapping zones (before mitigation; Phase 4A).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct HazardExposure {
    pub damage_raw: f32,
}

/// Raw vent flux from overlapping zones (before chemo scaling; Phase 5A).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct VentExposure {
    pub vent_flux: f32,
}

/// Mineral concentration at agent position for feeding and debug (G7).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct MineralExposure {
    pub concentration: f32,
}

/// Dead-agent biomass marker (signature `0x0200`, feeding).
#[derive(Component, Clone, Debug)]
pub struct Corpse {
    pub remaining_biomass: f32,
    pub radius: f32,
    pub ticks_remaining: u32,
}

/// Heritable DNA; mutated on reproduction.
#[derive(Component, Clone)]
pub struct Genome(pub AnyGenome);

/// Decoded traits; rebuild when genome changes.
#[derive(Component, Clone)]
pub struct Phenotype(pub AnyPhenotype);

impl Genome {
    pub fn clone_genome(&self) -> AnyGenome {
        self.0.clone_genome()
    }
}

impl Phenotype {
    pub fn body_radius(&self) -> f32 {
        phenotype_body_radius(&self.0)
    }

    pub fn max_speed(&self) -> f32 {
        phenotype_max_speed(&self.0)
    }

    pub fn color_rgb(&self) -> [f32; 3] {
        phenotype_color(&self.0)
    }

    pub fn repro_threshold_energy(&self) -> f32 {
        phenotype_repro_threshold_energy(&self.0)
    }
}

/// Ticks since spawn; drives logarithmic age metabolism.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Age {
    pub ticks: u32,
}

/// Per-entity energy store.
#[derive(Component, Clone)]
pub struct Energy {
    pub internal_energy: f32,
    /// Upper cap from decoded genome.
    pub max_internal_energy: f32,
}

impl Energy {
    pub fn new_v2(initial: f32, max_energy: f32) -> Self {
        let cap = max_energy.max(0.0);
        Self {
            internal_energy: initial.max(0.0).min(cap),
            max_internal_energy: cap,
        }
    }
}

/// Core and appendage hit points.
///
/// Phase 4: `part_hp` is always empty at spawn; hazard damage routes to
/// `core_hp` only. Phase 7 fills `part_hp` from morphology nodes and enables
/// exposure-based routing (see [ENTITIES.md](../../docs/ENTITIES.md)).
#[derive(Component, Clone)]
pub struct Health {
    pub core_hp: f32,
    pub core_max_hp: f32,
    pub part_hp: Vec<f32>,
}

impl Health {
    /// Whether appendage HP slots are active (Phase 7+ morphology).
    pub fn has_part_hp(&self) -> bool {
        !self.part_hp.is_empty()
    }

    /// Apply damage to `core_hp` (Phase 4 routing when `part_hp` is empty).
    pub fn apply_damage_to_core(&mut self, amount: f32) {
        if amount > 0.0 {
            self.core_hp -= amount;
        }
    }
}

/// Facing angle in radians (0 = +X); sensors and v2 movement are relative to heading.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Heading(pub f32);

impl Heading {
    pub fn from_angle(radians: f32) -> Self {
        Self(radians)
    }

    pub fn forward(&self) -> Vec2 {
        Vec2::from_angle(self.0)
    }

    pub fn set_from_velocity(&mut self, velocity: Vec2) {
        if velocity.length_squared() > 1e-8 {
            self.0 = velocity.y.atan2(velocity.x);
        }
    }
}

/// Motion; magnitude capped by `phenotype.max_speed` in movement systems.
#[derive(Component, Deref, DerefMut, Clone)]
pub struct Velocity(pub Vec2);

/// Physics / render hull radius (from phenotype).
#[derive(Component, Clone)]
pub struct Agent {
    pub radius: f32,
}

#[derive(Component, Clone)]
pub struct Lineage {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub generation: u32,
}

#[derive(Component, Clone)]
pub struct Cooldowns {
    pub repro_ticks_remaining: u32,
}

/// Why an entity was removed ([ENTITIES.md](../../docs/ENTITIES.md#death)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeathReason {
    /// Legacy metric bucket; starvation deaths are reported as [`CoreDestroyed`](DeathReason::CoreDestroyed).
    EnergyDepleted,
    CoreDestroyed,
    ReproFatality,
    /// Reserved (legacy old-age); not used while death is energy/HP/repro-driven.
    AgeExceeded,
}

/// Marked by reproduction when repro-risk kills the parent; consumed by death system same tick.
#[derive(Component, Clone, Copy)]
pub struct PendingDeath(pub DeathReason);

/// Lifecycle components shared by every agent entity.
#[derive(Bundle, Clone)]
pub struct LifeBundle {
    pub genome: Genome,
    pub phenotype: Phenotype,
    pub energy: Energy,
    pub health: Health,
    pub lineage: Lineage,
    pub cooldowns: Cooldowns,
    pub agent: Agent,
    pub age: Age,
}

impl LifeBundle {
    /// Build lifecycle bundle from genome and phenotype.
    /// Initial-spawn helper only (`spawn_energy` defaults). Offspring must use
    /// [`from_any_parts_with_vitals`](Self::from_any_parts_with_vitals) with parent endowment.
    pub fn from_any_parts(
        genome: AnyGenome,
        phenotype: AnyPhenotype,
        config: &SimConfig,
        id: u64,
        parent_id: Option<u64>,
        generation: u32,
    ) -> Self {
        debug_assert!(
            parent_id.is_none(),
            "from_any_parts uses spawn_energy; use from_any_parts_with_vitals for offspring"
        );
        Self::from_any_parts_with_energy(
            genome,
            phenotype,
            config,
            id,
            parent_id,
            generation,
            config.economy.spawn_energy,
        )
    }

    pub fn from_any_parts_with_energy(
        genome: AnyGenome,
        phenotype: AnyPhenotype,
        config: &SimConfig,
        id: u64,
        parent_id: Option<u64>,
        generation: u32,
        initial_energy: f32,
    ) -> Self {
        let (initial_hp, max_hp) = vitality_spawn_pools_for(&phenotype, config);
        Self::from_any_parts_with_vitals(
            genome,
            phenotype,
            config,
            id,
            parent_id,
            generation,
            initial_energy,
            initial_hp,
            max_hp,
        )
    }

    pub fn from_any_parts_with_vitals(
        genome: AnyGenome,
        phenotype: AnyPhenotype,
        config: &SimConfig,
        id: u64,
        parent_id: Option<u64>,
        generation: u32,
        initial_energy: f32,
        initial_core_hp: f32,
        initial_core_max_hp: f32,
    ) -> Self {
        let _ = config;
        let agent = Agent {
            radius: phenotype_body_radius(&phenotype),
        };
        let max_hp = initial_core_max_hp.max(0.0);
        let health = Health {
            core_hp: initial_core_hp.min(max_hp).max(0.0),
            core_max_hp: max_hp,
            part_hp: Vec::new(),
        };
        let energy = Energy::new_v2(initial_energy, phenotype.max_energy);
        let lineage = Lineage {
            id,
            parent_id,
            generation,
        };
        let cooldowns = Cooldowns {
            repro_ticks_remaining: 0,
        };

        Self {
            genome: Genome(genome),
            phenotype: Phenotype(phenotype),
            energy,
            health,
            lineage,
            cooldowns,
            agent,
            age: Age { ticks: 0 },
        }
    }

    /// Decode genome and build a lifecycle bundle; returns `None` if invalid or decode fails (stillborn).
    pub fn try_from_genome_bytes(
        bytes: Vec<u8>,
        config: &SimConfig,
        id: u64,
        parent_id: Option<u64>,
        generation: u32,
    ) -> Option<Self> {
        let genome = genome_from_bytes(bytes, config).ok()?;
        let phenotype = decode_phenotype(&genome, config).ok()?;
        Some(Self::from_any_parts(genome, phenotype, config, id, parent_id, generation))
    }
}

/// Spawn pools for energy and health from decoded phenotype.
pub fn vitality_spawn_pools_for(phenotype: &AnyPhenotype, config: &SimConfig) -> (f32, f32) {
    let hp_frac = config.economy.spawn_health_frac.clamp(0.0, 1.0);
    (phenotype.max_health * hp_frac, phenotype.max_health)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{ancestor_any_from_config, decode_v2};

    #[test]
    fn agent_radius_matches_phenotype() {
        let config = SimConfig::load_default().unwrap();
        let genome = ancestor_any_from_config(&config).unwrap();
        let phenotype = decode_v2(&genome, &config).unwrap();
        let bundle = LifeBundle::from_any_parts(genome, phenotype.clone(), &config, 1, None, 0);
        assert_eq!(bundle.agent.radius, phenotype.body_radius);
    }

    #[test]
    fn try_from_genome_bytes_rejects_invalid_header() {
        let config = SimConfig::load_default().unwrap();
        let mut bytes = config.genetics.parse_ancestor_bytes().unwrap();
        bytes[0] = b'X';
        assert!(LifeBundle::try_from_genome_bytes(bytes, &config, 1, None, 0).is_none());
    }

    #[test]
    fn life_bundle_has_all_marker_components() {
        let config = SimConfig::load_default().unwrap();
        let bytes = config.genetics.parse_ancestor_bytes().unwrap();
        let bundle = LifeBundle::try_from_genome_bytes(bytes, &config, 42, None, 0).unwrap();
        assert_eq!(bundle.lineage.id, 42);
        assert_eq!(bundle.health.part_hp.len(), 0);
        let expected_energy = config
            .economy
            .spawn_energy
            .min(bundle.energy.max_internal_energy);
        assert_eq!(bundle.energy.internal_energy, expected_energy);
    }

    /// Phase 4C stub: manual `part_hp` does not receive interaction damage until Phase 7.
    #[test]
    fn phase4_damage_routes_to_core_when_parts_present_stub() {
        let mut health = Health {
            core_hp: 100.0,
            core_max_hp: 100.0,
            part_hp: vec![50.0],
        };
        assert!(health.has_part_hp());
        health.apply_damage_to_core(10.0);
        assert_eq!(health.core_hp, 90.0);
        assert_eq!(
            health.part_hp[0], 50.0,
            "Phase 4 interactions do not mutate part_hp"
        );
    }
}

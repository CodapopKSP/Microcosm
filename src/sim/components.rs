//! ECS components for simulation entities (see [ENTITIES.md](../../docs/ENTITIES.md)).

use bevy::prelude::*;

use crate::config::SimConfig;
use crate::genome::{decode, Genome as GenomeData, Phenotype as PhenotypeData};

/// Scalar light multiplier at entity position (updated each tick by [`super::systems::sense_exposure`](crate::sim::sense_exposure)).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct LightExposure(pub f32);

/// Raw hazard damage per tick from overlapping zones (before mitigation; Phase 4A).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct HazardExposure {
    pub damage_raw: f32,
}

/// Heritable DNA; mutated on reproduction.
#[derive(Component, Clone)]
pub struct Genome(pub GenomeData);

/// Decoded traits; rebuild when genome changes.
#[derive(Component, Clone)]
pub struct Phenotype(pub PhenotypeData);

/// Ticks since spawn; drives logarithmic age metabolism ([GENOME.md](../../docs/GENOME.md)).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Age {
    pub ticks: u32,
}

/// Per-entity energy store.
#[derive(Component, Clone)]
pub struct Energy {
    pub internal_energy: f32,
}

/// Core and appendage hit points.
///
/// Phase 4: `part_hp` is always empty at spawn; hazard and collision damage route to
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
    /// Build a new agent from an already-validated genome and decoded phenotype.
    pub fn from_parts(
        genome: GenomeData,
        phenotype: PhenotypeData,
        config: &SimConfig,
        id: u64,
        parent_id: Option<u64>,
        generation: u32,
    ) -> Self {
        Self::from_parts_with_energy(
            genome,
            phenotype,
            config,
            id,
            parent_id,
            generation,
            config.economy.spawn_energy,
        )
    }

    /// Build lifecycle bundle with explicit starting energy and core HP (e.g. offspring vitality).
    pub fn from_parts_with_energy(
        genome: GenomeData,
        phenotype: PhenotypeData,
        config: &SimConfig,
        id: u64,
        parent_id: Option<u64>,
        generation: u32,
        initial_energy: f32,
    ) -> Self {
        let core_max_hp = phenotype.core_max_hp;
        Self::from_parts_with_vitals(
            genome,
            phenotype,
            config,
            id,
            parent_id,
            generation,
            initial_energy,
            core_max_hp,
        )
    }

    /// Build lifecycle bundle with explicit energy and core HP.
    pub fn from_parts_with_vitals(
        genome: GenomeData,
        phenotype: PhenotypeData,
        config: &SimConfig,
        id: u64,
        parent_id: Option<u64>,
        generation: u32,
        initial_energy: f32,
        initial_core_hp: f32,
    ) -> Self {
        let _ = config;
        let agent = Agent {
            radius: phenotype.body_radius,
        };
        let max_hp = phenotype.core_max_hp;
        let health = Health {
            core_hp: initial_core_hp.min(max_hp).max(0.0),
            core_max_hp: max_hp,
            part_hp: Vec::new(),
        };
        let energy = Energy {
            internal_energy: initial_energy.max(0.0),
        };
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
    pub fn try_from_genome(
        genome: GenomeData,
        config: &SimConfig,
        id: u64,
        parent_id: Option<u64>,
        generation: u32,
    ) -> Option<Self> {
        let genetics = &config.genetics;
        if !genome.is_valid(genetics.genome_version, genetics.core_length) {
            return None;
        }
        let phenotype = decode(&genome, config).ok()?;
        Some(Self::from_parts(genome, phenotype, config, id, parent_id, generation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::Genome;

    #[test]
    fn agent_radius_matches_phenotype() {
        let config = SimConfig::load_default().unwrap();
        let genome = Genome::ancestor_from_config(&config).unwrap();
        let phenotype = decode(&genome, &config).unwrap();
        let bundle = LifeBundle::from_parts(genome, phenotype.clone(), &config, 1, None, 0);
        assert_eq!(bundle.agent.radius, phenotype.body_radius);
    }

    #[test]
    fn try_from_genome_rejects_invalid_header() {
        let config = SimConfig::load_default().unwrap();
        let mut genome = Genome::ancestor_from_config(&config).unwrap();
        genome.bytes[0] = b'X';
        assert!(LifeBundle::try_from_genome(genome, &config, 1, None, 0).is_none());
    }

    #[test]
    fn life_bundle_has_all_marker_components() {
        let config = SimConfig::load_default().unwrap();
        let genome = Genome::ancestor_from_config(&config).unwrap();
        let bundle = LifeBundle::try_from_genome(genome, &config, 42, None, 0).unwrap();
        assert_eq!(bundle.lineage.id, 42);
        assert_eq!(bundle.health.part_hp.len(), 0);
        assert_eq!(bundle.energy.internal_energy, config.economy.spawn_energy);
    }

    /// Phase 4C stub: manual `part_hp` does not receive interaction damage until Phase 7.
    /// Repair policies in 4D (`core_first`, `cheapest`, `symmetric`) operate on core + parts
    /// when parts exist; hazard/collision damage stays core-only in Phase 4.
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

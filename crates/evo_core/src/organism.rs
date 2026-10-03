//! Organisms and carcasses.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::genome::Genome;
use crate::rng::Rng;
use crate::traits::Traits;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Activity {
    #[default]
    Drifting,
    Wandering,
    Foraging,
    Hunting,
    Fleeing,
    Schooling,
    Feeding,
    /// Eating a carcass.
    Scavenging,
    /// Filtering marine snow / sediment.
    Grazing,
    /// Absorbing vent minerals.
    Absorbing,
}

impl Activity {
    pub fn label(self) -> &'static str {
        match self {
            Activity::Drifting => "Drifting",
            Activity::Wandering => "Wandering",
            Activity::Foraging => "Foraging",
            Activity::Hunting => "Hunting",
            Activity::Fleeing => "Fleeing",
            Activity::Schooling => "Schooling",
            Activity::Feeding => "Feeding",
            Activity::Scavenging => "Eating a carcass",
            Activity::Grazing => "Eating marine snow",
            Activity::Absorbing => "Absorbing minerals",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeathCause {
    Starvation,
    OldAge,
    Eaten,
    Predation,
    Temperature,
    Spikes,
    Disease,
    Catastrophe,
    /// Couldn't breathe (too little oxygen for an aerobe).
    Suffocation,
    /// Toxins in the water, oxygen (for strict fermenters), or poisonous prey.
    Poison,
}

impl DeathCause {
    pub const ALL: [DeathCause; 10] = [
        DeathCause::Starvation,
        DeathCause::OldAge,
        DeathCause::Eaten,
        DeathCause::Predation,
        DeathCause::Temperature,
        DeathCause::Spikes,
        DeathCause::Disease,
        DeathCause::Catastrophe,
        DeathCause::Suffocation,
        DeathCause::Poison,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DeathCause::Starvation => "starvation",
            DeathCause::OldAge => "old age",
            DeathCause::Eaten => "eaten whole",
            DeathCause::Predation => "predation",
            DeathCause::Temperature => "temperature",
            DeathCause::Spikes => "spikes",
            DeathCause::Disease => "disease",
            DeathCause::Catastrophe => "catastrophe",
            DeathCause::Suffocation => "suffocation",
            DeathCause::Poison => "poison",
        }
    }
}

/// What an organism took from and gave to the water this tick (amounts, not rates),
/// plus how its breathing went. Written in the parallel act phase, applied to the fields after.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Chem {
    /// Positive = released into the water, negative = taken from it.
    pub oxygen: f32,
    pub co2: f32,
    pub minerals: f32,
    pub nutrients: f32,
    pub toxin: f32,
    /// Organic waste (energy) leaked into the marine snow.
    pub waste: f32,
    /// Share of the energy needs met by breathing oxygen (the rest fermented or unmet).
    pub aerobic_share: f32,
    /// Share of the energy needs that couldn't be met at all (suffocating).
    pub shortfall: f32,
    /// HP lost to toxins this tick.
    pub poisoned: f32,
}

/// A body held by a tentacle: where it is and how big, since when, and the last tick it was held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Grasp {
    pub pos: Vec2,
    pub radius: f32,
    pub since: u64,
    pub tick: u64,
}

impl Grasp {
    /// Note that a body is held this tick.
    pub fn hold(&mut self, pos: Vec2, radius: f32, tick: u64) {
        if self.tick + 1 < tick {
            self.since = tick;
        }
        self.pos = pos;
        self.radius = radius;
        self.tick = tick;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Organism {
    pub id: u64,
    pub parent: u64,
    pub species: u32,
    pub generation: u32,
    pub genome: Genome,
    /// Genes that differ from the parent's (bit i = gene i).
    pub mutations: u64,
    /// Decoded from the genome; not saved (re-decoded on load).
    #[serde(skip)]
    pub traits: Traits,

    pub pos: Vec2,
    /// Position at the start of the tick (for render interpolation).
    pub prev_pos: Vec2,
    pub vel: Vec2,
    pub heading: f32,
    pub mass: f32,
    pub radius: f32,
    pub energy: f32,
    pub hp: f32,
    /// Stored nutrients (needed to build body mass).
    #[serde(default)]
    pub nutrients: f32,
    /// Held in place by a holdfast.
    #[serde(default)]
    pub anchored: bool,
    /// Touching rock or the seafloor (only tracked for organisms with legs or holdfasts).
    #[serde(default)]
    pub touching: bool,
    /// Sim seconds.
    pub age: f32,
    pub repro_cooldown: f32,
    pub rng: Rng,

    pub activity: Activity,
    /// Last steering decision (reused between decision ticks).
    #[serde(default)]
    pub steer_heading: f32,
    #[serde(default)]
    pub steer_throttle: f32,
    /// Tick this organism last ate (for the feeding animation).
    #[serde(default)]
    pub ate_tick: u64,
    /// Tick this organism was last bitten, and the direction (world angle) the bite came from
    /// (for the hurt animation).
    #[serde(default)]
    pub hurt_tick: u64,
    #[serde(default)]
    pub hurt_dir: f32,
    pub born: u64,
    /// Where it budded off its parent: offset from the parent's center at birth (zero for
    /// founders). Used for the budding animation.
    #[serde(default)]
    pub bud_offset: Vec2,
    /// Tick it last budded off offspring, and how much it shrank doing so (radius before ÷
    /// after). Used for the budding animation.
    #[serde(default)]
    pub budded_tick: u64,
    #[serde(default)]
    pub bud_scale: f32,
    /// What each tentacle is holding, by grasp (`traits.grasps`), for drawing them wrapped
    /// around it. Empty until a tentacle first grabs something.
    #[serde(default)]
    pub grasps: Vec<Grasp>,
    pub offspring: u32,
    pub kills: u32,
    /// Last-tick diagnostics (energy/s), for the inspector.
    pub gain_rate: f32,
    pub cost_rate: f32,
    pub temperature: f32,
    /// This tick's exchange with the water.
    #[serde(skip)]
    pub chem: Chem,
    /// The water current where it is, refreshed every few ticks (it changes slowly).
    #[serde(skip)]
    pub water: Vec2,
    pub dead: Option<DeathCause>,
}

impl Organism {
    /// A new organism of `mass` with `energy` reserves.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: u64,
        parent: u64,
        species: u32,
        generation: u32,
        genome: Genome,
        mutations: u64,
        config: &Config,
        pos: Vec2,
        mass: f32,
        energy: f32,
        rng: Rng,
        tick: u64,
    ) -> Self {
        let traits = Traits::decode(&genome, config);
        let mut o = Self {
            id,
            parent,
            species,
            generation,
            genome,
            mutations,
            traits,
            pos,
            prev_pos: pos,
            vel: Vec2::ZERO,
            heading: 0.0,
            mass,
            radius: mass.sqrt(),
            energy: 0.0,
            hp: 0.0,
            nutrients: 0.0,
            anchored: false,
            touching: false,
            age: 0.0,
            repro_cooldown: 0.0,
            rng,
            activity: Activity::Drifting,
            steer_heading: 0.0,
            steer_throttle: 0.0,
            ate_tick: 0,
            hurt_tick: 0,
            hurt_dir: 0.0,
            born: tick,
            bud_offset: Vec2::ZERO,
            budded_tick: 0,
            bud_scale: 1.0,
            grasps: Vec::new(),
            offspring: 0,
            kills: 0,
            gain_rate: 0.0,
            cost_rate: 0.0,
            temperature: 0.0,
            chem: Chem::default(),
            water: Vec2::ZERO,
            dead: None,
        };
        o.heading = o.rng.angle();
        o.steer_heading = o.heading;
        o.energy = energy.min(o.max_energy());
        o.hp = o.max_hp();
        o.nutrients = 0.5 * o.max_nutrients(config);
        o
    }

    #[inline]
    pub fn max_nutrients(&self, config: &Config) -> f32 {
        self.mass * config.chemistry.nutrient_store
    }

    /// Nutrients locked in the body and its store (released when it dies or is eaten).
    pub fn body_nutrients(&self, config: &Config) -> f32 {
        self.mass * config.chemistry.nutrient_per_mass + self.nutrients.max(0.0)
    }

    #[inline]
    pub fn max_energy(&self) -> f32 {
        self.mass * self.traits.reserve_factor
    }

    #[inline]
    pub fn max_hp(&self) -> f32 {
        self.mass * self.traits.hp_factor
    }

    #[inline]
    pub fn is_adult(&self) -> bool {
        self.mass >= self.traits.adult_mass * 0.999
    }

    #[inline]
    pub fn alive(&self) -> bool {
        self.dead.is_none()
    }

    /// Energy value of the whole body when eaten or decomposed.
    pub fn body_value(&self, config: &Config) -> f32 {
        self.mass * config.metabolism.body_energy + self.energy.max(0.0)
    }

    #[inline]
    pub fn set_mass(&mut self, mass: f32) {
        self.mass = mass.max(1e-4);
        self.radius = self.mass.sqrt();
    }
}

/// A dead body: sinks, can be eaten on contact, and decays into detritus.
/// It keeps the look of the organism it was (drawn torn and grey).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Carcass {
    pub pos: Vec2,
    pub vel: Vec2,
    /// Remaining energy.
    pub energy: f32,
    /// Size of the body when it died (constant; eating takes bites out of it).
    pub radius: f32,
    pub color: [f32; 3],
    pub species: u32,
    #[serde(default)]
    pub initial_energy: f32,
    /// Nutrients still in the body (released as it rots or is eaten).
    #[serde(default)]
    pub nutrients: f32,
    #[serde(default)]
    pub heading: f32,
    /// The dead organism's genome (its body plan, for drawing) and an id-derived seed.
    #[serde(default)]
    pub genome: Genome,
    #[serde(default)]
    pub seed: f32,
    /// Tick it was last eaten from, and the direction (world angle) of the eater.
    #[serde(default)]
    pub hurt_tick: u64,
    #[serde(default)]
    pub hurt_dir: f32,
    /// Tick the organism died (for the dying animation).
    #[serde(default)]
    pub died_tick: u64,
}

impl Carcass {
    /// Fraction already eaten or decayed (0 = fresh).
    pub fn consumed(&self) -> f32 {
        if self.initial_energy <= 0.0 {
            0.0
        } else {
            (1.0 - self.energy / self.initial_energy).clamp(0.0, 1.0)
        }
    }
}

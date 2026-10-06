//! Simulation core for the evolution simulator.
//!
//! Plain Rust with no rendering dependencies: the viewer (`evo_app`) and the headless
//! runner (`evo_headless`) both drive a [`World`] and read its state.
//! See `docs/DESIGN.md` for the rules and `docs/ARCHITECTURE.md` for the code layout.

pub mod config;
pub mod currents;
pub mod environment;
pub mod events;
pub mod field;
pub mod genome;
pub mod grid;
pub mod notebook;
pub mod organism;
pub mod parts;
pub mod rng;
pub mod species;
pub mod stats;
pub mod terrain;
pub mod traits;
pub mod world;

pub use config::Config;
pub use genome::Genome;
pub use organism::Organism;
pub use traits::Traits;
pub use world::World;

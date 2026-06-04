//! Simulation ECS components and world resources.

mod agent_material;
mod app;
mod speed;
mod behavior;
mod components;
mod perception;
mod hazards;
mod minerals;
mod soup;
mod vents;
mod light;
mod metrics;
mod spawn;
mod systems;
mod world;

#[cfg(test)]
mod headless;
#[cfg(test)]
mod acceptance;

pub use agent_material::*;
pub use app::*;
pub use behavior::*;
pub use components::*;
pub use perception::*;
pub use hazards::*;
pub use minerals::*;
pub use soup::*;
pub use vents::*;
pub use light::*;
pub use metrics::*;
pub use spawn::*;
pub use speed::*;
pub use systems::*;
pub use world::*;

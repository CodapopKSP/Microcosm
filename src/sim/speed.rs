//! Visual-mode simulation speed multiplier (wall-clock only; logic stays tick-based).

use bevy::prelude::*;

/// Allowed speed steps: 1× through 16×.
pub const SIM_SPEED_STEPS: [u32; 5] = [1, 2, 4, 8, 16];

/// How many simulation ticks to run per rendered frame (visual mode).
#[derive(Resource, Clone, Copy)]
pub struct SimSpeed {
    index: usize,
}

impl Default for SimSpeed {
    fn default() -> Self {
        Self { index: 0 }
    }
}

impl SimSpeed {
    pub fn multiplier(&self) -> u32 {
        SIM_SPEED_STEPS[self.index]
    }

    pub fn step_up(&mut self) -> u32 {
        if self.index + 1 < SIM_SPEED_STEPS.len() {
            self.index += 1;
        }
        self.multiplier()
    }

    pub fn step_down(&mut self) -> u32 {
        if self.index > 0 {
            self.index -= 1;
        }
        self.multiplier()
    }
}

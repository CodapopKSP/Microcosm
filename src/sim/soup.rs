//! Global soup compound pool (Phase 6 minimal scalar, G7).

use bevy::prelude::*;

use crate::config::FeedingConfig;

/// Regenerating global soup concentration (config-only uptake rate in feeding).
#[derive(Debug, Clone, Resource)]
pub struct SoupField {
    pub concentration: f32,
    max: f32,
    regen_per_tick: f32,
    enabled: bool,
}

impl SoupField {
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
    pub fn from_feeding(feeding: &FeedingConfig) -> Self {
        Self {
            concentration: feeding.soup_initial_concentration,
            max: feeding.soup_max_concentration,
            regen_per_tick: feeding.soup_regen_per_tick,
            enabled: feeding.soup_enabled,
        }
    }

    pub fn tick_regen(&mut self) {
        if !self.enabled || self.regen_per_tick <= 0.0 {
            return;
        }
        self.concentration = (self.concentration + self.regen_per_tick).min(self.max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regen_clamps_to_max() {
        let mut soup = SoupField {
            concentration: 0.9,
            max: 1.0,
            regen_per_tick: 0.2,
            enabled: true,
        };
        soup.tick_regen();
        assert_eq!(soup.concentration, 1.0);
    }
}

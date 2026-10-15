//! Random world events: plagues, upwellings, meteor impacts, and rising barriers.
//!
//! Each event type is a Poisson process with a configurable mean interval (in sim hours).

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::rng::Rng;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WorldEvent {
    /// Members of `species` die at `kill_rate` per second until `until`.
    Plague {
        species: u32,
        until: f64,
        kill_rate: f32,
    },
    /// Minerals well up into the sunlit water around `x`.
    Upwelling {
        x: f32,
        width: f32,
        until: f64,
        rate: f32,
    },
    /// Dust from an impact dims the sun; `strength` fades to zero at `until`.
    ImpactWinter {
        started: f64,
        until: f64,
        strength: f32,
    },
    /// The water around `x` warms (`delta` > 0: a heat wave) or cools (a cold snap).
    TemperatureShift {
        x: f32,
        width: f32,
        delta: f32,
        started: f64,
        until: f64,
    },
    /// Fast churning currents in the upper water around `x`.
    Storm {
        x: f32,
        width: f32,
        started: f64,
        until: f64,
    },
    /// The sea falls by `drop` over `change` seconds, stays low for `low`, then rises back.
    SeaLevel {
        drop: f32,
        started: f64,
        change: f64,
        low: f64,
    },
    /// A rock wall rises from the seafloor at `x`, stands, then sinks.
    Barrier {
        id: u32,
        x: f32,
        width: f32,
        started: f64,
        rise: f64,
        stand: f64,
        /// Height the wall has been built to (for incremental terrain updates).
        built: f32,
    },
}

/// What the world should do this tick as a result of events.
#[derive(Default)]
pub struct EventEffects {
    /// Start a plague on the most dominant species (the world picks it).
    pub start_plague: bool,
    pub start_upwelling: Option<(f32, f32)>,
    /// Meteor strike at this point.
    pub impact: Option<Vec2>,
    pub start_barrier: Option<(f32, f32)>,
    /// Plagues/winters/upwellings/barriers that ended this tick (for the log).
    pub ended: Vec<WorldEvent>,
}

/// An event requested from outside (hotkeys, viewer commands).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trigger {
    Plague,
    Upwelling,
    Impact,
    Barrier,
}

impl std::str::FromStr for Trigger {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "plague" => Ok(Trigger::Plague),
            "upwelling" => Ok(Trigger::Upwelling),
            "impact" | "meteor" => Ok(Trigger::Impact),
            "barrier" | "wall" => Ok(Trigger::Barrier),
            _ => Err(format!(
                "unknown event '{s}' (plague, upwelling, impact, barrier)"
            )),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Events {
    pub active: Vec<WorldEvent>,
    /// Requested events, started on the next update regardless of the random schedule.
    #[serde(default)]
    pub pending: Vec<Trigger>,
    rng: Rng,
    next_barrier_id: u32,
    /// Seconds since the last Poisson check.
    since_check: f32,
}

const CHECK_INTERVAL: f32 = 10.0;

impl Events {
    pub fn new(seed: u64) -> Self {
        Self {
            active: Vec::new(),
            pending: Vec::new(),
            rng: Rng::new(seed ^ 0xE7E7_0001),
            next_barrier_id: 0,
            since_check: 0.0,
        }
    }

    fn roll(&mut self, mean_hours: f32) -> bool {
        mean_hours > 0.0 && self.rng.chance(CHECK_INTERVAL / (mean_hours * 3600.0))
    }

    pub fn is_active(&self, f: impl Fn(&WorldEvent) -> bool) -> bool {
        self.active.iter().any(f)
    }

    /// Sunlight multiplier from impact winters at `time`.
    pub fn sun_factor(&self, time: f64) -> f32 {
        let mut factor = 1.0;
        for e in &self.active {
            if let WorldEvent::ImpactWinter {
                started,
                until,
                strength,
            } = e
            {
                let t = ((time - started) / (until - started)).clamp(0.0, 1.0) as f32;
                // Dust thickens quickly, then settles slowly.
                let envelope = (t * 20.0).min(1.0) * (1.0 - t);
                factor *= 1.0 - strength * envelope;
            }
        }
        factor
    }

    /// Advance timers and roll for new events. `dt` is sim seconds since the last call.
    pub fn update(&mut self, time: f64, dt: f32, config: &Config) -> EventEffects {
        let cfg = &config.events;
        let mut fx = EventEffects::default();

        // Expire finished events.
        let mut i = 0;
        while i < self.active.len() {
            let done = match &self.active[i] {
                WorldEvent::Plague { until, .. }
                | WorldEvent::Upwelling { until, .. }
                | WorldEvent::ImpactWinter { until, .. }
                | WorldEvent::TemperatureShift { until, .. }
                | WorldEvent::Storm { until, .. } => time >= *until,
                WorldEvent::SeaLevel {
                    started,
                    change,
                    low,
                    ..
                } => time >= started + change * 2.0 + low,
                WorldEvent::Barrier {
                    started,
                    rise,
                    stand,
                    ..
                } => time >= started + rise * 2.0 + stand,
            };
            if done {
                fx.ended.push(self.active.swap_remove(i));
            } else {
                i += 1;
            }
        }

        let pending = std::mem::take(&mut self.pending);
        let forced = |t: Trigger| pending.contains(&t);
        let rolling = cfg.enabled && {
            self.since_check += dt;
            self.since_check >= CHECK_INTERVAL
        };
        if rolling {
            self.since_check = 0.0;
        }
        if !rolling && pending.is_empty() {
            return fx;
        }
        // Events happen in the ocean (not the basin beyond the mountain).
        let (w, h) = (config.ocean_width(), config.world.height);

        if (forced(Trigger::Plague) || rolling && self.roll(cfg.plague_every))
            && !self.is_active(|e| matches!(e, WorldEvent::Plague { .. }))
        {
            fx.start_plague = true;
        }
        if forced(Trigger::Upwelling) || rolling && self.roll(cfg.upwelling_every) {
            let x = self.rng.range(0.1, 0.9) * w;
            fx.start_upwelling = Some(self.start_upwelling(x, time, config));
        }
        if (forced(Trigger::Impact) || rolling && self.roll(cfg.impact_every))
            && !self.is_active(|e| matches!(e, WorldEvent::ImpactWinter { .. }))
        {
            let p = Vec2::new(self.rng.range(0.1, 0.9) * w, h - cfg.impact_radius * 0.4);
            fx.impact = Some(p);
            self.start_impact_winter(time, config);
        }
        if (forced(Trigger::Barrier) || rolling && self.roll(cfg.barrier_every))
            && !self.is_active(|e| matches!(e, WorldEvent::Barrier { .. }))
        {
            let x = self.rng.range(0.25, 0.7) * w;
            fx.start_barrier = Some(self.start_barrier(x, time, config));
        }
        fx
    }

    /// Start an upwelling centred on `x`. Returns its centre and width (for the log).
    pub fn start_upwelling(&mut self, x: f32, time: f64, config: &Config) -> (f32, f32) {
        let cfg = &config.events;
        self.active.push(WorldEvent::Upwelling {
            x,
            width: cfg.upwelling_width,
            until: time + cfg.upwelling_duration as f64,
            rate: cfg.upwelling_rate,
        });
        (x, cfg.upwelling_width)
    }

    /// Start an impact winter: dust from a meteor that just struck dims the sun.
    pub fn start_impact_winter(&mut self, time: f64, config: &Config) {
        let cfg = &config.events;
        self.active.push(WorldEvent::ImpactWinter {
            started: time,
            until: time + (cfg.impact_winter_days * config.time.day_length) as f64,
            strength: cfg.impact_winter,
        });
    }

    /// Start a heat wave (`warmer`) or cold snap around `x`. Returns the change in °C.
    pub fn start_temperature_shift(&mut self, x: f32, warmer: bool, time: f64, config: &Config) -> f32 {
        let cfg = &config.events;
        let delta = if warmer { cfg.temperature_shift } else { -cfg.temperature_shift };
        self.active.push(WorldEvent::TemperatureShift {
            x,
            width: cfg.temperature_shift_width,
            delta,
            started: time,
            until: time + cfg.temperature_shift_duration as f64,
        });
        delta
    }

    /// Start a storm around `x`.
    pub fn start_storm(&mut self, x: f32, time: f64, config: &Config) {
        let cfg = &config.events;
        self.active.push(WorldEvent::Storm {
            x,
            width: cfg.storm_width,
            started: time,
            until: time + cfg.storm_duration as f64,
        });
    }

    /// Start the sea falling by `drop` (at most 80% of the water's depth). Returns the drop.
    pub fn start_sea_level(&mut self, drop: f32, time: f64, config: &Config) -> f32 {
        let cfg = &config.events;
        let drop = drop.clamp(0.0, config.world.height * 0.8);
        self.active.push(WorldEvent::SeaLevel {
            drop,
            started: time,
            change: cfg.sea_level_change as f64,
            low: cfg.sea_level_low as f64,
        });
        drop
    }

    /// How far below its usual level the sea is at `time`.
    pub fn sea_drop(&self, time: f64) -> f32 {
        let mut most = 0.0f32;
        for e in &self.active {
            if let WorldEvent::SeaLevel {
                drop,
                started,
                change,
                low,
            } = e
            {
                let t = time - started;
                let change = change.max(1.0);
                let f = if t < change {
                    t / change
                } else if t < change + low {
                    1.0
                } else {
                    1.0 - (t - change - low) / change
                };
                let f = f.clamp(0.0, 1.0) as f32;
                most = most.max(drop * f * f * (3.0 - 2.0 * f));
            }
        }
        most
    }

    /// The heat waves and cold snaps now, as (centre, width, °C change at the surface right
    /// now), each building up and dying away gradually.
    pub fn temperature_shifts(&self, time: f64, out: &mut Vec<(f32, f32, f32)>) {
        out.clear();
        for e in &self.active {
            if let WorldEvent::TemperatureShift { x, width, delta, started, until } = e {
                out.push((*x, *width, delta * envelope(*started, *until, time)));
            }
        }
    }

    /// The storms now, as (centre, width, strength 0..1).
    pub fn storms(&self, time: f64, out: &mut Vec<(f32, f32, f32)>) {
        out.clear();
        for e in &self.active {
            if let WorldEvent::Storm { x, width, started, until } = e {
                out.push((*x, *width, envelope(*started, *until, time)));
            }
        }
    }

    /// Start a rock wall rising at `x`. Returns its position and width (for the log).
    pub fn start_barrier(&mut self, x: f32, time: f64, config: &Config) -> (f32, f32) {
        let cfg = &config.events;
        let id = self.next_barrier_id;
        self.next_barrier_id += 1;
        self.active.push(WorldEvent::Barrier {
            id,
            x,
            width: cfg.barrier_width,
            started: time,
            rise: cfg.barrier_rise as f64 * 3600.0,
            stand: cfg.barrier_duration as f64 * 3600.0,
            built: 0.0,
        });
        (x, cfg.barrier_width)
    }

    /// Register a plague on `species` (chosen by the world).
    pub fn add_plague(&mut self, species: u32, time: f64, config: &Config) {
        self.active.push(WorldEvent::Plague {
            species,
            until: time + config.events.plague_duration as f64,
            kill_rate: config.events.plague_kill_rate,
        });
    }

    pub fn plague_rate(&self, species: u32) -> f32 {
        self.active
            .iter()
            .map(|e| match e {
                WorldEvent::Plague {
                    species: s,
                    kill_rate,
                    ..
                } if *s == species => *kill_rate,
                _ => 0.0,
            })
            .sum()
    }
}

/// How strong an event lasting from `started` to `until` is at `time`: it builds up over the
/// first 15% and dies away over the last 15%.
pub fn envelope(started: f64, until: f64, time: f64) -> f32 {
    let t = ((time - started) / (until - started).max(1.0)).clamp(0.0, 1.0) as f32;
    (t / 0.15).min((1.0 - t) / 0.15).clamp(0.0, 1.0)
}

/// How much of an event centred on `x`, `width` wide, reaches `px`: all of it in the middle,
/// fading smoothly to nothing at the edges.
pub fn across(x: f32, width: f32, px: f32) -> f32 {
    let t = (1.0 - (px - x).abs() / (width * 0.5).max(1.0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Height of a barrier's top at `time` (world y), given the floor under it.
pub fn barrier_height(e: &WorldEvent, time: f64, floor: f32, world_height: f32) -> Option<f32> {
    let WorldEvent::Barrier {
        started,
        rise,
        stand,
        ..
    } = e
    else {
        return None;
    };
    let t = time - started;
    let top = world_height + 100.0;
    let frac = if t < *rise {
        t / rise
    } else if t < rise + stand {
        1.0
    } else {
        1.0 - (t - rise - stand) / rise
    };
    Some(floor + (top - floor) * frac.clamp(0.0, 1.0) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_fire_at_roughly_the_configured_rate() {
        let mut config = Config::default();
        config.events.upwelling_every = 1.0;
        config.events.plague_every = 0.0;
        config.events.impact_every = 0.0;
        config.events.barrier_every = 0.0;
        config.events.upwelling_duration = 1.0;
        let mut events = Events::new(3);
        let mut count = 0;
        let mut time = 0.0;
        for _ in 0..(200 * 3600) {
            time += 1.0;
            let fx = events.update(time, 1.0, &config);
            if fx.start_upwelling.is_some() {
                count += 1;
            }
        }
        assert!((150..260).contains(&count), "count {count}");
    }

    #[test]
    fn triggers_start_events_immediately() {
        let mut config = Config::default();
        config.events.enabled = false;
        let mut events = Events::new(5);
        events.pending.push(Trigger::Impact);
        events.pending.push(Trigger::Barrier);
        let fx = events.update(1.0, 0.033, &config);
        assert!(fx.impact.is_some() && fx.start_barrier.is_some());
        assert_eq!(events.active.len(), 2);
        assert!(events.pending.is_empty());
    }

    #[test]
    fn impact_winter_dims_then_clears() {
        let config = Config::default();
        let mut events = Events::new(1);
        events.active.push(WorldEvent::ImpactWinter {
            started: 0.0,
            until: 1000.0,
            strength: 0.8,
        });
        assert!(events.sun_factor(100.0) < 0.5);
        assert!(events.sun_factor(990.0) > 0.95);
        events.update(1001.0, 1.0, &config);
        assert!(events.active.is_empty());
    }

    #[test]
    fn barrier_rises_stands_and_sinks() {
        let e = WorldEvent::Barrier {
            id: 0,
            x: 0.0,
            width: 100.0,
            started: 0.0,
            rise: 100.0,
            stand: 100.0,
            built: 0.0,
        };
        let floor = 1000.0;
        let h = 6000.0;
        assert_eq!(barrier_height(&e, 0.0, floor, h), Some(floor));
        assert!(barrier_height(&e, 50.0, floor, h).unwrap() > 3000.0);
        assert!(barrier_height(&e, 150.0, floor, h).unwrap() > h);
        assert!(barrier_height(&e, 290.0, floor, h).unwrap() < 2000.0);
    }
}

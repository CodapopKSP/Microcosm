//! Shared helpers for interval-scheduled systems (sensors, biomass feeding, logic cadence, …).

use crate::config::RunConfig;

/// True when `tick` hits this repeating interval, optionally shifted by `phase_ticks`.
///
/// Phase is taken mod interval so sensors (phase 0) and touching (phase 30) can share a
/// 60-tick cadence without running on the same tick.
pub fn interval_active_this_tick(tick: u64, interval_ticks: u32, phase_ticks: u32) -> bool {
    let interval = u64::from(interval_ticks.max(1));
    let phase = u64::from(phase_ticks) % interval;
    tick.wrapping_sub(phase) % interval == 0
}

/// Whether non-movement simulation logic runs this tick ([`RunConfig::logic_interval_ticks`]).
pub fn logic_active_this_tick(tick: u64, run: &RunConfig) -> bool {
    interval_active_this_tick(
        tick,
        run.logic_interval_ticks,
        run.logic_interval_phase_ticks,
    )
}

/// Ticks of sim time accumulated in one logic pass (for scaling per-tick rates).
pub fn logic_elapsed_ticks(run: &RunConfig) -> u32 {
    run.logic_interval_ticks.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_zero_matches_modulo() {
        assert!(interval_active_this_tick(0, 60, 0));
        assert!(!interval_active_this_tick(1, 60, 0));
        assert!(interval_active_this_tick(60, 60, 0));
    }

    #[test]
    fn half_interval_phase_offsets_touching_from_sensors() {
        assert!(interval_active_this_tick(0, 60, 0));
        assert!(!interval_active_this_tick(0, 60, 30));
        assert!(interval_active_this_tick(30, 60, 30));
        assert!(!interval_active_this_tick(30, 60, 0));
        assert!(interval_active_this_tick(60, 60, 0));
        assert!(interval_active_this_tick(90, 60, 30));
    }

    #[test]
    fn logic_interval_runs_every_six_ticks_by_default() {
        let run = RunConfig {
            master_seed: None,
            max_ticks: 0,
            logic_interval_ticks: 6,
            logic_interval_phase_ticks: 0,
            snapshot_interval: 60,
            metrics_path: "metrics.jsonl".into(),
        };
        assert!(logic_active_this_tick(0, &run));
        assert!(!logic_active_this_tick(1, &run));
        assert!(logic_active_this_tick(6, &run));
        assert_eq!(logic_elapsed_ticks(&run), 6);
    }
}

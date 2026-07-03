//! JSONL metrics export for headless runs (Phase 3C).

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use bevy::prelude::*;
use serde::Serialize;

use super::behavior::{BehaviorMode, BehaviorState};
use super::components::{Agent, DeathReason, Energy, Genome, Health, Phenotype};
use super::world::SimConfigResource;

/// Repair energy spent since the previous metrics sample (Phase 4D).
#[derive(Resource, Clone, Default)]
pub struct RepairStats {
    pub energy_spent_this_period: f32,
}

impl RepairStats {
    pub fn reset(&mut self) {
        self.energy_spent_this_period = 0.0;
    }
}

/// Death and birth counts since the previous metrics sample.
#[derive(Resource, Clone, Default)]
pub struct MetricsPeriodCounters {
    pub births: u32,
    pub deaths_energy_depleted: u32,
    pub deaths_core_destroyed: u32,
    pub deaths_repro_fatality: u32,
    pub deaths_predation: u32,
    pub deaths_age_exceeded: u32,
}

impl MetricsPeriodCounters {
    pub fn record_death(&mut self, reason: DeathReason) {
        match reason {
            DeathReason::EnergyDepleted => self.deaths_energy_depleted += 1,
            DeathReason::CoreDestroyed => self.deaths_core_destroyed += 1,
            DeathReason::ReproFatality => self.deaths_repro_fatality += 1,
            DeathReason::Predation => self.deaths_predation += 1,
            DeathReason::AgeExceeded => self.deaths_age_exceeded += 1,
        }
    }

    pub fn record_birth(&mut self) {
        self.births += 1;
    }

    pub fn total_deaths(&self) -> u32 {
        self.deaths_energy_depleted
            .saturating_add(self.deaths_core_destroyed)
            .saturating_add(self.deaths_repro_fatality)
            .saturating_add(self.deaths_predation)
            .saturating_add(self.deaths_age_exceeded)
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Append-only JSONL writer; enabled for headless runs only.
#[derive(Resource)]
pub struct MetricsWriter {
    pub path: String,
    pub enabled: bool,
    writer: Option<BufWriter<File>>,
}

impl MetricsWriter {
    pub fn new(path: String, enabled: bool) -> Self {
        Self {
            path,
            enabled,
            writer: None,
        }
    }

    /// Truncate `metrics_path` and open for append (headless startup).
    pub fn open_truncate(&mut self) -> std::io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if let Some(parent) = Path::new(&self.path).parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let file = File::create(&self.path)?;
        self.writer = Some(BufWriter::new(file));
        Ok(())
    }

    pub fn write_line(&mut self, line: &MetricsLine) -> std::io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let writer = self
            .writer
            .as_mut()
            .expect("MetricsWriter::open_truncate must run before write_line");
        serde_json::to_writer(&mut *writer, line)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        Ok(())
    }
}

/// One JSONL record ([PHASE_3.md](../../docs/PHASE_3.md) schema).
#[derive(Debug, Serialize)]
pub struct MetricsLine {
    pub tick: u64,
    pub population: u32,
    pub births: u32,
    pub deaths: u32,
    pub deaths_energy_depleted: u32,
    pub deaths_core_destroyed: u32,
    pub deaths_repro_fatality: u32,
    pub deaths_predation: u32,
    pub deaths_age_exceeded: u32,
    #[serde(serialize_with = "serialize_rounded_f32")]
    pub mean_energy: f32,
    #[serde(serialize_with = "serialize_rounded_f32")]
    pub median_energy: f32,
    #[serde(serialize_with = "serialize_rounded_f32")]
    pub mean_photo_strength: f32,
    #[serde(serialize_with = "serialize_rounded_f32")]
    pub mean_age_metabolism_scale: f32,
    #[serde(serialize_with = "serialize_rounded_f32")]
    pub min_age_metabolism_scale: f32,
    #[serde(serialize_with = "serialize_rounded_f32")]
    pub max_age_metabolism_scale: f32,
    pub color_bins: BTreeMap<String, u32>,
    pub radius_bins: BTreeMap<String, u32>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub genotype_hash_histogram: BTreeMap<String, u32>,
    #[serde(serialize_with = "serialize_rounded_f32")]
    pub mean_skin_hardness: f32,
    #[serde(serialize_with = "serialize_rounded_f32")]
    pub mean_core_hp_frac: f32,
    #[serde(serialize_with = "serialize_rounded_f32")]
    pub repair_energy_spent: f32,
    #[serde(serialize_with = "serialize_rounded_f32")]
    pub mean_greenness: f32,
    pub genome_version: u8,
    #[serde(serialize_with = "serialize_rounded_f32")]
    pub mean_max_energy: f32,
    #[serde(serialize_with = "serialize_rounded_f32")]
    pub mean_max_health: f32,
    pub behavior_wander: u32,
    pub behavior_pursue: u32,
    pub behavior_avoid: u32,
}

pub fn round_metric(x: f32) -> f64 {
    let v = (f64::from(x) * 1e4).round() / 1e4;
    if v == -0.0 {
        0.0
    } else {
        v
    }
}

fn serialize_rounded_f32<S>(value: &f32, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_f64(round_metric(*value))
}

fn fold_genome_key_v2(genome: &crate::genome::GenomeV2) -> String {
    let fold = genome
        .bytes
        .iter()
        .fold(0u64, |acc, &b| acc.wrapping_mul(31).wrapping_add(u64::from(b)));
    format!("{fold:016x}")
}

fn color_bin_key(color: [f32; 3]) -> String {
    let r = (color[0].clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (color[1].clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (color[2].clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("{r},{g},{b}")
}

fn radius_bin_key(radius: f32) -> String {
    format!("{}", radius.round() as i32)
}

fn median_sorted(values: &mut [f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        values[mid]
    } else {
        (values[mid - 1] + values[mid]) * 0.5
    }
}

pub fn build_metrics_line(
    tick: u64,
    counters: &MetricsPeriodCounters,
    genome_version: u8,
    energies: &[f32],
    photo_strengths: &[f32],
    age_metabolism_scales: &[f32],
    color_keys: &[String],
    radii: &[f32],
    genotype_keys: &[String],
    skin_hardnesses: &[f32],
    core_hp_fracs: &[f32],
    repair_energy_spent: f32,
    greennesses: &[f32],
    max_energies: &[f32],
    max_healths: &[f32],
    behavior_wander: u32,
    behavior_pursue: u32,
    behavior_avoid: u32,
) -> MetricsLine {
    let population = energies.len() as u32;
    let mean_energy = if population == 0 {
        0.0
    } else {
        energies.iter().sum::<f32>() / population as f32
    };
    let mut energy_sorted = energies.to_vec();
    let median_energy = median_sorted(&mut energy_sorted);

    let mean_photo_strength = if photo_strengths.is_empty() {
        0.0
    } else {
        photo_strengths.iter().sum::<f32>() / photo_strengths.len() as f32
    };

    let mean_skin_hardness = if skin_hardnesses.is_empty() {
        0.0
    } else {
        skin_hardnesses.iter().sum::<f32>() / skin_hardnesses.len() as f32
    };

    let mean_core_hp_frac = if core_hp_fracs.is_empty() {
        0.0
    } else {
        core_hp_fracs.iter().sum::<f32>() / core_hp_fracs.len() as f32
    };

    let mean_greenness = if greennesses.is_empty() {
        0.0
    } else {
        greennesses.iter().sum::<f32>() / greennesses.len() as f32
    };

    let (mean_age_metabolism_scale, min_age_metabolism_scale, max_age_metabolism_scale) =
        if age_metabolism_scales.is_empty() {
            (0.0, 0.0, 0.0)
        } else {
            let sum: f32 = age_metabolism_scales.iter().sum();
            let mean = sum / age_metabolism_scales.len() as f32;
            let min = age_metabolism_scales
                .iter()
                .copied()
                .fold(f32::INFINITY, f32::min);
            let max = age_metabolism_scales
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max);
            (mean, min, max)
        };

    let mut color_bins = BTreeMap::new();
    for key in color_keys {
        *color_bins.entry(key.clone()).or_insert(0) += 1;
    }

    let mut radius_bins = BTreeMap::new();
    for &r in radii {
        let key = radius_bin_key(r);
        *radius_bins.entry(key).or_insert(0) += 1;
    }

    let mut genotype_hash_histogram = BTreeMap::new();
    for key in genotype_keys {
        *genotype_hash_histogram.entry(key.clone()).or_insert(0) += 1;
    }

    let mean_max_energy = if max_energies.is_empty() {
        0.0
    } else {
        max_energies.iter().sum::<f32>() / max_energies.len() as f32
    };
    let mean_max_health = if max_healths.is_empty() {
        0.0
    } else {
        max_healths.iter().sum::<f32>() / max_healths.len() as f32
    };

    MetricsLine {
        tick,
        population,
        births: counters.births,
        deaths: counters.total_deaths(),
        deaths_energy_depleted: counters.deaths_energy_depleted,
        deaths_core_destroyed: counters.deaths_core_destroyed,
        deaths_repro_fatality: counters.deaths_repro_fatality,
        deaths_predation: counters.deaths_predation,
        deaths_age_exceeded: counters.deaths_age_exceeded,
        mean_energy,
        median_energy,
        mean_photo_strength,
        mean_age_metabolism_scale,
        min_age_metabolism_scale,
        max_age_metabolism_scale,
        color_bins,
        radius_bins,
        genotype_hash_histogram,
        mean_skin_hardness,
        mean_core_hp_frac,
        repair_energy_spent,
        mean_greenness,
        genome_version,
        mean_max_energy,
        mean_max_health,
        behavior_wander,
        behavior_pursue,
        behavior_avoid,
    }
}

/// Sample population metrics when `tick > 0` and `tick % snapshot_interval == 0`.
pub fn metrics_sample(
    tick: Res<super::world::SimTick>,
    config: Res<SimConfigResource>,
    mut counters: ResMut<MetricsPeriodCounters>,
    mut repair_stats: ResMut<RepairStats>,
    mut writer: ResMut<MetricsWriter>,
    query: Query<(
        &Energy,
        &Phenotype,
        &Genome,
        &Agent,
        &Health,
        Option<&BehaviorState>,
    )>,
) {
    if !writer.enabled {
        return;
    }

    let interval = config.0.run.snapshot_interval;
    if interval == 0 || tick.tick == 0 || tick.tick % interval != 0 {
        return;
    }

    let mut energies = Vec::new();
    let mut photo_strengths = Vec::new();
    let mut age_metabolism_scales = Vec::new();
    let mut color_keys = Vec::new();
    let mut radii = Vec::new();
    let mut genotype_keys = Vec::new();
    let mut skin_hardnesses = Vec::new();
    let mut core_hp_fracs = Vec::new();
    let mut greennesses = Vec::new();
    let mut max_energies = Vec::new();
    let mut max_healths = Vec::new();
    let mut behavior_wander = 0_u32;
    let mut behavior_pursue = 0_u32;
    let mut behavior_avoid = 0_u32;
    let genome_version = config.0.genetics.genome_version;

    for (energy, phenotype, genome, agent, health, behavior) in &query {
        energies.push(energy.internal_energy);
        max_energies.push(energy.max_internal_energy);
        radii.push(agent.radius);
        let frac = if health.core_max_hp > 0.0 {
            health.core_hp / health.core_max_hp
        } else {
            0.0
        };
        core_hp_fracs.push(frac);
        max_healths.push(health.core_max_hp);

        if let Some(b) = behavior {
            match b.mode {
                BehaviorMode::Wander => behavior_wander += 1,
                BehaviorMode::Pursue => behavior_pursue += 1,
                BehaviorMode::Avoid => behavior_avoid += 1,
            }
        }

        let p = &phenotype.0;
        let g = &genome.0;
        photo_strengths.push(p.photo_effective);
        age_metabolism_scales.push(0.0);
        color_keys.push(color_bin_key(p.color));
        genotype_keys.push(fold_genome_key_v2(g));
        skin_hardnesses.push(p.armor_stroke);
        greennesses.push(p.color[1]);
    }

    let repair_spent = repair_stats.energy_spent_this_period;
    let line = build_metrics_line(
        tick.tick,
        &counters,
        genome_version,
        &energies,
        &photo_strengths,
        &age_metabolism_scales,
        &color_keys,
        &radii,
        &genotype_keys,
        &skin_hardnesses,
        &core_hp_fracs,
        repair_spent,
        &greennesses,
        &max_energies,
        &max_healths,
        behavior_wander,
        behavior_pursue,
        behavior_avoid,
    );

    if let Err(err) = writer.write_line(&line) {
        eprintln!("metrics: failed to write {}: {err}", writer.path);
    }

    counters.reset();
    repair_stats.reset();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_metric_four_decimal_places() {
        assert_eq!(round_metric(1.234567), 1.2346);
        assert_eq!(round_metric(0.0), 0.0);
    }

    #[test]
    fn death_subcounts_sum_to_deaths() {
        let mut counters = MetricsPeriodCounters::default();
        counters.record_death(DeathReason::EnergyDepleted);
        counters.record_death(DeathReason::ReproFatality);
        counters.record_death(DeathReason::AgeExceeded);
        let line = build_metrics_line(
            60,
            &counters,
            1,
            &[1.0],
            &[0.5],
            &[3000.0],
            &[],
            &[12.0],
            &[],
            &[0.2],
            &[0.9],
            1.5,
            &[0.6],
            &[100.0],
            &[50.0],
            1,
            0,
            0,
        );
        assert_eq!(line.deaths, 3);
        assert_eq!(
            line.deaths_energy_depleted
                + line.deaths_core_destroyed
                + line.deaths_repro_fatality
                + line.deaths_predation
                + line.deaths_age_exceeded,
            line.deaths
        );
    }

    #[test]
    fn json_line_contains_required_fields() {
        let counters = MetricsPeriodCounters::default();
        let line = build_metrics_line(
            60,
            &counters,
            2,
            &[10.0, 20.0],
            &[0.4, 0.5],
            &[2000.0, 4000.0],
            &["1,2,3".into(), "1,2,3".into()],
            &[12.0, 12.0],
            &["abc".into()],
            &[0.0, 0.5],
            &[1.0, 0.8],
            0.0,
            &[0.5, 0.7],
            &[100.0, 120.0],
            &[80.0, 90.0],
            2,
            0,
            0,
        );
        let json = serde_json::to_string(&line).unwrap();
        for key in [
            "tick",
            "population",
            "births",
            "deaths",
            "deaths_energy_depleted",
            "deaths_core_destroyed",
            "deaths_repro_fatality",
            "deaths_predation",
            "deaths_age_exceeded",
            "mean_energy",
            "median_energy",
            "mean_photo_strength",
            "mean_age_metabolism_scale",
            "min_age_metabolism_scale",
            "max_age_metabolism_scale",
            "color_bins",
            "radius_bins",
            "mean_skin_hardness",
            "mean_core_hp_frac",
            "repair_energy_spent",
            "mean_greenness",
            "genome_version",
            "mean_max_energy",
            "mean_max_health",
            "behavior_wander",
            "behavior_pursue",
            "behavior_avoid",
        ] {
            assert!(json.contains(key), "missing {key} in {json}");
        }
        assert!(json.contains("genotype_hash_histogram"));
    }

    #[test]
    fn headless_metrics_include_phase4_fields() {
        let counters = MetricsPeriodCounters::default();
        let line = build_metrics_line(
            60,
            &counters,
            1,
            &[10.0],
            &[0.5],
            &[3000.0],
            &[],
            &[12.0],
            &[],
            &[0.25],
            &[0.75],
            2.5,
            &[0.8],
            &[50.0],
            &[40.0],
            0,
            0,
            0,
        );
        assert_eq!(round_metric(line.mean_skin_hardness), 0.25);
        assert_eq!(round_metric(line.mean_core_hp_frac), 0.75);
        assert_eq!(round_metric(line.repair_energy_spent), 2.5);
        assert_eq!(round_metric(line.mean_greenness), 0.8);
    }
}

//! Phase 3E acceptance: headless Bevy metrics (main thread; `harness = false`).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;

use evolution_sim::config::SimConfig;
use evolution_sim::genome::Genome;
use evolution_sim::sim::{run_headless_bevy, SimRunMode};

fn main() {
    headless_writes_valid_jsonl();
    headless_metrics_deterministic_same_seed();
    headless_metrics_differ_different_seed();
}

fn headless_metrics_config(seed: u64, path: &Path) -> SimConfig {
    let mut config = SimConfig::load_default().expect("load default config");
    config.run.master_seed = Some(seed);
    config.run.max_ticks = 240;
    config.run.snapshot_interval = 60;
    config.run.metrics_path = path.to_string_lossy().into_owned();
    config.world.initial_population = 40;
    config
}

fn assert_valid_jsonl_lines(lines: &[String]) {
    assert_eq!(
        lines.len(),
        4,
        "expected samples at ticks 60, 120, 180, 240 (got {} lines)",
        lines.len()
    );
    for line in lines {
        let v: serde_json::Value = serde_json::from_str(line).expect("valid JSON");
        for key in [
            "tick",
            "population",
            "births",
            "deaths",
            "deaths_energy_depleted",
            "deaths_core_destroyed",
            "deaths_repro_fatality",
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
        ] {
            assert!(v.get(key).is_some(), "missing {key} in {line}");
        }
        let deaths = v["deaths"].as_u64().unwrap();
        let sub = v["deaths_energy_depleted"].as_u64().unwrap()
            + v["deaths_core_destroyed"].as_u64().unwrap()
            + v["deaths_repro_fatality"].as_u64().unwrap()
            + v["deaths_age_exceeded"].as_u64().unwrap();
        assert_eq!(deaths, sub, "death sub-counts must sum to deaths");
    }
}

fn read_jsonl(path: &Path) -> Vec<String> {
    let file = std::fs::File::open(path).expect("metrics file should exist");
    BufReader::new(file)
        .lines()
        .map(|l| l.expect("read line"))
        .collect()
}

/// One in-process Bevy headless run (main thread only).
fn headless_writes_valid_jsonl() {
    let path = std::env::temp_dir().join("evolution_sim_phase3c_metrics.jsonl");
    let _ = std::fs::remove_file(&path);
    let config = headless_metrics_config(42, &path);
    let ancestor = Genome::ancestor_from_config(&config).expect("ancestor");
    run_headless_bevy(config, ancestor);
    assert_valid_jsonl_lines(&read_jsonl(&path));
}

/// Two CLI subprocesses — fresh event loop per run (determinism policy).
fn headless_metrics_deterministic_same_seed() {
    let path_a = std::env::temp_dir().join("evolution_sim_metrics_det_a.jsonl");
    let path_b = std::env::temp_dir().join("evolution_sim_metrics_det_b.jsonl");
    let config_a = write_headless_fixture(99, &path_a);
    let config_b = write_headless_fixture(99, &path_b);

    run_headless_cli(&config_a, 99);
    run_headless_cli(&config_b, 99);

    let bytes_a = std::fs::read(&path_a).expect("read metrics a");
    let bytes_b = std::fs::read(&path_b).expect("read metrics b");
    assert_eq!(
        bytes_a, bytes_b,
        "identical seed+config must produce identical metrics file bytes"
    );
}

fn headless_metrics_differ_different_seed() {
    let path_a = std::env::temp_dir().join("evolution_sim_metrics_seed_a.jsonl");
    let path_b = std::env::temp_dir().join("evolution_sim_metrics_seed_b.jsonl");
    let config_a = write_headless_fixture(1, &path_a);
    let config_b = write_headless_fixture(2, &path_b);

    run_headless_cli(&config_a, 1);
    run_headless_cli(&config_b, 2);

    let bytes_a = std::fs::read(&path_a).expect("read metrics a");
    let bytes_b = std::fs::read(&path_b).expect("read metrics b");
    assert_ne!(
        bytes_a, bytes_b,
        "different master_seed should change metrics output"
    );
}

fn write_headless_fixture(seed: u64, metrics_path: &Path) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut content =
        std::fs::read_to_string(root.join("configs/default.toml")).expect("read default.toml");
    content = content
        .replace("max_ticks = 360000", "max_ticks = 240")
        .replace(
            "metrics_path = \"metrics.jsonl\"",
            &format!(
                "metrics_path = \"{}\"",
                metrics_path.to_string_lossy().replace('\\', "/")
            ),
        )
        .replace("master_seed = 42", &format!("master_seed = {seed}"))
        .replace("initial_population = 80", "initial_population = 40");

    let fixture = std::env::temp_dir().join(format!(
        "evolution_sim_headless_fixture_{seed}_{}.toml",
        metrics_path.file_name().unwrap().to_string_lossy()
    ));
    std::fs::write(&fixture, content).expect("write fixture");
    fixture
}

fn run_headless_cli(config_path: &Path, seed: u64) {
    let bin = env!("CARGO_BIN_EXE_evolution_sim");
    let status = Command::new(bin)
        .args([
            "--headless",
            "--config",
            config_path.to_str().expect("utf8 path"),
            "--seed",
            &seed.to_string(),
        ])
        .status()
        .expect("spawn evolution_sim");
    assert!(
        status.success(),
        "headless CLI failed for {} seed {seed}",
        config_path.display()
    );
}

#[allow(dead_code)]
fn _link_sim_run_mode() {
    let _ = SimRunMode::Headless;
}

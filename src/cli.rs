//! Command-line interface (Phase 3A).

use std::path::PathBuf;

use clap::Parser;

use crate::config::{ConfigError, SimConfig, default_config_path};

/// `evolution_sim [--config PATH] [--headless] [--seed U64]`
#[derive(Parser, Debug, Clone)]
#[command(name = "evolution_sim", version, about = "Evolution simulation")]
pub struct Cli {
    /// Path to simulation TOML (default: `configs/default.toml` in crate root).
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Run without a window until `run.max_ticks` in the config.
    #[arg(long)]
    pub headless: bool,

    /// Override `run.master_seed` after loading the config file.
    #[arg(long, value_name = "U64")]
    pub seed: Option<u64>,
}

impl Cli {
    pub fn config_path(&self) -> PathBuf {
        self.config
            .clone()
            .unwrap_or_else(default_config_path)
    }

    /// Load config from [`Self::config_path`], apply CLI overrides, validate headless constraints.
    pub fn load_config(&self) -> Result<SimConfig, ConfigError> {
        let mut config = SimConfig::load_from_path(self.config_path())?;
        if let Some(seed) = self.seed {
            config.run.master_seed = Some(seed);
        }
        if self.headless && config.run.max_ticks == 0 {
            return Err(ConfigError::Validation(
                "--headless requires run.max_ticks > 0 in the config".into(),
            ));
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_path_when_flag_omitted() {
        let cli = Cli::parse_from(["evolution_sim"]);
        assert_eq!(cli.config_path(), default_config_path());
        assert!(!cli.headless);
        assert!(cli.seed.is_none());
    }

    #[test]
    fn seed_override_applied_after_load() {
        let cli = Cli::parse_from([
            "evolution_sim",
            "--config",
            default_config_path().to_str().unwrap(),
            "--seed",
            "99",
        ]);
        let config = cli.load_config().unwrap();
        assert_eq!(config.run.master_seed, Some(99));
    }

    #[test]
    fn missing_config_file_is_readable_error() {
        let cli = Cli::parse_from(["evolution_sim", "--config", "/no/such/config.toml"]);
        let err = cli.load_config().unwrap_err();
        assert!(err.to_string().contains("config.toml"));
    }

    #[test]
    fn headless_rejects_zero_max_ticks() {
        let default_path = default_config_path();
        let mut contents = std::fs::read_to_string(&default_path).unwrap();
        let mut lines: Vec<String> = contents.lines().map(String::from).collect();
        for line in &mut lines {
            if line.starts_with("max_ticks = ") {
                *line = "max_ticks = 0".into();
            }
        }
        contents = format!("{}\n", lines.join("\n"));

        let path = std::env::temp_dir().join("evolution_sim_cli_test_zero_ticks.toml");
        std::fs::write(&path, contents).unwrap();

        let cli = Cli::parse_from([
            "evolution_sim",
            "--headless",
            "--config",
            path.to_str().unwrap(),
        ]);
        let err = cli.load_config().unwrap_err();
        assert!(err.to_string().contains("max_ticks"));

        let _ = std::fs::remove_file(path);
    }
}

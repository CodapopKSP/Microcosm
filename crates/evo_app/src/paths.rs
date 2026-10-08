//! Where the game keeps things, so it runs from anywhere (an install folder, a download):
//! the default settings and the built-in maps are built into the program, and saves and the
//! player's own maps go in the system's per-user data folder:
//!
//! - Windows: `%APPDATA%\Microcosm`
//! - macOS: `~/Library/Application Support/Microcosm`
//! - Linux: `$XDG_DATA_HOME/microcosm` (usually `~/.local/share/microcosm`)

use std::path::{Path, PathBuf};

use evo_core::Config;
use evo_core::config::MapSource;
use evo_core::map::PaintedMap;

/// The default settings (`configs/default.toml`, built in).
const DEFAULT_CONFIG: &str = include_str!("../../../configs/default.toml");

/// Maps that come with the game (`configs/maps`, built in).
const BUILTIN_MAPS: &[(&str, &[u8])] = &[(
    "Deep Ocean",
    include_bytes!("../../../configs/maps/Deep Ocean.map"),
)];

/// The game's folder in the system's per-user data folder (created when first needed).
pub fn data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Microcosm"))
    } else if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library/Application Support/Microcosm"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home.map(|h| h.join(".local/share")))
            .map(|d| d.join("microcosm"))
    };
    // No home folder at all (unusual): beside the program.
    base.unwrap_or_else(|| PathBuf::from("microcosm-data"))
}

/// Where worlds are saved (unless `--save-dir` says otherwise).
pub fn saves_dir() -> PathBuf {
    data_dir().join("saves")
}

/// Where maps made in the editor are saved.
pub fn user_maps_dir() -> PathBuf {
    data_dir().join("maps")
}

/// A user map's file.
pub fn user_map_path(name: &str) -> PathBuf {
    user_maps_dir().join(format!("{name}.map"))
}

/// Whether a map of this name comes with the game.
pub fn is_builtin_map(name: &str) -> bool {
    BUILTIN_MAPS.iter().any(|(n, _)| *n == name)
}

/// Find map `name`: the player's own maps first (so saving a map under a built-in's name
/// replaces it for them), then maps beside a config file given on the command line, then the
/// built-in ones.
pub fn find_map(name: &str, config_dir: Option<&Path>) -> Result<Option<MapSource>, String> {
    let mut dirs = vec![user_maps_dir()];
    if let Some(dir) = config_dir {
        dirs.push(dir.join("maps"));
    }
    for dir in dirs {
        let painted = dir.join(format!("{name}.map"));
        if painted.exists() {
            return PaintedMap::load(&painted).map(|m| Some(MapSource::Painted(m)));
        }
        let generated = dir.join(format!("{name}.toml"));
        if let Ok(text) = std::fs::read_to_string(&generated) {
            return Ok(Some(MapSource::Generated(text)));
        }
    }
    match BUILTIN_MAPS.iter().find(|(n, _)| *n == name) {
        Some((_, bytes)) => PaintedMap::from_bytes(bytes)
            .map(|m| Some(MapSource::Painted(m)))
            .map_err(|e| format!("built-in map \"{name}\": {e}")),
        None => Ok(None),
    }
}

/// A painted map by name (the player's, else built in), for the editor.
pub fn load_painted(name: &str) -> Result<PaintedMap, String> {
    match find_map(name, None)? {
        Some(MapSource::Painted(m)) => Ok(m),
        Some(MapSource::Generated(_)) => Err(format!("\"{name}\" isn't a painted map")),
        None => Err(format!("there's no map called \"{name}\"")),
    }
}

/// The settings: the config file given on the command line, else the built-in defaults; with
/// map `map` (else the config's own).
pub fn load_config(config_file: Option<&Path>, map: Option<&str>) -> Result<Config, String> {
    match config_file {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
            let dir = path.parent().unwrap_or(Path::new("."));
            Config::from_text(&text, &path.display().to_string(), map, &|name| {
                find_map(name, Some(dir))
            })
        }
        None => Config::from_text(DEFAULT_CONFIG, "default settings", map, &|name| {
            find_map(name, None)
        }),
    }
}

/// Every map to choose from, as (name, description): the player's own and the built-in ones
/// (a player's map of the same name replaces the built-in one), sorted by name.
pub fn list_maps() -> Vec<(String, String)> {
    let describe = |map: &PaintedMap| {
        if map.description.is_empty() {
            "Painted in the map editor".to_string()
        } else {
            map.description.clone()
        }
    };
    let mut maps: Vec<(String, String)> = std::fs::read_dir(user_maps_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            (path.extension()? == "map").then_some(())?;
            let name = path.file_stem()?.to_str()?.to_string();
            let map = PaintedMap::load(&path).ok()?;
            Some((name, describe(&map)))
        })
        .collect();
    for (name, bytes) in BUILTIN_MAPS {
        if !maps.iter().any(|(n, _)| n == name)
            && let Ok(map) = PaintedMap::from_bytes(bytes)
        {
            maps.push((name.to_string(), describe(&map)));
        }
    }
    maps.sort();
    maps
}

/// Names of the player's own saved maps.
pub fn user_map_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(user_maps_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            (path.extension()? == "map").then_some(())?;
            path.file_stem()?.to_str().map(str::to_string)
        })
        .collect();
    names.sort();
    names
}

/// Names of every painted map (the player's and the built-in ones), sorted.
pub fn all_map_names() -> Vec<String> {
    let mut names = user_map_names();
    for (name, _) in BUILTIN_MAPS {
        if !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
    }
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_settings_and_maps_load() {
        for (name, bytes) in BUILTIN_MAPS {
            PaintedMap::from_bytes(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
        let config = Config::from_text(DEFAULT_CONFIG, "default", None, &|name| {
            Ok(BUILTIN_MAPS
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, b)| MapSource::Painted(PaintedMap::from_bytes(b).unwrap())))
        })
        .unwrap();
        assert_eq!(config.world.map, "Deep Ocean");
        assert!(config.terrain.painted.is_some());
    }
}

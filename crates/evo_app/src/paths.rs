//! Where the game keeps things, so it runs from anywhere (an install folder, a download):
//! the default settings, the built-in maps and the manual are built into the program, and saves, the
//! player's own maps and the organisms they make in the genome editor go in the system's
//! per-user data folder:
//!
//! - Windows: `%APPDATA%\Microcosm`
//! - macOS: `~/Library/Application Support/Microcosm`
//! - Linux: `$XDG_DATA_HOME/microcosm` (usually `~/.local/share/microcosm`)

use std::path::{Path, PathBuf};

use evo_core::config::MapSource;
use evo_core::map::PaintedMap;
use evo_core::{Config, Genome};

/// The default settings (`configs/default.toml`, built in).
const DEFAULT_CONFIG: &str = include_str!("../../../configs/default.toml");

/// Maps that come with the game (`configs/maps`, built in).
const BUILTIN_MAPS: &[(&str, &[u8])] = &[(
    "Deep Ocean",
    include_bytes!("../../../configs/maps/Deep Ocean.map"),
)];

/// The manual (`docs/manual`, built in), opened from the main menu.
const MANUAL: &[u8] = include_bytes!("../../../docs/manual/Microcosm-Manual.pdf");

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

/// The player's settings (master volume and so on).
pub fn settings_path() -> PathBuf {
    data_dir().join("settings.toml")
}

/// Open the manual in the system's PDF viewer. The built-in copy is written to the data folder
/// first (again whenever the game's copy differs, so a new version replaces an old one).
pub fn open_manual() -> Result<(), String> {
    let path = data_dir().join("Microcosm-Manual.pdf");
    if std::fs::read(&path).ok().as_deref() != Some(MANUAL) {
        std::fs::create_dir_all(data_dir()).map_err(|e| e.to_string())?;
        std::fs::write(&path, MANUAL).map_err(|e| format!("couldn't write the manual: {e}"))?;
    }
    let mut command = if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else {
        std::process::Command::new("xdg-open")
    };
    let mut child = command
        .arg(&path)
        .spawn()
        .map_err(|e| format!("couldn't open {}: {e}", path.display()))?;
    // Reap the opener when it exits (it hands the file to the viewer and quits).
    std::thread::spawn(move || child.wait());
    Ok(())
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

/// Where organisms made in the genome editor are saved (one `.toml` file each).
pub fn organisms_dir() -> PathBuf {
    data_dir().join("organisms")
}

/// An organism made in the genome editor: a starting preset for new worlds.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedOrganism {
    pub name: String,
    pub description: String,
    pub genome: Genome,
}

impl SavedOrganism {
    fn to_toml(&self) -> String {
        let mut t = toml::Table::new();
        t.insert("name".into(), self.name.clone().into());
        t.insert("description".into(), self.description.clone().into());
        t.insert("genome".into(), self.genome.to_string().into());
        toml::to_string(&t).unwrap_or_default()
    }

    fn from_toml(text: &str) -> Result<Self, String> {
        let t: toml::Table = text.parse().map_err(|e: toml::de::Error| e.to_string())?;
        let field = |k: &str| t.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        Ok(Self {
            name: field("name"),
            description: field("description"),
            genome: Genome::parse(&field("genome"))?,
        })
    }
}

/// The file an organism named `name` is saved in.
fn organism_path(name: &str) -> PathBuf {
    organisms_dir().join(format!("{name}.toml"))
}

/// The player's saved organisms, sorted by name (files that can't be read are skipped).
pub fn list_organisms() -> Vec<SavedOrganism> {
    let mut out: Vec<SavedOrganism> = std::fs::read_dir(organisms_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            (path.extension()? == "toml").then_some(())?;
            let mut o = SavedOrganism::from_toml(&std::fs::read_to_string(&path).ok()?).ok()?;
            if o.name.is_empty() {
                o.name = path.file_stem()?.to_str()?.to_string();
            }
            Some(o)
        })
        .collect();
    out.sort_by_key(|o| o.name.to_lowercase());
    out
}

/// Save an organism under its name (replacing one of the same name).
pub fn save_organism(o: &SavedOrganism) -> Result<PathBuf, String> {
    std::fs::create_dir_all(organisms_dir()).map_err(|e| e.to_string())?;
    let path = organism_path(&o.name);
    std::fs::write(&path, o.to_toml()).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

pub fn delete_organism(name: &str) -> Result<(), String> {
    std::fs::remove_file(organism_path(name)).map_err(|e| e.to_string())
}

pub fn organism_exists(name: &str) -> bool {
    organism_path(name).exists()
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
    fn saved_organisms_read_back() {
        let o = SavedOrganism {
            name: "Glow worm".into(),
            description: "Lures prey \"in\" the dark.\nSecond line.".into(),
            genome: Genome::parse(&Config::default().population.ancestor).unwrap(),
        };
        assert_eq!(SavedOrganism::from_toml(&o.to_toml()).unwrap(), o);
    }

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

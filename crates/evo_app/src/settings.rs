//! The player's settings that last between sessions (for now the master volume), kept in
//! `settings.toml` in the data folder (see `paths.rs`).

use bevy::prelude::*;

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Settings::load());
    }
}

/// Everything plays this much louder than its own level: +4 dB.
pub const BOOST: f32 = 1.585;

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Settings {
    /// Master volume, 0..1: every sound in the game is scaled by it (see [`Settings::level`]).
    pub volume: f32,
    /// Muted: silent, keeping the volume to come back to.
    pub muted: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            volume: 1.0,
            muted: false,
        }
    }
}

impl Settings {
    /// What to scale every sound by: the volume (with the overall boost), or nothing while
    /// muted.
    pub fn level(&self) -> f32 {
        if self.muted { 0.0 } else { self.volume * BOOST }
    }

    /// The saved settings, or the defaults (anything missing or unreadable keeps its default).
    pub fn load() -> Settings {
        std::fs::read_to_string(crate::paths::settings_path())
            .ok()
            .and_then(|text| Settings::from_toml(&text))
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = crate::paths::settings_path();
        let result = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|_| std::fs::write(&path, self.to_toml()));
        if let Err(e) = result {
            warn!("couldn't save the settings to {}: {e}", path.display());
        }
    }

    fn to_toml(&self) -> String {
        format!("volume = {}\nmuted = {}\n", self.volume, self.muted)
    }

    fn from_toml(text: &str) -> Option<Settings> {
        let table: toml::Table = text.parse().ok()?;
        let mut settings = Settings::default();
        if let Some(v) = table
            .get("volume")
            .and_then(|v| v.as_float().or(v.as_integer().map(|i| i as f64)))
        {
            settings.volume = (v as f32).clamp(0.0, 1.0);
        }
        if let Some(m) = table.get("muted").and_then(|v| v.as_bool()) {
            settings.muted = m;
        }
        Some(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_read_back() {
        let s = Settings {
            volume: 0.35,
            muted: true,
        };
        assert_eq!(Settings::from_toml(&s.to_toml()), Some(s.clone()));
        assert_eq!(s.level(), 0.0, "muted is silent");
        assert_eq!(Settings { muted: false, ..s }.level(), 0.35 * BOOST);
        assert!((20.0 * BOOST.log10() - 4.0).abs() < 0.01, "+4 dB");
        assert_eq!(
            Settings::from_toml("volume = 3").map(|s| s.volume),
            Some(1.0),
            "clamped"
        );
        assert_eq!(
            Settings::from_toml("").map(|s| s.volume),
            Some(1.0),
            "default"
        );
    }
}

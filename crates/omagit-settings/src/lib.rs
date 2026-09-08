//! Persisted preferences, in TOML.
//!
//! M0 stores what the first window needs: which theme to apply and at what
//! density. The keymap and the persisted layout arrive with M9.

use std::path::{Path, PathBuf};

use omagit_theme::{DensityMode, Mode};

/// Which of the four theme sources to use (SPEC §6.1), in priority order.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "source", rename_all = "kebab-case")]
pub enum ThemeChoice {
    /// An explicit user override. Disables all tracking and persists.
    Named { name: String },
    /// Follow the platform: Omarchy Quattro on Linux, the system appearance on
    /// macOS. Wired up at M1; at M0 it resolves to the embedded default.
    #[default]
    FollowSystem,
    /// An embedded theme pinned by mode.
    Embedded { mode: ModeSetting },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModeSetting {
    Dark,
    Light,
}

impl From<ModeSetting> for Mode {
    fn from(value: ModeSetting) -> Self {
        match value {
            ModeSetting::Dark => Mode::Dark,
            ModeSetting::Light => Mode::Light,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub theme: ThemeChoice,
    pub density: DensityMode,
}

impl Settings {
    pub const FILE_NAME: &'static str = "settings.toml";

    /// Read the settings file, or return defaults.
    ///
    /// A malformed file is reported and then ignored rather than fatal: losing a
    /// preference must never keep the app from starting.
    pub fn load(dir: &Path) -> Self {
        let file = dir.join(Self::FILE_NAME);
        let text = match std::fs::read_to_string(&file) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(error) => {
                tracing::warn!(path = %file.display(), %error, "settings unreadable, using defaults");
                return Self::default();
            }
        };
        match toml::from_str(&text) {
            Ok(settings) => settings,
            Err(error) => {
                tracing::warn!(path = %file.display(), %error, "settings malformed, using defaults");
                Self::default()
            }
        }
    }

    pub fn save(&self, dir: &Path) -> Result<PathBuf, SettingsError> {
        std::fs::create_dir_all(dir).map_err(|source| SettingsError::Io {
            path: dir.to_owned(),
            source,
        })?;
        let file = dir.join(Self::FILE_NAME);
        let text = toml::to_string_pretty(self)?;
        std::fs::write(&file, text).map_err(|source| SettingsError::Io {
            path: file.clone(),
            source,
        })?;
        Ok(file)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not serialise settings: {0}")]
    Serialise(#[from] toml::ser::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_the_file_is_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let settings = Settings::load(dir.path());
        assert_eq!(settings, Settings::default());
        assert_eq!(settings.density, DensityMode::Compact);
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let settings = Settings {
            theme: ThemeChoice::Named {
                name: "Gruvbox".into(),
            },
            density: DensityMode::Comfortable,
        };
        settings.save(dir.path()).expect("save");
        assert_eq!(Settings::load(dir.path()), settings);
    }

    #[test]
    fn a_malformed_file_falls_back_instead_of_failing() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(Settings::FILE_NAME), "this is not toml {{{")
            .expect("write");
        assert_eq!(Settings::load(dir.path()), Settings::default());
    }
}

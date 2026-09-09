//! What omagit remembers between sessions, in TOML.
//!
//! Two files, deliberately:
//!
//! * [`Settings`] — which theme source to use and at what density. The
//!   *source*, not the resolved palette: a stored palette would go stale the
//!   moment the system theme changed.
//! * [`Library`] — the repositories the user has added, and the groups they
//!   arranged them in.
//!
//! They are written at completely different rates, and a corrupted write to one
//! must not take the other with it. The keymap and the persisted layout arrive
//! with M9.

use std::path::{Path, PathBuf};

use omagit_theme::{DensityMode, ThemeSource};

pub mod library;

pub use library::{Entry, Group, Library, Location};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub theme: ThemeSource,
    pub density: DensityMode,
    /// How large the interface is drawn, as a multiple of the design's own
    /// sizes.
    ///
    /// DESIGN-TOKENS §8 fixes the type *scale* — 13 base, 12.5 mono, 11
    /// metadata — and that is a contract about proportion, which this does not
    /// touch: every size moves together. What it does touch is the absolute
    /// size, and that is a property of the screen the app is on, not of the
    /// design. A 13px base is comfortable on the 1600×1000 reference frame the
    /// boards are drawn in and small on a 3440-wide display at arm's length.
    #[serde(default = "default_scale")]
    pub ui_scale: f32,
    /// How wide the resizable columns have been dragged, by name.
    ///
    /// A map rather than named fields: the panes are an interface concern and
    /// this crate has no business knowing that one of them is called
    /// `history`. A pane that disappears leaves a key nobody reads, which is
    /// cheaper than a migration.
    #[serde(default)]
    pub panes: std::collections::BTreeMap<String, f32>,
}

/// Slightly larger than the boards, because the boards are a reference frame
/// and not a screen. Set `ui_scale = 1.0` in `settings.toml` for exactly what
/// the boards draw, or higher still.
fn default_scale() -> f32 {
    1.15
}

/// The range a stored value is held to. Below the floor the interface stops
/// being legible, which is the problem this exists to solve; above the ceiling
/// the fixed panel widths stop fitting their contents.
pub const UI_SCALE_RANGE: std::ops::RangeInclusive<f32> = 0.8..=2.0;

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeSource::default(),
            density: DensityMode::default(),
            ui_scale: default_scale(),
            panes: std::collections::BTreeMap::new(),
        }
    }
}

impl Settings {
    pub const FILE_NAME: &'static str = "settings.toml";

    /// The stored scale, held to a range a window can actually be used at.
    ///
    /// Clamped rather than refused: a settings file is edited by hand, and a
    /// typo that made the interface unusable would leave no way to fix it from
    /// inside the app.
    pub fn scale(&self) -> f32 {
        if self.ui_scale.is_finite() {
            self.ui_scale
                .clamp(*UI_SCALE_RANGE.start(), *UI_SCALE_RANGE.end())
        } else {
            default_scale()
        }
    }

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
        // Comfortable, which is what the boards are drawn at; compact is the
        // opt-in for a dense desktop.
        assert_eq!(settings.density, DensityMode::Comfortable);
        // Nothing chosen yet, so the source is resolved from what the machine
        // offers rather than pinned to a theme.
        assert_eq!(settings.theme, ThemeSource::Automatic);
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let settings = Settings {
            theme: ThemeSource::UserOverride {
                name: "Gruvbox".into(),
            },
            density: DensityMode::Comfortable,
            ui_scale: 1.25,
            panes: [("history".to_owned(), 480.0)].into_iter().collect(),
        };
        settings.save(dir.path()).expect("save");
        assert_eq!(Settings::load(dir.path()), settings);
    }

    #[test]
    fn a_scale_out_of_range_is_clamped_rather_than_refused() {
        // A settings file is edited by hand. A typo that made the interface
        // unusable would leave no way to fix it from inside the app, so the
        // value is held to a range instead of rejected.
        let huge = Settings {
            ui_scale: 40.0,
            ..Settings::default()
        };
        assert_eq!(huge.scale(), *UI_SCALE_RANGE.end());

        let tiny = Settings {
            ui_scale: 0.01,
            ..Settings::default()
        };
        assert_eq!(tiny.scale(), *UI_SCALE_RANGE.start());

        let nonsense = Settings {
            ui_scale: f32::NAN,
            ..Settings::default()
        };
        assert_eq!(nonsense.scale(), Settings::default().ui_scale);
    }

    #[test]
    fn a_malformed_file_falls_back_instead_of_failing() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(Settings::FILE_NAME), "this is not toml {{{")
            .expect("write");
        assert_eq!(Settings::load(dir.path()), Settings::default());
    }
}

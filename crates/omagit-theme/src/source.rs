//! The four theme sources of SPEC §6.1, resolved in priority order.
//!
//! 1. **User override** — a theme picked explicitly in preferences. Disables all
//!    tracking and persists between sessions.
//! 2. **Omarchy Quattro** (Linux) — the live system palette, watched. Selected by
//!    default on first launch *if* a valid Quattro state is found.
//! 3. **macOS system appearance** — follows light/dark, automatic switching
//!    included, mapping each to an embedded theme.
//! 4. **Embedded default** — Tokyo Night dark, Rosé Pine Dawn light.
//!
//! A source that cannot work on this machine is **not offered** rather than
//! offered and broken: no Omarchy entry in preferences on macOS, and none on a
//! Linux box whose Quattro state is absent or invalid (SPEC §6.1).

use std::path::PathBuf;

use crate::embedded;
use crate::omarchy;
use crate::theme::Theme;
use crate::tokens::Mode;

/// Where a theme comes from. This is what gets persisted, not the palette
/// itself: a stored palette would go stale the moment the system theme changed.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "source", rename_all = "kebab-case")]
pub enum ThemeSource {
    /// Priority 1: an explicit choice, which disables all tracking.
    UserOverride { name: String },
    /// Priority 2: the live Omarchy Quattro palette, watched.
    Omarchy,
    /// Priority 3: the platform's light/dark preference.
    SystemAppearance,
    /// Priority 4: an embedded theme pinned by mode.
    Embedded { mode: Mode },
    /// No choice recorded yet — resolve to whatever this machine offers.
    #[default]
    Automatic,
}

/// What this machine can actually offer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sources {
    /// The Quattro `current` directory, when the platform has one.
    pub omarchy_state: Option<PathBuf>,
    /// The system's light/dark preference, when the platform reports one.
    pub system_appearance: Option<Mode>,
}

/// A theme, and the source it actually came from — which is not always the
/// source that was asked for.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub theme: Theme,
    /// The **effective** source, never [`ThemeSource::Automatic`]: callers use
    /// it to decide what to follow, and "automatic" is not something you can
    /// follow.
    pub source: ThemeSource,
    /// Set when the requested source could not be honoured. The UI surfaces it
    /// instead of silently showing a different theme than the one selected.
    pub fell_back_from: Option<ThemeSource>,
}

impl Sources {
    /// Whether a readable, complete Omarchy palette exists right now.
    pub fn omarchy_available(&self) -> bool {
        self.omarchy_state
            .as_deref()
            .is_some_and(|dir| omarchy::read(dir).is_ok())
    }

    /// The sources worth showing in preferences, in priority order.
    pub fn available(&self) -> Vec<ThemeSource> {
        let mut sources = Vec::new();
        if self.omarchy_available() {
            sources.push(ThemeSource::Omarchy);
        }
        if self.system_appearance.is_some() {
            sources.push(ThemeSource::SystemAppearance);
        }
        sources.push(ThemeSource::Embedded { mode: Mode::Dark });
        sources.push(ThemeSource::Embedded { mode: Mode::Light });
        sources
    }

    /// What to use when nothing is persisted yet.
    ///
    /// Omarchy wins when a valid Quattro state is found, which is what SPEC §6.1
    /// means by "enabled by default on first launch".
    pub fn default_source(&self) -> ThemeSource {
        if self.omarchy_available() {
            ThemeSource::Omarchy
        } else if self.system_appearance.is_some() {
            ThemeSource::SystemAppearance
        } else {
            ThemeSource::Embedded { mode: Mode::Dark }
        }
    }

    /// Resolve a source into a theme, falling back down the chain when it
    /// cannot be honoured.
    pub fn resolve(&self, requested: &ThemeSource) -> Resolved {
        // `Automatic` is a placeholder for "whatever this machine offers", so it
        // is substituted before resolving rather than reported back. Leaving it
        // in `Resolved::source` would tell the caller there is nothing to
        // follow, on exactly the machines where there is.
        let requested = match requested {
            ThemeSource::Automatic => self.default_source(),
            explicit => explicit.clone(),
        };

        match self.try_resolve(&requested) {
            Some(theme) => Resolved {
                theme,
                source: requested,
                fell_back_from: None,
            },
            None => {
                let fallback = self.default_source();
                let theme = self
                    .try_resolve(&fallback)
                    .unwrap_or_else(|| embedded::default_for(self.appearance()));
                tracing::info!(
                    ?requested,
                    ?fallback,
                    "theme source unavailable, falling back"
                );
                Resolved {
                    theme,
                    source: fallback,
                    fell_back_from: Some(requested.clone()),
                }
            }
        }
    }

    fn try_resolve(&self, source: &ThemeSource) -> Option<Theme> {
        match source {
            // Substituted in `resolve` before reaching here; a direct call is a
            // caller bug, not a state to render.
            ThemeSource::Automatic => None,
            ThemeSource::UserOverride { name } => embedded::by_name(name),
            ThemeSource::Omarchy => self
                .omarchy_state
                .as_deref()
                .and_then(|dir| omarchy::read(dir).ok()),
            ThemeSource::SystemAppearance => self.system_appearance.map(embedded::default_for),
            ThemeSource::Embedded { mode } => Some(embedded::default_for(*mode)),
        }
    }

    fn appearance(&self) -> Mode {
        self.system_appearance.unwrap_or(Mode::Dark)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r##"
        background = "#101418"
        foreground = "#e0e6ec"
        accent     = "#6fb2ff"
        selection  = "#22303c"
        color8     = "#40505c"
        mode       = "dark"
    "##;

    fn quattro_state(contents: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let current = dir.path().join("current");
        std::fs::create_dir_all(current.join("theme")).expect("create");
        std::fs::write(current.join("theme.name"), "Probe").expect("name");
        std::fs::write(current.join("theme/colors.toml"), contents).expect("colors");
        (dir, current)
    }

    #[test]
    fn a_machine_without_omarchy_is_not_offered_it() {
        // macOS: no state directory at all.
        let macos = Sources {
            omarchy_state: None,
            system_appearance: Some(Mode::Light),
        };
        assert!(!macos.available().contains(&ThemeSource::Omarchy));
        assert_eq!(macos.default_source(), ThemeSource::SystemAppearance);
    }

    #[test]
    fn an_invalid_quattro_state_is_not_offered_either() {
        // SPEC §6.1: no partial support, no guessing.
        let (_guard, current) = quattro_state("this is not toml {{{");
        let sources = Sources {
            omarchy_state: Some(current),
            system_appearance: None,
        };
        assert!(!sources.available().contains(&ThemeSource::Omarchy));
        assert_eq!(
            sources.default_source(),
            ThemeSource::Embedded { mode: Mode::Dark }
        );
    }

    #[test]
    fn a_valid_quattro_state_is_the_first_launch_default() {
        let (_guard, current) = quattro_state(MINIMAL);
        let sources = Sources {
            omarchy_state: Some(current),
            system_appearance: None,
        };
        assert_eq!(sources.default_source(), ThemeSource::Omarchy);
        let resolved = sources.resolve(&ThemeSource::Omarchy);
        assert_eq!(resolved.theme.name, "Probe");
        assert!(resolved.fell_back_from.is_none());
    }

    #[test]
    fn a_user_override_wins_over_a_live_omarchy_palette() {
        let (_guard, current) = quattro_state(MINIMAL);
        let sources = Sources {
            omarchy_state: Some(current),
            system_appearance: Some(Mode::Dark),
        };
        let resolved = sources.resolve(&ThemeSource::UserOverride {
            name: "Gruvbox".into(),
        });
        assert_eq!(resolved.theme.name, "Gruvbox");
        assert!(resolved.fell_back_from.is_none());
    }

    #[test]
    fn an_unknown_override_falls_back_and_says_so() {
        let sources = Sources {
            omarchy_state: None,
            system_appearance: Some(Mode::Light),
        };
        let requested = ThemeSource::UserOverride {
            name: "Solarized".into(),
        };
        let resolved = sources.resolve(&requested);
        assert_eq!(resolved.theme.name, "Rosé Pine Dawn", "light appearance");
        assert_eq!(
            resolved.fell_back_from,
            Some(requested),
            "the UI must be able to say the selection was not honoured"
        );
    }

    #[test]
    fn the_system_appearance_picks_the_matching_default() {
        for (appearance, expected) in [(Mode::Dark, "Tokyo Night"), (Mode::Light, "Rosé Pine Dawn")]
        {
            let sources = Sources {
                omarchy_state: None,
                system_appearance: Some(appearance),
            };
            let resolved = sources.resolve(&ThemeSource::SystemAppearance);
            assert_eq!(resolved.theme.name, expected);
        }
    }

    #[test]
    fn automatic_reports_the_source_it_landed_on() {
        // The bug this pins: reporting `Automatic` back tells the caller there
        // is nothing to follow, on the machines where there most certainly is.
        let macos = Sources {
            omarchy_state: None,
            system_appearance: Some(Mode::Light),
        };
        let resolved = macos.resolve(&ThemeSource::Automatic);
        assert_eq!(resolved.source, ThemeSource::SystemAppearance);
        assert_eq!(resolved.theme.name, "Rosé Pine Dawn");
        assert!(
            resolved.fell_back_from.is_none(),
            "automatic is not a fallback"
        );

        let (_guard, current) = quattro_state(MINIMAL);
        let linux = Sources {
            omarchy_state: Some(current),
            system_appearance: None,
        };
        assert_eq!(
            linux.resolve(&ThemeSource::Automatic).source,
            ThemeSource::Omarchy
        );
    }

    #[test]
    fn a_machine_offering_nothing_still_resolves() {
        let bare = Sources::default();
        let resolved = bare.resolve(&ThemeSource::Omarchy);
        assert_eq!(resolved.theme.name, "Tokyo Night");
        assert_eq!(resolved.source, ThemeSource::Embedded { mode: Mode::Dark });
    }
}

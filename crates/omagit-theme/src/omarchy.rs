//! Reading and tracking an Omarchy Quattro palette (SPEC §6.1, source 2).
//!
//! Linux only in practice — on macOS the state directory never exists, and the
//! option is simply not offered rather than offered and broken.
//!
//! Two deliberate narrowings against what other readers of this format do:
//!
//! * **Legacy layouts are not supported.** `~/.config/omarchy/current` is not
//!   consulted. SPEC §6.1 is explicit: no partial support, no guessing. If the
//!   Quattro state is absent or invalid, the source is not offered at all.
//! * **All six guaranteed keys are required.** DESIGN-TOKENS §2.1 lists
//!   `background`, `foreground`, `accent`, `selection`, `color8` and `mode`; if
//!   any is missing or malformed the *whole* palette is rejected. A partial
//!   blend of two sources produces unreadable combinations, so there is none.

use std::path::{Path, PathBuf};

use crate::color::Rgb;
use crate::theme::{Inputs, NamedStatus, Theme, Tuning};
use crate::tokens::Mode;

/// `theme.name`, relative to the current-theme directory.
const NAME_FILE: &str = "theme.name";
/// `theme/colors.toml`, relative to the current-theme directory.
const COLORS_FILE: &str = "theme/colors.toml";

/// The Quattro state directory, given a home directory.
pub fn state_dir(home: &Path) -> PathBuf {
    home.join(".local/state/omarchy/current")
}

/// The Quattro state directory for this process, if `HOME` is set.
pub fn state_dir_from_env() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(|home| state_dir(Path::new(&home)))
}

#[derive(Debug, thiserror::Error)]
pub enum OmarchyError {
    #[error("no Omarchy state at {0}")]
    Absent(PathBuf),

    #[error("{path} is unreadable: {source}")]
    Unreadable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The file was read but the palette is unusable, so the whole thing is
    /// rejected (DESIGN-TOKENS §2.1).
    #[error("{path}: {source}")]
    Invalid {
        path: PathBuf,
        #[source]
        source: ParseError,
    },
}

/// Why a `colors.toml` body could not become a palette.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("not valid TOML: {0}")]
    Toml(#[from] toml::de::Error),

    /// One of the six guaranteed keys is missing or malformed.
    #[error("{0}")]
    Incomplete(String),
}

/// Read the palette the Quattro state currently points at.
pub fn read(current: &Path) -> Result<Theme, OmarchyError> {
    let colors = current.join(COLORS_FILE);
    if !colors.exists() {
        return Err(OmarchyError::Absent(current.to_owned()));
    }
    let contents = std::fs::read_to_string(&colors).map_err(|source| OmarchyError::Unreadable {
        path: colors.clone(),
        source,
    })?;

    // A missing or blank theme.name is not a reason to reject the palette: it
    // is not one of the guaranteed keys, only a label.
    let name = std::fs::read_to_string(current.join(NAME_FILE))
        .ok()
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Omarchy".to_owned());

    parse(&name, &contents).map_err(|source| OmarchyError::Invalid {
        path: colors,
        source,
    })
}

/// Parse a `colors.toml` body. Public so the fallback tests can drive every
/// shape of broken file without touching the filesystem.
pub fn parse(name: &str, contents: &str) -> Result<Theme, ParseError> {
    let table: toml::Table = toml::from_str(contents).map_err(ParseError::Toml)?;

    let color = |key: &str| -> Result<Rgb, ParseError> {
        let value = table
            .get(key)
            .ok_or_else(|| ParseError::Incomplete(format!("missing `{key}`")))?;
        let text = value
            .as_str()
            .ok_or_else(|| ParseError::Incomplete(format!("`{key}` must be a string")))?;
        Rgb::parse(text)
            .map_err(|_| ParseError::Incomplete(format!("`{key}` is not a #RRGGBB colour")))
    };

    // Optional (DESIGN-TOKENS §2.2): unparseable values are dropped rather than
    // fatal, because an absent status colour has a defined fallback and a
    // missing guaranteed one does not.
    let optional = |key: &str| -> Option<Rgb> {
        table
            .get(key)
            .and_then(toml::Value::as_str)
            .and_then(|text| Rgb::parse(text).ok())
    };

    let mode = match table.get("mode").and_then(toml::Value::as_str) {
        Some("dark") => Mode::Dark,
        Some("light") => Mode::Light,
        Some(other) => {
            return Err(ParseError::Incomplete(format!(
                "`mode` must be \"dark\" or \"light\", not {other:?}"
            )));
        }
        None => return Err(ParseError::Incomplete("missing `mode`".into())),
    };

    Ok(Theme {
        name: name.to_owned(),
        inputs: Inputs {
            background: color("background")?,
            foreground: color("foreground")?,
            accent: color("accent")?,
            selection: color("selection")?,
            bright_black: color("color8")?,
            mode,
        },
        status: NamedStatus {
            red: optional("red"),
            green: optional("green"),
            yellow: optional("yellow"),
            blue: optional("blue"),
        },
        // An Omarchy palette carries no tuning of its own, so its mode's
        // defaults apply and §4.3's correction does the rest (DESIGN-TOKENS §2.3).
        tuning: Tuning::defaults_for(mode),
    })
}

/// Turns "the state directory may have changed" into "the theme actually
/// changed", which is the only thing worth a repaint.
///
/// This is the whole of DESIGN-TOKENS §10 test 4: replacing the `current`
/// symlink emits exactly one update, and rewriting a file with identical
/// contents emits none. Keeping the comparison here — on the parsed theme, not
/// on mtimes or inode numbers — is what makes that true regardless of how the
/// change was delivered.
#[derive(Debug, Default)]
pub struct Tracker {
    last: Option<Theme>,
}

impl Tracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed the tracker with what is already applied, so the first refresh does
    /// not re-emit it.
    pub fn seeded_with(theme: Theme) -> Self {
        Self { last: Some(theme) }
    }

    /// Re-read `current` and return the theme only if it differs from the last
    /// one seen.
    ///
    /// A read failure is *not* a change: a half-written `colors.toml` caught
    /// mid-save would otherwise flip the app to the fallback theme and back.
    /// The last good palette stays until a readable, different one replaces it.
    pub fn refresh(&mut self, current: &Path) -> Option<Theme> {
        match read(current) {
            Ok(theme) => {
                if self.last.as_ref() == Some(&theme) {
                    return None;
                }
                self.last = Some(theme.clone());
                Some(theme)
            }
            Err(error) => {
                tracing::debug!(%error, "Omarchy state unreadable, keeping the current palette");
                None
            }
        }
    }

    pub fn current(&self) -> Option<&Theme> {
        self.last.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMPLETE: &str = r##"
        background = "#1a1b26"
        foreground = "#c0caf5"
        accent     = "#7aa2f7"
        selection  = "#283457"
        color8     = "#414868"
        mode       = "dark"
        red        = "#f7768e"
        green      = "#9ece6a"
        yellow     = "#e0af68"
        blue       = "#7aa2f7"
    "##;

    /// The six guaranteed keys and nothing else.
    const MINIMAL: &str = r##"
        background = "#1a1b26"
        foreground = "#c0caf5"
        accent     = "#7aa2f7"
        selection  = "#283457"
        color8     = "#414868"
        mode       = "dark"
    "##;

    fn write_state(dir: &Path, name: &str, colors: &str) {
        std::fs::create_dir_all(dir.join("theme")).expect("create theme dir");
        std::fs::write(dir.join(NAME_FILE), name).expect("write name");
        std::fs::write(dir.join(COLORS_FILE), colors).expect("write colors");
    }

    #[test]
    fn a_complete_palette_carries_its_named_status_colours() {
        let theme = parse("Tokyo Night", COMPLETE).expect("complete palette parses");
        assert_eq!(theme.inputs.background, Rgb::hex(0x1a1b26));
        assert_eq!(theme.inputs.mode, Mode::Dark);
        assert_eq!(theme.status.red, Some(Rgb::hex(0xf7768e)));
        // No tuning of its own, so the mode defaults apply.
        assert_eq!(theme.tuning, Tuning::defaults_for(Mode::Dark));
    }

    #[test]
    fn a_minimal_palette_derives_the_status_colours() {
        let theme = parse("Minimal", MINIMAL).expect("the guaranteed keys are enough");
        assert_eq!(theme.status, NamedStatus::default());
        // And it still produces a full, usable token set.
        let tokens = theme.tokens();
        assert_ne!(tokens.danger, tokens.success);
    }

    #[test]
    fn every_broken_shape_is_rejected_whole() {
        // DESIGN-TOKENS §10 test 2. Each of these must fail to parse, so the
        // caller falls back to an embedded theme rather than blending sources.
        let cases = [
            ("empty", ""),
            ("corrupt", "this is not toml {{{"),
            (
                "missing a guaranteed key",
                r##"background = "#1a1b26"
                   foreground = "#c0caf5"
                   accent     = "#7aa2f7"
                   mode       = "dark""##,
            ),
            (
                "malformed colour",
                r##"background = "not-a-colour"
                   foreground = "#c0caf5"
                   accent     = "#7aa2f7"
                   selection  = "#283457"
                   color8     = "#414868"
                   mode       = "dark""##,
            ),
            (
                "colour of the wrong type",
                r##"background = 1
                   foreground = "#c0caf5"
                   accent     = "#7aa2f7"
                   selection  = "#283457"
                   color8     = "#414868"
                   mode       = "dark""##,
            ),
            (
                "unknown mode",
                r##"background = "#1a1b26"
                   foreground = "#c0caf5"
                   accent     = "#7aa2f7"
                   selection  = "#283457"
                   color8     = "#414868"
                   mode       = "sepia""##,
            ),
        ];
        for (label, contents) in cases {
            assert!(
                parse("Broken", contents).is_err(),
                "{label}: should have been rejected whole"
            );
        }
    }

    #[test]
    fn an_out_of_gamut_status_colour_does_not_sink_the_palette() {
        // An optional key that cannot be read is dropped, not fatal: the
        // fallback for a status colour is defined, unlike a guaranteed one.
        let contents = format!("{MINIMAL}\n green = \"#gggggg\"\n red = \"#f7768e\"");
        let theme = parse("Partly optional", &contents).expect("guaranteed keys are intact");
        assert_eq!(theme.status.green, None, "the unreadable one is dropped");
        assert_eq!(
            theme.status.red,
            Some(Rgb::hex(0xf7768e)),
            "the good one stays"
        );
    }

    #[test]
    fn a_missing_theme_name_is_a_label_problem_not_a_palette_problem() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("theme")).expect("create");
        std::fs::write(dir.path().join(COLORS_FILE), MINIMAL).expect("write");
        let theme = read(dir.path()).expect("palette is complete");
        assert_eq!(theme.name, "Omarchy");
    }

    #[test]
    fn an_absent_state_directory_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(read(&dir.path().join("nothing-here")).is_err());
    }

    #[test]
    fn the_tracker_emits_once_per_real_change() {
        // DESIGN-TOKENS §10 test 4, end to end over a real filesystem.
        let root = tempfile::tempdir().expect("tempdir");
        let one = root.path().join("theme-one");
        let two = root.path().join("theme-two");
        write_state(&one, "One", COMPLETE);
        write_state(&two, "Two", MINIMAL);

        let current = root.path().join("current");
        std::os::unix::fs::symlink(&one, &current).expect("symlink");

        let mut tracker = Tracker::new();

        // First read: a change, since nothing was applied yet.
        assert_eq!(
            tracker.refresh(&current).map(|t| t.name),
            Some("One".to_owned())
        );

        // Reading again without touching anything: no change.
        assert!(tracker.refresh(&current).is_none());

        // Rewriting the file with identical contents: still no change. This is
        // the half that a mtime- or event-based tracker gets wrong.
        write_state(&one, "One", COMPLETE);
        assert!(
            tracker.refresh(&current).is_none(),
            "an identical rewrite must not emit"
        );

        // Replacing the symlink: exactly one change.
        std::fs::remove_file(&current).expect("unlink");
        std::os::unix::fs::symlink(&two, &current).expect("relink");
        assert_eq!(
            tracker.refresh(&current).map(|t| t.name),
            Some("Two".to_owned())
        );
        assert!(tracker.refresh(&current).is_none(), "and only one");
    }

    #[test]
    fn a_broken_write_does_not_flip_the_palette() {
        let root = tempfile::tempdir().expect("tempdir");
        let current = root.path().join("current");
        write_state(&current, "One", COMPLETE);

        let mut tracker = Tracker::new();
        assert!(tracker.refresh(&current).is_some());

        // A save caught halfway through: unreadable, but not a reason to fall
        // back and then bounce straight back again.
        std::fs::write(current.join(COLORS_FILE), "backgro").expect("write");
        assert!(tracker.refresh(&current).is_none());
        assert_eq!(tracker.current().map(|t| t.name.as_str()), Some("One"));
    }
}

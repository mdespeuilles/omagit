//! The embedded theme catalogue — six dark, two light.
//!
//! On macOS there is no Omarchy: these are the default experience there, not a
//! degraded fallback (SPEC §6.1). Their quality and the presence of a
//! first-class light theme count as much as the Omarchy mapping.
//!
//! Every theme here passes the six tests of DESIGN-TOKENS §10; `catalogue` is
//! what those tests iterate over, so adding one means it has to hold up.

use crate::color::Rgb;
use crate::theme::{Inputs, NamedStatus, Theme, Tuning};
use crate::tokens::Mode;

/// The five guaranteed colours of a theme, as packed hex literals.
///
/// Named fields rather than positional arguments on purpose: `selection` and
/// `bright_black` are both mid-tone greys, and swapping them by accident would
/// produce a theme that looks almost right.
struct Palette {
    name: &'static str,
    background: u32,
    foreground: u32,
    accent: u32,
    selection: u32,
    bright_black: u32,
}

impl Palette {
    fn into_theme(self, mode: Mode, status: NamedStatus, tuning: Tuning) -> Theme {
        Theme {
            name: self.name.to_owned(),
            inputs: Inputs {
                background: Rgb::hex(self.background),
                foreground: Rgb::hex(self.foreground),
                accent: Rgb::hex(self.accent),
                selection: Rgb::hex(self.selection),
                bright_black: Rgb::hex(self.bright_black),
                mode,
            },
            status,
            tuning,
        }
    }
}

fn dark(palette: Palette, status: NamedStatus, tuning: Tuning) -> Theme {
    palette.into_theme(Mode::Dark, status, tuning)
}

fn light(palette: Palette, status: NamedStatus) -> Theme {
    palette.into_theme(Mode::Light, status, Tuning::defaults_for(Mode::Light))
}

// ── Dark ────────────────────────────────────────────────────────────────────

/// The default dark theme (SPEC §6.1), and the one the mock-ups are drawn in.
pub fn tokyo_night() -> Theme {
    dark(
        Palette {
            name: "Tokyo Night",
            background: 0x1a1b26,
            foreground: 0xc0caf5,
            accent: 0x7aa2f7,
            selection: 0x283457,
            bright_black: 0x414868,
        },
        NamedStatus::all(
            Rgb::hex(0xf7768e),
            Rgb::hex(0x9ece6a),
            Rgb::hex(0xe0af68),
            Rgb::hex(0x7aa2f7),
        ),
        Tuning::defaults_for(Mode::Dark).with_lanes(0.74, 0.13),
    )
}

/// Warm and dark, with yellow and orange as close neighbours — the palette that
/// stresses hue separation hardest (mock-up 04).
pub fn gruvbox() -> Theme {
    dark(
        Palette {
            name: "Gruvbox",
            background: 0x282828,
            foreground: 0xebdbb2,
            accent: 0xd79921,
            selection: 0x3c3836,
            bright_black: 0x928374,
        },
        NamedStatus::all(
            Rgb::hex(0xfb4934),
            Rgb::hex(0xb8bb26),
            Rgb::hex(0xfabd2f),
            Rgb::hex(0x83a598),
        ),
        Tuning::defaults_for(Mode::Dark).with_lanes(0.75, 0.12),
    )
}

/// Near-monochrome, and it names **no** status colour: the theme that exercises
/// the whole OKLCH fallback path and the lane generator's reason for existing.
pub fn matte_black() -> Theme {
    dark(
        Palette {
            name: "Matte Black",
            background: 0x121212,
            foreground: 0xeaeaea,
            accent: 0x9e9e9e,
            selection: 0x262626,
            bright_black: 0x3a3a3a,
        },
        NamedStatus::default(),
        Tuning::defaults_for(Mode::Dark).with_lanes(0.76, 0.13),
    )
}

pub fn catppuccin_mocha() -> Theme {
    dark(
        Palette {
            name: "Catppuccin Mocha",
            background: 0x1e1e2e,
            foreground: 0xcdd6f4,
            accent: 0x89b4fa,
            selection: 0x45475a,
            bright_black: 0x6c7086,
        },
        NamedStatus::all(
            Rgb::hex(0xf38ba8),
            Rgb::hex(0xa6e3a1),
            Rgb::hex(0xf9e2af),
            Rgb::hex(0x89b4fa),
        ),
        Tuning::defaults_for(Mode::Dark),
    )
}

pub fn nord() -> Theme {
    dark(
        Palette {
            name: "Nord",
            background: 0x2e3440,
            foreground: 0xd8dee9,
            accent: 0x88c0d0,
            selection: 0x434c5e,
            bright_black: 0x4c566a,
        },
        NamedStatus::all(
            Rgb::hex(0xbf616a),
            Rgb::hex(0xa3be8c),
            Rgb::hex(0xebcb8b),
            Rgb::hex(0x81a1c1),
        ),
        Tuning::defaults_for(Mode::Dark),
    )
}

pub fn everforest() -> Theme {
    dark(
        Palette {
            name: "Everforest",
            background: 0x2d353b,
            foreground: 0xd3c6aa,
            accent: 0xa7c080,
            selection: 0x475258,
            bright_black: 0x859289,
        },
        NamedStatus::all(
            Rgb::hex(0xe67e80),
            Rgb::hex(0xa7c080),
            Rgb::hex(0xdbbc7f),
            Rgb::hex(0x7fbbb3),
        ),
        Tuning::defaults_for(Mode::Dark),
    )
}

// ── Light ───────────────────────────────────────────────────────────────────

/// The default light theme, and the one mock-up 04 measures.
pub fn rose_pine_dawn() -> Theme {
    light(
        Palette {
            name: "Rosé Pine Dawn",
            background: 0xfaf4ed,
            foreground: 0x575279,
            accent: 0x286983,
            selection: 0xf2e9e1,
            bright_black: 0x9893a5,
        },
        NamedStatus::all(
            Rgb::hex(0xb4637a),
            Rgb::hex(0x3e8c68),
            Rgb::hex(0xa6772a),
            Rgb::hex(0x56949f),
        ),
    )
}

pub fn catppuccin_latte() -> Theme {
    light(
        Palette {
            name: "Catppuccin Latte",
            background: 0xeff1f5,
            foreground: 0x4c4f69,
            accent: 0x1e66d5,
            selection: 0xccd0da,
            bright_black: 0x9ca0b0,
        },
        NamedStatus::all(
            Rgb::hex(0xd20f39),
            Rgb::hex(0x40a02b),
            Rgb::hex(0xdf8e1d),
            Rgb::hex(0x1e66d5),
        ),
    )
}

/// Every embedded theme, in catalogue order.
pub fn catalogue() -> Vec<Theme> {
    vec![
        tokyo_night(),
        gruvbox(),
        matte_black(),
        catppuccin_mocha(),
        nord(),
        everforest(),
        rose_pine_dawn(),
        catppuccin_latte(),
    ]
}

/// Look a theme up by name, case-insensitively. `None` when it is not one of
/// ours — the caller decides whether that is worth a warning.
pub fn by_name(name: &str) -> Option<Theme> {
    catalogue()
        .into_iter()
        .find(|theme| theme.name.eq_ignore_ascii_case(name))
}

/// The theme used when nothing else resolves, per `mode`.
pub fn default_for(mode: Mode) -> Theme {
    match mode {
        Mode::Dark => tokyo_night(),
        Mode::Light => rose_pine_dawn(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_holds_six_dark_and_two_light() {
        // DESIGN-TOKENS §10 test 5.
        let (dark, light): (Vec<_>, Vec<_>) = catalogue()
            .into_iter()
            .partition(|theme| theme.mode() == Mode::Dark);
        assert!(dark.len() >= 6, "only {} dark themes", dark.len());
        assert!(light.len() >= 2, "only {} light themes", light.len());
    }

    #[test]
    fn theme_names_are_unique_and_findable() {
        let mut names: Vec<String> = catalogue().into_iter().map(|t| t.name).collect();
        let count = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), count, "duplicate theme name in the catalogue");

        assert_eq!(
            by_name("tokyo night").map(|t| t.name),
            Some("Tokyo Night".into())
        );
        assert!(by_name("Solarized").is_none());
    }
}

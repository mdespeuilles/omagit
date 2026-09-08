//! The embedded theme catalogue.
//!
//! On macOS there is no Omarchy: embedded themes are the default experience
//! there, not a degraded fallback (SPEC §6.1). M0 ships the two the mock-ups are
//! drawn against; the full catalogue — six dark and two light — arrives at M1
//! with the tests that hold it (DESIGN-TOKENS §10 test 5).

use crate::color::Rgb;
use crate::theme::{Inputs, StatusColors, Theme, Tuning};
use crate::tokens::Mode;

/// Tokyo Night — the default dark theme (SPEC §6.1).
pub fn tokyo_night() -> Theme {
    Theme {
        name: "Tokyo Night".into(),
        inputs: Inputs {
            background: Rgb::hex(0x1a1b26),
            foreground: Rgb::hex(0xc0caf5),
            accent: Rgb::hex(0x7aa2f7),
            selection: Rgb::hex(0x283457),
            bright_black: Rgb::hex(0x414868),
            mode: Mode::Dark,
        },
        status: StatusColors {
            red: Rgb::hex(0xf7768e),
            green: Rgb::hex(0x9ece6a),
            yellow: Rgb::hex(0xe0af68),
            blue: Rgb::hex(0x7aa2f7),
        },
        tuning: Tuning::defaults_for(Mode::Dark),
    }
}

/// Rosé Pine Dawn — the default light theme. A first-class light theme matters
/// as much as the Omarchy mapping, because macOS follows the system appearance.
pub fn rose_pine_dawn() -> Theme {
    Theme {
        name: "Rosé Pine Dawn".into(),
        inputs: Inputs {
            background: Rgb::hex(0xfaf4ed),
            foreground: Rgb::hex(0x575279),
            accent: Rgb::hex(0x286983),
            selection: Rgb::hex(0xf2e9e1),
            bright_black: Rgb::hex(0x9893a5),
            mode: Mode::Light,
        },
        status: StatusColors {
            red: Rgb::hex(0xb4637a),
            green: Rgb::hex(0x3e8c68),
            yellow: Rgb::hex(0xa6772a),
            blue: Rgb::hex(0x56949f),
        },
        tuning: Tuning::defaults_for(Mode::Light),
    }
}

/// Every embedded theme, in catalogue order.
pub fn catalogue() -> Vec<Theme> {
    vec![tokyo_night(), rose_pine_dawn()]
}

/// The theme used when nothing else resolves, per `mode`.
pub fn default_for(mode: Mode) -> Theme {
    match mode {
        Mode::Dark => tokyo_night(),
        Mode::Light => rose_pine_dawn(),
    }
}

//! Inputs, tuning coefficients, and the derivation into [`Tokens`].

use crate::color::{Percent, Rgb, mix};
use crate::tokens::{Mode, Tint, Tokens};

/// The palette entries a theme source provides.
///
/// The six here are the guaranteed ones (DESIGN-TOKENS §2.1): if any is missing
/// or invalid, the *whole* palette falls back to an embedded theme — never a
/// partial blend of two sources.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inputs {
    pub background: Rgb,
    pub foreground: Rgb,
    pub accent: Rgb,
    pub selection: Rgb,
    pub bright_black: Rgb,
    pub mode: Mode,
}

/// The named status colours.
///
/// M0 ships only embedded themes, which all name their four. Reading them from
/// an Omarchy palette — where they are optional and fall back to OKLCH
/// derivation (DESIGN-TOKENS §4.2) — arrives with the rest of §6 at M1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusColors {
    pub red: Rgb,
    pub green: Rgb,
    pub yellow: Rgb,
    pub blue: Rgb,
}

/// Per-theme tuning. These are **fields of the theme, not constants of the
/// code** (DESIGN-TOKENS §2.3): a frozen coefficient produces either a washed-out
/// light theme or a garish dark one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    /// Proportion of `foreground` in `text_muted`.
    pub muted_mix: Percent,
    /// Proportion of `foreground` in `text_dim`.
    pub dim_mix: Percent,
    /// L of the commit-graph lanes.
    pub lane_lightness: f32,
    /// C of the commit-graph lanes.
    pub lane_chroma: f32,
}

impl Tuning {
    /// Defaults applied to a palette that carries no tuning of its own — an
    /// Omarchy theme, for instance (DESIGN-TOKENS §2.3).
    pub const fn defaults_for(mode: Mode) -> Self {
        match mode {
            Mode::Dark => Self {
                muted_mix: Percent::new(65.0),
                dim_mix: Percent::new(45.0),
                lane_lightness: 0.74,
                lane_chroma: 0.13,
            },
            Mode::Light => Self {
                muted_mix: Percent::new(90.0),
                dim_mix: Percent::new(66.0),
                lane_lightness: 0.52,
                lane_chroma: 0.14,
            },
        }
    }
}

/// A resolved theme: a name, its inputs, its status colours and its tuning.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub name: String,
    pub inputs: Inputs,
    pub status: StatusColors,
    pub tuning: Tuning,
}

impl Theme {
    pub fn mode(&self) -> Mode {
        self.inputs.mode
    }

    /// Derive the full token set, following DESIGN-TOKENS §4.1, §4.2 and §5.
    ///
    /// Contrast correction (§4.3) is not applied here yet — it lands with the
    /// rest of §6 at M1, together with the six tests that hold it to ≥ 4.5:1.
    /// The two embedded themes shipped at M0 already clear that bar.
    pub fn tokens(&self) -> Tokens {
        let Inputs {
            background: bg,
            foreground: fg,
            accent,
            selection,
            bright_black,
            ..
        } = self.inputs;

        let danger = self.status.red;
        let success = self.status.green;

        Tokens {
            bg,
            surface: mix(fg, bg, Percent::new(4.0)),
            surface_raised: selection,
            surface_hover: mix(fg, bg, Percent::new(6.0)),
            border: bright_black,
            border_focus: accent,
            text: fg,
            text_muted: mix(fg, bg, self.tuning.muted_mix),
            text_dim: mix(fg, bg, self.tuning.dim_mix),
            accent,

            danger,
            success,
            warning: self.status.yellow,
            info: self.status.blue,

            diff_added: self.tint(success, Tint::Subtle),
            diff_added_word: self.tint(success, Tint::Strong),
            diff_deleted: self.tint(danger, Tint::Subtle),
            diff_deleted_word: self.tint(danger, Tint::Strong),
        }
    }

    /// Lay a status colour over `bg` at one of the three sanctioned levels.
    /// The only way a component may tint a surface (DESIGN-TOKENS §5).
    pub fn tint(&self, status: Rgb, level: Tint) -> Rgb {
        mix(
            status,
            self.inputs.background,
            Percent::new(level.percent()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedded;

    #[test]
    fn structure_tokens_follow_the_contract() {
        let theme = embedded::tokyo_night();
        let t = theme.tokens();

        assert_eq!(t.bg, theme.inputs.background, "bg is the raw background");
        assert_eq!(t.text, theme.inputs.foreground);
        assert_eq!(
            t.surface_raised, theme.inputs.selection,
            "raised is selection"
        );
        assert_eq!(t.border, theme.inputs.bright_black, "border is color8");
        assert_eq!(t.border_focus, theme.inputs.accent);
    }

    #[test]
    fn hover_and_selected_stay_distinct() {
        // surface_hover exists precisely so that hovering a row does not reuse
        // surface_raised, which means "selected" (DESIGN-TOKENS §4.1).
        for theme in embedded::catalogue() {
            let t = theme.tokens();
            assert_ne!(
                t.surface_hover, t.surface_raised,
                "{}: hover and selection must not collapse",
                theme.name
            );
        }
    }

    #[test]
    fn light_theme_keeps_more_foreground_in_muted_text() {
        // The case a frozen coefficient breaks (DESIGN-TOKENS §2.3, §10 test 6).
        let dark = embedded::tokyo_night();
        let light = embedded::rose_pine_dawn();
        assert!(light.tuning.muted_mix > dark.tuning.muted_mix);
        assert!(light.tuning.dim_mix > dark.tuning.dim_mix);
    }

    #[test]
    fn word_highlight_is_stronger_than_the_line_behind_it() {
        for theme in embedded::catalogue() {
            let t = theme.tokens();
            let distance = |a: Rgb, b: Rgb| {
                (a.r as i32 - b.r as i32).abs()
                    + (a.g as i32 - b.g as i32).abs()
                    + (a.b as i32 - b.b as i32).abs()
            };
            assert!(
                distance(t.diff_added_word, t.bg) > distance(t.diff_added, t.bg),
                "{}: the 24% word tint must read stronger than the 12% line tint",
                theme.name
            );
            assert!(
                distance(t.diff_deleted_word, t.bg) > distance(t.diff_deleted, t.bg),
                "{}: same on the deletion side",
                theme.name
            );
        }
    }
}

//! Inputs, tuning coefficients, and the derivation into [`Tokens`].

use crate::color::{Percent, Rgb, mix};
use crate::oklch::{MIN_CONTRAST, Oklch, contrast_ratio, ensure_contrast};
use crate::tokens::{Mode, Tint, Tokens};

/// The palette entries a theme source must provide.
///
/// These six are the guaranteed ones (DESIGN-TOKENS §2.1): if any is missing or
/// invalid, the **whole** palette falls back to an embedded theme — never a
/// partial blend of two sources, which would produce unreadable combinations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inputs {
    pub background: Rgb,
    pub foreground: Rgb,
    pub accent: Rgb,
    pub selection: Rgb,
    pub bright_black: Rgb,
    pub mode: Mode,
}

/// The optional named status colours (DESIGN-TOKENS §2.2). Absent ones are
/// derived in OKLCH from their hue alone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NamedStatus {
    pub red: Option<Rgb>,
    pub green: Option<Rgb>,
    pub yellow: Option<Rgb>,
    pub blue: Option<Rgb>,
}

impl NamedStatus {
    /// A theme that names all four.
    pub const fn all(red: Rgb, green: Rgb, yellow: Rgb, blue: Rgb) -> Self {
        Self {
            red: Some(red),
            green: Some(green),
            yellow: Some(yellow),
            blue: Some(blue),
        }
    }
}

/// The hues status colours fall back to when a theme does not name them
/// (DESIGN-TOKENS §4.2). Lightness and chroma come from [`StatusFallback`].
pub mod hue {
    pub const DANGER: f32 = 25.0;
    pub const SUCCESS: f32 = 145.0;
    pub const WARNING: f32 = 85.0;
    pub const INFO: f32 = 250.0;
}

/// Lightness and chroma for derived status colours, per mode.
///
/// The dark values are Matte Black's, which DESIGN-TOKENS §4.2 gives exactly
/// because that theme names none of the four and so exercises this path end to
/// end.
#[derive(Clone, Copy, Debug, PartialEq)]
struct StatusFallback {
    danger: (f32, f32),
    success: (f32, f32),
    warning: (f32, f32),
    info: (f32, f32),
}

impl StatusFallback {
    const fn for_mode(mode: Mode) -> Self {
        match mode {
            Mode::Dark => Self {
                danger: (0.68, 0.17),
                success: (0.74, 0.15),
                warning: (0.80, 0.14),
                info: (0.70, 0.13),
            },
            // Light backgrounds need darker, slightly less saturated status
            // colours to stay legible: L ≈ 0.52, C ≈ 0.14 (DESIGN-TOKENS §4.2).
            Mode::Light => Self {
                danger: (0.52, 0.14),
                success: (0.52, 0.14),
                warning: (0.52, 0.14),
                info: (0.52, 0.14),
            },
        }
    }
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
    /// Defaults applied to a palette that carries no tuning of its own — every
    /// theme read from Omarchy, for instance (DESIGN-TOKENS §2.3).
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

    pub const fn with_lanes(mut self, lightness: f32, chroma: f32) -> Self {
        self.lane_lightness = lightness;
        self.lane_chroma = chroma;
        self
    }
}

/// The floor `text_dim` is held to.
///
/// Not 4.5:1, and the reason matters. `text_dim` is the disabled/placeholder
/// token (DESIGN.md §3, board 01), and WCAG 1.4.3 exempts inactive components.
/// Measured on the delivered themes it sits near 3.1:1 by construction — the
/// per-mode coefficients of §2.3 are what keep `text_muted` over 4.5:1, and
/// forcing `text_dim` to the same floor would push it *past* `text_muted`,
/// inverting the very hierarchy the two tokens exist to express. So it gets the
/// non-text floor of WCAG 1.4.11, plus the ordering invariant enforced in
/// [`Theme::tokens`].
pub const DIM_MIN_CONTRAST: f32 = 3.0;

/// A resolved theme: a name, its inputs, its named status colours and its
/// tuning.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub name: String,
    pub inputs: Inputs,
    pub status: NamedStatus,
    pub tuning: Tuning,
}

impl Theme {
    pub fn mode(&self) -> Mode {
        self.inputs.mode
    }

    /// Derive the full token set: DESIGN-TOKENS §4.1, §4.2, §4.3 and §5.
    pub fn tokens(&self) -> Tokens {
        let Inputs {
            background: bg,
            foreground: fg,
            accent,
            selection,
            bright_black,
            mode,
        } = self.inputs;

        let fallback = StatusFallback::for_mode(mode);
        let surface = mix(fg, bg, Percent::new(4.0));

        // §4.3: correction applies to colours *read* from the theme as much as
        // to derived ones. Nothing guarantees an Omarchy palette provides a
        // `green` legible on its own background.
        //
        // §4.3 words the target as `bg`, but text is drawn on `surface` too —
        // panels, lists, the whole content area — and `surface` is 4% closer to
        // the foreground, so it is always the harder of the two. Correcting
        // against `bg` alone leaves the `surface` pairs at ~4.1–4.4:1, which
        // §10 test 1 rejects. Targeting `surface` satisfies both, so that is
        // the target; it is a strictly stronger guarantee, never a weaker one.
        let legible = |color: Rgb| ensure_contrast(color, surface, MIN_CONTRAST);
        let status = |named: Option<Rgb>, (lightness, chroma): (f32, f32), hue: f32| {
            legible(named.unwrap_or_else(|| Oklch::new(lightness, chroma, hue).to_rgb()))
        };

        let danger = status(self.status.red, fallback.danger, hue::DANGER);
        let success = status(self.status.green, fallback.success, hue::SUCCESS);

        let text = legible(fg);
        let text_muted = legible(mix(fg, bg, self.tuning.muted_mix));
        let text_dim = {
            let dim = ensure_contrast(mix(fg, bg, self.tuning.dim_mix), surface, DIM_MIN_CONTRAST);
            // A hostile palette could push the corrected dim past muted. Rather
            // than render "disabled" as the more prominent of the two, collapse
            // them: losing the distinction is honest, inverting it is a bug.
            if contrast_ratio(dim, surface) > contrast_ratio(text_muted, surface) {
                text_muted
            } else {
                dim
            }
        };

        Tokens {
            bg,
            surface,
            surface_raised: selection,
            surface_hover: mix(fg, bg, Percent::new(6.0)),
            border: bright_black,
            border_focus: legible(accent),
            text,
            text_muted,
            text_dim,
            accent: legible(accent),

            danger,
            success,
            warning: status(self.status.yellow, fallback.warning, hue::WARNING),
            info: status(self.status.blue, fallback.info, hue::INFO),

            // Diff surfaces derive from the *corrected* status colours, so a
            // theme with an illegible green does not get an illegible addition
            // background either.
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
        assert_eq!(
            t.surface_raised, theme.inputs.selection,
            "raised is selection"
        );
        assert_eq!(
            t.border, theme.inputs.bright_black,
            "border is the palette's mid grey"
        );
        // Tokyo Night's own foreground and accent are already legible, so
        // correction leaves them alone and the tokens are the raw inputs.
        assert_eq!(t.text, theme.inputs.foreground);
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

    #[test]
    fn a_theme_naming_no_status_colour_derives_all_four() {
        // Matte Black is the theme that exercises the OKLCH fallback path in
        // full (DESIGN-TOKENS §4.2).
        let theme = embedded::matte_black();
        assert_eq!(theme.status, NamedStatus::default(), "it names none");

        let t = theme.tokens();
        let hue_of = |c: Rgb| Oklch::from(c).hue;
        for (name, token, expected) in [
            ("danger", t.danger, hue::DANGER),
            ("success", t.success, hue::SUCCESS),
            ("warning", t.warning, hue::WARNING),
            ("info", t.info, hue::INFO),
        ] {
            let drift = (hue_of(token) - expected).abs();
            assert!(drift < 10.0, "{name} landed {drift}° from hue {expected}");
        }
    }

    #[test]
    fn text_dim_never_outshines_text_muted() {
        // The ordering invariant, checked on every embedded theme.
        for theme in embedded::catalogue() {
            let t = theme.tokens();
            assert!(
                contrast_ratio(t.text_dim, t.bg) <= contrast_ratio(t.text_muted, t.bg),
                "{}: dim reads stronger than muted",
                theme.name
            );
        }
    }
}

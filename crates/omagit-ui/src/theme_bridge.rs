//! Projects `omagit-theme` tokens into the renderer.
//!
//! Two consumers, and they are not the same thing:
//!
//! * **omagit's own components** read [`Palette`] straight off the GPUI global.
//!   Those tokens carry the canonical names of DESIGN-TOKENS.md, including the
//!   four (`surface_hover`, `text_dim`, `info`, the diff surfaces) that have no
//!   equivalent in `gpui-omarchy`.
//! * **vendored `gpui-omarchy` components** read `gpui_omarchy::Theme`, whose
//!   vocabulary (`inset`, `bright`, `secondary`, `on_accent`) predates our
//!   contract. [`apply`] fills it in so those components sit in the same palette.
//!
//! The mapping below is therefore lossy by design, in one direction only: the
//! canonical tokens are the source, `gpui_omarchy::Theme` the projection.

use gpui_kit::base::ThemeAppearance;
use gpui_kit::{App, Global, Hsla, SharedString, rgb};
use gpui_omarchy::Theme as OmarchyTheme;

use omagit_theme::{Density, DensityMode, Mode, Rgb, Theme, Tokens, lanes};

use crate::fonts::{ActiveFonts, Fonts};

/// What every omagit component reads: the resolved tokens and the metrics.
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub name: SharedString,
    pub mode: Mode,
    pub tokens: Tokens,
    pub density: Density,
    /// The commit-graph lanes, generated for this theme's lightness and chroma.
    /// Precomputed here because they never change while a theme is applied, and
    /// because a component that derived its own would be reading the theme —
    /// which DESIGN-TOKENS §6 forbids.
    pub lanes: Vec<Rgb>,
}

impl Palette {
    /// The colour of lane `index`, cycling past the eighth.
    pub fn lane(&self, index: usize) -> Rgb {
        self.lanes[index % self.lanes.len()]
    }
}

impl Global for Palette {}

/// Convert a theme colour into the renderer's colour type.
pub fn hsla(color: Rgb) -> Hsla {
    rgb(color.packed()).into()
}

/// Read the active palette. Panics only if [`apply`] was never called, which
/// would be a start-up ordering bug rather than a runtime condition.
pub trait ActivePalette {
    fn palette(&self) -> &Palette;
}

impl ActivePalette for App {
    fn palette(&self) -> &Palette {
        self.global::<Palette>()
    }
}

/// Install `theme` as the active palette and repaint every window.
///
/// [`crate::fonts::resolve`] must have run first: the projection needs the
/// resolved families, not the raw stacks.
pub fn apply(theme: &Theme, density: DensityMode, cx: &mut App) {
    let tokens = theme.tokens();
    let fonts = cx.fonts().clone();

    // `gpui_omarchy::Theme::apply` also stops the crate's own once-a-second
    // Omarchy poll, which we never want running: omagit owns theme sourcing
    // (SPEC §6.1), and on macOS there is no Omarchy to poll at all.
    projection(theme, &tokens, &fonts).apply(cx);

    // gpui-omarchy sets the mono family to the UI family. DESIGN-TOKENS §8 puts
    // every Git literal — hashes, refs, paths, diff content — in a real
    // monospace, so the projection is corrected here rather than in the vendored
    // crate, which keeps the vendor diff empty.
    let base = gpui_kit::base::Theme::global_mut(cx);
    base.tokens.typography.mono = fonts.mono.clone();

    cx.set_global(Palette {
        name: theme.name.clone().into(),
        mode: theme.mode(),
        tokens,
        density: density.metrics(),
        lanes: lanes::lane_colors(
            lanes::LANE_COUNT,
            theme.tuning.lane_lightness,
            theme.tuning.lane_chroma,
        ),
    });
    cx.refresh_windows();
}

fn projection(theme: &Theme, tokens: &Tokens, fonts: &Fonts) -> OmarchyTheme {
    OmarchyTheme {
        name: theme.name.clone().into(),
        appearance: match theme.mode() {
            Mode::Dark => ThemeAppearance::Dark,
            Mode::Light => ThemeAppearance::Light,
        },
        background: hsla(tokens.bg),
        surface: hsla(tokens.surface),
        // `inset` backs gpui-base's `muted` surface. `surface_hover` is the
        // nearest canonical token: one step off the background, below selection.
        inset: hsla(tokens.surface_hover),
        foreground: hsla(tokens.text),
        secondary: hsla(tokens.text_muted),
        bright: hsla(tokens.text),
        accent: hsla(tokens.accent),
        // Accent-filled controls carry background-coloured text throughout the
        // mock-ups (`background: --ac; color: --bg`).
        on_accent: hsla(tokens.bg),
        selection: hsla(tokens.surface_raised),
        border: hsla(tokens.border),
        danger: hsla(tokens.danger),
        warning: hsla(tokens.warning),
        success: hsla(tokens.success),
        font: fonts.ui.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn the_projection_carries_the_canonical_tokens() {
        let theme = omagit_theme::embedded::tokyo_night();
        let tokens = theme.tokens();
        let fonts = Fonts {
            ui: "Inter".into(),
            mono: "JetBrains Mono".into(),
        };
        let projected = projection(&theme, &tokens, &fonts);

        assert_eq!(projected.background, hsla(tokens.bg));
        assert_eq!(projected.selection, hsla(tokens.surface_raised));
        assert_eq!(projected.secondary, hsla(tokens.text_muted));
        assert_eq!(projected.border, hsla(tokens.border));
        assert_eq!(projected.appearance, ThemeAppearance::Dark);
    }

    #[test]
    fn the_projection_carries_the_ui_family_not_the_mono_one() {
        // The bug this guards: gpui-omarchy sets `mono` to the UI family, which
        // would render every hash and path in a proportional face. `apply`
        // corrects it afterwards; the projection itself must carry the UI one.
        let theme = omagit_theme::embedded::tokyo_night();
        let fonts = Fonts {
            ui: "Inter".into(),
            mono: "JetBrains Mono".into(),
        };
        assert_eq!(projection(&theme, &theme.tokens(), &fonts).font, fonts.ui);
    }
}

//! The main window: the shell every screen is drawn inside.
//!
//! What it renders is deliberately a view onto the theme system rather than a
//! product screen: M1 is the theme milestone, and "the palette resolves and
//! tracks its source" has to be something you can see, not only something the
//! tests assert. The three real screens land at M3, M4 and M6.

use gpui_kit::prelude::*;
use gpui_kit::{
    App, Bounds, Context, FontWeight, IntoElement, Pixels, SharedString, Subscription, Window,
    WindowBounds, WindowOptions, div, px, size,
};

use omagit_theme::oklch::contrast_ratio;
use omagit_theme::{DensityMode, Rgb};
use omagit_ui::{ActiveFonts, ActivePalette, Fonts, Palette, hsla};

use crate::platform::{self, Platform, TOPBAR_HEIGHT_COMFORTABLE, TOPBAR_HEIGHT_COMPACT};
use crate::theme_runtime::{ThemeRuntime, Tracking};

/// Statusbar heights, per density (mock-up 08).
const STATUSBAR_HEIGHT_COMFORTABLE: f32 = 24.0;
const STATUSBAR_HEIGHT_COMPACT: f32 = 22.0;

/// The reference window of the mock-ups is 1600×1000; below 1100px of usable
/// width the layout collapses to tabs, which is why the minimum sits here.
const DEFAULT_SIZE: (f32, f32) = (1600.0, 1000.0);
const MIN_SIZE: (f32, f32) = (900.0, 600.0);

pub struct Shell {
    density: DensityMode,
    platform: &'static dyn Platform,
    /// Held for its lifetime: dropping it stops the window following the
    /// system's light/dark preference.
    _appearance: Subscription,
}

impl Shell {
    pub fn new(density: DensityMode, appearance: Subscription) -> Self {
        Self {
            density,
            platform: platform::current(),
            _appearance: appearance,
        }
    }

    fn topbar_height(&self) -> Pixels {
        px(match self.density {
            DensityMode::Compact => TOPBAR_HEIGHT_COMPACT,
            DensityMode::Comfortable => TOPBAR_HEIGHT_COMFORTABLE,
        })
    }

    fn statusbar_height(&self) -> Pixels {
        px(match self.density {
            DensityMode::Compact => STATUSBAR_HEIGHT_COMPACT,
            DensityMode::Comfortable => STATUSBAR_HEIGHT_COMFORTABLE,
        })
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette().clone();
        let fonts = cx.fonts().clone();
        let t = palette.tokens;
        let metrics = palette.density;
        let reserve = self.platform.topbar_reserve();

        let (source, tracking, fell_back) = match cx.try_global::<ThemeRuntime>() {
            Some(runtime) => (
                format!("{:?}", runtime.resolved.source),
                runtime.tracking.label(),
                runtime
                    .resolved
                    .fell_back_from
                    .as_ref()
                    .map(|source| format!("{source:?}")),
            ),
            None => ("—".to_owned(), Tracking::None.label(), None),
        };

        let topbar = div()
            .flex()
            .items_center()
            .w_full()
            .h(self.topbar_height())
            .bg(hsla(t.bg))
            .border_b_1()
            .border_color(hsla(t.border))
            // Leading reserve: a flex spacer, never a conditional padding. On
            // macOS the traffic lights are drawn here by the system.
            .child(div().w(reserve.leading).flex_none())
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(metrics.gap))
                    .px(px(metrics.pad))
                    .child(
                        div()
                            .font_family(fonts.mono.clone())
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(hsla(t.text))
                            .child("omagit"),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(hsla(t.text_muted))
                            .child("aucun dépôt ouvert"),
                    ),
            )
            .child(div().flex_1())
            .child(div().w(reserve.trailing).flex_none());

        let body = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .bg(hsla(t.surface))
            .gap(px(metrics.gap))
            .p(px(metrics.pad * 2.0))
            .child(heading(format!("Thème · {}", palette.name), &palette))
            .child(row("Source", source, &palette, &fonts.mono))
            .child(row("Suivi", tracking, &palette, &fonts.ui))
            .children(fell_back.map(|requested| {
                // The UI says so rather than quietly showing something else.
                row_colored("Repli depuis", requested, &palette, &fonts.mono, t.warning)
            }))
            .child(row(
                "Plateforme",
                self.platform.name(),
                &palette,
                &fonts.mono,
            ))
            // The modifier glyph is interface text, not a Git literal: it stays
            // in the UI face, which covers ⌘ where JetBrains Mono does not.
            .child(row(
                "Modificateur",
                self.platform.primary_modifier().label(),
                &palette,
                &fonts.ui,
            ))
            .child(heading("Tokens", &palette))
            .child(swatches(&palette, &fonts))
            .child(heading("Lanes du graphe", &palette))
            .child(lane_strip(&palette, &fonts))
            .child(heading("Contraste texte / fond", &palette))
            .child(contrast_table(&palette, &fonts));

        let statusbar = div()
            .flex()
            .items_center()
            .w_full()
            .h(self.statusbar_height())
            .px(px(metrics.pad))
            .gap(px(metrics.gap))
            .bg(hsla(t.bg))
            .border_t_1()
            .border_color(hsla(t.border))
            .text_size(px(11.0))
            .text_color(hsla(t.text_dim))
            .child(SharedString::from(format!(
                "M1 · thème — {} thèmes embarqués",
                omagit_theme::catalogue().len()
            )));

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(hsla(t.bg))
            .text_color(hsla(t.text))
            .font_family(fonts.ui.clone())
            .text_size(px(13.0))
            .child(topbar)
            .child(body)
            .child(statusbar)
    }
}

fn heading(label: impl Into<SharedString>, palette: &Palette) -> impl IntoElement {
    div()
        .mt(px(palette.density.pad))
        .text_size(px(11.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(hsla(palette.tokens.text_muted))
        .child(label.into())
}

/// A label/value line. `face` decides which stack the value is set in:
/// DESIGN-TOKENS §8 puts every Git literal in the mono face, and only those.
fn row(
    label: &'static str,
    value: impl Into<SharedString>,
    palette: &Palette,
    face: &SharedString,
) -> impl IntoElement {
    row_colored(label, value, palette, face, palette.tokens.text)
}

fn row_colored(
    label: &'static str,
    value: impl Into<SharedString>,
    palette: &Palette,
    face: &SharedString,
    color: Rgb,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .h(px(palette.density.row_height))
        .gap(px(palette.density.gap * 2.0))
        .child(
            div()
                .w(px(180.0))
                .flex_none()
                .text_size(px(11.0))
                .text_color(hsla(palette.tokens.text_muted))
                .child(label),
        )
        .child(
            div()
                .font_family(face.clone())
                .text_size(px(12.5))
                .text_color(hsla(color))
                .child(value.into()),
        )
}

/// The token strip. "Applies a theme" is only credible if the derived tokens
/// are on screen: a swatch that collapses into its neighbour is a derivation
/// bug you can see.
fn swatches(palette: &Palette, fonts: &Fonts) -> impl IntoElement {
    let t = palette.tokens;
    let entries = [
        ("bg", t.bg),
        ("surface", t.surface),
        ("raised", t.surface_raised),
        ("hover", t.surface_hover),
        ("border", t.border),
        ("text", t.text),
        ("muted", t.text_muted),
        ("dim", t.text_dim),
        ("accent", t.accent),
        ("danger", t.danger),
        ("success", t.success),
        ("warning", t.warning),
        ("info", t.info),
        ("+ line", t.diff_added),
        ("+ word", t.diff_added_word),
        ("− line", t.diff_deleted),
        ("− word", t.diff_deleted_word),
    ];

    div()
        .flex()
        .flex_wrap()
        .gap(px(palette.density.gap))
        .children(entries.map(|(name, color)| swatch(name, color, palette, fonts, px(72.0))))
}

/// The eight generated lanes. They never read the theme — only their lightness
/// and chroma do (DESIGN-TOKENS §6) — so seeing them stay evenly spaced across
/// a palette switch is the point.
fn lane_strip(palette: &Palette, fonts: &Fonts) -> impl IntoElement {
    div().flex().gap(px(palette.density.gap)).children(
        palette
            .lanes
            .iter()
            .enumerate()
            .map(|(index, color)| swatch(format!("lane {index}"), *color, palette, fonts, px(56.0)))
            .collect::<Vec<_>>(),
    )
}

fn swatch(
    name: impl Into<SharedString>,
    color: Rgb,
    palette: &Palette,
    fonts: &Fonts,
    width: Pixels,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .w(width)
                .h(px(28.0))
                .bg(hsla(color))
                .border_1()
                .border_color(hsla(palette.tokens.border)),
        )
        .child(
            div()
                .font_family(fonts.mono.clone())
                .text_size(px(11.0))
                .text_color(hsla(palette.tokens.text_dim))
                .child(name.into()),
        )
}

/// The measured ratios behind DESIGN-TOKENS §10 test 1. On screen because a
/// number that only exists in a test is a number nobody looks at.
fn contrast_table(palette: &Palette, fonts: &Fonts) -> impl IntoElement {
    let t = palette.tokens;
    let rows = [
        ("text", t.text),
        ("text_muted", t.text_muted),
        ("text_dim", t.text_dim),
        ("accent", t.accent),
        ("danger", t.danger),
        ("success", t.success),
        ("warning", t.warning),
        ("info", t.info),
    ];

    div()
        .flex()
        .flex_wrap()
        .gap(px(palette.density.gap * 2.0))
        .children(rows.map(|(name, color)| {
            let ratio = contrast_ratio(color, t.surface);
            div()
                .flex()
                .items_center()
                .gap(px(palette.density.gap))
                .child(
                    div()
                        .font_family(fonts.mono.clone())
                        .text_size(px(11.0))
                        .text_color(hsla(color))
                        .child(SharedString::from(name)),
                )
                .child(
                    div()
                        .font_family(fonts.mono.clone())
                        .text_size(px(11.0))
                        // Below the floor reads as a warning, so a regression is
                        // visible without opening the test output.
                        .text_color(hsla(if ratio >= 4.5 { t.text_dim } else { t.warning }))
                        .child(SharedString::from(format!("{ratio:.2}:1"))),
                )
        }))
}

/// Window options for the main window, with the platform's decoration rules.
pub fn options(platform: &dyn Platform, cx: &mut App) -> WindowOptions {
    let bounds = Bounds::centered(None, size(px(DEFAULT_SIZE.0), px(DEFAULT_SIZE.1)), cx);
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: platform.titlebar(),
        window_decorations: platform.window_decorations(),
        window_min_size: Some(size(px(MIN_SIZE.0), px(MIN_SIZE.1))),
        app_id: Some("dev.omagit.omagit".into()),
        // The app draws its own topbar and owns dragging from it, so AppKit
        // neither drags nor delays clicks in that band.
        app_owns_titlebar_drag: true,
        ..Default::default()
    }
}

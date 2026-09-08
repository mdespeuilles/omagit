//! The main window: the shell every screen is drawn inside.
//!
//! M0 draws the chrome — topbar, body, statusbar — under the active palette, so
//! that "a decorated window starts and applies a theme" is something you can see
//! and a test can assert. The three screens land at M3, M4 and M6.

use gpui_kit::prelude::*;
use gpui_kit::{
    App, Bounds, Context, FontWeight, IntoElement, Pixels, SharedString, Window, WindowBounds,
    WindowOptions, div, px, size,
};

use omagit_theme::DensityMode;
use omagit_ui::{ActiveFonts, ActivePalette, Fonts, hsla};

use crate::platform::{self, Platform, TOPBAR_HEIGHT_COMFORTABLE, TOPBAR_HEIGHT_COMPACT};

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
}

impl Shell {
    pub fn new(density: DensityMode) -> Self {
        Self {
            density,
            platform: platform::current(),
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
            .bg(hsla(t.surface))
            .gap(px(metrics.gap))
            .p(px(metrics.pad * 2.0))
            .child(
                div()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(t.text))
                    .child(SharedString::from(format!("Thème · {}", palette.name))),
            )
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
            .child(row(
                "Credential helper",
                self.platform.credential_helper(),
                &palette,
                &fonts.mono,
            ))
            .child(row(
                "Réserve topbar",
                &format!(
                    "{} / {}",
                    f32::from(reserve.leading),
                    f32::from(reserve.trailing)
                ),
                &palette,
                &fonts.mono,
            ))
            .child(swatches(&palette, &fonts));

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
            .child("M0 · squelette");

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

/// A label/value line. `face` decides which stack the value is set in:
/// DESIGN-TOKENS §8 puts every Git literal in the mono face, and only those.
fn row(
    label: &str,
    value: &str,
    palette: &omagit_ui::Palette,
    face: &SharedString,
) -> impl IntoElement {
    let t = palette.tokens;
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
                .text_color(hsla(t.text_muted))
                .child(SharedString::from(label.to_owned())),
        )
        .child(
            div()
                .font_family(face.clone())
                .text_size(px(12.5))
                .text_color(hsla(t.text))
                .child(SharedString::from(value.to_owned())),
        )
}

/// The token strip. Present because "applies a theme" is only credible if the
/// derived tokens are on screen: a swatch that collapses into its neighbour is
/// a derivation bug you can see.
fn swatches(palette: &omagit_ui::Palette, fonts: &Fonts) -> impl IntoElement {
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
        .mt(px(palette.density.pad))
        .children(entries.map(|(name, color)| {
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .w(px(72.0))
                        .h(px(28.0))
                        .bg(hsla(color))
                        .border_1()
                        .border_color(hsla(t.border)),
                )
                .child(
                    div()
                        .font_family(fonts.mono.clone())
                        .text_size(px(11.0))
                        .text_color(hsla(t.text_dim))
                        .child(SharedString::from(name)),
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

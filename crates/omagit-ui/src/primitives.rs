//! The atoms of board 01, at the sizes board 06 actually uses.
//!
//! They are here rather than inline in the screen for one reason: every one of
//! them encodes a decision from DESIGN.md that would otherwise be re-derived —
//! and re-derived slightly differently — the next time a screen needs a chip or
//! a section header. A hard-coded `px(18.0)` in a screen is how two chips end up
//! a pixel apart.
//!
//! Nothing here reads a token by name from a component: everything takes the
//! [`Palette`] and picks from it, which is what SPEC §3 rule 4 asks for.

use gpui_kit::prelude::*;
use gpui_kit::{FontWeight, Hsla, IntoElement, SharedString, div, px};

use omagit_theme::Rgb;

use crate::{Palette, hsla};

/// The label column of a card's field grid (board 06: `150px minmax(0,1fr)`,
/// 16px apart).
pub const FIELD_LABEL_WIDTH: f32 = 150.0;
pub const FIELD_GAP: f32 = 16.0;

/// A section title: `REPOSITORY`, `WORKING COPY`, `REMOTES`.
///
/// The board sets these in 11px uppercase with 0.08em of tracking. The renderer
/// has no letter-spacing, so the tracking is dropped rather than faked with
/// spaces between characters — which would break text selection and copy. The
/// uppercase is applied here, to the string, so the caller writes the label the
/// way it reads.
pub fn section(label: &str, palette: &Palette) -> gpui_kit::Div {
    div()
        .flex()
        .items_center()
        .h(px(26.0))
        .w_full()
        .border_b_1()
        .border_color(hsla(palette.tokens.border))
        .text_size(px(11.0))
        .text_color(hsla(palette.tokens.text_muted))
        .child(SharedString::from(label.to_uppercase()))
}

/// One line of a card: a right-aligned label, then whatever the value is.
pub fn field(label: &str, palette: &Palette, value: impl IntoElement) -> gpui_kit::Div {
    div()
        .flex()
        .items_center()
        .gap(px(FIELD_GAP))
        .w_full()
        .min_h(px(20.0))
        .child(
            div()
                .w(px(FIELD_LABEL_WIDTH))
                .flex_none()
                .text_right()
                .text_size(px(12.0))
                .text_color(hsla(palette.tokens.text_muted))
                .child(SharedString::from(label.to_owned())),
        )
        // `flex_1` on the value column, not just `min_w_0`: a field whose value
        // is a text editor has no intrinsic width, and without this it collapses
        // to a single character.
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .flex_1()
                .min_w_0()
                .child(value),
        )
}

/// The block of fields under a section header (board 06: `10px 0 14px`).
pub fn fields(rows: Vec<gpui_kit::AnyElement>) -> gpui_kit::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(5.0))
        .pt(px(10.0))
        .pb(px(14.0))
        .w_full()
        .children(rows)
}

/// A bordered inline chip: `↑8 ↓2`, `origin/main`, `copier`.
///
/// `border` and `text` are separate because the board uses both shapes — a
/// neutral chip (border, muted text) and a coloured one (both in the same
/// status colour).
pub fn chip(label: impl Into<SharedString>, border: Rgb, text: Rgb) -> gpui_kit::Div {
    div()
        .flex()
        .items_center()
        .flex_none()
        .h(px(18.0))
        .px(px(6.0))
        .border_1()
        .border_color(hsla(border))
        .text_size(px(11.0))
        .text_color(hsla(text))
        .child(label.into())
}

/// The square status pip of DESIGN §4.
///
/// Filled for a working copy that is clean, modified or in conflict; **hollow**
/// for a detached `HEAD`. That is the point: "la forme distingue autant que la
/// couleur", so the state survives a greyscale check and a palette where two
/// status hues collapse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pip {
    Filled,
    Hollow,
}

pub fn pip(kind: Pip, color: Rgb, size: f32) -> gpui_kit::Div {
    let base = div().w(px(size)).h(px(size)).flex_none();
    match kind {
        Pip::Filled => base.bg(hsla(color)),
        Pip::Hollow => base.border_1().border_color(hsla(color)),
    }
}

/// The 90-day activity sparkline: one bar per three days, the last three in
/// accent (DESIGN §4).
///
/// Two tones below that, split at half the peak, so the shape reads as a shape
/// rather than as a row of ticks — the board's own bars do the same.
pub fn sparkline(buckets: &[u32], peak: u32, palette: &Palette) -> gpui_kit::Div {
    const HEIGHT: f32 = 30.0;
    const BAR: f32 = 5.0;
    /// A bar for a bucket with no commits would vanish; two pixels says "this
    /// period exists and nothing happened", which is different from a gap.
    const FLOOR: f32 = 2.0;

    let t = palette.tokens;
    let last_three = buckets.len().saturating_sub(3);
    div()
        .flex()
        .items_end()
        .gap(px(2.0))
        .h(px(HEIGHT))
        .children(buckets.iter().enumerate().map(|(index, count)| {
            let height = (f64::from(*count) / f64::from(peak.max(1)) * f64::from(HEIGHT)) as f32;
            let color = if index >= last_three {
                t.accent
            } else if *count * 2 >= peak {
                t.text_muted
            } else {
                t.border
            };
            div()
                .w(px(BAR))
                .h(px(height.max(FLOOR)))
                .flex_none()
                .bg(hsla(color))
        }))
}

/// The focus ring of DESIGN §1: 1px `border_focus`, inset on a full-width row.
///
/// Never a shadow, never a size change — which is why this sets a border colour
/// on an element that already has a border, rather than adding one.
pub fn focus_border(focused: bool, palette: &Palette) -> Hsla {
    hsla(if focused {
        palette.tokens.border_focus
    } else {
        palette.tokens.border
    })
}

/// A push button, in the two weights the board uses: filled accent for the
/// primary action, outlined for everything else.
pub fn button(
    label: impl Into<SharedString>,
    palette: &Palette,
    kind: ButtonKind,
    focused: bool,
) -> gpui_kit::Div {
    let t = palette.tokens;
    let (background, text, border) = match kind {
        ButtonKind::Primary => (Some(t.accent), t.bg, t.accent),
        ButtonKind::Neutral => (None, t.text, t.border),
        ButtonKind::Danger => (None, t.danger, t.danger),
    };
    let mut element = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .flex_none()
        .h(px(26.0))
        .px(px(12.0))
        .border_1()
        .border_color(hsla(border))
        .text_size(px(13.0))
        .text_color(hsla(text))
        .child(label.into());
    if let Some(background) = background {
        element = element.bg(hsla(background)).font_weight(FontWeight::MEDIUM);
    }
    if focused {
        // Offset by 2px on a button, inset on a row (DESIGN §1). An outline
        // rather than a second border, so the button does not change size.
        element = element
            .border_color(hsla(t.border_focus))
            .shadow(vec![gpui_kit::BoxShadow {
                color: hsla(t.border_focus),
                offset: gpui_kit::point(px(0.0), px(0.0)),
                blur_radius: px(0.0),
                spread_radius: px(2.0),
                inset: false,
            }]);
    }
    element
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Neutral,
    Danger,
}

/// A keyboard hint, as it appears beside a label: `⏎`, `⌘O`.
///
/// Set in the mono face because it is a literal key, and dimmed because it is
/// an aid rather than part of the sentence. On an accent-filled button it drops
/// to 75% of the button's own foreground instead: `text_dim` on accent has no
/// contrast guarantee, since DESIGN-TOKENS §10 only ever measured it against
/// `bg` and `surface`.
pub fn shortcut(
    label: impl Into<SharedString>,
    palette: &Palette,
    mono: &SharedString,
    on_accent: bool,
) -> gpui_kit::Div {
    div()
        .flex_none()
        .font_family(mono.clone())
        .text_size(px(11.0))
        .text_color(hsla(if on_accent {
            palette.tokens.bg
        } else {
            palette.tokens.text_dim
        }))
        .when(on_accent, |element| element.opacity(0.75))
        .child(label.into())
}

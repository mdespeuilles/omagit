//! The tokens, as CSS custom properties.
//!
//! The projection this crate had into `gpui-omarchy`'s theme structure is
//! replaced by this one. It is smaller, because CSS variables cascade: a
//! component asks for `var(--text-muted)` and nothing has to hand it a palette.
//!
//! **The canonical names, not the mock-ups' shorthands.** DESIGN-TOKENS §1 is
//! explicit that the long names are the contract and that the `--fg` / `--ac` /
//! `--raised` spellings in the delivered HTML are a delivery snapshot. Moving to
//! a web front end is not a reason to promote them: a component that asks for
//! `--ac` is as wrong as a Rust one that read `theme.fg`.

use crate::color::Rgb;
use crate::theme::Theme;
use crate::tokens::{Density, DensityMode, Tokens, font, text};

/// Every token, as `--name: value;` declarations for a `:root` block.
///
/// Returned as a body rather than a whole rule so the caller decides where it
/// goes — a `<style>` element at start-up, or a `style` attribute on the root
/// when the theme changes without a reload.
pub fn variables(theme: &Theme, density: DensityMode) -> String {
    let t = theme.tokens();
    let metrics = density.metrics();
    let mut css = String::with_capacity(1024);

    for (name, value) in colours(&t) {
        push(&mut css, name, &hex(value));
    }
    for (name, value) in on_filled(&t) {
        push(&mut css, name, &hex(value));
    }
    for (name, value) in lanes(theme) {
        push(&mut css, &name, &hex(value));
    }
    for (name, value) in geometry(&metrics) {
        push(&mut css, name, &format!("{value}px"));
    }
    for (name, value) in type_scale() {
        push(&mut css, name, &format!("{value}px"));
    }

    push(&mut css, "font-ui", &stack(font::UI));
    push(&mut css, "font-mono", &stack(font::MONO));
    css
}

/// The same, plus how large to draw the whole thing.
///
/// The lengths are **not** multiplied. Every number here is the design's, and a
/// token whose value depended on a display setting would stop being one — the
/// next person reading `--text-body` would have no way to tell 13 from 15
/// without knowing what the user had chosen.
///
/// Instead the factor is emitted on its own, and the front end applies it once,
/// at the root, with `zoom`. One place, one mechanism, and every fixed pixel in
/// the stylesheet scales with the tokens rather than drifting away from them —
/// which is what multiplying only the tokens would have done to the sixteen
/// component dimensions that have no token of their own.
pub fn scaled(theme: &Theme, density: DensityMode, scale: f32) -> String {
    let mut css = variables(theme, density);
    push(&mut css, "scale", &format!("{}", round(scale)));
    css
}

/// Two decimals: a scale is a preference, not a measurement.
fn round(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

fn colours(t: &Tokens) -> [(&'static str, Rgb); 18] {
    [
        ("bg", t.bg),
        ("surface", t.surface),
        ("surface-raised", t.surface_raised),
        ("surface-hover", t.surface_hover),
        ("border", t.border),
        ("border-focus", t.border_focus),
        ("text", t.text),
        ("text-muted", t.text_muted),
        ("text-dim", t.text_dim),
        ("accent", t.accent),
        ("danger", t.danger),
        ("success", t.success),
        ("warning", t.warning),
        ("info", t.info),
        ("diff-added", t.diff_added),
        ("diff-added-word", t.diff_added_word),
        ("diff-deleted", t.diff_deleted),
        ("diff-deleted-word", t.diff_deleted_word),
    ]
}

/// What to write *on* a filled surface.
///
/// A filled accent is the one place the interface puts text on something other
/// than the background, and `--bg` is not always the answer: a theme whose
/// accent is dark — Matte Black's, or a light theme's — puts near-black on
/// near-black, which is what made a selected sidebar row unreadable. So the
/// pair is chosen by contrast rather than assumed, which is DESIGN-TOKENS §4.3
/// applied to a case §4.3 did not name.
fn on_filled(t: &Tokens) -> [(&'static str, Rgb); 2] {
    [
        ("on-accent", readable_on(t.accent, t.bg, t.text)),
        ("on-danger", readable_on(t.danger, t.bg, t.text)),
    ]
}

/// Whichever of the two reads better against `surface`.
fn readable_on(surface: Rgb, dark: Rgb, light: Rgb) -> Rgb {
    if crate::oklch::contrast_ratio(dark, surface) >= crate::oklch::contrast_ratio(light, surface) {
        dark
    } else {
        light
    }
}

/// The graph lanes, which never read the theme — only their lightness and
/// chroma do (DESIGN-TOKENS §6).
fn lanes(theme: &Theme) -> Vec<(String, Rgb)> {
    crate::lanes::lane_colors(
        crate::lanes::LANE_COUNT,
        theme.tuning.lane_lightness,
        theme.tuning.lane_chroma,
    )
    .into_iter()
    .enumerate()
    .map(|(index, colour)| (format!("lane-{index}"), colour))
    .collect()
}

fn geometry(d: &Density) -> [(&'static str, f32); 7] {
    [
        ("row-height", d.row_height),
        ("row-padding", d.row_padding),
        ("gap", d.gap),
        ("pad", d.pad),
        ("control-height", d.control_height),
        ("header-height", d.header_height),
        ("line-height", d.line_height),
    ]
}

/// The type scale of DESIGN-TOKENS §8, so a stylesheet never has to name a
/// size. It does not change with density: the two modes change how much room
/// things have, not how big the letters are.
fn type_scale() -> [(&'static str, f32); 5] {
    [
        ("text-title", text::TITLE),
        ("text-panel", text::PANEL),
        ("text-body", text::BODY),
        ("text-meta", text::META),
        ("text-mono", text::MONO),
    ]
}

/// A CSS font stack, quoted where a family name needs it.
fn stack(families: &[&str]) -> String {
    families
        .iter()
        .map(|family| {
            if family.contains(' ') {
                format!("'{family}'")
            } else {
                (*family).to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn push(css: &mut String, name: &str, value: &str) {
    css.push_str("--");
    css.push_str(name);
    css.push_str(": ");
    css.push_str(value);
    css.push_str("; ");
}

fn hex(colour: Rgb) -> String {
    format!("#{:06x}", colour.packed())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedded;

    #[test]
    fn every_token_reaches_the_stylesheet() {
        let css = variables(&embedded::tokyo_night(), DensityMode::Compact);
        // The eighteen colours, the eight lanes, the six metrics, two fonts.
        assert_eq!(css.matches("--").count(), 18 + 2 + 8 + 7 + 5 + 2);
        assert!(css.contains("--surface-raised: #283457;"));
        assert!(css.contains("--lane-0: #"));
        assert!(css.contains("--row-height: 26px;"));
        assert!(css.contains("--line-height: 17px;"));
        // The scale is the same in both modes: density changes how much room
        // things have, not how big the letters are.
        assert!(css.contains("--text-body: 13px;"));
        assert!(css.contains("--text-mono: 12.5px;"));
    }

    #[test]
    fn the_names_are_the_canonical_ones() {
        // DESIGN-TOKENS §1: the long names are the contract. The mock-ups'
        // `--fg` / `--ac` / `--raised` were a delivery snapshot, and moving to
        // CSS is not a reason to promote them.
        let css = variables(&embedded::tokyo_night(), DensityMode::Compact);
        for shorthand in ["--fg:", "--ac:", "--raised:", "--muted:", "--dim:"] {
            assert!(
                !css.contains(shorthand),
                "{shorthand} is the mock-ups' spelling, not the contract's"
            );
        }
        assert!(css.contains("--text-muted:"));
        assert!(css.contains("--accent:"));
    }

    #[test]
    fn a_family_with_a_space_is_quoted() {
        let css = variables(&embedded::tokyo_night(), DensityMode::Compact);
        assert!(
            css.contains("'JetBrains Mono'"),
            "an unquoted two-word family is a CSS syntax error"
        );
    }
}

#[cfg(test)]
mod scaling {
    use super::*;
    use crate::theme::Theme;

    fn tokyo() -> Theme {
        crate::embedded::catalogue()
            .iter()
            .find(|theme| theme.name == "Tokyo Night")
            .expect("the catalogue has Tokyo Night")
            .clone()
    }

    #[test]
    fn the_tokens_are_the_designs_numbers_whatever_the_scale() {
        // A token whose value depended on a display setting would stop being
        // one: the next person reading `--text-body` would have no way to tell
        // 13 from 15 without knowing what the user had chosen.
        for scale in [1.0, 1.15, 2.0] {
            let css = scaled(&tokyo(), DensityMode::Comfortable, scale);
            assert!(css.contains("--text-body: 13px;"), "at {scale}: {css}");
            assert!(css.contains("--text-mono: 12.5px;"), "at {scale}");
            assert!(css.contains("--row-height: 32px;"), "at {scale}");
            assert!(css.contains("--line-height: 20px;"), "at {scale}");
        }
    }

    #[test]
    fn the_scale_crosses_on_its_own() {
        let css = scaled(&tokyo(), DensityMode::Comfortable, 1.15);
        assert!(css.contains("--scale: 1.15;"), "{css}");
        // And the colours are untouched by any of it.
        assert!(css.contains("--surface-raised: #283457;"));
    }

    #[test]
    fn a_scale_is_a_preference_not_a_measurement() {
        let css = scaled(&tokyo(), DensityMode::Comfortable, 1.0 / 3.0);
        assert!(css.contains("--scale: 0.33;"), "{css}");
    }
}

#[cfg(test)]
mod on_a_filled_surface {
    use super::*;
    use crate::oklch::contrast_ratio;

    /// Every theme in the catalogue, because the failure is per-theme: the
    /// pairing that reads on Tokyo Night's accent is the one that does not read
    /// on Matte Black's.
    #[test]
    fn text_on_a_filled_accent_is_the_readable_one_in_every_theme() {
        for theme in crate::embedded::catalogue() {
            let t = theme.tokens();
            for (surface, chosen) in [
                (t.accent, readable_on(t.accent, t.bg, t.text)),
                (t.danger, readable_on(t.danger, t.bg, t.text)),
            ] {
                let other = if chosen == t.bg { t.text } else { t.bg };
                assert!(
                    contrast_ratio(chosen, surface) >= contrast_ratio(other, surface),
                    "{}: picked the worse of the two against {surface:?}",
                    theme.name
                );
            }
        }
    }

    #[test]
    fn a_selected_row_is_legible_in_every_theme() {
        // The bug this pins: a sidebar row filled with the accent, written in
        // `--bg`, came out near-black on near-black. 4.5 is the floor
        // DESIGN-TOKENS §4.3 sets for text.
        for theme in crate::embedded::catalogue() {
            let t = theme.tokens();
            let on = readable_on(t.accent, t.bg, t.text);
            let ratio = contrast_ratio(on, t.accent);
            assert!(
                ratio >= 4.5,
                "{}: text on a filled accent is {ratio:.2}:1",
                theme.name
            );
        }
    }
}

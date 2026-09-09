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
use crate::tokens::{Density, DensityMode, Tokens, font};

/// Every token, as `--name: #rrggbb;` declarations for a `:root` block.
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
    for (name, value) in lanes(theme) {
        push(&mut css, &name, &hex(value));
    }
    for (name, value) in geometry(&metrics) {
        push(&mut css, name, &format!("{value}px"));
    }

    push(&mut css, "font-ui", &stack(font::UI));
    push(&mut css, "font-mono", &stack(font::MONO));
    css
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

fn geometry(d: &Density) -> [(&'static str, f32); 6] {
    [
        ("row-height", d.row_height),
        ("row-padding", d.row_padding),
        ("gap", d.gap),
        ("pad", d.pad),
        ("control-height", d.control_height),
        ("header-height", d.header_height),
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
        assert_eq!(css.matches("--").count(), 18 + 8 + 6 + 2);
        assert!(css.contains("--surface-raised: #283457;"));
        assert!(css.contains("--lane-0: #"));
        assert!(css.contains("--row-height: 26px;"));
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

//! The six tests of DESIGN-TOKENS §10 that hold the theme system.
//!
//! They live here rather than inside a module because they are about the
//! contract as a whole, not about one function. Four of the six are unit tests
//! next to the code they constrain — the tracker's change detection, the lane
//! distances, the whole-palette fallback, and the catalogue's shape. The two
//! that span every theme at once are here:
//!
//! * **Test 1** — every text/background pair actually used in the UI reaches
//!   ≥ 4.5:1, on every embedded theme.
//! * **Test 6** — in light mode, `text_muted` and `text_dim` stay readable.

use omagit_theme::embedded;
use omagit_theme::oklch::{MIN_CONTRAST, contrast_ratio};
use omagit_theme::theme::DIM_MIN_CONTRAST;
use omagit_theme::{Mode, Rgb, Tokens};

/// The text-bearing tokens, paired with the surfaces they are actually drawn
/// on.
///
/// `bg` and `surface` only: those are the two backgrounds text sits on in the
/// mock-ups. On `surface_raised` — a selected row — the design uses full-strength
/// `text`, never `text_muted`, so that pair is not in the UI and is not tested.
/// `border` is not text either; as a graphical object its floor is WCAG 1.4.11's
/// 3:1, checked separately.
fn text_pairs(t: &Tokens) -> Vec<(&'static str, Rgb, &'static str, Rgb)> {
    let surfaces = [("bg", t.bg), ("surface", t.surface)];
    let texts = [
        ("text", t.text),
        ("text_muted", t.text_muted),
        ("accent", t.accent),
        ("danger", t.danger),
        ("success", t.success),
        ("warning", t.warning),
        ("info", t.info),
    ];
    texts
        .iter()
        .flat_map(|(text_name, text)| {
            surfaces
                .iter()
                .map(move |(surface_name, surface)| (*text_name, *text, *surface_name, *surface))
        })
        .collect()
}

/// §10 test 1 — the one that makes the accessibility promise keepable on
/// palettes we do not control.
#[test]
fn every_text_pair_clears_four_and_a_half_to_one() {
    let mut failures = Vec::new();

    for theme in embedded::catalogue() {
        let tokens = theme.tokens();
        for (text_name, text, surface_name, surface) in text_pairs(&tokens) {
            let ratio = contrast_ratio(text, surface);
            if ratio < MIN_CONTRAST {
                failures.push(format!(
                    "  {:<18} {text_name} on {surface_name}: {ratio:.2}:1",
                    theme.name
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} text pairs below {MIN_CONTRAST}:1\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Diff line backgrounds carry text too, so the pairing has to hold there as
/// well — otherwise an added line would be legible everywhere except where it
/// matters.
#[test]
fn text_stays_legible_on_the_diff_surfaces() {
    for theme in embedded::catalogue() {
        let t = theme.tokens();
        for (name, surface) in [
            ("diff_added", t.diff_added),
            ("diff_added_word", t.diff_added_word),
            ("diff_deleted", t.diff_deleted),
            ("diff_deleted_word", t.diff_deleted_word),
        ] {
            let ratio = contrast_ratio(t.text, surface);
            assert!(
                ratio >= MIN_CONTRAST,
                "{}: text on {name} is {ratio:.2}:1",
                theme.name
            );
        }
    }
}

/// A border that has genuinely vanished into its surface.
///
/// Not WCAG 1.4.11's 3:1: `border` comes straight from the palette's `color8`,
/// and DESIGN.md §1 says outright that two tokens may collapse — "on Matte
/// Black, accent and border come close", and selection stays readable because
/// it changes surface and carries a ring. Matte Black measures 1.51:1 here, by
/// design. The floor is set just under that, to catch a border that has
/// actually disappeared rather than to relitigate a design decision.
#[test]
fn borders_do_not_vanish_into_their_surfaces() {
    for theme in embedded::catalogue() {
        let t = theme.tokens();
        for (name, surface) in [("bg", t.bg), ("surface", t.surface)] {
            assert_ne!(t.border, surface, "{}: border equals {name}", theme.name);
            let ratio = contrast_ratio(t.border, surface);
            assert!(
                ratio >= 1.4,
                "{}: border on {name} is {ratio:.2}:1 — invisible",
                theme.name
            );
        }
    }
}

/// §10 test 6 — the case a frozen coefficient breaks, and the reason the
/// mixing proportions are per-theme fields (DESIGN-TOKENS §2.3).
#[test]
fn light_themes_keep_muted_and_dim_readable() {
    let light: Vec<_> = embedded::catalogue()
        .into_iter()
        .filter(|theme| theme.mode() == Mode::Light)
        .collect();
    assert!(!light.is_empty(), "the catalogue must contain light themes");

    for theme in light {
        let t = theme.tokens();
        let muted = contrast_ratio(t.text_muted, t.bg);
        let dim = contrast_ratio(t.text_dim, t.bg);

        assert!(
            muted >= MIN_CONTRAST,
            "{}: text_muted is {muted:.2}:1 — washed out",
            theme.name
        );
        assert!(
            dim >= DIM_MIN_CONTRAST,
            "{}: text_dim is {dim:.2}:1 — below the placeholder floor",
            theme.name
        );
        assert!(
            dim <= muted,
            "{}: dim reads stronger than muted, inverting the hierarchy",
            theme.name
        );
    }
}

/// The same, on every theme rather than only the light ones: the ordering is
/// what the two tokens exist to express.
#[test]
fn the_three_text_weights_stay_ordered_everywhere() {
    for theme in embedded::catalogue() {
        let t = theme.tokens();
        let text = contrast_ratio(t.text, t.bg);
        let muted = contrast_ratio(t.text_muted, t.bg);
        let dim = contrast_ratio(t.text_dim, t.bg);
        assert!(
            text >= muted && muted >= dim,
            "{}: text {text:.2} / muted {muted:.2} / dim {dim:.2} are out of order",
            theme.name
        );
    }
}

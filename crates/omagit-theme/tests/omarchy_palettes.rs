//! Real Omarchy palettes, read as they actually ship.
//!
//! Every other test of the Omarchy reader uses a fixture written by hand from
//! the description in SPEC §6.2 — and that description is wrong. It names a
//! guaranteed key `color8`; no Omarchy theme has one. A fixture written from a
//! document can only ever confirm the document, so M1 passed its tests, shipped,
//! and rejected every palette on the machines the feature exists for.
//!
//! These read the files themselves, copied verbatim into
//! `tests/fixtures/omarchy/`. If Omarchy changes its format, this is what says
//! so — and it says so by failing, rather than by the app quietly showing the
//! wrong colours.

use std::path::PathBuf;

use omagit_theme::{Mode, Rgb};

fn fixture(name: &str) -> (String, String) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/omarchy")
        .join(format!("{name}.colors.toml"));
    let contents = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} is missing: {error}", path.display()));
    (name.to_owned(), contents)
}

fn parse(name: &str) -> omagit_theme::Theme {
    let (name, contents) = fixture(name);
    omagit_theme::omarchy::parse(&name, &contents)
        .unwrap_or_else(|error| panic!("{name} is a real Omarchy palette and must parse: {error}"))
}

#[test]
fn every_shipped_palette_parses() {
    // The whole point: these are not written from the spec, they are copied
    // from `/usr/share/omarchy/themes/`.
    for name in ["catppuccin-latte", "tokyo-night", "matte-black"] {
        let theme = parse(name);
        assert_eq!(theme.name, name);
        assert_ne!(
            theme.inputs.background, theme.inputs.foreground,
            "{name} parsed into something unusable"
        );
    }
}

#[test]
fn the_mid_grey_comes_from_muted() {
    // Omarchy's `tokyo-night` and omagit's embedded Tokyo Night are the same
    // palette, which makes this an equality rather than an opinion: whatever
    // key the mid grey is read from has to produce the value the embedded theme
    // carries as `bright_black`.
    let from_omarchy = parse("tokyo-night");
    let embedded = omagit_theme::catalogue()
        .into_iter()
        .find(|theme| theme.name == "Tokyo Night")
        .expect("Tokyo Night is in the catalogue");

    assert_eq!(
        from_omarchy.inputs.bright_black, embedded.inputs.bright_black,
        "`muted` is the key DESIGN-TOKENS calls `input.bright_black`"
    );
    assert_eq!(from_omarchy.inputs.bright_black, Rgb::hex(0x414868));
}

#[test]
fn a_light_palette_is_read_as_light() {
    let latte = parse("catppuccin-latte");
    assert_eq!(latte.mode(), Mode::Light);
    assert_eq!(latte.inputs.background, Rgb::hex(0xeff1f5));
    assert_eq!(latte.inputs.accent, Rgb::hex(0x1e66f5));
    // The optional status colours of §2.2 are present in the real files, so
    // they are read rather than derived.
    assert_eq!(latte.status.red, Some(Rgb::hex(0xd20f39)));
    assert_eq!(latte.status.green, Some(Rgb::hex(0x40a02b)));
}

#[test]
fn every_shipped_palette_still_meets_the_contrast_floor() {
    // DESIGN-TOKENS §10 test 1 is enforced across the *embedded* catalogue,
    // which omagit controls. These palettes come from outside it, and the
    // promise has to hold on them too — that is what the contrast correction of
    // §4.3 is for.
    use omagit_theme::oklch::{MIN_CONTRAST, contrast_ratio};

    for name in ["catppuccin-latte", "tokyo-night", "matte-black"] {
        let tokens = parse(name).tokens();
        for (label, color) in [
            ("text", tokens.text),
            ("text_muted", tokens.text_muted),
            ("accent", tokens.accent),
            ("danger", tokens.danger),
            ("success", tokens.success),
            ("warning", tokens.warning),
            ("info", tokens.info),
        ] {
            for (surface_name, surface) in [("bg", tokens.bg), ("surface", tokens.surface)] {
                let ratio = contrast_ratio(color, surface);
                assert!(
                    ratio >= MIN_CONTRAST,
                    "{name}: {label} on {surface_name} is {ratio:.2}:1, below {MIN_CONTRAST}:1"
                );
            }
        }
    }
}

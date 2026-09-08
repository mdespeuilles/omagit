//! Perceptual colour operations: OKLCH derivation and contrast correction.
//!
//! Two jobs, both from DESIGN-TOKENS.md:
//!
//! * §4.2 — deriving a status colour a theme did not name, from a hue alone.
//! * §4.3 — the mandatory contrast correction, applied to colours **read** from
//!   a theme as much as to derived ones. Nothing guarantees that an Omarchy
//!   palette provides a `green` legible on its own background.
//!
//! Conversions go through `palette`, so the sRGB ⇄ Oklab maths and the WCAG
//! luminance formula are not reimplemented here. What *is* implemented here is
//! the part `palette` does not do: mapping an out-of-gamut colour back into sRGB
//! by giving up chroma rather than by clamping channels, which would shift the
//! hue.

use palette::color_difference::Wcag21RelativeContrast;
use palette::{FromColor, IntoColor, Oklch as PaletteOklch, Srgb};

use crate::color::Rgb;

/// The contrast floor every text/background pair has to clear
/// (DESIGN-TOKENS §4.3, WCAG 1.4.3 level AA).
pub const MIN_CONTRAST: f32 = 4.5;

/// A colour in OKLCH: lightness 0–1, unbounded chroma, hue in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oklch {
    pub lightness: f32,
    pub chroma: f32,
    pub hue: f32,
}

impl Oklch {
    pub const fn new(lightness: f32, chroma: f32, hue: f32) -> Self {
        Self {
            lightness,
            chroma,
            hue,
        }
    }

    /// Convert to sRGB, giving up chroma until the colour fits in the gamut.
    ///
    /// Clamping the channels instead would be simpler and wrong: it shifts the
    /// hue, so two lanes 45° apart could come out closer than they were meant
    /// to be, which is exactly what the lane distance test guards.
    pub fn to_rgb(self) -> Rgb {
        let mut low = 0.0_f32;
        let mut high = self.chroma;

        if in_gamut(self.lightness, high, self.hue) {
            return quantise(self.lightness, high, self.hue);
        }

        // 12 halvings resolve chroma far below one 8-bit step.
        for _ in 0..12 {
            let mid = f32::midpoint(low, high);
            if in_gamut(self.lightness, mid, self.hue) {
                low = mid;
            } else {
                high = mid;
            }
        }
        quantise(self.lightness, low, self.hue)
    }
}

impl From<Rgb> for Oklch {
    fn from(value: Rgb) -> Self {
        let srgb = Srgb::new(value.r, value.g, value.b).into_format::<f32>();
        let oklch = PaletteOklch::from_color(srgb);
        Self {
            lightness: oklch.l,
            chroma: oklch.chroma,
            hue: oklch.hue.into_positive_degrees(),
        }
    }
}

fn linear(
    lightness: f32,
    chroma: f32,
    hue: f32,
) -> palette::rgb::Rgb<palette::encoding::Srgb, f32> {
    PaletteOklch::new(lightness, chroma, hue).into_color()
}

fn in_gamut(lightness: f32, chroma: f32, hue: f32) -> bool {
    let rgb = linear(lightness, chroma, hue);
    [rgb.red, rgb.green, rgb.blue]
        .iter()
        .all(|channel| (-0.0005..=1.0005).contains(channel))
}

fn quantise(lightness: f32, chroma: f32, hue: f32) -> Rgb {
    let rgb: Srgb<u8> = linear(lightness, chroma, hue).into_format();
    Rgb::new(rgb.red, rgb.green, rgb.blue)
}

/// The WCAG 2.1 contrast ratio between two colours, between 1.0 and 21.0.
pub fn contrast_ratio(a: Rgb, b: Rgb) -> f32 {
    let a = Srgb::new(a.r, a.g, a.b).into_format::<f32>();
    let b = Srgb::new(b.r, b.g, b.b).into_format::<f32>();
    a.relative_contrast(b)
}

/// Push `color`'s lightness until it reaches `minimum` contrast against
/// `background`, keeping its hue and as much of its chroma as the gamut allows.
///
/// Returns the colour unchanged when it already clears the floor — the common
/// case for a well-built theme, and the reason this is a safety net rather than
/// a normaliser. When even pure black or pure white cannot reach the floor
/// (a mid-grey background), it returns the best it found: refusing to render
/// would be worse than rendering the most legible colour available.
pub fn ensure_contrast(color: Rgb, background: Rgb, minimum: f32) -> Rgb {
    if contrast_ratio(color, background) >= minimum {
        return color;
    }

    let source = Oklch::from(color);
    // Move away from the background: darken on a light background, lighten on a
    // dark one. Going the other way could satisfy the ratio while inverting the
    // theme's intent — light text turning dark on a dark background.
    let toward_white = Oklch::from(background).lightness < 0.5;

    let mut best = color;
    let mut best_ratio = contrast_ratio(color, background);
    let mut lightness = source.lightness;

    // 1% steps: fine enough to stop close to the floor rather than overshoot to
    // pure white, coarse enough to terminate quickly.
    for _ in 0..100 {
        lightness = if toward_white {
            (lightness + 0.01).min(1.0)
        } else {
            (lightness - 0.01).max(0.0)
        };

        let candidate = Oklch::new(lightness, source.chroma, source.hue).to_rgb();
        let ratio = contrast_ratio(candidate, background);
        if ratio > best_ratio {
            best = candidate;
            best_ratio = ratio;
        }
        if ratio >= minimum {
            return candidate;
        }
        if lightness <= 0.0 || lightness >= 1.0 {
            break;
        }
    }

    tracing::debug!(
        ?color,
        ?background,
        best_ratio,
        minimum,
        "contrast floor unreachable, using the most legible variant"
    );
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_oklch() {
        for color in [
            Rgb::hex(0x1a1b26),
            Rgb::hex(0xc0caf5),
            Rgb::hex(0xff0000),
            Rgb::hex(0x000000),
            Rgb::hex(0xffffff),
        ] {
            let back = Oklch::from(color).to_rgb();
            let drift = |a: u8, b: u8| (a as i32 - b as i32).abs();
            assert!(
                drift(back.r, color.r) <= 1
                    && drift(back.g, color.g) <= 1
                    && drift(back.b, color.b) <= 1,
                "{color:?} came back as {back:?}"
            );
        }
    }

    #[test]
    fn known_contrast_ratios() {
        let ratio = contrast_ratio(Rgb::hex(0xffffff), Rgb::hex(0x000000));
        assert!(
            (ratio - 21.0).abs() < 0.01,
            "black on white is 21:1, got {ratio}"
        );
        let same = contrast_ratio(Rgb::hex(0x7aa2f7), Rgb::hex(0x7aa2f7));
        assert!(
            (same - 1.0).abs() < 0.001,
            "a colour on itself is 1:1, got {same}"
        );
    }

    #[test]
    fn out_of_gamut_requests_lose_chroma_not_hue() {
        // Chroma 0.4 at hue 145° is far outside sRGB. The result must stay a
        // green, not become a clamped colour with a different hue.
        let requested = Oklch::new(0.74, 0.4, 145.0);
        let landed = Oklch::from(requested.to_rgb());
        assert!(
            landed.chroma < requested.chroma,
            "chroma should be given up"
        );
        let hue_drift = (landed.hue - requested.hue).abs();
        assert!(hue_drift < 6.0, "hue drifted by {hue_drift}°");
    }

    #[test]
    fn correction_leaves_a_legible_colour_alone() {
        let bg = Rgb::hex(0x1a1b26);
        let already_fine = Rgb::hex(0xc0caf5);
        assert_eq!(
            ensure_contrast(already_fine, bg, MIN_CONTRAST),
            already_fine
        );
    }

    #[test]
    fn correction_lifts_an_illegible_colour_over_the_floor() {
        let bg = Rgb::hex(0x1a1b26);
        // A dark red on a dark background: unreadable as given.
        let too_dark = Rgb::hex(0x5a1020);
        assert!(contrast_ratio(too_dark, bg) < MIN_CONTRAST);

        let fixed = ensure_contrast(too_dark, bg, MIN_CONTRAST);
        assert!(
            contrast_ratio(fixed, bg) >= MIN_CONTRAST,
            "still {} after correction",
            contrast_ratio(fixed, bg)
        );
        // And it is still recognisably the same hue.
        let hue_drift = (Oklch::from(fixed).hue - Oklch::from(too_dark).hue).abs();
        assert!(hue_drift < 15.0, "hue drifted by {hue_drift}°");
    }

    #[test]
    fn correction_moves_away_from_the_background_on_a_light_theme() {
        let bg = Rgb::hex(0xfaf4ed);
        let too_pale = Rgb::hex(0xe8dcd0);
        let fixed = ensure_contrast(too_pale, bg, MIN_CONTRAST);
        assert!(
            Oklch::from(fixed).lightness < Oklch::from(too_pale).lightness,
            "on a light background the colour must darken"
        );
        assert!(contrast_ratio(fixed, bg) >= MIN_CONTRAST);
    }
}

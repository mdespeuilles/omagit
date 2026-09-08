//! The colour type used across the theme contract.
//!
//! Deliberately minimal and UI-free: `omagit-theme` must stay testable without a
//! window (SPEC §6). `omagit-ui` converts these into the renderer's own type.

use std::fmt;

/// An opaque 8-bit-per-channel sRGB colour.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Build from a packed `0xRRGGBB` literal.
    pub const fn hex(value: u32) -> Self {
        Self::new(
            ((value >> 16) & 0xff) as u8,
            ((value >> 8) & 0xff) as u8,
            (value & 0xff) as u8,
        )
    }

    /// Parse `#RRGGBB` or `RRGGBB`. Nothing else is accepted: a theme file that
    /// uses another notation is invalid, and invalid means whole-palette fallback
    /// (DESIGN-TOKENS §2.1), never a guess.
    pub fn parse(text: &str) -> Result<Self, ParseColorError> {
        let hex = text.trim().strip_prefix('#').unwrap_or(text.trim());
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ParseColorError(text.to_owned()));
        }
        u32::from_str_radix(hex, 16)
            .map(Self::hex)
            .map_err(|_| ParseColorError(text.to_owned()))
    }

    pub const fn packed(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
    }
}

impl fmt::Debug for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:06x}", self.packed())
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:06x}", self.packed())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("`{0}` is not a #RRGGBB colour")]
pub struct ParseColorError(String);

/// `mix(a, b, p)` is **p percent of `a`**, the rest `b`, in sRGB.
///
/// Exact CSS equivalent: `color-mix(in srgb, a p%, b)`. DESIGN-TOKENS §3 is
/// explicit that this is the inverse of the ambiguous wording in the first spec
/// draft, so the argument order is load-bearing — see the tests.
pub fn mix(a: Rgb, b: Rgb, p: Percent) -> Rgb {
    let t = p.as_fraction();
    let channel = |a: u8, b: u8| {
        (a as f32 * t + b as f32 * (1.0 - t))
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Rgb::new(channel(a.r, b.r), channel(a.g, b.g), channel(a.b, b.b))
}

/// A mixing proportion, stored as percent so theme files read like the contract.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct Percent(pub f32);

impl Percent {
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    pub fn as_fraction(self) -> f32 {
        (self.0 / 100.0).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_hex_spellings() {
        assert_eq!(Rgb::parse("#1a1b26").unwrap(), Rgb::hex(0x1a1b26));
        assert_eq!(Rgb::parse(" 1a1b26 ").unwrap(), Rgb::hex(0x1a1b26));
    }

    #[test]
    fn rejects_anything_else() {
        for bad in ["#1a1b2", "#1a1b2g", "rgb(1,2,3)", "", "#1a1b26ff"] {
            assert!(Rgb::parse(bad).is_err(), "{bad} should not parse");
        }
    }

    #[test]
    fn mix_takes_p_percent_of_the_first_argument() {
        let white = Rgb::hex(0xffffff);
        let black = Rgb::hex(0x000000);
        // 100% of a, none of b.
        assert_eq!(mix(white, black, Percent(100.0)), white);
        assert_eq!(mix(white, black, Percent(0.0)), black);
        // 65% of white over black is light, not dark. This is the assertion that
        // catches an inverted mix, which is the failure DESIGN-TOKENS §3 warns about.
        assert_eq!(mix(white, black, Percent(65.0)), Rgb::hex(0xa6a6a6));
    }
}

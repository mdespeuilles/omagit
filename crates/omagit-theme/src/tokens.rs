//! The token vocabulary, under the canonical names of DESIGN-TOKENS.md.
//!
//! These names are the contract. The delivered HTML mock-ups use CSS shorthands
//! (`--fg`, `--ac`, `--raised`); those are a delivery snapshot, never the names
//! used here or in components (SPEC §3 rule 4).

use crate::color::Rgb;

/// The fourteen semantic colour tokens, plus the four diff surfaces derived from
/// the status tints. No component may invent a colour outside this struct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tokens {
    // Structure — DESIGN-TOKENS §4.1
    pub bg: Rgb,
    pub surface: Rgb,
    pub surface_raised: Rgb,
    pub surface_hover: Rgb,
    pub border: Rgb,
    pub border_focus: Rgb,
    pub text: Rgb,
    pub text_muted: Rgb,
    pub text_dim: Rgb,
    pub accent: Rgb,

    // Status — DESIGN-TOKENS §4.2
    pub danger: Rgb,
    pub success: Rgb,
    pub warning: Rgb,
    pub info: Rgb,

    // Diff surfaces — DESIGN-TOKENS §5
    pub diff_added: Rgb,
    pub diff_added_word: Rgb,
    pub diff_deleted: Rgb,
    pub diff_deleted_word: Rgb,
}

/// Light or dark. Drives the default tuning coefficients (DESIGN-TOKENS §2.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Dark,
    Light,
}

/// The status-tint scale. Every tinted surface in the app goes through it;
/// no component invents its own percentage (DESIGN-TOKENS §5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tint {
    /// 12% — diff line backgrounds.
    Subtle,
    /// 16% — badges, pills, status chips.
    Medium,
    /// 24% — intra-line word highlighting.
    Strong,
}

impl Tint {
    pub const fn percent(self) -> f32 {
        match self {
            Tint::Subtle => 12.0,
            Tint::Medium => 16.0,
            Tint::Strong => 24.0,
        }
    }
}

/// Row and control metrics. Values are tokens, not component constants
/// (DESIGN-TOKENS §7).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Density {
    pub row_height: f32,
    pub row_padding: f32,
    pub gap: f32,
    pub pad: f32,
    pub control_height: f32,
    pub header_height: f32,
}

impl Density {
    pub const COMPACT: Self = Self {
        row_height: 26.0,
        row_padding: 6.0,
        gap: 4.0,
        pad: 6.0,
        control_height: 22.0,
        header_height: 20.0,
    };

    pub const COMFORTABLE: Self = Self {
        row_height: 32.0,
        row_padding: 8.0,
        gap: 8.0,
        pad: 10.0,
        control_height: 26.0,
        header_height: 24.0,
    };

    /// The accessibility floor held in both modes: a click target is never
    /// smaller than 24×24px, so in compact the hit area extends past the drawn
    /// control (DESIGN-TOKENS §7).
    pub const MIN_HIT_TARGET: f32 = 24.0;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DensityMode {
    /// The default on Omarchy, where the app lives beside a dense terminal.
    #[default]
    Compact,
    Comfortable,
}

impl DensityMode {
    pub const fn metrics(self) -> Density {
        match self {
            DensityMode::Compact => Density::COMPACT,
            DensityMode::Comfortable => Density::COMFORTABLE,
        }
    }
}

/// Typography — DESIGN-TOKENS §8. The mock-ups list `'Segoe UI Variable Text'`
/// in the UI stack; Windows is out of scope, so it is dropped here.
pub mod font {
    /// Every Git literal — hashes, branch names, paths, diff content, remote
    /// URLs — is set in the mono stack.
    pub const MONO: &[&str] = &["JetBrains Mono", "ui-monospace", "monospace"];
    pub const UI: &[&str] = &["system-ui", "-apple-system", "Inter", "sans-serif"];
}

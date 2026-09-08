//! omagit's theme system — DESIGN-TOKENS.md, implemented.
//!
//! This crate has no UI dependency, which is what makes it testable on its own
//! (SPEC §6). It owns the token vocabulary; `omagit-ui` owns its projection into
//! the renderer.
//!
//! ## Milestone status
//!
//! M0 ships the token vocabulary, the sRGB `mix` derivation of §4.1/§5, the
//! density and typography scales, and two embedded themes. The rest of §6 —
//! the four theme sources, OKLCH status derivation, contrast correction, the
//! lane generator, live Omarchy tracking, macOS appearance tracking and the
//! full catalogue — is M1.

pub mod color;
pub mod embedded;
pub mod theme;
pub mod tokens;

pub use color::{Percent, Rgb, mix};
pub use embedded::catalogue;
pub use theme::{Inputs, StatusColors, Theme, Tuning};
pub use tokens::{Density, DensityMode, Mode, Tint, Tokens, font};

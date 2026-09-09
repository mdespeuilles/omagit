//! omagit's theme system — DESIGN-TOKENS.md, implemented.
//!
//! This crate has no UI dependency, which is what makes it testable on its own
//! (SPEC §6). It owns the token vocabulary; `omagit-ui` owns its projection into
//! the renderer.
//!
//! The pieces, in the order DESIGN-TOKENS.md introduces them:
//!
//! * [`color`] — the sRGB type and `mix`, whose argument order is load-bearing.
//! * [`css`] — the tokens as CSS custom properties, for the web front end.
//! * [`oklch`] — perceptual derivation and the mandatory contrast correction.
//! * [`theme`] — inputs, per-theme tuning, and the derivation into tokens.
//! * [`tokens`] — the canonical vocabulary, plus density and typography.
//! * [`lanes`] — commit-graph lanes, generated and never read from a palette.
//! * [`embedded`] — the catalogue: six dark themes, two light.
//! * [`omarchy`] — reading and tracking an Omarchy Quattro palette.
//! * [`source`] — the four sources of §6.1, resolved in priority order.

pub mod color;
pub mod css;
pub mod embedded;
pub mod lanes;
pub mod oklch;
pub mod omarchy;
pub mod source;
pub mod theme;
pub mod tokens;

pub use color::{Percent, Rgb, mix};
pub use embedded::catalogue;
pub use lanes::{LANE_COUNT, lane_colors};
pub use oklch::{Oklch, contrast_ratio};
pub use source::{Resolved, Sources, ThemeSource};
pub use theme::{Inputs, NamedStatus, Theme, Tuning};
pub use tokens::{Density, DensityMode, Mode, Tint, Tokens, font};

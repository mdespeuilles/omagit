//! omagit's application components, built on the vendored `gpui-omarchy`.
//!
//! The two pieces every screen sits on: the bridge that puts `omagit-theme` in
//! charge of the renderer's palette — tokens, density and graph lanes — and the
//! resolution of the typography stacks. The screens themselves — diff view,
//! commit graph, file tree, command palette — arrive with their milestones.

pub mod fonts;
pub mod theme_bridge;

pub use fonts::{ActiveFonts, Fonts};
pub use theme_bridge::{ActivePalette, Palette, apply, hsla};

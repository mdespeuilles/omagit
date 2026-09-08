//! omagit's application components, built on the vendored `gpui-omarchy`.
//!
//! The pieces every screen sits on: the bridge that puts `omagit-theme` in
//! charge of the renderer's palette — tokens, density and graph lanes — the
//! resolution of the typography stacks, the icon set, and the atoms of
//! DESIGN.md board 01 at the sizes the screens use.
//!
//! The heavy components — diff view, commit graph, file tree, command palette —
//! arrive with their milestones. Screens themselves live in `omagit-app`, which
//! owns state and routing (SPEC §7); what lives here is what more than one of
//! them will draw.

pub mod diff_view;
pub mod fonts;
pub mod icons;
pub mod primitives;
pub mod syntax;
pub mod theme_bridge;

pub use diff_view::DiffView;
pub use fonts::{ActiveFonts, Fonts};
pub use icons::Icon;
pub use syntax::{Highlighter, Role};
pub use theme_bridge::{ActivePalette, Palette, apply, hsla};

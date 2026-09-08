//! omagit's icons, compiled in.
//!
//! Embedded with `include_bytes!` and handed to the renderer as SVG bytes,
//! rather than resolved through an asset source at run time. Two reasons: an
//! icon that fails to load is a blank square nobody notices until a screenshot,
//! and a single binary with no data directory is what SPEC §9's packaging wants
//! on both platforms.
//!
//! They are 16×16, drawn on a 16-unit grid with a 1.5px stroke and no fill —
//! the "linear glyphs" of DESIGN §2. They inherit the text colour of wherever
//! they are drawn, which is what keeps them in the palette without any of them
//! naming a token.
//!
//! DESIGN §4 asks two of them to carry meaning by *shape*, not by colour: a
//! repository is a bound rectangle, a group is a folder, and a repository that
//! is no longer on disk is the same bound rectangle struck through. That is why
//! [`Icon::RepositoryMissing`] exists as its own glyph rather than as a colour
//! applied to [`Icon::Repository`].

use gpui_kit::{IntoElement, Styled, svg};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// A repository: a bound rectangle.
    Repository,
    /// A repository whose folder is gone, struck through.
    RepositoryMissing,
    /// A user group: a folder.
    Group,
    Search,
    ChevronDown,
    ChevronRight,
    Plus,
    Alert,
}

impl Icon {
    fn data(self) -> &'static [u8] {
        match self {
            Icon::Repository => include_bytes!("../../../assets/icons/repository.svg"),
            Icon::RepositoryMissing => {
                include_bytes!("../../../assets/icons/repository-missing.svg")
            }
            Icon::Group => include_bytes!("../../../assets/icons/group.svg"),
            Icon::Search => include_bytes!("../../../assets/icons/search.svg"),
            Icon::ChevronDown => include_bytes!("../../../assets/icons/chevron-down.svg"),
            Icon::ChevronRight => include_bytes!("../../../assets/icons/chevron-right.svg"),
            Icon::Plus => include_bytes!("../../../assets/icons/plus.svg"),
            Icon::Alert => include_bytes!("../../../assets/icons/alert.svg"),
        }
    }

    /// The glyph at `size`, in the colour of whatever draws it.
    ///
    /// The colour has to be set explicitly: an `svg` with no text colour paints
    /// nothing at all, which is a blank square rather than an error.
    pub fn render(self, size: gpui_kit::Pixels, color: gpui_kit::Hsla) -> impl IntoElement {
        svg()
            .data(self.data())
            .size(size)
            .flex_none()
            .text_color(color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_is_a_stroked_sixteen_grid_glyph() {
        // A fill or a stroke width that drifts is invisible in review and
        // obvious on screen next to its neighbours.
        for icon in [
            Icon::Repository,
            Icon::RepositoryMissing,
            Icon::Group,
            Icon::Search,
            Icon::ChevronDown,
            Icon::ChevronRight,
            Icon::Plus,
            Icon::Alert,
        ] {
            let source = std::str::from_utf8(icon.data()).expect("an icon is text");
            assert!(
                source.contains(r#"viewBox="0 0 16 16""#),
                "{icon:?} is not on the 16-unit grid"
            );
            assert!(
                source.contains(r#"stroke-width="1.5""#),
                "{icon:?} does not use the 1.5px stroke of DESIGN §2"
            );
            assert!(
                source.contains(r#"stroke="currentColor""#),
                "{icon:?} does not inherit its colour, so it cannot follow the palette"
            );
            assert!(
                source.contains(r#"fill="none""#),
                "{icon:?} is filled; the icons are linear"
            );
        }
    }

    #[test]
    fn a_missing_repository_is_the_repository_glyph_struck_through() {
        // DESIGN §4: shape distinguishes as much as colour, so the two must
        // share their outline and differ only by the cross.
        let repository = std::str::from_utf8(Icon::Repository.data()).expect("text");
        let missing = std::str::from_utf8(Icon::RepositoryMissing.data()).expect("text");
        for path in repository
            .lines()
            .filter(|line| line.trim_start().starts_with("<path"))
        {
            assert!(
                missing.contains(path.trim()),
                "the struck-through glyph dropped {path:?} instead of adding to it"
            );
        }
        assert!(
            missing.matches("<path").count() > repository.matches("<path").count(),
            "and it adds the cross"
        );
    }
}

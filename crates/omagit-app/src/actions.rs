//! Named actions and their default bindings.
//!
//! Actions rather than inline key handlers, for three reasons that all arrive
//! later: M9 makes the keymap reassignable and needs names to reassign, the
//! command palette lists actions by name, and the macOS menu bar (SPEC §9)
//! dispatches them. Naming them now costs nothing and means none of that is a
//! rewrite.
//!
//! The bindings live here rather than in the screen so that
//! `docs/KEYMAP.md` has one file to correspond to.

use gpui_kit::{App, KeyBinding, actions};

actions!(
    omagit,
    [
        /// Add a repository already on disk.
        AddLocalRepository,
        /// Clone a URL. The network arrives at M7; until then it says so.
        CloneRepository,
        /// The command palette. M9.
        OpenCommandPalette,
        /// Put the caret in the sidebar's filter box.
        FocusFilter,
        /// Zone jumps. The same three keys mean the same three zones on every
        /// screen (DESIGN §5).
        FocusSidebar,
        FocusCentre,
        FocusDetail,
        /// Move through the screen's tab stops.
        FocusNext,
        FocusPrevious,
        /// Move within the focused zone.
        SelectNext,
        SelectPrevious,
        /// Fold and unfold the group the selection is in.
        CollapseGroup,
        ExpandGroup,
        /// Open the selected repository.
        Confirm,
        /// Up one level: overlay → zone → the sidebar.
        Cancel,
        /// Take the selected repository out of the list.
        RemoveSelected,
        /// Start a new group.
        NewGroup,
        /// Re-read every repository.
        RefreshAll,
    ]
);

/// The key context the bindings below are scoped to. A binding with no context
/// would fire on every screen.
pub const CONTEXT: &str = "Repositories";

/// The same screen, but only while no text field has the caret.
///
/// A single character is a *binding* to someone navigating and a *character* to
/// someone typing, and there is no way to tell them apart except by asking
/// where the focus is. `Input` is the context `gpui-base`'s editor puts on
/// itself. Without this, `/` then `j` filters for nothing and moves the
/// selection — which is exactly what `tests/keyboard.rs` caught.
const OUTSIDE_TEXT: &str = "Repositories && !Input";

/// Install the default keymap.
///
/// `secondary-` resolves to `cmd` on macOS and `ctrl` elsewhere, which is what
/// SPEC §9 asks for and what keeps this file free of a `cfg`.
pub fn bind(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-o", AddLocalRepository, Some(CONTEXT)),
        KeyBinding::new("secondary-shift-n", CloneRepository, Some(CONTEXT)),
        KeyBinding::new("secondary-k", OpenCommandPalette, Some(CONTEXT)),
        KeyBinding::new("secondary-r", RefreshAll, Some(CONTEXT)),
        KeyBinding::new("secondary-backspace", RemoveSelected, Some(CONTEXT)),
        KeyBinding::new("/", FocusFilter, Some(OUTSIDE_TEXT)),
        KeyBinding::new("1", FocusSidebar, Some(OUTSIDE_TEXT)),
        KeyBinding::new("2", FocusCentre, Some(OUTSIDE_TEXT)),
        KeyBinding::new("3", FocusDetail, Some(OUTSIDE_TEXT)),
        KeyBinding::new("tab", FocusNext, Some(CONTEXT)),
        KeyBinding::new("shift-tab", FocusPrevious, Some(CONTEXT)),
        KeyBinding::new("down", SelectNext, Some(CONTEXT)),
        KeyBinding::new("up", SelectPrevious, Some(CONTEXT)),
        // The vim letters of DESIGN §5, alongside the arrows rather than
        // instead of them — and only outside a text field, where they are keys
        // rather than letters.
        KeyBinding::new("j", SelectNext, Some(OUTSIDE_TEXT)),
        KeyBinding::new("k", SelectPrevious, Some(OUTSIDE_TEXT)),
        KeyBinding::new("h", CollapseGroup, Some(OUTSIDE_TEXT)),
        KeyBinding::new("l", ExpandGroup, Some(OUTSIDE_TEXT)),
        // The arrows stay available everywhere: up and down move the list while
        // the caret is in the filter box, which is how filtering is meant to be
        // used. Left and right are the editor's when it has focus — its own
        // bindings sit closer to the focus and win.
        KeyBinding::new("left", CollapseGroup, Some(CONTEXT)),
        KeyBinding::new("right", ExpandGroup, Some(CONTEXT)),
        KeyBinding::new("enter", Confirm, Some(CONTEXT)),
        KeyBinding::new("escape", Cancel, Some(CONTEXT)),
        KeyBinding::new("secondary-g", NewGroup, Some(CONTEXT)),
    ]);
}

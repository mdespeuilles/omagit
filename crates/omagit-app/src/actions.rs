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
        /// Open the Working Copy of the selected repository, and come back.
        ShowWorkingCopy,
        ShowRepositories,
        ShowHistory,
        /// Unified ↔ side-by-side.
        ToggleDiffMode,
        /// Move the diff cursor a line, or a hunk (DESIGN board 09).
        NextDiffLine,
        PreviousDiffLine,
        NextHunk,
        PreviousHunk,
        /// Pick the changed line under the diff cursor.
        ToggleLinePick,
        /// Pick every changed line of the hunk the cursor is in.
        ToggleHunkPick,
        /// Move what is picked — or the whole file, when nothing is — between
        /// the working copy and the index.
        StagePicked,
        UnstagePicked,
        /// Undo it in the working tree. Destructive; confirmed first.
        DiscardPicked,
        /// Stage every unstaged file (DESIGN board 09).
        StageAll,
        /// Commit what is staged.
        Commit,
        /// The three flags of SPEC §11, each a deliberate act rather than a
        /// setting that stays on.
        ToggleAmend,
        ToggleSignOff,
        ToggleNoVerify,
        /// The operations journal (SPEC §11).
        ToggleJournal,
    ]
);

/// The key context the bindings below are scoped to. A binding with no context
/// would fire on every screen.
pub const CONTEXT: &str = "Repositories";

/// The Working Copy screen's own context.
pub const CONTEXT_WORKING_COPY: &str = "WorkingCopy";

/// The History screen's own context.
pub const CONTEXT_HISTORY: &str = "History";

/// The same screen, but only while no text field has the caret.
///
/// A single character is a *binding* to someone navigating and a *character* to
/// someone typing, and there is no way to tell them apart except by asking
/// where the focus is. `Input` is the context `gpui-base`'s editor puts on
/// itself. Without this, `/` then `j` filters for nothing and moves the
/// selection — which is exactly what `tests/keyboard.rs` caught.
const OUTSIDE_TEXT: &str = "Repositories && !Input";
const WORKING_COPY_OUTSIDE_TEXT: &str = "WorkingCopy && !Input";
const HISTORY_OUTSIDE_TEXT: &str = "History && !Input";

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
        // Working Copy. The movement keys are deliberately the same letters as
        // on Repositories: DESIGN §5 wants one vocabulary across the screens,
        // not one per screen.
        KeyBinding::new("down", SelectNext, Some(CONTEXT_WORKING_COPY)),
        KeyBinding::new("up", SelectPrevious, Some(CONTEXT_WORKING_COPY)),
        KeyBinding::new("j", SelectNext, Some(WORKING_COPY_OUTSIDE_TEXT)),
        KeyBinding::new("k", SelectPrevious, Some(WORKING_COPY_OUTSIDE_TEXT)),
        KeyBinding::new("secondary-r", RefreshAll, Some(CONTEXT_WORKING_COPY)),
        KeyBinding::new("secondary-d", ToggleDiffMode, Some(CONTEXT_WORKING_COPY)),
        KeyBinding::new("escape", ShowRepositories, Some(CONTEXT_WORKING_COPY)),
        // The diff, from DESIGN board 09: "j k ligne · ⌥ j k hunk suivant /
        // précédent · Space sélectionne les lignes · ⌥S indexe le hunk
        // focalisé · ⌥D le rejette".
        //
        // `alt-` rather than `secondary-`: these act on the diff under the
        // cursor, and holding them apart from the screen-wide `⌘` commands is
        // what keeps "stage this hunk" from being one slip away from
        // "refresh everything".
        KeyBinding::new("alt-j", NextDiffLine, Some(WORKING_COPY_OUTSIDE_TEXT)),
        KeyBinding::new("alt-k", PreviousDiffLine, Some(WORKING_COPY_OUTSIDE_TEXT)),
        KeyBinding::new("alt-down", NextDiffLine, Some(CONTEXT_WORKING_COPY)),
        KeyBinding::new("alt-up", PreviousDiffLine, Some(CONTEXT_WORKING_COPY)),
        KeyBinding::new("alt-shift-j", NextHunk, Some(WORKING_COPY_OUTSIDE_TEXT)),
        KeyBinding::new("alt-shift-k", PreviousHunk, Some(WORKING_COPY_OUTSIDE_TEXT)),
        KeyBinding::new("space", ToggleLinePick, Some(WORKING_COPY_OUTSIDE_TEXT)),
        KeyBinding::new("alt-space", ToggleHunkPick, Some(CONTEXT_WORKING_COPY)),
        KeyBinding::new("alt-s", StagePicked, Some(CONTEXT_WORKING_COPY)),
        KeyBinding::new("alt-u", UnstagePicked, Some(CONTEXT_WORKING_COPY)),
        KeyBinding::new("alt-d", DiscardPicked, Some(CONTEXT_WORKING_COPY)),
        // Commit is the one write with a `⌘`: it is the screen's primary
        // action, and DESIGN board 03 draws it as ⌘⏎.
        KeyBinding::new("a", StageAll, Some(WORKING_COPY_OUTSIDE_TEXT)),
        KeyBinding::new("secondary-enter", Commit, Some(CONTEXT_WORKING_COPY)),
        KeyBinding::new("secondary-shift-a", ToggleAmend, Some(CONTEXT_WORKING_COPY)),
        KeyBinding::new(
            "secondary-shift-s",
            ToggleSignOff,
            Some(CONTEXT_WORKING_COPY),
        ),
        KeyBinding::new(
            "secondary-shift-v",
            ToggleNoVerify,
            Some(CONTEXT_WORKING_COPY),
        ),
        KeyBinding::new("secondary-l", ToggleJournal, Some(CONTEXT_WORKING_COPY)),
        // `2` is the zone key DESIGN §5 gives the centre column on every
        // screen; from the Working Copy, History is what it opens.
        KeyBinding::new("secondary-2", ShowHistory, Some(CONTEXT_WORKING_COPY)),
        // History. The same movement vocabulary again (DESIGN §5): one set of
        // letters across the screens, not one per screen.
        KeyBinding::new("down", SelectNext, Some(CONTEXT_HISTORY)),
        KeyBinding::new("up", SelectPrevious, Some(CONTEXT_HISTORY)),
        KeyBinding::new("j", SelectNext, Some(HISTORY_OUTSIDE_TEXT)),
        KeyBinding::new("k", SelectPrevious, Some(HISTORY_OUTSIDE_TEXT)),
        KeyBinding::new("escape", ShowWorkingCopy, Some(CONTEXT_HISTORY)),
    ]);
}

//! Everything that differs between Linux and macOS.
//!
//! The UI layer never carries a `#[cfg(target_os)]` (SPEC §3 rule 6); it asks
//! this module instead. The trait exists because the two platforms already
//! diverge, not to leave room for a third (SPEC §2).

use std::path::PathBuf;

use gpui_kit::Pixels;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

/// The horizontal bands of the topbar that belong to the window system.
///
/// These are **reserves, not margins** (DESIGN-TOKENS §9): they are laid out as
/// flex spacers, so nothing else shifts when they change. A control placed at
/// 12px from the left edge would end up under the macOS traffic lights.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TopbarReserve {
    pub leading: Pixels,
    pub trailing: Pixels,
}

/// Which modifier the platform spells as "primary" (SPEC §9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimaryModifier {
    Control,
    Command,
}

impl PrimaryModifier {
    /// How the shortcut reads in the interface. The docs say ⌘/Ctrl; the app
    /// shows one or the other, never both (DESIGN.md §2, "un seul modificateur").
    pub fn label(self) -> &'static str {
        match self {
            PrimaryModifier::Control => "Ctrl",
            PrimaryModifier::Command => "⌘",
        }
    }
}

/// The platform-dependent facts the app needs to open its first window.
pub trait Platform: Send + Sync + 'static {
    fn name(&self) -> &'static str;

    /// Where settings, keymap and persisted layout live (SPEC §9).
    fn config_dir(&self) -> Option<PathBuf>;

    fn primary_modifier(&self) -> PrimaryModifier;

    /// The platform's own UI family, standing in for the CSS generic
    /// `system-ui` at the head of the typography stack (DESIGN-TOKENS §8).
    /// The font list never enumerates it by name.
    fn system_ui_family(&self) -> &'static str;

    /// The bands the window system draws over.
    fn topbar_reserve(&self) -> TopbarReserve;

    /// Titlebar configuration for the main window.
    fn titlebar(&self) -> Option<gpui_kit::TitlebarOptions>;

    /// Client- or server-side decorations. Only Wayland/X11 read this.
    fn window_decorations(&self) -> Option<gpui_kit::WindowDecorations>;

    /// The credential helper Git is expected to use (SPEC §9). Reported at
    /// start-up so a missing helper is visible rather than a silent auth
    /// failure later.
    fn credential_helper(&self) -> &'static str;
}

/// The platform this build runs on.
pub fn current() -> &'static dyn Platform {
    #[cfg(target_os = "linux")]
    {
        &linux::Linux
    }
    #[cfg(target_os = "macos")]
    {
        &macos::MacOs
    }
}

/// The topbar is 48px tall in comfortable density, 40px in compact
/// (DESIGN-TOKENS §7, mock-up 08).
pub const TOPBAR_HEIGHT_COMFORTABLE: f32 = 48.0;
pub const TOPBAR_HEIGHT_COMPACT: f32 = 40.0;

//! Everything that differs between Linux and macOS.
//!
//! The trait exists because the two platforms already diverge, not to leave
//! room for a third (SPEC §2). It shrank when the interface moved to the web:
//! window decoration is `tauri.conf.json`'s business now, and the typography
//! stacks resolve themselves — `system-ui` is a real CSS keyword, so the
//! resolution `omagit-ui::fonts` had to do by hand is gone.
//!
//! What is left is what the *front end* cannot know: where config lives, which
//! modifier the platform calls primary, the band the window system draws over,
//! and where Omarchy keeps its theme.

use std::path::PathBuf;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

/// The horizontal bands of the topbar that belong to the window system.
///
/// **Reserves, not margins** (DESIGN-TOKENS §9): the front end lays them out as
/// flex spacers, so nothing else shifts when they change. A control placed at
/// 12px from the left edge would end up under the macOS traffic lights.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct TopbarReserve {
    pub leading: f32,
    pub trailing: f32,
}

/// Which modifier the platform spells as "primary" (SPEC §9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PrimaryModifier {
    Control,
    Command,
}

impl PrimaryModifier {
    /// How the shortcut reads in the interface. The docs say ⌘/Ctrl; the app
    /// shows one or the other, never both (DESIGN §2).
    pub fn label(self) -> &'static str {
        match self {
            PrimaryModifier::Control => "Ctrl",
            PrimaryModifier::Command => "⌘",
        }
    }
}

/// What the front end needs to know about where it is running.
#[derive(Debug, serde::Serialize)]
pub struct PlatformFacts {
    pub name: &'static str,
    pub modifier: PrimaryModifier,
    pub modifier_label: &'static str,
    pub reserve: TopbarReserve,
    pub credential_helper: &'static str,
    /// The user's home directory, empty when the environment does not say.
    ///
    /// Reported rather than guessed from a path's shape, because the clone
    /// dialog has to *propose* a destination before there is any path to guess
    /// from — and `~/src` is where board 06 puts one.
    pub home: String,
}

pub trait Platform: Send + Sync + 'static {
    fn name(&self) -> &'static str;

    /// Where settings, keymap and persisted layout live (SPEC §9).
    fn config_dir(&self) -> Option<PathBuf>;

    fn primary_modifier(&self) -> PrimaryModifier;

    /// The bands the window system draws over.
    fn topbar_reserve(&self) -> TopbarReserve;

    /// The credential helper Git is expected to use (SPEC §9). Reported at
    /// start-up so a missing helper is visible rather than a silent auth
    /// failure later.
    fn credential_helper(&self) -> &'static str;

    /// The Omarchy Quattro `current` directory, when the platform has one.
    ///
    /// `None` on macOS: there is no Omarchy there, so the source is not offered
    /// rather than offered and permanently failing (SPEC §6.1).
    fn omarchy_state_dir(&self) -> Option<PathBuf>;

    /// Whether the platform reports a light/dark preference worth following.
    fn reports_system_appearance(&self) -> bool;

    fn facts(&self) -> PlatformFacts {
        PlatformFacts {
            name: self.name(),
            modifier: self.primary_modifier(),
            modifier_label: self.primary_modifier().label(),
            reserve: self.topbar_reserve(),
            credential_helper: self.credential_helper(),
            home: std::env::var("HOME")
                .ok()
                .or_else(|| std::env::var("USERPROFILE").ok())
                .unwrap_or_default(),
        }
    }
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
/// (DESIGN-TOKENS §7, board 08).
pub const TOPBAR_HEIGHT_COMFORTABLE: f32 = 48.0;
pub const TOPBAR_HEIGHT_COMPACT: f32 = 40.0;

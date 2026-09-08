//! macOS — the distribution target.

use std::path::PathBuf;

use gpui_kit::{Pixels, TitlebarOptions, WindowDecorations, px};

use super::{Platform, PrimaryModifier, TopbarReserve};

pub struct MacOs;

/// The system draws the traffic lights over the topbar. The first 78 pixels are
/// off limits — no control ever enters them (DESIGN-TOKENS §9, SPEC §9).
const TRAFFIC_LIGHT_RESERVE: f32 = 78.0;

impl Platform for MacOs {
    fn name(&self) -> &'static str {
        "macos"
    }

    fn config_dir(&self) -> Option<PathBuf> {
        std::env::var_os("HOME")
            .filter(|home| !home.is_empty())
            .map(|home| PathBuf::from(home).join("Library/Application Support/omagit"))
    }

    fn primary_modifier(&self) -> PrimaryModifier {
        PrimaryModifier::Command
    }

    fn system_ui_family(&self) -> &'static str {
        // AppKit's system face, as CoreText names it. "SF Pro" is not a family
        // the text system resolves.
        ".SystemUIFont"
    }

    fn topbar_reserve(&self) -> TopbarReserve {
        TopbarReserve {
            leading: px(TRAFFIC_LIGHT_RESERVE),
            trailing: Pixels::ZERO,
        }
    }

    fn titlebar(&self) -> Option<TitlebarOptions> {
        Some(TitlebarOptions {
            title: Some("omagit".into()),
            // Hide the system titlebar and draw our own topbar under the
            // traffic lights, which stay system-drawn.
            appears_transparent: true,
            traffic_light_position: None,
        })
    }

    fn window_decorations(&self) -> Option<WindowDecorations> {
        // Wayland/X11 only; macOS ignores it.
        None
    }

    fn credential_helper(&self) -> &'static str {
        "osxkeychain"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn the_traffic_light_band_is_reserved_on_the_leading_edge() {
        let reserve = MacOs.topbar_reserve();
        assert_eq!(reserve.leading, px(78.0));
        assert_eq!(reserve.trailing, Pixels::ZERO);
    }

    #[test]
    fn the_titlebar_is_transparent_so_the_topbar_can_be_drawn() {
        let titlebar = MacOs.titlebar().expect("macOS configures a titlebar");
        assert!(titlebar.appears_transparent);
    }
}

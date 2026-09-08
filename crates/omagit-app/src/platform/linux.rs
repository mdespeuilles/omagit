//! Linux — Wayland primary, X11 as the fallback. The design target.

use std::path::PathBuf;

use gpui_kit::{Pixels, TitlebarOptions, WindowDecorations};

use super::{Platform, PrimaryModifier, TopbarReserve};

pub struct Linux;

impl Platform for Linux {
    fn name(&self) -> &'static str {
        "linux"
    }

    fn config_dir(&self) -> Option<PathBuf> {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .filter(|home| !home.is_empty())
                    .map(|home| PathBuf::from(home).join(".config"))
            })
            .map(|dir| dir.join("omagit"))
    }

    fn primary_modifier(&self) -> PrimaryModifier {
        PrimaryModifier::Control
    }

    fn system_ui_family(&self) -> &'static str {
        // fontconfig resolves the generic to whatever the session configures as
        // its interface face, which is exactly what `system-ui` means here.
        "sans-serif"
    }

    fn topbar_reserve(&self) -> TopbarReserve {
        // Tiled under Hyprland the compositor owns close and resize, so the
        // reserve is zero. The conditional 115px band — three 38px caption
        // buttons plus a 1px separator (DESIGN-TOKENS §9) — arrives with the
        // topbar itself, once the app reads `gtk-decoration-layout` to know
        // which buttons to draw and on which side.
        TopbarReserve {
            leading: Pixels::ZERO,
            trailing: Pixels::ZERO,
        }
    }

    fn titlebar(&self) -> Option<TitlebarOptions> {
        Some(TitlebarOptions {
            title: Some("omagit".into()),
            appears_transparent: true,
            traffic_light_position: None,
        })
    }

    fn window_decorations(&self) -> Option<WindowDecorations> {
        // The compositor draws neither titlebar nor buttons; the app does.
        Some(WindowDecorations::Client)
    }

    fn credential_helper(&self) -> &'static str {
        "libsecret"
    }

    fn omarchy_state_dir(&self) -> Option<PathBuf> {
        omagit_theme::omarchy::state_dir_from_env()
    }

    fn reports_system_appearance(&self) -> bool {
        // Desktop portals do expose a colour-scheme preference, but omagit does
        // not read it: on Linux the Omarchy palette is the system theme, and a
        // second, competing notion of light/dark would only be able to disagree
        // with it. Falls through to the embedded default when Omarchy is absent.
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn config_dir_prefers_xdg_config_home() {
        // Reading the process environment, so this stays a single test.
        let previous = std::env::var_os("XDG_CONFIG_HOME");
        // SAFETY: single-threaded test, and the value is restored below.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", "/tmp/xdg") };
        assert_eq!(Linux.config_dir(), Some(PathBuf::from("/tmp/xdg/omagit")));

        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        unsafe { std::env::set_var("HOME", "/home/tester") };
        assert_eq!(
            Linux.config_dir(),
            Some(PathBuf::from("/home/tester/.config/omagit"))
        );

        match previous {
            Some(value) => unsafe { std::env::set_var("XDG_CONFIG_HOME", value) },
            None => unsafe { std::env::remove_var("XDG_CONFIG_HOME") },
        }
    }

    #[test]
    fn linux_asks_for_client_side_decorations() {
        assert_eq!(Linux.window_decorations(), Some(WindowDecorations::Client));
    }
}

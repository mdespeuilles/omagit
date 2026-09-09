//! Linux — Wayland primary, X11 as the fallback. The design target.

use std::path::PathBuf;

use super::{Caption, CaptionSide, Platform, PrimaryModifier, TopbarReserve};

pub struct Linux;

impl Linux {
    /// The rule itself, split from the environment that answers it so both
    /// branches can be read — and tested — without touching the process.
    fn caption_under(hyprland: bool) -> Caption {
        Caption {
            minimize: !hyprland,
            maximize: !hyprland,
            close: true,
            side: CaptionSide::Trailing,
        }
    }
}

impl Platform for Linux {
    fn name(&self) -> &'static str {
        "linux"
    }

    /// Turns off WebKit's DMABUF renderer on the NVIDIA proprietary driver.
    ///
    /// That path hands the compositor a buffer NVIDIA's EGL cannot share, and
    /// Wayland answers by killing the connection the instant the window is
    /// mapped — `Gdk-Message: Error 71 (Protocol error)`, no window, no useful
    /// log. WebKit then falls back to a software composite: slower, but a
    /// running window beats a fast one that never appears.
    ///
    /// Narrow on purpose. Only the proprietary module is affected — nouveau,
    /// Intel and AMD keep the accelerated path — and an explicit setting in the
    /// environment always wins, so the workaround can be undone from the shell
    /// the day WebKit or NVIDIA fixes it.
    fn prepare_display(&self) -> Option<&'static str> {
        const VAR: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";

        if std::env::var_os(VAR).is_some()
            || std::env::var_os("WAYLAND_DISPLAY").is_none()
            || !std::path::Path::new("/sys/module/nvidia_drm").exists()
        {
            return None;
        }

        // SAFETY: single-threaded — this runs before `run()` starts anything.
        unsafe { std::env::set_var(VAR, "1") };
        Some("nvidia under wayland: WebKit's DMABUF renderer disabled")
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

    fn topbar_reserve(&self) -> TopbarReserve {
        // Zero on both edges, and it stays zero now that the caption buttons
        // exist: a reserve is a band the *window system* draws over, and Linux
        // draws nothing. Board 02's 115px is the width those buttons take up as
        // real elements at the trailing edge, not a hole left for someone else.
        TopbarReserve {
            leading: 0.0,
            trailing: 0.0,
        }
    }

    fn credential_helper(&self) -> &'static str {
        "libsecret"
    }

    /// Board 02's three, minus what the compositor already owns.
    ///
    /// Under Hyprland only **close** is drawn. The board hides all three when
    /// tiled, on the grounds that the compositor handles closing and sizing from
    /// the keyboard; that is true of sizing — a tiled window has no size of its
    /// own to minimise or maximise — but a window with no visible way out reads
    /// as stuck, so close stays. Everywhere else the three appear.
    ///
    /// `HYPRLAND_INSTANCE_SIGNATURE` is the test because Hyprland sets it in
    /// every process it starts; nothing else does.
    ///
    /// The side is the trailing edge, always. Board 02 wants it read from
    /// `gtk-decoration-layout` — a GTK setting, so it needs a handle on the
    /// initialised toolkit rather than an environment variable, and it changes
    /// the answer only for a session that has moved its buttons to the left.
    /// Left for the day that session exists.
    fn caption(&self) -> Caption {
        Linux::caption_under(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some())
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
    fn hyprland_keeps_only_the_way_out() {
        let tiling = Linux::caption_under(true);
        assert_eq!(
            tiling,
            Caption {
                minimize: false,
                maximize: false,
                close: true,
                side: CaptionSide::Trailing,
            }
        );

        // Everywhere else the window is a window: board 02's three, on the
        // right.
        let desktop = Linux::caption_under(false);
        assert!(desktop.minimize && desktop.maximize && desktop.close);
        assert_eq!(desktop.side, CaptionSide::Trailing);
    }

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
}

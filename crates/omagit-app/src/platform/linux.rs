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

    /// Brings `GDK_SCALE` down to the largest screen Hyprland drives, when the
    /// window is about to go through XWayland.
    ///
    /// The AppImage forces `GDK_BACKEND=x11` — linuxdeploy's GTK hook does it,
    /// for tauri-apps/tauri#8541 — and Hyprland with
    /// `xwayland:force_zero_scaling` draws X11 windows unscaled, leaving the
    /// scale to `GDK_SCALE`. Omarchy sets that to 2 whatever the screen, so on a
    /// monitor at scale 1 every pixel of the interface came out doubled.
    ///
    /// Only ever lowered, and only past the largest screen rounded up: a
    /// HiDPI setup keeps exactly what it asked for, a 1.5 screen keeps 2. Off
    /// Hyprland, or when the compositor does not answer, nothing changes.
    fn fit_gdk_scale() -> Option<String> {
        let x11 = std::env::var("GDK_BACKEND")
            .is_ok_and(|backend| backend.split(',').next().map(str::trim) == Some("x11"));
        let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
        if !x11 || std::env::var_os("WAYLAND_DISPLAY").is_none() {
            return None;
        }
        let asked: u32 = std::env::var("GDK_SCALE").ok()?.trim().parse().ok()?;

        let socket = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?)
            .join("hypr")
            .join(signature)
            .join(".socket.sock");
        let fitted = Linux::fitted_gdk_scale(asked, &hyprland_monitors(&socket)?)?;

        // SAFETY: single-threaded — this runs before `run()` starts anything.
        unsafe { std::env::set_var("GDK_SCALE", fitted.to_string()) };
        Some(format!(
            "xwayland on hyprland: GDK_SCALE {asked} lowered to {fitted}, the largest screen"
        ))
    }

    /// The rule, apart from the environment: `Some` only when `asked` is
    /// above every screen in Hyprland's `j/monitors` answer.
    fn fitted_gdk_scale(asked: u32, monitors: &str) -> Option<u32> {
        let monitors: Vec<serde_json::Value> = serde_json::from_str(monitors).ok()?;
        let largest = monitors
            .iter()
            .filter_map(|monitor| monitor.get("scale")?.as_f64())
            .fold(None, |max: Option<f64>, scale| {
                Some(max.map_or(scale, |max| max.max(scale)))
            })?;
        let ceiling = (largest.ceil() as u32).max(1);
        (asked > ceiling).then_some(ceiling)
    }
}

/// Asks Hyprland's IPC socket for its monitors. Bounded, because this runs
/// before there is a window and a stuck compositor must not become a stuck
/// start.
fn hyprland_monitors(socket: &std::path::Path) -> Option<String> {
    use std::io::{Read, Write};

    let timeout = Some(std::time::Duration::from_millis(500));
    let mut stream = std::os::unix::net::UnixStream::connect(socket).ok()?;
    stream.set_read_timeout(timeout).ok()?;
    stream.set_write_timeout(timeout).ok()?;
    stream.write_all(b"j/monitors").ok()?;
    let mut answer = String::new();
    stream.read_to_string(&mut answer).ok()?;
    Some(answer)
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
    ///
    /// Also holds `GDK_SCALE` to the screens Hyprland has: see
    /// [`Linux::fit_gdk_scale`].
    fn prepare_display(&self) -> Vec<String> {
        const VAR: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
        let mut notes = Vec::new();

        if std::env::var_os(VAR).is_none()
            && std::env::var_os("WAYLAND_DISPLAY").is_some()
            && std::path::Path::new("/sys/module/nvidia_drm").exists()
        {
            // SAFETY: single-threaded — this runs before `run()` starts anything.
            unsafe { std::env::set_var(VAR, "1") };
            notes.push("nvidia under wayland: WebKit's DMABUF renderer disabled".into());
        }

        notes.extend(Linux::fit_gdk_scale());
        notes
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

    fn opener(&self) -> &'static str {
        "xdg-open"
    }

    fn omarchy_state_dir(&self) -> Option<PathBuf> {
        omagit_theme::omarchy::state_dir_from_env()
    }

    /// None (SPEC §9): everything is in the window here, and a menu bar would
    /// be a second place to find it.
    fn native_menus(&self) -> bool {
        false
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
    fn gdk_scale_is_held_to_the_largest_screen() {
        let one = r#"[{"name":"HDMI-A-1","scale":1.0}]"#;
        let mixed = r#"[{"scale":1.0},{"scale":1.5}]"#;
        let hidpi = r#"[{"scale":2.0}]"#;

        // Omarchy's 2 on a plain screen: the bug.
        assert_eq!(Linux::fitted_gdk_scale(2, one), Some(1));
        // A screen at 1.5 rounds up, so Omarchy's 2 stands.
        assert_eq!(Linux::fitted_gdk_scale(2, mixed), None);
        assert_eq!(Linux::fitted_gdk_scale(3, mixed), Some(2));
        // What already fits is left alone, and nothing is ever raised.
        assert_eq!(Linux::fitted_gdk_scale(2, hidpi), None);
        assert_eq!(Linux::fitted_gdk_scale(1, hidpi), None);
        // An answer that says nothing usable changes nothing.
        assert_eq!(Linux::fitted_gdk_scale(2, "[]"), None);
        assert_eq!(Linux::fitted_gdk_scale(2, "unknown request"), None);
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

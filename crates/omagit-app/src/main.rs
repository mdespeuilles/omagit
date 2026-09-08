//! omagit — a Git client for Omarchy and macOS.

use anyhow::{Context as _, Result};
use gpui_kit::App;
use gpui_kit::prelude::*;

use omagit_app::platform;
use omagit_app::theme_runtime;
use omagit_app::window::{Shell, options};
use omagit_settings::Settings;

fn main() {
    let platform = platform::current();
    let config_dir = platform.config_dir();
    let _log_guard = omagit_app::logging::init(config_dir.as_deref());

    tracing::info!(
        platform = platform.name(),
        config_dir = ?config_dir,
        credential_helper = platform.credential_helper(),
        "omagit starting"
    );

    gpui_kit::application().run(move |cx: &mut App| {
        // The thread that runs this closure owns the windows. Registering it
        // arms the debug guard that panics if Git work is ever started here
        // (SPEC §15 risk 1).
        omagit_git::mark_render_thread();

        // `gpui_omarchy::init` also starts its own once-a-second Omarchy poll.
        // Applying a theme below stops it: omagit owns theme sourcing (SPEC
        // §6.1), and on macOS there is no Omarchy state to poll at all.
        gpui_omarchy::init(cx);

        // Resolve the two typography stacks against the families this machine
        // actually has, before anything asks for one (DESIGN-TOKENS §8).
        omagit_ui::fonts::resolve(platform.system_ui_family(), cx);

        let settings = match config_dir.as_deref() {
            Some(dir) => Settings::load(dir),
            None => {
                tracing::warn!("no config directory for this platform, using defaults");
                Settings::default()
            }
        };
        let density = settings.density;

        // Resolves the four sources of SPEC §6.1 in priority order, applies the
        // winner, and starts following it when it is a live one.
        let tracking = theme_runtime::install(platform, settings, cx);
        tracing::info!(tracking = tracking.label(), "theme tracking");

        if let Err(error) = open_main_window(density, cx) {
            tracing::error!(%error, "could not open the main window");
            cx.quit();
        }
    });
}

fn open_main_window(density: omagit_theme::DensityMode, cx: &mut App) -> Result<()> {
    let platform = platform::current();
    let options = options(platform, cx);
    cx.open_window(options, |window, cx| {
        // Following the system appearance is a per-window subscription: the
        // window is what the platform reports light/dark through.
        let appearance = theme_runtime::follow_window_appearance(window);
        cx.new(|_| Shell::new(density, appearance))
    })
    .context("open_window failed")?;
    Ok(())
}

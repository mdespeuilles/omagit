//! omagit — a Git client for Omarchy and macOS.

use anyhow::{Context as _, Result};
use gpui_kit::App;
use gpui_kit::prelude::*;

use omagit_app::platform;
use omagit_app::window::{Shell, options};
use omagit_settings::{Settings, ThemeChoice};
use omagit_theme::{Mode, embedded};

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
        // `omagit_ui::apply` below stops it: omagit owns theme sourcing, and on
        // macOS there is no Omarchy state to poll at all.
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

        let theme = resolve_theme(&settings.theme);
        tracing::info!(theme = %theme.name, density = ?settings.density, "applying theme");
        omagit_ui::apply(&theme, settings.density, cx);

        if let Err(error) = open_main_window(settings.density, cx) {
            tracing::error!(%error, "could not open the main window");
            cx.quit();
        }
    });
}

fn open_main_window(density: omagit_theme::DensityMode, cx: &mut App) -> Result<()> {
    let platform = platform::current();
    let options = options(platform, cx);
    cx.open_window(options, |_, cx| cx.new(|_| Shell::new(density)))
        .context("open_window failed")?;
    Ok(())
}

/// Resolve the configured choice into a theme.
///
/// M0 knows only the embedded catalogue. The other three sources of SPEC §6.1 —
/// the user override, Omarchy Quattro, and the macOS system appearance — arrive
/// at M1, which is why `FollowSystem` lands on the embedded default here.
fn resolve_theme(choice: &ThemeChoice) -> omagit_theme::Theme {
    match choice {
        ThemeChoice::Named { name } => embedded::catalogue()
            .into_iter()
            .find(|theme| theme.name.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| {
                tracing::warn!(%name, "unknown theme, falling back to the embedded default");
                embedded::default_for(Mode::Dark)
            }),
        ThemeChoice::Embedded { mode } => embedded::default_for((*mode).into()),
        ThemeChoice::FollowSystem => embedded::default_for(Mode::Dark),
    }
}

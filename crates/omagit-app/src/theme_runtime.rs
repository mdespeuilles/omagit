//! Keeping the applied theme in step with its source.
//!
//! Two live sources, and neither may do its work on the render thread:
//!
//! * **Omarchy Quattro** (Linux) — the state directory is watched on its own
//!   thread, and every event is funnelled through [`omarchy::Tracker`], which
//!   answers the only question that matters: did the palette *actually* change?
//!   Filesystem watchers report differently on inotify and FSEvents and fire
//!   several times for one logical change; the tracker makes that irrelevant.
//! * **The system appearance** (macOS) — the window reports light/dark changes,
//!   including the automatic switch at dusk.
//!
//! A user override switches both off by construction: [`Sources::resolve`]
//! never consults them for [`ThemeSource::UserOverride`], and no watcher is
//! started for a source that is not selected.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use gpui_kit::{App, Global, Subscription, Task, Window};
use notify::{RecursiveMode, Watcher};

use omagit_settings::Settings;
use omagit_theme::{Mode, Resolved, Sources, Theme, ThemeSource, omarchy};

use crate::platform::Platform;

/// How long to wait for a burst of filesystem events to settle. Matches the
/// watcher debounce of SPEC §10, for the same reason: one save produces several
/// events, and a theme switch produces several more.
const DEBOUNCE: Duration = Duration::from_millis(150);

/// The applied theme and everything needed to re-resolve it.
pub struct ThemeRuntime {
    pub settings: Settings,
    pub sources: Sources,
    pub resolved: Resolved,
    /// What is actually being followed. Computed once, here, so the UI reports
    /// the truth rather than re-deriving it from "is Omarchy available", which
    /// is a different question.
    pub tracking: Tracking,
    /// Dropped on quit, which closes the channel and ends the watcher thread.
    _watch: Option<Task<()>>,
}

impl Global for ThemeRuntime {}

/// What `install` ended up doing, for the start-up log and the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tracking {
    Omarchy,
    SystemAppearance,
    /// An explicit theme, or a machine with nothing to follow.
    None,
}

impl Tracking {
    pub fn label(self) -> &'static str {
        match self {
            Tracking::Omarchy => "Omarchy (live)",
            Tracking::SystemAppearance => "apparence système",
            Tracking::None => "figé",
        }
    }
}

/// Read the environment, resolve the configured source, and apply the result.
pub fn install(platform: &dyn Platform, settings: Settings, cx: &mut App) -> Tracking {
    let sources = Sources {
        omarchy_state: platform.omarchy_state_dir(),
        system_appearance: platform
            .reports_system_appearance()
            .then(|| mode_of(cx.window_appearance())),
    };

    let resolved = sources.resolve(&settings.theme);
    if let Some(requested) = &resolved.fell_back_from {
        tracing::warn!(?requested, applied = %resolved.theme.name, "theme source unavailable");
    }
    tracing::info!(
        theme = %resolved.theme.name,
        source = ?resolved.source,
        omarchy = sources.omarchy_available(),
        appearance = ?sources.system_appearance,
        "resolved theme"
    );

    omagit_ui::apply(&resolved.theme, settings.density, cx);

    let watch = watch_omarchy(&sources, &resolved, cx);
    let tracking = if watch.is_some() {
        Tracking::Omarchy
    } else if sources.system_appearance.is_some()
        && matches!(resolved.source, ThemeSource::SystemAppearance)
    {
        Tracking::SystemAppearance
    } else {
        Tracking::None
    };

    cx.set_global(ThemeRuntime {
        settings,
        sources,
        resolved,
        tracking,
        _watch: watch,
    });
    tracking
}

/// Re-resolve the configured source and re-apply if the palette actually moved.
pub fn reapply(cx: &mut App) {
    let Some(runtime) = cx.try_global::<ThemeRuntime>() else {
        return;
    };
    let requested = runtime.settings.theme.clone();
    let density = runtime.settings.density;
    let sources = runtime.sources.clone();

    let resolved = sources.resolve(&requested);
    if resolved.theme == runtime.resolved.theme {
        return;
    }
    tracing::info!(theme = %resolved.theme.name, "theme changed");
    omagit_ui::apply(&resolved.theme, density, cx);
    cx.global_mut::<ThemeRuntime>().resolved = resolved;
}

/// Follow the window's appearance. Called once per window.
///
/// It only reapplies when the configured source actually depends on the
/// appearance: an explicit theme must not move when the OS switches to dark.
pub fn follow_window_appearance(window: &mut Window) -> Subscription {
    window.observe_window_appearance(|window, cx| {
        let appearance = mode_of(window.appearance());
        let Some(runtime) = cx.try_global::<ThemeRuntime>() else {
            return;
        };
        if runtime.sources.system_appearance == Some(appearance) {
            return;
        }
        tracing::info!(?appearance, "system appearance changed");
        cx.global_mut::<ThemeRuntime>().sources.system_appearance = Some(appearance);
        reapply(cx);
    })
}

fn mode_of(appearance: gpui_kit::WindowAppearance) -> Mode {
    use gpui_kit::WindowAppearance::{Dark, Light, VibrantDark, VibrantLight};
    match appearance {
        Dark | VibrantDark => Mode::Dark,
        Light | VibrantLight => Mode::Light,
    }
}

/// Start watching the Quattro state, if the applied theme actually came from it.
fn watch_omarchy(sources: &Sources, resolved: &Resolved, cx: &mut App) -> Option<Task<()>> {
    // An explicit theme disables tracking (SPEC §6.1): no watcher, no thread.
    if !matches!(resolved.source, ThemeSource::Omarchy) {
        return None;
    }
    let current = sources.omarchy_state.clone()?;

    // The watcher thread pushes real changes across; the foreground task
    // applies them. `AsyncApp` is not `Send`, which is why the split exists.
    let (tx, rx) = async_channel::unbounded::<Theme>();
    let seed = resolved.theme.clone();
    let watch_path = current.clone();
    std::thread::Builder::new()
        .name("omagit-omarchy-watch".into())
        .spawn(move || watch_thread(watch_path, seed, tx))
        .inspect_err(|error| tracing::warn!(%error, "could not start the Omarchy watcher"))
        .ok()?;

    Some(cx.spawn(async move |cx| {
        // `recv` fails once the sender is gone, which is how this task ends.
        // The task itself is dropped on quit, so `update` is never reached
        // after the app is torn down.
        while let Ok(theme) = rx.recv().await {
            cx.update(|cx| {
                tracing::info!(theme = %theme.name, "Omarchy palette changed");
                reapply(cx);
            });
        }
    }))
}

/// Owns the filesystem watcher for the life of the app.
///
/// Everything here is off the render thread, so blocking is fine and the code
/// stays straightforward. It ends when `tx` can no longer be sent on — that is,
/// when the foreground task is dropped.
pub(crate) fn watch_thread(current: PathBuf, seed: Theme, tx: async_channel::Sender<Theme>) {
    let (events_tx, events) = mpsc::channel();
    let mut watcher = match notify::recommended_watcher(move |event| {
        let _ = events_tx.send(event);
    }) {
        Ok(watcher) => watcher,
        Err(error) => {
            tracing::warn!(%error, "no filesystem watcher available; Omarchy will not be followed");
            return;
        }
    };

    // Two paths, because one theme change can arrive as either.
    //
    // Omarchy switches themes by replacing the `current` symlink, which shows
    // up as a change in its *parent* directory, not in `current` itself.
    // Editing a palette in place shows up under the resolved theme directory
    // instead. The second watch is re-established after every change, since
    // replacing the symlink makes the previous resolved path stale.
    if let Some(parent) = current.parent()
        && let Err(error) = watcher.watch(parent, RecursiveMode::NonRecursive)
    {
        tracing::warn!(path = %parent.display(), %error, "cannot watch the Omarchy state directory");
        return;
    }
    let mut theme_dir = watch_theme_dir(&mut watcher, &current, None);

    let mut tracker = omarchy::Tracker::seeded_with(seed);
    while events.recv().is_ok() {
        // Let the burst settle, then drain it: one logical change, one refresh.
        while events.recv_timeout(DEBOUNCE).is_ok() {}

        if let Some(theme) = tracker.refresh(&current) {
            theme_dir = watch_theme_dir(&mut watcher, &current, theme_dir);
            if tx.send_blocking(theme).is_err() {
                return;
            }
        }
    }
}

/// Point the second watch at the theme directory `current` resolves to now,
/// dropping the previous one. Failure is not fatal: the symlink watch alone
/// still catches theme switches, which is how Omarchy changes themes.
fn watch_theme_dir(
    watcher: &mut notify::RecommendedWatcher,
    current: &Path,
    previous: Option<PathBuf>,
) -> Option<PathBuf> {
    let resolved = current.join("theme");
    if previous.as_deref() == Some(resolved.as_path()) {
        return previous;
    }
    if let Some(previous) = previous {
        let _ = watcher.unwatch(&previous);
    }
    match watcher.watch(&resolved, RecursiveMode::NonRecursive) {
        Ok(()) => Some(resolved),
        Err(error) => {
            tracing::debug!(path = %resolved.display(), %error, "theme directory not watchable");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use std::time::Instant;

    /// Two real palettes, not two written from the description of the format.
    ///
    /// The hand-written pair these replace used `color8`, which no Omarchy theme
    /// has — the mistake that made M1's Omarchy support inert for two
    /// milestones (`docs/ARCHITECTURE.md` §2.9). A watcher test that seeds an
    /// unparseable palette tests nothing, and this one would not have noticed.
    const ONE: &str = include_str!("../../../tests/fixtures/omarchy/tokyo-night.colors.toml");
    const TWO: &str = include_str!("../../../tests/fixtures/omarchy/matte-black.colors.toml");

    fn write_theme(dir: &Path, name: &str, colors: &str) {
        std::fs::create_dir_all(dir.join("theme")).expect("create");
        std::fs::write(dir.join("theme.name"), name).expect("name");
        std::fs::write(dir.join("theme/colors.toml"), colors).expect("colors");
    }

    /// Drives the real filesystem watcher, on whichever backend this platform
    /// uses. SPEC §10 warns that inotify and FSEvents do not report the same
    /// events; this asserts the outcome the app depends on rather than the
    /// events themselves, so it is meaningful on both.
    ///
    /// The `Tracker`'s own dedup logic is unit-tested in `omagit-theme`; what is
    /// under test here is the wiring: watched paths, debounce, and the handover
    /// to the foreground.
    #[test]
    fn replacing_the_current_symlink_delivers_one_theme() {
        let root = tempfile::tempdir().expect("tempdir");
        let one = root.path().join("one");
        let two = root.path().join("two");
        write_theme(&one, "One", ONE);
        write_theme(&two, "Two", TWO);

        let current = root.path().join("current");
        std::os::unix::fs::symlink(&one, &current).expect("symlink");

        let seed = omarchy::read(&current).expect("seed palette");
        assert_eq!(seed.name, "One");

        let (tx, rx) = async_channel::unbounded();
        let watch_path = current.clone();
        let watcher = std::thread::spawn(move || watch_thread(watch_path, seed, tx));

        // Give the watcher a moment to establish itself before changing
        // anything, or the change races the registration.
        std::thread::sleep(Duration::from_millis(400));

        std::fs::remove_file(&current).expect("unlink");
        std::os::unix::fs::symlink(&two, &current).expect("relink");

        // Polled rather than blocked on: a watcher that never fires must fail
        // the test, not hang it.
        let deadline = Instant::now() + Duration::from_secs(10);
        let received = loop {
            match rx.try_recv() {
                Ok(theme) => break theme,
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => panic!("no theme delivered within 10s of replacing the symlink"),
            }
        };
        assert_eq!(received.name, "Two", "the new palette should arrive");

        // And nothing further, since nothing else changed. The debounce plus the
        // tracker is what collapses a burst of events into this one delivery.
        std::thread::sleep(Duration::from_millis(500));
        assert!(rx.is_empty(), "a single change must not deliver twice");

        // The watcher thread is left to be reaped when the test binary exits.
        // Joining it would mean waking it from `events.recv()` first, which is
        // more choreography than the thing under test is worth.
        drop(watcher);
    }
}

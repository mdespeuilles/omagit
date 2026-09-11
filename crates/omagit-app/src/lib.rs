//! omagit's backend: the Git core, exposed to a web front end through Tauri.
//!
//! The window is HTML; everything it asks for is answered here. What crosses is
//! in [`dto`] and is shaped for the wire, not mirrored from `omagit-git` —
//! keeping that crate free of any interface concern is the rule (SPEC §3 rule 5)
//! that made moving off GPUI a rewrite of the interface rather than of the
//! project.

pub mod agent;
pub mod commands;
pub mod dto;
pub mod editor;
pub mod edits;
pub mod log;
pub mod logging;
pub mod menu;
pub mod platform;
pub mod state;

use state::AppState;

/// Build and run the application.
pub fn run() {
    let platform = platform::current();
    let display_note = platform.prepare_display();
    let config_dir = platform.config_dir();
    let _log_guard = logging::init(config_dir.as_deref());

    if let Some(note) = display_note {
        tracing::info!("{note}");
    }

    tracing::info!(
        platform = platform.name(),
        config_dir = ?config_dir,
        credential_helper = platform.credential_helper(),
        "omagit starting"
    );

    // Before the window exists, which is where SPEC §8 wants the `git` check.
    let state = AppState::detect(config_dir);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        // A menu item does not act; it names an action, and the front end runs
        // it — the same table, the same three rules, whether the action was
        // reached by a key, by the palette or from up here (SPEC §9).
        .on_menu_event(|app, event| {
            use tauri::Emitter as _;
            let _ = app.emit("menu", event.id().as_ref());
        })
        .setup(|app| {
            // Before anything can be opened: a repository opened without the
            // window to report to would be one that never refreshes itself.
            {
                use tauri::Manager as _;
                app.state::<AppState>().attach(app.handle().clone());
            }
            if let Some(dir) = platform::current().omarchy_state_dir() {
                follow_system_palette(app.handle().clone(), dir);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::platform,
            commands::set_menu,
            commands::theme,
            commands::preferences,
            commands::set_theme,
            commands::set_density,
            commands::set_scale,
            commands::panes,
            commands::set_pane,
            commands::keymap,
            commands::set_binding,
            commands::language,
            commands::set_language,
            commands::agents,
            commands::agent_chosen,
            commands::set_agent,
            commands::set_agent_guidelines,
            commands::draft_message,
            commands::git_status,
            commands::repositories,
            commands::add_repository,
            commands::forget_repository,
            commands::touch_repository,
            commands::create_group,
            commands::rename_group,
            commands::remove_group,
            commands::collapse_group,
            commands::move_group,
            commands::move_repository,
            commands::close_repository,
            commands::summary,
            commands::status,
            commands::file_diff,
            commands::stage,
            commands::stage_all,
            commands::discard,
            commands::committer,
            commands::commit_template,
            commands::commit,
            commands::head_message,
            commands::history,
            commands::history_more,
            commands::commit_detail,
            commands::commit_file_diff,
            commands::compare,
            commands::compare_file_diff,
            commands::refs,
            commands::checkout,
            commands::create_branch,
            commands::delete_branch,
            commands::fetch,
            commands::pull,
            commands::pull_reconcile_configured,
            commands::push,
            commands::clone_directory,
            commands::check_remote,
            commands::clone_repository,
            commands::cancel_operation,
            commands::merge,
            commands::rebase,
            commands::abort_operation,
            commands::conflict_sides,
            commands::conflict_file,
            commands::resolve_hunks,
            commands::open_in_editor,
            commands::resolve_conflict,
            commands::continue_operation,
            commands::stashes,
            commands::stash_files,
            commands::stash_file_diff,
            commands::stash_push,
            commands::stash_restore,
            commands::stash_drop,
            commands::journal,
            commands::log,
        ])
        .run(tauri::generate_context!())
        .expect("omagit could not start");
}

/// Follow the Omarchy palette while the window is open (SPEC §6.1, source 2).
///
/// The spec calls that source *watched*, and `omarchy::Tracker` was written to
/// serve it, but nothing ever drove it: the window read the palette once at
/// start-up and then ignored the system for the rest of the session.
///
/// Watches the `current` directory, not the files under it, because switching a
/// theme replaces the whole thing — `omarchy-theme-set` does `rm -rf theme`, a
/// rename over it, then writes `theme.name`. A watch registered on `colors.toml`
/// would be watching a file that no longer exists, and the burst of events that
/// swap produces is exactly what the tracker is for: it answers "is the palette
/// we can read *now* different from the last one we read", rather than
/// "did something happen".
///
/// The stylesheet is re-resolved rather than rendered from the new palette
/// directly, because Omarchy is only one of SPEC §6.1's four sources: with a
/// user override in settings the palette may change and the window must not.
/// Comparing the result to what was last sent is what makes that free.
fn follow_system_palette(app: tauri::AppHandle, dir: std::path::PathBuf) {
    use notify::Watcher as _;
    use tauri::{Emitter as _, Manager as _};

    std::thread::spawn(move || {
        let (sender, events) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(sender) {
            Ok(watcher) => watcher,
            Err(error) => {
                tracing::warn!(%error, "no theme watcher: the palette will not follow");
                return;
            }
        };
        if let Err(error) = watcher.watch(&dir, notify::RecursiveMode::NonRecursive) {
            // Absent on a machine without Omarchy, which is not a fault: the
            // source simply is not offered (SPEC §6.1).
            tracing::debug!(%error, dir = ?dir, "no Omarchy state to watch");
            return;
        }

        let mut tracker = omagit_theme::omarchy::Tracker::new();
        let mut sent = commands::stylesheet(&app.state::<AppState>().settings());

        for event in events {
            if event.is_err() {
                continue;
            }
            if tracker.refresh(&dir).is_none() {
                continue;
            }
            let css = commands::stylesheet(&app.state::<AppState>().settings());
            if css == sent {
                continue;
            }
            sent = css.clone();
            tracing::info!("the system palette changed, the window follows");
            // The window may be gone; the thread ends with the channel.
            let _ = app.emit("theme", css);
        }
    });
}

//! omagit's backend: the Git core, exposed to a web front end through Tauri.
//!
//! The window is HTML; everything it asks for is answered here. What crosses is
//! in [`dto`] and is shaped for the wire, not mirrored from `omagit-git` —
//! keeping that crate free of any interface concern is the rule (SPEC §3 rule 5)
//! that made moving off GPUI a rewrite of the interface rather than of the
//! project.

pub mod commands;
pub mod dto;
pub mod edits;
pub mod log;
pub mod logging;
pub mod platform;
pub mod state;
pub mod time;

use state::AppState;

/// Build and run the application.
pub fn run() {
    let platform = platform::current();
    let config_dir = platform.config_dir();
    let _log_guard = logging::init(config_dir.as_deref());

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
        .invoke_handler(tauri::generate_handler![
            commands::platform,
            commands::theme,
            commands::git_status,
            commands::repositories,
            commands::add_repository,
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
            commands::journal,
            commands::log,
        ])
        .run(tauri::generate_context!())
        .expect("omagit could not start");
}

//! What the front end may ask for.
//!
//! Every one of these runs on Tauri's blocking pool, never on the thread that
//! drives the webview — `omagit_git::assert_off_render_thread` still guards it,
//! and `#[tauri::command(async)]` is what keeps the guard satisfied. The rule of
//! SPEC §3 did not change with the stack; only the thread it names did.

use std::path::PathBuf;

use omagit_git::diff::{DiffOptions, staged_file, unstaged_file};
use omagit_git::status::{Status, StatusOptions};
use omagit_git::{RepoPath, Summary};
use tauri::State;

use crate::dto;
use crate::state::AppState;

/// Errors cross as their message. A `GitError` carries a boxed source chain
/// that means nothing to a webview; what the interface needs is the sentence,
/// and SPEC §3 rule 3 wants that sentence to be Git's own words.
type Answer<T> = Result<T, String>;

fn say(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// Where the app is running: the modifier it spells shortcuts with, and the
/// bands the window system draws over.
#[tauri::command]
pub fn platform() -> crate::platform::PlatformFacts {
    crate::platform::current().facts()
}

/// The theme, as the CSS custom properties the stylesheet reads.
#[tauri::command]
pub fn theme(state: State<'_, AppState>) -> String {
    let settings = state.settings();
    // The four sources of SPEC §6.1 resolve here; the window's own appearance
    // is asked for by the front end and passed back in at M6b's next step.
    let sources = omagit_theme::Sources::default();
    let resolved = sources.resolve(&settings.theme);
    omagit_theme::css::variables(&resolved.theme, settings.density)
}

/// Whether writing is possible at all, and why not when it is not.
#[tauri::command]
pub fn git_status(state: State<'_, AppState>) -> Option<String> {
    state.git_error().map(ToOwned::to_owned)
}

/// The repositories the user has added.
#[tauri::command]
pub fn repositories(state: State<'_, AppState>) -> Vec<dto::LibraryRow> {
    state.with_library(|library| {
        library
            .iter()
            .map(|(at, entry)| dto::LibraryRow {
                group: at.group,
                index: at.index,
                path: entry.path.display().to_string(),
                name: entry.name.clone(),
            })
            .collect()
    })
}

/// Add a folder, if it is a repository.
#[tauri::command(async)]
pub fn add_repository(state: State<'_, AppState>, path: String) -> Answer<dto::RepoSummary> {
    let path = PathBuf::from(path);
    // Opening is what says whether it is one, and it is Git work.
    let open = state.open(&path).map_err(say)?;
    let summary = Summary::load(&open.repo, &state.cancel()).map_err(say)?;

    state.with_library(|library| {
        if library.find(&open.path).is_none() {
            library.add(omagit_settings::Entry::new(open.path.clone()), None);
        }
    });
    Ok(dto::summary(&open.path, &summary))
}

/// Everything known about one repository, for the card and the sidebar.
#[tauri::command(async)]
pub fn summary(state: State<'_, AppState>, path: String) -> Answer<dto::RepoSummary> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let summary = Summary::load(&open.repo, &state.cancel()).map_err(say)?;
    Ok(dto::summary(&open.path, &summary))
}

/// The working copy, one row per path.
#[tauri::command(async)]
pub fn status(state: State<'_, AppState>, path: String) -> Answer<Vec<dto::StatusRow>> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let status =
        Status::load(&open.repo, StatusOptions::default(), &state.cancel()).map_err(say)?;
    Ok(dto::status(&status))
}

/// One file's diff, on one side of the index.
///
/// The rows are flattened here rather than in the front end: which line is on
/// which side is already known, and a webview should be given what it draws.
#[tauri::command(async)]
pub fn file_diff(
    state: State<'_, AppState>,
    path: String,
    file: String,
    staged: bool,
) -> Answer<Option<dto::Diff>> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let cancel = state.cancel();
    let status = Status::load(&open.repo, StatusOptions::default(), &cancel).map_err(say)?;
    let wanted = RepoPath::from_bytes(file.into_bytes());
    let Some(entry) = status.entries.iter().find(|entry| entry.path == wanted) else {
        return Ok(None);
    };

    let diff = if staged {
        staged_file(&open.repo, entry, DiffOptions::default())
    } else {
        unstaged_file(&open.repo, entry, DiffOptions::default())
    }
    .map_err(say)?;

    Ok(diff.as_ref().map(dto::diff))
}

/// The operations journal (SPEC §11): the exact command, not a summary.
#[tauri::command]
pub fn journal(state: State<'_, AppState>) -> Vec<dto::JournalRow> {
    state
        .journal()
        .entries()
        .iter()
        .rev()
        .map(dto::journal_row)
        .collect()
}

/// What the front end has to say, in the same log as everything else.
///
/// A webview's console is behind a devtools window nobody has open when it
/// matters. Routing it here means a start-up failure appears in the file the
/// rest of the app already writes to, which is where anyone will look.
#[tauri::command]
pub fn log(level: String, message: String) {
    match level.as_str() {
        "error" => tracing::error!(target: "omagit::web", "{message}"),
        "warn" => tracing::warn!(target: "omagit::web", "{message}"),
        _ => tracing::info!(target: "omagit::web", "{message}"),
    }
}

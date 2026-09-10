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
use crate::edits::{self, Target};
use crate::state::AppState;

/// Errors cross as their message. A `GitError` carries a boxed source chain
/// that means nothing to a webview; what the interface needs is the sentence,
/// and SPEC §3 rule 3 wants that sentence to be Git's own words.
type Answer<T> = Result<T, String>;

/// Git's words, without the command that produced them.
///
/// `Display` on a failed command reads "git merge feature/x failed: CONFLICT
/// (content): Merge conflict in src/merge.rs" — the command first, the reason
/// last. That order is right in the journal and in the log, where *which*
/// command failed is the question. It is wrong everywhere the message is shown
/// to somebody who just pressed the button: the echo repeats what they did and
/// pushes the only new information off the end of the line. Found the first
/// time in the clone dialog (§2.35) and fixed there alone; the status bar
/// showed the same failure the same way, so the rule moves here, where every
/// error crosses.
///
/// The exact line is not lost: the journal records it, marked, before it runs.
fn say(error: omagit_git::GitError) -> String {
    error.reason()
}

/// A hash the front end sent that is not one.
///
/// Its own helper because it is not a `GitError` at all: nothing ran, and what
/// failed is the argument. Kept as its own sentence rather than dressed up as a
/// Git failure.
fn unreadable(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// Where the app is running: the modifier it spells shortcuts with, and the
/// bands the window system draws over.
#[tauri::command]
pub fn platform() -> crate::platform::PlatformFacts {
    crate::platform::current().facts()
}

/// Hang the native menu bar on the application (SPEC §9).
///
/// The items come from the front end because the table of what omagit can do
/// lives there — `web/src/keymap.ts`, read by the key handler, the palette, the
/// `?` sheet and now this. Sent again when what the window can do changes, so
/// an item is greyed rather than silently doing nothing.
///
/// A no-op where the platform has no menu bar: the front end does not call it on
/// Linux, and a command that would build a GTK menu anyway if someone did is a
/// trap rather than a convenience.
#[tauri::command]
pub fn set_menu(app: tauri::AppHandle, entries: Vec<crate::menu::Entry>) -> Result<(), String> {
    if !crate::platform::current().native_menus() {
        return Ok(());
    }
    crate::menu::install(&app, &entries).map_err(|error| {
        tracing::warn!(%error, "the menu bar could not be built");
        error.to_string()
    })
}

/// The theme, as the CSS custom properties the stylesheet reads.
#[tauri::command]
pub fn theme(state: State<'_, AppState>) -> String {
    stylesheet(&state.settings())
}

/// Resolve SPEC §6.1's four sources against this machine and render the result.
///
/// Shared with the watcher that pushes a new one when the system palette
/// changes: the two must agree, or following the system theme would mean
/// swapping the window to a stylesheet the next `theme` call contradicts.
pub fn stylesheet(settings: &omagit_settings::Settings) -> String {
    let resolved = crate::platform::theme_sources().resolve(&settings.theme);
    omagit_theme::css::scaled(&resolved.theme, settings.density, settings.scale())
}

/// How wide the resizable columns are, by name.
#[tauri::command]
pub fn panes(state: State<'_, AppState>) -> std::collections::BTreeMap<String, f32> {
    state.settings().panes
}

/// Remember a column's width.
///
/// Called when a drag ends rather than while it moves: the settings file is
/// rewritten on every call, and a drag is a hundred calls.
#[tauri::command]
pub fn set_pane(state: State<'_, AppState>, name: String, width: f32) {
    state.with_settings(|settings| {
        settings.panes.insert(name, width);
    });
}

/// Let go of a repository the window has closed.
///
/// Not an error when it was never open: closing a tab twice, or closing one
/// whose folder has gone, is a thing that happens and nothing depends on the
/// difference.
#[tauri::command]
pub fn close_repository(state: State<'_, AppState>, path: String) {
    state.close(&PathBuf::from(path));
}

/// The interface language, or `None` while it follows the system.
///
/// Stored here and chosen in the front end, which owns the catalogues: this
/// side knows there is a preference, never what it can be.
#[tauri::command]
pub fn language(state: State<'_, AppState>) -> Option<String> {
    state.settings().language
}

/// Change it, or `None` to follow the system again.
#[tauri::command]
pub fn set_language(state: State<'_, AppState>, language: Option<String>) {
    state.with_settings(|settings| settings.language = language);
}

/// The bindings the user has changed, by action id (SPEC §11).
///
/// Overrides only. The table of what omagit can do is the front end's, and this
/// is the difference from it — see `omagit_settings::Settings::keymap` for why
/// storing the whole thing would go stale.
#[tauri::command]
pub fn keymap(state: State<'_, AppState>) -> std::collections::BTreeMap<String, String> {
    state.settings().keymap
}

/// Change one binding, or put it back to the table's own.
///
/// Nothing here reads the string: what a binding may be — which keys are free,
/// which are already taken, which belong to movement — is knowledge the table
/// has, and the table is in the front end. This stores what it was told.
#[tauri::command]
pub fn set_binding(state: State<'_, AppState>, id: String, binding: Option<String>) {
    state.with_settings(|settings| match binding {
        Some(binding) => {
            settings.keymap.insert(id, binding);
        }
        None => {
            settings.keymap.remove(&id);
        }
    });
}

/// Whether writing is possible at all, and why not when it is not.
#[tauri::command]
pub fn git_status(state: State<'_, AppState>) -> Option<String> {
    state.git_error().map(ToOwned::to_owned)
}

/// The repositories the user has added.
///
/// The library and a stat per row, and nothing Git knows. Reading a summary for
/// each would mean a status walk over every repository the user has ever added
/// before the screen drew anything; the rows arrive first and the front end
/// fills them in.
#[tauri::command(async)]
pub fn repositories(state: State<'_, AppState>) -> Vec<dto::LibraryRow> {
    state.with_library(|library| {
        library
            .groups
            .iter()
            .enumerate()
            .flat_map(|(group, entries)| {
                entries
                    .repositories
                    .iter()
                    .enumerate()
                    .map(move |(index, entry)| dto::LibraryRow {
                        group,
                        index,
                        group_name: entries.name.clone(),
                        path: entry.path.display().to_string(),
                        name: entry.name.clone(),
                        description: entry.description.clone(),
                        last_opened: entry.last_opened,
                        // Stat rather than open: a directory that is gone must
                        // keep its row (DESIGN §4), and finding that out by
                        // failing to open it would cost a `gix` open per row.
                        missing: !entry.path.exists(),
                    })
            })
            .collect()
    })
}

/// Add a folder, remembering nothing about it until it turns out to be one.
///
/// Separate from [`add_repository`], which also loads a summary: the picker
/// hands back a path and the answer to "is this a repository" is the first
/// thing the window needs.
#[tauri::command(async)]
pub fn forget_repository(state: State<'_, AppState>, path: String) -> Answer<()> {
    let path = PathBuf::from(path);
    state.with_library(|library| {
        if let Some(at) = library.find(&path) {
            library.remove(at);
        }
    });
    Ok(())
}

/// Record that this repository was opened, for the "Last Opened" line.
#[tauri::command(async)]
pub fn touch_repository(state: State<'_, AppState>, path: String) {
    let path = PathBuf::from(path);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or_default();
    state.with_library(|library| {
        if let Some(at) = library.find(&path)
            && let Some(entry) = library.get_mut(at)
        {
            entry.last_opened = Some(now);
        }
    });
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

    // A conflicted path has no stage-0 entry in the index — that is what
    // unmerged means — so neither side of the index can answer for it. The
    // pane asked for one all the same and was told "no diff", which it drew as
    // "this file is no longer in the status" over a file plainly in it.
    let diff = if entry.conflict.is_some() {
        omagit_git::diff::conflicted_file(&open.repo, entry, DiffOptions::default())
    } else if staged {
        staged_file(&open.repo, entry, DiffOptions::default())
    } else {
        unstaged_file(&open.repo, entry, DiffOptions::default())
    }
    .map_err(say)?;

    Ok(diff.as_ref().map(dto::diff))
}

// ── History ─────────────────────────────────────────────────────────────────

/// The first page of a history walk, starting a new one.
///
/// Always restarts. Asking for "the history" is what the screen does when it
/// opens, when the query changes and when the repository moves underneath —
/// all three want the walk to begin again, and a command that sometimes
/// continued would be the one that made a scroll show two histories spliced
/// together.
#[tauri::command(async)]
pub fn history(
    state: State<'_, AppState>,
    path: String,
    query: crate::log::Query,
) -> Answer<crate::log::Page> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let cancel = state.cancel();
    let mut session = crate::log::Session::start(&open.repo, query, &cancel).map_err(say)?;
    let page = session.next(crate::log::PAGE, &cancel).map_err(say)?;

    *open
        .history
        .lock()
        .expect("the history lock is not poisoned") = Some(session);
    Ok(page)
}

/// The next page of the walk already in progress.
///
/// Refuses rather than silently starting one: a "load more" that answered from
/// a fresh walk would hand back the rows the list already has, and the reader
/// would see the history repeat itself.
#[tauri::command(async)]
pub fn history_more(state: State<'_, AppState>, path: String) -> Answer<crate::log::Page> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let cancel = state.cancel();
    let mut parked = open
        .history
        .lock()
        .expect("the history lock is not poisoned");
    let session = parked
        .as_mut()
        .ok_or("aucun parcours d'historique en cours")?;
    session.next(crate::log::PAGE, &cancel).map_err(say)
}

/// One commit: its message, who made it, and what it changed.
#[tauri::command(async)]
pub fn commit_detail(
    state: State<'_, AppState>,
    path: String,
    id: String,
) -> Answer<dto::CommitDetail> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let cancel = state.cancel();
    let id = id.parse().map_err(unreadable)?;
    let commit = omagit_git::history::commit(&open.repo, id).map_err(say)?;
    // Against the first parent, which is what `Diff::commit` does and what
    // `git show` shows. A merge's other parents are M6's A ↔ B comparison.
    let diff =
        omagit_git::Diff::commit(&open.repo, id, DiffOptions::default(), &cancel).map_err(say)?;
    Ok(dto::commit_detail(&commit, &diff))
}

/// One file of one commit, against its first parent.
#[tauri::command(async)]
pub fn commit_file_diff(
    state: State<'_, AppState>,
    path: String,
    id: String,
    file: String,
) -> Answer<Option<dto::Diff>> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let id = id.parse().map_err(unreadable)?;
    let diff = omagit_git::Diff::commit(&open.repo, id, DiffOptions::default(), &state.cancel())
        .map_err(say)?;
    let wanted = RepoPath::from_bytes(file.into_bytes());
    Ok(diff
        .files
        .iter()
        .find(|file| file.path == wanted)
        .map(dto::diff))
}

/// What differs between two commits, in either direction.
///
/// `from` and `to` are exactly that and not "older" and "newer": comparing a
/// commit with one of its own descendants is the ordinary case, and comparing
/// two tips of divergent branches is the interesting one, where neither is
/// older. The diff is `from` → `to`, whatever their order in the history.
#[tauri::command(async)]
pub fn compare(
    state: State<'_, AppState>,
    path: String,
    from: String,
    to: String,
) -> Answer<dto::Comparison> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let (from, to) = (
        from.parse().map_err(unreadable)?,
        to.parse().map_err(unreadable)?,
    );
    let diff = omagit_git::Diff::between(
        &open.repo,
        Some(from),
        Some(to),
        DiffOptions::default(),
        &state.cancel(),
    )
    .map_err(say)?;

    Ok(dto::Comparison {
        from: from.into(),
        to: to.into(),
        files: diff.files.iter().map(dto::file_row).collect(),
    })
}

/// One file of a comparison.
#[tauri::command(async)]
pub fn compare_file_diff(
    state: State<'_, AppState>,
    path: String,
    from: String,
    to: String,
    file: String,
) -> Answer<Option<dto::Diff>> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let (from, to) = (
        from.parse().map_err(unreadable)?,
        to.parse().map_err(unreadable)?,
    );
    let diff = omagit_git::Diff::between(
        &open.repo,
        Some(from),
        Some(to),
        DiffOptions::default(),
        &state.cancel(),
    )
    .map_err(say)?;

    let wanted = RepoPath::from_bytes(file.into_bytes());
    Ok(diff
        .files
        .iter()
        .find(|file| file.path == wanted)
        .map(dto::diff))
}

// ── Branches (M7) ───────────────────────────────────────────────────────────

/// Every reference, for the sidebar's tree.
#[tauri::command(async)]
pub fn refs(state: State<'_, AppState>, path: String) -> Answer<dto::Refs> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let cancel = state.cancel();
    let refs = omagit_git::refs::Refs::load(&open.repo, &cancel).map_err(say)?;

    // Without `git` there is still a tree to draw — reads are `gix` (SPEC §8) —
    // so a missing binary costs the merged marks and not the sidebar.
    let merged = match state.git() {
        Ok(git) => omagit_git::ops::merged(git, &open.repo, &cancel).unwrap_or_default(),
        Err(_) => Default::default(),
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or_default();

    Ok(dto::refs(&open.repo, &refs, &merged, now))
}

/// Switch to a branch, or to a commit with `HEAD` detached.
#[tauri::command(async)]
pub fn checkout(
    state: State<'_, AppState>,
    path: String,
    name: String,
    detach: bool,
) -> Answer<()> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();

    let _serialised = open.write_lock.lock();
    if detach {
        omagit_git::ops::checkout_detached(&git, &open.repo, &name, &state.cancel())
    } else {
        omagit_git::ops::checkout(&git, &open.repo, &name, &state.cancel())
    }
    .map_err(say)
}

/// Create a branch, and optionally switch to it.
#[tauri::command(async)]
pub fn create_branch(
    state: State<'_, AppState>,
    path: String,
    name: String,
    start: String,
    switch: bool,
) -> Answer<()> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();
    let start = start.trim();

    let _serialised = open.write_lock.lock();
    omagit_git::ops::create(
        &git,
        &open.repo,
        &name,
        (!start.is_empty()).then_some(start),
        switch,
        &state.cancel(),
    )
    .map_err(say)
}

/// Delete a branch. `force` is what throws away commits that are on no other.
#[tauri::command(async)]
pub fn delete_branch(
    state: State<'_, AppState>,
    path: String,
    name: String,
    force: bool,
) -> Answer<()> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();

    let _serialised = open.write_lock.lock();
    omagit_git::ops::delete(&git, &open.repo, &name, force, &state.cancel()).map_err(say)
}

// ── Preferences (M9) ────────────────────────────────────────────────────────
//
// The screen that replaces editing `settings.toml` by hand. Everything here
// answers with the freshly rendered stylesheet as well as with the state,
// because a preference nobody can see the effect of is a preference nobody
// trusts: the window applies it in the same tick it is set.

/// Everything the Preferences screen draws.
#[tauri::command(async)]
pub fn preferences(state: State<'_, AppState>) -> dto::Preferences {
    let settings = state.settings();
    let sources = crate::platform::theme_sources();
    let resolved = sources.resolve(&settings.theme);
    let git = state.git().ok().cloned();
    let editor = git
        .as_ref()
        .map(|git| describe_editor(git, &state))
        .unwrap_or_else(|| "git est indisponible".to_owned());

    dto::Preferences {
        source: match &settings.theme {
            omagit_theme::ThemeSource::UserOverride { .. } => "user-override",
            omagit_theme::ThemeSource::Omarchy => "omarchy",
            omagit_theme::ThemeSource::SystemAppearance => "system-appearance",
            omagit_theme::ThemeSource::Embedded { .. } => "embedded",
            omagit_theme::ThemeSource::Automatic => "automatic",
        },
        theme: match &settings.theme {
            omagit_theme::ThemeSource::UserOverride { name } => name.clone(),
            _ => String::new(),
        },
        mode: match &settings.theme {
            omagit_theme::ThemeSource::Embedded {
                mode: omagit_theme::Mode::Light,
            } => "light",
            _ => "dark",
        },
        density: match settings.density {
            omagit_theme::DensityMode::Compact => "compact",
            omagit_theme::DensityMode::Comfortable => "comfortable",
        },
        scale: settings.scale(),
        resolved: resolved.theme.name.clone(),
        omarchy: sources.omarchy_state.is_some(),
        system_appearance: sources.system_appearance.is_some()
            || crate::platform::current().reports_system_appearance(),
        catalogue: omagit_theme::embedded::catalogue()
            .into_iter()
            .map(|theme| dto::ThemeRow {
                mode: match theme.mode() {
                    omagit_theme::Mode::Light => "light",
                    omagit_theme::Mode::Dark => "dark",
                },
                name: theme.name.clone(),
            })
            .collect(),
        editor,
        credential_helper: crate::platform::current().credential_helper(),
        git: git
            .map(|git| git.version().to_string())
            .unwrap_or_else(|| "absent".to_owned()),
    }
}

/// What "Ouvrir dans l'éditeur" will launch, said before anyone presses it.
///
/// Not always what `core.editor` holds: a terminal editor is handed to the
/// desktop's opener instead (`editor.rs`), and the screen that admits it is
/// cheaper than the surprise.
fn describe_editor(git: &omagit_git::cli::Git, state: &AppState) -> String {
    let open = state.settings();
    let _ = open;
    let opener = crate::platform::current().opener();
    let Some(repo) = state.any_open() else {
        return format!("{opener} — ouvre un dépôt pour lire sa configuration");
    };
    let configured = crate::editor::configured(git, &repo, &state.cancel());
    let launch = crate::editor::decide(configured.as_deref(), opener);
    match launch.instead {
        None => launch.program,
        Some(why) => format!("{} — {why}", launch.program),
    }
}

/// Choose a theme source, and hand back the stylesheet it renders to.
#[tauri::command(async)]
pub fn set_theme(state: State<'_, AppState>, source: String, name: String) -> Answer<String> {
    let chosen = match source.as_str() {
        "omarchy" => omagit_theme::ThemeSource::Omarchy,
        "system-appearance" => omagit_theme::ThemeSource::SystemAppearance,
        "embedded-dark" => omagit_theme::ThemeSource::Embedded {
            mode: omagit_theme::Mode::Dark,
        },
        "embedded-light" => omagit_theme::ThemeSource::Embedded {
            mode: omagit_theme::Mode::Light,
        },
        "user-override" => omagit_theme::ThemeSource::UserOverride { name },
        "automatic" => omagit_theme::ThemeSource::Automatic,
        other => return Err(format!("source de thème inconnue : {other}")),
    };
    state.with_settings(|settings| settings.theme = chosen);
    Ok(stylesheet(&state.settings()))
}

/// Compact or comfortable (board 08).
#[tauri::command(async)]
pub fn set_density(state: State<'_, AppState>, density: String) -> Answer<String> {
    let chosen = match density.as_str() {
        "compact" => omagit_theme::DensityMode::Compact,
        "comfortable" => omagit_theme::DensityMode::Comfortable,
        other => return Err(format!("densité inconnue : {other}")),
    };
    state.with_settings(|settings| settings.density = chosen);
    Ok(stylesheet(&state.settings()))
}

/// How large the interface is drawn. Clamped by `Settings::scale`, not here: a
/// value typed into a file by hand has to be held to the same range as one
/// chosen on screen.
#[tauri::command(async)]
pub fn set_scale(state: State<'_, AppState>, scale: f32) -> Answer<String> {
    state.with_settings(|settings| settings.ui_scale = scale);
    Ok(stylesheet(&state.settings()))
}

// ── Conflicts (M8) ──────────────────────────────────────────────────────────

/// Name the two versions a conflicted file has, or nothing when no operation is
/// running.
#[tauri::command(async)]
pub fn conflict_sides(state: State<'_, AppState>, path: String) -> Answer<Option<dto::Sides>> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let sides = omagit_git::conflict::sides(&open.repo).map_err(say)?;
    Ok(sides.as_ref().map(dto::sides))
}

/// Keep one side of a conflicted file whole, and mark it settled.
#[tauri::command(async)]
pub fn resolve_conflict(
    state: State<'_, AppState>,
    path: String,
    file: String,
    side: edits::Side,
) -> Answer<()> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();
    let wanted = RepoPath::from_bytes(file.into_bytes());

    let _serialised = open.write_lock.lock();
    edits::resolve(&git, &open.repo, &wanted, side, &state.cancel()).map_err(say)
}

/// Carry on with the half-finished operation.
///
/// Which operation is read from the repository rather than passed in, for the
/// reason `abort_operation` gives: the front end's idea of what is running is a
/// copy, and a stale copy sends `git merge --continue` to a rebase.
#[tauri::command(async)]
pub fn continue_operation(state: State<'_, AppState>, path: String) -> Answer<String> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();
    let operation = open
        .repo
        .operation()
        .ok_or("aucune opération n'est en cours")?;

    let _serialised = open.write_lock.lock();
    omagit_git::ops::resume(&git, &open.repo, operation, &state.cancel()).map_err(say)
}

/// One conflicted file's markers, as the dialog draws them.
#[tauri::command(async)]
pub fn conflict_file(
    state: State<'_, AppState>,
    path: String,
    file: String,
) -> Answer<dto::Conflicted> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let wanted = RepoPath::from_bytes(file.into_bytes());
    let read = omagit_git::conflict::read(&open.repo, &wanted).map_err(say)?;
    Ok(dto::conflicted(&read))
}

/// Rewrite a conflicted file with one side chosen per conflict, and stage it.
///
/// The answers are matched against the file as it is when this runs, not as the
/// dialog read it: a count that no longer agrees means somebody edited it in
/// between, and the refusal is better than writing stale answers over their
/// work.
#[tauri::command(async)]
pub fn resolve_hunks(
    state: State<'_, AppState>,
    path: String,
    file: String,
    choices: Vec<edits::Choice>,
) -> Answer<()> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();
    let wanted = RepoPath::from_bytes(file.into_bytes());
    let choices: Vec<omagit_git::Choice> = choices.into_iter().map(Into::into).collect();

    let _serialised = open.write_lock.lock();
    omagit_git::ops::conflict::resolve(&git, &open.repo, &wanted, &choices, &state.cancel())
        .map_err(say)
}

/// Hand the file to the editor the user configured, and leave it open.
///
/// Answers with what was launched, because it is not always what was
/// configured: a terminal editor started from a window with no terminal is a
/// process nobody can see (`editor.rs`).
#[tauri::command(async)]
pub fn open_in_editor(state: State<'_, AppState>, path: String, file: String) -> Answer<String> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();
    let wanted = RepoPath::from_bytes(file.into_bytes());
    crate::editor::open(
        &git,
        &open.repo,
        &wanted,
        crate::platform::current().opener(),
        &state.cancel(),
    )
    .map_err(say)
}

// ── Stashes (M8) ────────────────────────────────────────────────────────────
//
// Reads are addressed by index, writes by commit. `git` addresses a stash by
// its position in a log — `stash@{0}` is the most recent — and that position
// moves: dropping one renumbers every entry below it. An index that crossed to
// the webview and came back is a copy of a numbering that may have changed
// since, so each write below resolves the commit to the index it has *now*,
// under the same lock as the command it feeds. It is the same rule
// `abort_operation` follows for the running operation, for the same reason.

/// Everything on the shelf, newest first.
#[tauri::command(async)]
pub fn stashes(state: State<'_, AppState>, path: String) -> Answer<Vec<dto::StashRow>> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let shelf = omagit_git::stash::list(&open.repo).map_err(say)?;
    Ok(shelf.iter().map(dto::stash_row).collect())
}

/// What one stash holds, one row per file.
///
/// Not `commit_detail` with the stash's own id, which would be the same call
/// for less code: that diffs against the first parent and stops, and a stash
/// made with `--include-untracked` keeps those files in a third parent. The
/// files a preview omits are exactly the ones nobody else has a copy of.
#[tauri::command(async)]
pub fn stash_files(
    state: State<'_, AppState>,
    path: String,
    id: String,
) -> Answer<Vec<dto::FileRow>> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let id = id.parse().map_err(unreadable)?;
    let diff = omagit_git::stash::diff(&open.repo, id, DiffOptions::default(), &state.cancel())
        .map_err(say)?;
    Ok(diff.files.iter().map(dto::file_row).collect())
}

/// One file of one stash.
#[tauri::command(async)]
pub fn stash_file_diff(
    state: State<'_, AppState>,
    path: String,
    id: String,
    file: String,
) -> Answer<Option<dto::Diff>> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let id = id.parse().map_err(unreadable)?;
    let diff = omagit_git::stash::diff(&open.repo, id, DiffOptions::default(), &state.cancel())
        .map_err(say)?;
    let wanted = RepoPath::from_bytes(file.into_bytes());
    Ok(diff
        .files
        .iter()
        .find(|file| file.path == wanted)
        .map(dto::diff))
}

/// Put the working copy on the shelf.
#[tauri::command(async)]
pub fn stash_push(
    state: State<'_, AppState>,
    path: String,
    message: String,
    untracked: bool,
) -> Answer<String> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();

    let _serialised = open.write_lock.lock();
    omagit_git::ops::stash::push(&git, &open.repo, &message, untracked, &state.cancel())
        .map_err(say)
}

/// Bring one back: applying leaves it on the shelf, popping takes it off.
///
/// One command for the two because they differ by a flag to `git` and by
/// nothing here — and because the pair is what makes the difference legible in
/// the journal, which records the line that ran.
#[tauri::command(async)]
pub fn stash_restore(
    state: State<'_, AppState>,
    path: String,
    id: String,
    keep: bool,
) -> Answer<String> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();
    let id = id.parse().map_err(unreadable)?;

    let _serialised = open.write_lock.lock();
    let entry = omagit_git::stash::find(&open.repo, id).map_err(say)?;
    if keep {
        omagit_git::ops::stash::apply(&git, &open.repo, entry.index, &state.cancel())
    } else {
        omagit_git::ops::stash::pop(&git, &open.repo, entry.index, &state.cancel())
    }
    .map_err(say)
}

/// Throw one away without applying it.
///
/// Destructive and quiet: the working tree does not move, and what disappears
/// is reachable only through the reflog line `git` prints on its way out —
/// which is why that line is what comes back. The confirmation is the front
/// end's, before this is ever called (SPEC §3 rule 7).
#[tauri::command(async)]
pub fn stash_drop(state: State<'_, AppState>, path: String, id: String) -> Answer<String> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();
    let id = id.parse().map_err(unreadable)?;

    let _serialised = open.write_lock.lock();
    let entry = omagit_git::stash::find(&open.repo, id).map_err(say)?;
    omagit_git::ops::stash::drop(&git, &open.repo, entry.index, &state.cancel()).map_err(say)
}

// ── The network (M7) ────────────────────────────────────────────────────────
//
// Everything here can take minutes and can be stopped. Two rules hold it
// together: one operation at a time per window, and nothing ever waits for a
// person — `GIT_TERMINAL_PROMPT=0` is set for every invocation, so a repository
// whose credentials no helper can supply *fails* rather than hanging on a
// prompt there is no terminal to answer.

/// One line of `git`'s progress, on its way to the overlay.
#[derive(Clone, serde::Serialize)]
pub struct Progress {
    pub what: String,
    pub phase: String,
    pub percent: Option<u8>,
}

/// Build the callback that turns `git`'s stderr into events.
fn reporting(app: &tauri::AppHandle, what: &str) -> omagit_git::cli::Progress {
    use tauri::Emitter as _;
    let app = app.clone();
    let what = what.to_owned();
    std::sync::Arc::new(move |line: &str| {
        let step = omagit_git::ops::Step::parse(line);
        // A failure to emit is not worth failing the operation for: the window
        // has gone, and the fetch it asked for can finish without it.
        let _ = app.emit(
            "progress",
            Progress {
                what: what.clone(),
                phase: step.phase,
                percent: step.percent,
            },
        );
    })
}

/// Bring refs down from every remote.
#[tauri::command(async)]
pub fn fetch(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
    remote: String,
) -> Answer<String> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();
    let slot = state.start_network("Fetch")?;
    let remote = remote.trim();

    omagit_git::ops::fetch(
        &git,
        &open.repo,
        (!remote.is_empty()).then_some(remote),
        Some(reporting(&app, "Fetch")),
        &slot.cancel,
    )
    .map_err(say)
}

/// Whether anything says how this branch reconciles a divergence.
///
/// Asked before pulling rather than deduced from the failure afterwards: since
/// 2.27 `git pull` refuses on a diverged branch when nothing configures it, and
/// the wall of hints it prints can only be answered from a terminal. The window
/// has no preferences screen yet, so it asks instead — and it asks on facts,
/// not on a message that is translated on a machine whose `git` speaks another
/// language.
#[tauri::command(async)]
pub fn pull_reconcile_configured(state: State<'_, AppState>, path: String) -> Answer<bool> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();
    let head = open.repo.head().map_err(say)?;
    let omagit_git::Head::Branch { branch, .. } = head else {
        // Detached or unborn: there is no branch to pull into, and `git` will
        // say so better than a guess made here.
        return Ok(true);
    };
    omagit_git::ops::reconcile_configured(&git, &open.repo, &branch, &state.cancel()).map_err(say)
}

/// Fetch and integrate, the way the repository is configured to.
///
/// `reconcile` is sent only when nothing configures one — see above. Passing it
/// otherwise would override a decision somebody made for the repository
/// (§2.33), and it is never written to the configuration: it answers this pull
/// and no other.
#[tauri::command(async)]
pub fn pull(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
    reconcile: Option<String>,
) -> Answer<String> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();
    let reconcile = match reconcile.as_deref() {
        Some("merge") => Some(omagit_git::ops::Reconcile::Merge),
        Some("rebase") => Some(omagit_git::ops::Reconcile::Rebase),
        _ => None,
    };
    let slot = state.start_network("Pull")?;

    // A pull writes the working tree, so it takes the same lock every other
    // write does.
    let _serialised = open.write_lock.lock();
    omagit_git::ops::pull(
        &git,
        &open.repo,
        reconcile,
        Some(reporting(&app, "Pull")),
        &slot.cancel,
    )
    .map_err(say)
}

/// Send a branch to a remote.
#[tauri::command(async)]
pub fn push(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
    remote: String,
    branch: String,
    force: bool,
    set_upstream: bool,
) -> Answer<String> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();
    let slot = state.start_network("Push")?;

    omagit_git::ops::push(
        &git,
        &open.repo,
        &omagit_git::ops::Push {
            remote: &remote,
            branch: &branch,
            force: if force {
                omagit_git::ops::PushForce::WithLease
            } else {
                omagit_git::ops::PushForce::Never
            },
            set_upstream,
        },
        Some(reporting(&app, "Push")),
        &slot.cancel,
    )
    .map_err(say)
}

/// The folder `git clone` would create for this URL.
///
/// Asked as the URL is typed, so the destination line can fill itself in. A
/// form that made someone type the name of the thing they just pasted a URL to
/// is a form asking a question it can answer.
#[tauri::command]
pub fn clone_directory(url: String) -> Option<String> {
    omagit_git::ops::directory_for(&url)
}

/// Ask a remote whether it is there and whether we are allowed in.
///
/// Cheap, and asked before the clone rather than after: the alternative is
/// finding out from a four-minute operation that failed on its first second.
/// It does not take the network slot — it is a question, not an operation, and
/// blocking it behind a running fetch would leave the dialog unable to say
/// anything about what was typed into it.
#[tauri::command(async)]
pub fn check_remote(state: State<'_, AppState>, url: String) -> Answer<()> {
    let git = state.git().map_err(say)?.clone();
    // Its reason rather than its whole `Display`: the command echo would be two
    // thirds of a message shown beside the field the URL was typed into.
    omagit_git::ops::reachable(&git, url.trim(), &state.cancel()).map_err(|failed| failed.reason())
}

/// Everything the clone dialog collected.
///
/// One struct rather than six arguments, and not only for clippy's count:
/// `clone(url, parent, name, true, false, None)` is a call where swapping the
/// two flags compiles and does something else.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneRequest {
    pub url: String,
    /// The folder the clone is created *in*. `git clone <url> <dir>` creates
    /// `<dir>`, so the destination the dialog shows is these two joined.
    pub parent: String,
    pub name: String,
    pub shallow: bool,
    pub submodules: bool,
    /// Which group of the library it joins, or the default one.
    pub group: Option<usize>,
}

/// Copy a remote repository onto the disk, and remember it.
///
/// Returns the summary of what landed, so the window can open it without a
/// second round trip — a clone that finished and left you looking at the same
/// list you started from has made you find it yourself.
#[tauri::command(async)]
pub fn clone_repository(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: CloneRequest,
) -> Answer<dto::RepoSummary> {
    let CloneRequest {
        url,
        parent,
        name,
        shallow,
        submodules,
        group,
    } = request;
    let git = state.git().map_err(say)?.clone();
    let slot = state.start_network("Clonage")?;

    let landed = omagit_git::ops::clone(
        &git,
        url.trim(),
        &PathBuf::from(parent),
        name.trim(),
        &omagit_git::ops::CloneOptions {
            shallow,
            submodules,
        },
        Some(reporting(&app, "Clonage")),
        &slot.cancel,
    )
    .map_err(say)?;

    // Opening it is what says it is a repository. A clone that produced a
    // directory this cannot read is a failure worth hearing about here rather
    // than from a struck-through row in the list.
    let open = state.open(&landed).map_err(say)?;
    let summary = Summary::load(&open.repo, &state.cancel()).map_err(say)?;

    state.with_library(|library| {
        if library.find(&open.path).is_none() {
            library.add(omagit_settings::Entry::new(open.path.clone()), group);
        }
    });
    Ok(dto::summary(&open.path, &summary))
}

/// Stop whatever is running on the network.
///
/// Nothing to stop is not an error: the button is pressed at the moment an
/// operation ends often enough that treating it as one would be noise.
#[tauri::command]
pub fn cancel_operation(state: State<'_, AppState>) -> Option<String> {
    state.cancel_network()
}

/// Merge a branch into the current one.
#[tauri::command(async)]
pub fn merge(
    state: State<'_, AppState>,
    path: String,
    branch: String,
    no_fast_forward: bool,
    squash: bool,
) -> Answer<String> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();

    let _serialised = open.write_lock.lock();
    omagit_git::ops::merge(
        &git,
        &open.repo,
        &branch,
        &omagit_git::ops::MergeOptions {
            no_fast_forward,
            squash,
        },
        &state.cancel(),
    )
    .map_err(say)
}

/// Replay the current branch on top of another.
#[tauri::command(async)]
pub fn rebase(state: State<'_, AppState>, path: String, onto: String) -> Answer<String> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();

    let _serialised = open.write_lock.lock();
    omagit_git::ops::rebase(&git, &open.repo, &onto, &state.cancel()).map_err(say)
}

/// Put the repository back where the half-finished operation found it.
///
/// Which operation is read from the repository rather than passed in: the
/// front end's idea of what is running is a copy, and a copy that had gone
/// stale would send `git merge --abort` to a rebase.
#[tauri::command(async)]
pub fn abort_operation(state: State<'_, AppState>, path: String) -> Answer<()> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();
    let operation = open
        .repo
        .operation()
        .ok_or("aucune opération n'est en cours")?;

    let _serialised = open.write_lock.lock();
    omagit_git::ops::abort(&git, &open.repo, operation, &state.cancel()).map_err(say)
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

// ── Writing ─────────────────────────────────────────────────────────────────
//
// Every one of these takes the repository's write lock for its duration, which
// is what SPEC §10 asks for: two writing Git commands must never run at once on
// one repository. The lock lives on the open handle, so the serialisation is a
// property of where it sits rather than a rule anyone has to remember.

/// Move part of a file into the index, or out of it.
#[tauri::command(async)]
pub fn stage(
    state: State<'_, AppState>,
    path: String,
    file: String,
    target: Target,
    unstage: bool,
) -> Answer<()> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();
    let wanted = RepoPath::from_bytes(file.into_bytes());

    let _serialised = open.write_lock.lock();
    edits::stage(&git, &open.repo, &wanted, &target, unstage, &state.cancel()).map_err(say)
}

/// Move every change into the index, or take every change back out.
///
/// One Git command rather than one per row: a loop in the front end would spawn
/// a process per file and write a journal line per file for what was asked as a
/// single thing.
#[tauri::command(async)]
pub fn stage_all(state: State<'_, AppState>, path: String, unstage: bool) -> Answer<()> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();

    let _serialised = open.write_lock.lock();
    if unstage {
        omagit_git::ops::unstage_all(&git, &open.repo, &state.cancel())
    } else {
        omagit_git::ops::stage_all(&git, &open.repo, &state.cancel())
    }
    .map_err(say)
}

/// Undo part of a file in the working tree.
///
/// Destructive: what it removes was never committed and is not in the reflog.
/// The confirmation SPEC §3 rule 7 requires is the front end's — by the time a
/// command is invoked, the asking is over.
#[tauri::command(async)]
pub fn discard(
    state: State<'_, AppState>,
    path: String,
    file: String,
    target: Target,
) -> Answer<()> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();
    let wanted = RepoPath::from_bytes(file.into_bytes());

    let _serialised = open.write_lock.lock();
    edits::discard(&git, &open.repo, &wanted, &target, &state.cancel()).map_err(say)
}

/// Who `git` would record as the author, or nothing when it has none.
///
/// Asked before a message is written rather than after: `git commit` with no
/// identity fails with a wall of text about `git config --global`, and an app
/// that lets someone write a message and then shows them that has wasted their
/// work (SPEC §11).
#[tauri::command(async)]
pub fn committer(state: State<'_, AppState>, path: String) -> Answer<Option<String>> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();
    let who =
        omagit_git::ops::committer_identity(&git, &open.repo, &state.cancel()).map_err(say)?;
    Ok(who.map(|who| format!("{} <{}>", who.name, who.email)))
}

/// The repository's `commit.template`, if it configures one.
#[tauri::command(async)]
pub fn commit_template(state: State<'_, AppState>, path: String) -> Answer<Option<String>> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();
    omagit_git::ops::template(&git, &open.repo, &state.cancel()).map_err(say)
}

/// Make the commit.
#[tauri::command(async)]
pub fn commit(
    state: State<'_, AppState>,
    path: String,
    message: String,
    amend: bool,
    sign_off: bool,
    no_verify: bool,
) -> Answer<dto::Made> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();
    let options = omagit_git::ops::CommitOptions {
        amend,
        sign_off,
        no_verify,
    };

    let _serialised = open.write_lock.lock();
    omagit_git::ops::commit(&git, &open.repo, &message, &options, &state.cancel())
        .map(|made| dto::Made {
            id: made.id.into(),
            notes: made.notes,
        })
        .map_err(say)
}

/// The message of the commit `HEAD` points at.
///
/// What an amend starts from. Without it the box would open empty and the
/// commit being replaced would lose its message to a slip of a checkbox —
/// which is precisely the kind of loss SPEC §3 rule 7 is about.
#[tauri::command(async)]
pub fn head_message(state: State<'_, AppState>, path: String) -> Answer<Option<String>> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let Some(id) = open.repo.head().map_err(say)?.commit().copied() else {
        return Ok(None);
    };
    let commit = omagit_git::history::commit(&open.repo, id).map_err(say)?;
    Ok(Some(match commit.body.as_str() {
        "" => commit.summary,
        body => format!("{}\n\n{}", commit.summary, body),
    }))
}

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

    let diff = if staged {
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
    let id = id.parse().map_err(say)?;
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
    let id = id.parse().map_err(say)?;
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
    let (from, to) = (from.parse().map_err(say)?, to.parse().map_err(say)?);
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
    let (from, to) = (from.parse().map_err(say)?, to.parse().map_err(say)?);
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

/// Fetch and integrate, the way the repository is configured to.
#[tauri::command(async)]
pub fn pull(app: tauri::AppHandle, state: State<'_, AppState>, path: String) -> Answer<String> {
    let open = state.open(&PathBuf::from(path)).map_err(say)?;
    let git = state.git().map_err(say)?.clone();
    let slot = state.start_network("Pull")?;

    // A pull writes the working tree, so it takes the same lock every other
    // write does.
    let _serialised = open.write_lock.lock();
    omagit_git::ops::pull(
        &git,
        &open.repo,
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

/// Stop whatever is running on the network.
///
/// Nothing to stop is not an error: the button is pressed at the moment an
/// operation ends often enough that treating it as one would be noise.
#[tauri::command]
pub fn cancel_operation(state: State<'_, AppState>) -> Option<String> {
    state.cancel_network()
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

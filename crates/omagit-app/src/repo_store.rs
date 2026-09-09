//! One open repository, and everything the Working Copy screen reads from it.
//!
//! This is the `Entity<RepoStore>` of SPEC §10, and it is where the diagram
//! finally has all of its parts: a status and a per-file diff, each an
//! [`AsyncState`] with a generation, all read on the background executor, and a
//! filesystem watcher that invalidates **only what actually changed**.
//!
//! The invalidation is the interesting half. `omagit_git::watch` reports what
//! kind of thing changed rather than that something did, so a `git push` in a
//! terminal — which writes under `.git/refs/` and nothing else — does not cost
//! a working-copy scan, and a build writing into `target/` costs nothing at
//! all. Without that, a client on a busy repository spends its life recomputing
//! a status it throws away.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::{AppContext, Context, Task};
use omagit_git::diff::{FileDiff, staged_file, unstaged_file};
use omagit_git::{
    Cancel, DiffOptions, GitError, RepoPath, Repository, Status, StatusOptions, Summary, Watcher,
};

use omagit_git::Commit;
use omagit_git::cli::Git;
use omagit_git::graph::{Graph, Row as GraphRow};
use omagit_git::history::{HistoryQuery, Walk};
use omagit_git::ops::CommitOutcome;

use crate::async_state::{AsyncState, Generation};
use crate::writes::{Done, Queue, Write};

/// How many commits a page of history carries. SPEC §12 measures the first
/// thousand of a hundred thousand, so a thousand is the unit that budget is
/// written in.
const HISTORY_PAGE: usize = 1000;

/// Which side of the index a diff is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Side {
    /// `HEAD` → index: what committing now would record.
    Staged,
    /// Index → working tree: what committing now would leave behind.
    Unstaged,
}

impl Side {
    pub fn label(self) -> &'static str {
        match self {
            Side::Staged => "Staged",
            Side::Unstaged => "Unstaged",
        }
    }
}

pub struct RepoStore {
    repo: Repository,
    path: PathBuf,
    status: AsyncState<Status>,
    summary: AsyncState<Summary>,
    /// One entry per file the user has actually opened. Diffs are read on
    /// demand — SPEC §12: a commit touching 900 files must show the first
    /// immediately, so nothing computes 900 diffs.
    diffs: HashMap<(RepoPath, Side), AsyncState<FileDiff>>,
    asked: HashMap<Key, Generation>,
    latest: Generation,
    /// Cancels every read in flight. Replaced when a new round starts, which is
    /// what stops a slow scan from finishing into a state nobody is looking at.
    cancel: Cancel,
    /// Held for its lifetime: dropping it stops watching. `None` when the
    /// platform refused — the screen says so rather than pretending to be live.
    _watcher: Option<Watcher>,
    _watching: Option<Task<()>>,
    watch_error: Option<String>,
    /// Writes, one at a time (SPEC §10). Per store, so per repository.
    writes: Queue,
    /// The last commit made here, so the screen can clear its message box only
    /// once the commit actually exists.
    last_commit: Option<CommitOutcome>,
    /// The history, as far as it has been walked.
    history: History,
    /// The walk and the lane assignment, parked here between pages.
    ///
    /// `None` while a page is in flight: they are moved to the background
    /// executor and handed back, which is what makes the next page resume
    /// exactly where this one stopped rather than re-walking (SPEC §11).
    walk: Option<Walk>,
    graph: Option<Graph>,
}

/// One row of the History screen: the commit, and where its node sits.
#[derive(Clone, Debug)]
pub struct HistoryRow {
    pub commit: Commit,
    pub graph: GraphRow,
}

/// The history as far as it has been read.
///
/// Not an `AsyncState`: it is never wholly absent or wholly present, it grows.
/// What the screen has to render is "these rows, and possibly more coming",
/// which is a different shape from the four states of SPEC §10.
#[derive(Debug, Default)]
pub struct History {
    pub rows: Vec<HistoryRow>,
    /// Whether a page is being read right now.
    pub loading: bool,
    /// Whether the walk has reached the roots.
    pub complete: bool,
    /// The failure that stopped it, if one did.
    pub failure: Option<GitError>,
}

impl History {
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The widest gutter any row needs, so the column is sized once rather
    /// than per frame.
    pub fn gutter_lanes(&self) -> usize {
        self.rows
            .iter()
            .map(|row| row.graph.width)
            .max()
            .unwrap_or(1)
    }
}

/// What a generation is counted against.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Key {
    Status,
    Summary,
    Diff(RepoPath, Side),
}

impl RepoStore {
    /// Take an already-opened repository and start reading it.
    ///
    /// Opening is the caller's, and deliberately: `Repository::open` is Git
    /// work, and the render-thread guard would fire if a store were built from
    /// a path in the middle of a frame. The screen that opens a repository does
    /// it on the background executor and hands the result here.
    pub fn new(repo: Repository, path: PathBuf, cx: &mut Context<Self>) -> Self {
        let mut store = Self {
            repo,
            path,
            status: AsyncState::Idle,
            summary: AsyncState::Idle,
            diffs: HashMap::new(),
            asked: HashMap::new(),
            latest: Generation::default(),
            cancel: Cancel::new(),
            _watcher: None,
            _watching: None,
            watch_error: None,
            writes: Queue::default(),
            last_commit: None,
            history: History::default(),
            walk: None,
            graph: None,
        };
        store.start_watching(cx);
        store.refresh(cx);
        store
    }

    pub fn repository(&self) -> &Repository {
        &self.repo
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn status(&self) -> &AsyncState<Status> {
        &self.status
    }

    pub fn summary(&self) -> &AsyncState<Summary> {
        &self.summary
    }

    /// Whether live updates are running, and why not if they are not.
    pub fn history(&self) -> &History {
        &self.history
    }

    /// Read the next page of history, starting the walk if it has not begun.
    ///
    /// Called when the screen opens and again when the list nears its end, so
    /// a hundred thousand commits arrive a thousand at a time rather than all
    /// at once (SPEC §12).
    pub fn load_more_history(&mut self, cx: &mut Context<Self>) {
        if self.history.loading || self.history.complete {
            return;
        }
        self.history.loading = true;
        self.history.failure = None;

        let repo = self.repo.clone();
        let cancel = self.cancel.clone();
        // Taken, not borrowed: they cross to the background executor and come
        // back, which is what lets the next page resume rather than re-walk.
        let walk = self.walk.take();
        let graph = self.graph.take();

        cx.spawn(async move |this, cx| {
            let read = cx
                .background_spawn(async move {
                    let mut walk = match walk {
                        Some(walk) => walk,
                        None => match Walk::new(&repo, HistoryQuery::all(), &cancel) {
                            Ok(walk) => walk,
                            Err(error) => return (None, None, Err(error)),
                        },
                    };
                    let mut graph = graph.unwrap_or_default();
                    match walk.next_page(HISTORY_PAGE, &cancel) {
                        Ok(commits) => {
                            let rows = commits
                                .into_iter()
                                .map(|commit| {
                                    let graph_row = graph.push(&commit);
                                    HistoryRow {
                                        commit,
                                        graph: graph_row,
                                    }
                                })
                                .collect::<Vec<_>>();
                            let done = walk.is_done();
                            (Some(walk), Some(graph), Ok((rows, done)))
                        }
                        Err(error) => (Some(walk), Some(graph), Err(error)),
                    }
                })
                .await;

            this.update(cx, |store, cx| {
                let (walk, graph, outcome) = read;
                store.walk = walk;
                store.graph = graph;
                store.history.loading = false;
                match outcome {
                    Ok((rows, done)) => {
                        store.history.rows.extend(rows);
                        store.history.complete = done;
                    }
                    Err(error) => {
                        // The rows already read stay: a page that failed is not
                        // a reason to blank a list someone is reading.
                        tracing::warn!(%error, "reading a page of history failed");
                        store.history.failure = Some(error);
                        store.history.complete = true;
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// What the write queue is doing, for the interface to render.
    pub fn writes(&self) -> &Queue {
        &self.writes
    }

    pub fn last_commit(&self) -> Option<&CommitOutcome> {
        self.last_commit.as_ref()
    }

    pub fn dismiss_write_failure(&mut self, cx: &mut Context<Self>) {
        self.writes.clear_failure();
        cx.notify();
    }

    /// Queue a write and start the queue if nothing is running.
    ///
    /// Confirmation for a destructive write is the screen's, not this: SPEC §3
    /// rule 7 is about asking the user, and by the time something is queued the
    /// asking is over.
    pub fn submit(&mut self, write: Write, git: Arc<Git>, cx: &mut Context<Self>) {
        self.writes.push(write);
        self.pump(git, cx);
        cx.notify();
    }

    fn pump(&mut self, git: Arc<Git>, cx: &mut Context<Self>) {
        let Some(write) = self.writes.start() else {
            return;
        };
        let repo = self.repo.clone();
        // A write is not cancelled by a refresh: a half-applied patch is worse
        // than a slow one, so it gets its own token that nothing else touches.
        let cancel = Cancel::new();

        cx.spawn(async move |this, cx| {
            let outcome = {
                let (write, git, repo) = (write.clone(), git.clone(), repo);
                cx.background_spawn(async move { write.run(&git, &repo, &cancel) })
                    .await
            };
            this.update(cx, |store, cx| {
                let outcome = store.writes.finish(outcome);
                match outcome {
                    Ok(Done::Committed(made)) => {
                        tracing::info!(commit = %made.id, "committed");
                        store.last_commit = Some(made);
                    }
                    Ok(Done::Staged) => {}
                    Err(error) => tracing::warn!(%error, "write failed"),
                }
                // The repository moved — or was meant to and did not. Either
                // way what is on screen is now a guess.
                store.refresh(cx);
                store.pump(git, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn watch_error(&self) -> Option<&str> {
        self.watch_error.as_deref()
    }

    pub fn diff(&self, path: &RepoPath, side: Side) -> &AsyncState<FileDiff> {
        static IDLE: AsyncState<FileDiff> = AsyncState::Idle;
        self.diffs.get(&(path.clone(), side)).unwrap_or(&IDLE)
    }

    /// Re-read the status and the summary.
    /// Throw the history away and start it again.
    ///
    /// A commit, a checkout or a fetch changes what the walk would produce, and
    /// there is no way to patch a lane assignment in place: the rows below a
    /// new commit shift, and a graph that is half old and half new is worse
    /// than one that is briefly empty.
    fn restart_history(&mut self, cx: &mut Context<Self>) {
        if self.history.is_empty() && self.walk.is_none() {
            return;
        }
        self.history = History::default();
        self.walk = None;
        self.graph = None;
        self.load_more_history(cx);
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.cancel.cancel();
        self.cancel = Cancel::new();
        self.read_status(cx);
        self.read_summary(cx);
        // Every open diff is now suspect: the file may have changed under it.
        let open: Vec<(RepoPath, Side)> = self.diffs.keys().cloned().collect();
        for (path, side) in open {
            self.read_diff(path, side, cx);
        }
        // And so is the history, if anyone has asked for it: a commit, a
        // checkout or a fetch changes what the walk produces.
        self.restart_history(cx);
    }

    /// Read one file's diff, unless it is already in flight.
    pub fn open_diff(&mut self, path: RepoPath, side: Side, cx: &mut Context<Self>) {
        if self.diffs.contains_key(&(path.clone(), side)) {
            return;
        }
        self.read_diff(path, side, cx);
    }

    fn read_status(&mut self, cx: &mut Context<Self>) {
        let generation = self.bump(Key::Status);
        self.status.begin();
        let repo = self.repo.clone();
        let cancel = self.cancel.clone();
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_spawn(
                    async move { Status::load(&repo, StatusOptions::default(), &cancel) },
                )
                .await;
            this.update(cx, |store, cx| {
                let current = store.current(&Key::Status, generation);
                if store.status.finish(read, generation, current) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn read_summary(&mut self, cx: &mut Context<Self>) {
        let generation = self.bump(Key::Summary);
        self.summary.begin();
        let repo = self.repo.clone();
        let cancel = self.cancel.clone();
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_spawn(async move { Summary::load(&repo, &cancel) })
                .await;
            this.update(cx, |store, cx| {
                let current = store.current(&Key::Summary, generation);
                if store.summary.finish(read, generation, current) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn read_diff(&mut self, path: RepoPath, side: Side, cx: &mut Context<Self>) {
        let key = Key::Diff(path.clone(), side);
        let generation = self.bump(key.clone());
        self.diffs.entry((path.clone(), side)).or_default().begin();

        let repo = self.repo.clone();
        let cancel = self.cancel.clone();
        let wanted = path.clone();
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_spawn(async move {
                    // The diff of a file is defined against its status entry —
                    // that is what says whether it is a rename, and against
                    // what. So the status is read first rather than the file
                    // being diffed against a guess.
                    let status = Status::load(&repo, StatusOptions::default(), &cancel)?;
                    let entry = status
                        .entries
                        .iter()
                        .find(|entry| entry.path == wanted)
                        .ok_or_else(|| {
                            GitError::NotFound(format!("{wanted} in the working copy"))
                        })?;
                    let options = DiffOptions::default();
                    let file = match side {
                        Side::Staged => staged_file(&repo, entry, options)?,
                        Side::Unstaged => unstaged_file(&repo, entry, options)?,
                    };
                    file.ok_or_else(|| {
                        GitError::NotFound(format!("a {} diff for {wanted}", side.label()))
                    })
                })
                .await;
            this.update(cx, |store, cx| {
                let current = store.current(&key, generation);
                let state = store.diffs.entry((path, side)).or_default();
                if state.finish(read, generation, current) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Start the filesystem watcher and route what it reports.
    ///
    /// Registering a recursive watch walks the tree — one inotify descriptor
    /// per directory on Linux — so it starts on the background executor like
    /// every other filesystem operation, and installs itself when it is ready.
    fn start_watching(&mut self, cx: &mut Context<Self>) {
        let repo = self.repo.clone();
        cx.spawn(async move |this, cx| {
            let started = cx
                .background_spawn(async move { Watcher::start(&repo) })
                .await;
            this.update(cx, |store, cx| match started {
                Ok((watcher, changes)) => store.install_watcher(watcher, changes, cx),
                Err(error) => {
                    // Reported rather than swallowed: a client that has quietly
                    // stopped noticing changes looks exactly like a broken one,
                    // and the likely cause — the platform's watch limit — is
                    // something the user can act on once told.
                    tracing::warn!(%error, "live updates are off for this repository");
                    store.watch_error = Some(error.to_string());
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn install_watcher(
        &mut self,
        watcher: Watcher,
        changes: async_channel::Receiver<omagit_git::Changes>,
        cx: &mut Context<Self>,
    ) {
        self._watcher = Some(watcher);
        self._watching = Some(cx.spawn(async move |this, cx| {
            while let Ok(changes) = changes.recv().await {
                let landed = this.update(cx, |store, cx| {
                    // Targeted: a branch moving does not re-scan the working
                    // copy, and a file changing does not re-count every branch.
                    if changes.invalidates_status() {
                        store.cancel.cancel();
                        store.cancel = Cancel::new();
                        store.read_status(cx);
                        let open: Vec<(RepoPath, Side)> = store.diffs.keys().cloned().collect();
                        for (path, side) in open {
                            store.read_diff(path, side, cx);
                        }
                    }
                    if changes.invalidates_refs() || changes.invalidates_status() {
                        store.read_summary(cx);
                    }
                });
                if landed.is_err() {
                    return;
                }
            }
        }));
    }

    fn bump(&mut self, key: Key) -> Generation {
        self.latest = self.latest.next();
        self.asked.insert(key, self.latest);
        self.latest
    }

    fn current(&self, key: &Key, fallback: Generation) -> Generation {
        self.asked.get(key).copied().unwrap_or(fallback)
    }
}

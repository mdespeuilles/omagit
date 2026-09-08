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

use gpui_kit::{AppContext, Context, Task};
use omagit_git::diff::{FileDiff, staged_file, unstaged_file};
use omagit_git::{
    Cancel, DiffOptions, GitError, RepoPath, Repository, Status, StatusOptions, Summary, Watcher,
};

use crate::async_state::{AsyncState, Generation};

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
    pub fn watch_error(&self) -> Option<&str> {
        self.watch_error.as_deref()
    }

    pub fn diff(&self, path: &RepoPath, side: Side) -> &AsyncState<FileDiff> {
        static IDLE: AsyncState<FileDiff> = AsyncState::Idle;
        self.diffs.get(&(path.clone(), side)).unwrap_or(&IDLE)
    }

    /// Re-read the status and the summary.
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

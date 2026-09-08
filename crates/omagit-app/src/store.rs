//! The repository list, and what is known about each one.
//!
//! The single source of truth for the Repositories screen (SPEC §10): the
//! persisted [`Library`] on one side, and a [`Summary`] per repository read in
//! the background on the other. Components observe it and hold only view state
//! — selection, filter text, focus — never a copy of anything here.
//!
//! Two rules it exists to keep:
//!
//! * **Nothing Git touches the render thread.** Every read goes out on the
//!   background executor and comes back through a foreground task. The guard in
//!   `omagit-git` panics in debug if that is ever got wrong, and this is the
//!   only place in the app that could.
//! * **A stale answer never wins.** Clicking through four repositories starts
//!   four reads that finish out of order; each carries the generation it was
//!   asked at, and an older one is dropped rather than drawn.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gpui_kit::{AppContext, Context, Entity};
use omagit_git::{Cancel, GitError, Repository, Summary};
use omagit_settings::{Entry, Library, Location};

use crate::async_state::{AsyncState, Generation};

pub struct Store {
    /// `None` when the platform gave us nowhere to write: the app still runs,
    /// it simply forgets between sessions, and says so in the log.
    config_dir: Option<PathBuf>,
    library: Library,
    summaries: HashMap<PathBuf, AsyncState<Summary>>,
    /// The generation each path has most recently been asked at.
    asked: HashMap<PathBuf, Generation>,
    latest: Generation,
    /// The repository the user opened. M4 turns this into a screen; until then
    /// it is what the topbar names.
    open: Option<PathBuf>,
    /// The token the in-flight reads poll. Replaced wholesale when the whole
    /// list is refreshed, which cancels everything still running.
    cancel: Cancel,
}

impl Store {
    pub fn new(config_dir: Option<PathBuf>, cx: &mut Context<Self>) -> Self {
        let library = match config_dir.as_deref() {
            Some(dir) => Library::load(dir),
            None => Library::default(),
        };
        let mut store = Self {
            config_dir,
            library,
            summaries: HashMap::new(),
            asked: HashMap::new(),
            latest: Generation::default(),
            open: None,
            cancel: Cancel::new(),
        };
        store.refresh_all(cx);
        store
    }

    pub fn library(&self) -> &Library {
        &self.library
    }

    pub fn open_repository(&self) -> Option<&Path> {
        self.open.as_deref()
    }

    /// What is known about the repository at `path`.
    pub fn summary(&self, path: &Path) -> &AsyncState<Summary> {
        static IDLE: AsyncState<Summary> = AsyncState::Idle;
        self.summaries.get(path).unwrap_or(&IDLE)
    }

    /// Re-read everything, cancelling whatever is still in flight.
    pub fn refresh_all(&mut self, cx: &mut Context<Self>) {
        self.cancel.cancel();
        self.cancel = Cancel::new();
        let paths: Vec<PathBuf> = self
            .library
            .iter()
            .map(|(_, entry)| entry.path.clone())
            .collect();
        for path in paths {
            self.refresh(&path, cx);
        }
    }

    /// Re-read one repository.
    pub fn refresh(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.latest = self.latest.next();
        let generation = self.latest;
        self.asked.insert(path.to_path_buf(), generation);
        self.summaries
            .entry(path.to_path_buf())
            .or_default()
            .begin();

        let path = path.to_path_buf();
        let cancel = self.cancel.clone();
        cx.spawn(async move |this, cx| {
            let read = {
                let path = path.clone();
                // Blocking Git work, on the background executor. The future has
                // to be `Send`, which is exactly why `Repository` is a handle
                // over `gix`'s thread-safe half rather than a live view.
                cx.background_spawn(async move {
                    let repo = Repository::open(&path)?;
                    Summary::load(&repo, &cancel)
                })
                .await
            };
            this.update(cx, |store, cx| {
                let current = store.asked.get(&path).copied().unwrap_or(generation);
                let state = store.summaries.entry(path).or_default();
                if state.finish(read, generation, current) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Add a repository, from a path the user picked or dropped.
    ///
    /// The path is taken as given rather than searched upwards: dropping a
    /// subdirectory of a repository and silently getting its root would be a
    /// surprise. [`Repository::discover`] is the caller's to run first if it
    /// wants that.
    pub fn add(&mut self, path: PathBuf, group: Option<usize>, cx: &mut Context<Self>) -> Location {
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        let at = self.library.add(Entry::new(path.clone()), group);
        self.persist();
        self.refresh(&path, cx);
        at
    }

    /// Take a repository out of the list. Nothing on disk is touched, which is
    /// why this needs no confirmation (SPEC §3 rule 7 is about destruction).
    pub fn remove(&mut self, at: Location, cx: &mut Context<Self>) -> Option<Entry> {
        let removed = self.library.remove(at)?;
        self.summaries.remove(&removed.path);
        self.asked.remove(&removed.path);
        if self.open.as_deref() == Some(removed.path.as_path()) {
            self.open = None;
        }
        self.persist();
        cx.notify();
        Some(removed)
    }

    pub fn move_entry(
        &mut self,
        from: Location,
        to: Location,
        cx: &mut Context<Self>,
    ) -> Option<Location> {
        let at = self.library.move_entry(from, to)?;
        self.persist();
        cx.notify();
        Some(at)
    }

    pub fn add_group(&mut self, name: impl Into<String>, cx: &mut Context<Self>) -> usize {
        let index = self.library.add_group(name);
        self.persist();
        cx.notify();
        index
    }

    pub fn toggle_group(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(group) = self.library.groups.get_mut(index) {
            group.collapsed = !group.collapsed;
            self.persist();
            cx.notify();
        }
    }

    /// Edit the user's note. Stored in the library, not in the repository: Git
    /// has no such field, and writing one into `.git/config` would be omagit
    /// leaving traces in someone else's repository.
    pub fn set_description(&mut self, at: Location, text: String, cx: &mut Context<Self>) {
        if let Some(entry) = self.library.get_mut(at)
            && entry.description != text
        {
            entry.description = text;
            self.persist();
            cx.notify();
        }
    }

    /// Point an entry at a folder the user found again, keeping its name,
    /// description and place in the list.
    pub fn relocate(&mut self, at: Location, path: PathBuf, cx: &mut Context<Self>) {
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        let Some(entry) = self.library.get_mut(at) else {
            return;
        };
        let previous = std::mem::replace(&mut entry.path, path.clone());
        self.summaries.remove(&previous);
        self.asked.remove(&previous);
        self.persist();
        self.refresh(&path, cx);
    }

    /// Record that the user opened this repository, and make it the open one.
    pub fn mark_opened(&mut self, at: Location, cx: &mut Context<Self>) {
        let Some(entry) = self.library.get_mut(at) else {
            return;
        };
        entry.last_opened = Some(now_seconds());
        let path = entry.path.clone();
        self.open = Some(path);
        self.persist();
        cx.notify();
    }

    /// How many repositories are no longer where they were left.
    pub fn missing(&self) -> impl Iterator<Item = (Location, &Entry)> {
        self.library.iter().filter(|(_, entry)| {
            matches!(
                self.summary(&entry.path).error(),
                Some(GitError::RepositoryMissing(_) | GitError::NotARepository(_))
            )
        })
    }

    fn persist(&self) {
        let Some(dir) = self.config_dir.as_deref() else {
            return;
        };
        if let Err(error) = self.library.save(dir) {
            // Not fatal, and not silent: the list is still correct in memory,
            // and the next write may well succeed.
            tracing::error!(%error, "could not save the repository list");
        }
    }
}

/// A store that never touches the disk — for tests, and for the case where the
/// platform has no config directory.
impl Store {
    pub fn in_memory(cx: &mut Context<Self>) -> Self {
        Self::new(None, cx)
    }
}

pub fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

/// A weak handle to the store, for the places that need to reach it from a
/// callback without keeping it alive.
pub type StoreHandle = Entity<Store>;

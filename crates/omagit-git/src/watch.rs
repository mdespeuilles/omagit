//! Noticing that a repository changed underneath us.
//!
//! A Git client that only refreshes when clicked is wrong every time the user
//! touches a terminal, and one that refreshes on every filesystem event spends
//! a build recomputing a status it will throw away. SPEC §10 asks for three
//! things, and this is all three:
//!
//! * **Debounced at 150 ms.** A save is several events, a `git commit` is
//!   dozens, and `cargo build` is tens of thousands. They coalesce into one
//!   notification carrying the union of what changed.
//! * **`.git/index.lock` ignored.** Git creates and removes it around every
//!   write. Reacting to it means reacting to Git reacting to us.
//! * **Targeted.** A write under `.git/refs/` does not invalidate the working
//!   copy, and a change to a file does not invalidate the branch list. The
//!   caller is told *what* changed and re-reads only that.
//!
//! ## What "ignored" means here
//!
//! Files excluded by `.gitignore` produce no invalidation: a build writing into
//! `target/` must not cost a status per 150 ms. The exclusion is asked of the
//! repository itself rather than pattern-matched here, so it is the same answer
//! `git status` would give, `.gitignore` files nested anywhere included.
//!
//! They still produce *events*, though, and that is the limitation worth
//! stating: the watch is recursive, so the platform still hands us every write
//! under `target/` or `node_modules/` and still spends a descriptor on every
//! directory there. On Linux that is one inotify watch per directory against
//! `fs.inotify.max_user_watches`. Walking the tree and watching only the
//! directories Git cares about would fix both, and is the shape to reach for if
//! the descriptor limit is ever actually hit — [`Watcher::start`] reports that
//! failure rather than swallowing it, so it will be visible when it happens
//! instead of looking like a client that stopped refreshing.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher as _};

use crate::{GitError, Repository, Result};

/// How long to wait for the rest of a burst before reporting it.
///
/// SPEC §10 names 150 ms. Short enough that a save feels immediate, long enough
/// that `git commit` — which rewrites the index, moves `HEAD`, and appends to
/// two reflogs — arrives as one notification rather than five.
pub const DEBOUNCE: Duration = Duration::from_millis(150);

/// What changed, in the terms the caller invalidates by.
///
/// A set rather than a single value: one debounce window routinely covers more
/// than one kind, and a commit covers four.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// A file in the work tree. Invalidates the status.
    pub working_copy: bool,
    /// The index. Invalidates the status, and nothing else.
    pub index: bool,
    /// `HEAD` moved: a checkout, a commit, a rebase step.
    pub head: bool,
    /// A branch or tag moved. Invalidates the reference list and every
    /// ahead/behind count.
    pub refs: bool,
    /// A merge, rebase, cherry-pick or bisect started or finished.
    pub operation: bool,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Everything a working-copy status depends on.
    pub fn invalidates_status(&self) -> bool {
        self.working_copy || self.index || self.head || self.operation
    }

    /// Everything the branch list and its divergence counts depend on.
    pub fn invalidates_refs(&self) -> bool {
        self.refs || self.head
    }

    fn merge(&mut self, other: Self) {
        self.working_copy |= other.working_copy;
        self.index |= other.index;
        self.head |= other.head;
        self.refs |= other.refs;
        self.operation |= other.operation;
    }
}

/// A running watch. Dropping it stops the thread and closes the channel.
pub struct Watcher {
    /// `Option` so [`Drop`] can release it *before* joining. `Drop::drop` runs
    /// ahead of the fields, so a join that came first would wait on a thread
    /// still being fed by a live watcher.
    inner: Option<notify::RecommendedWatcher>,
    /// How the loop is actually told to stop.
    ///
    /// Inferring shutdown from the event channel closing does not work:
    /// dropping the `notify` watcher does not reliably drop the handler that
    /// holds the sender, so the receive never returns and the join below waits
    /// forever. Asking explicitly costs one atomic load per idle wakeup and
    /// cannot be got wrong by a dependency's drop order.
    stop: Arc<AtomicBool>,
    /// Joined rather than detached, so a watcher that has been dropped has
    /// actually stopped by the time the drop returns — no thread left holding a
    /// repository open behind a screen that closed.
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        drop(self.inner.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Watcher {
    /// Start watching `repo`, and receive coalesced change sets.
    ///
    /// Fails rather than degrading silently: a client that quietly stops
    /// noticing changes is indistinguishable from one that is broken, and the
    /// most likely cause — the platform's watch limit — is something the user
    /// can act on once told.
    pub fn start(repo: &Repository) -> Result<(Self, async_channel::Receiver<Changes>)> {
        crate::assert_off_render_thread();
        // Resolved once, so the paths watched are the paths the platform will
        // report back. macOS is why: `/var` is a symlink to `/private/var` and
        // FSEvents reports the resolved form, so a watch registered on the
        // unresolved path delivers events whose prefix does not match — and
        // every working-copy change is silently dropped. A repository reached
        // through any symlink has the same problem on any platform.
        let git_dir = resolved(repo.git_dir());
        let work_dir = repo.work_dir().map(resolved);

        let (raw_tx, raw_rx) = mpsc::channel();
        let mut inner = notify::recommended_watcher(move |event| {
            // A send failure means the classifier is gone, which happens on
            // drop and is not worth reporting.
            let _ = raw_tx.send(event);
        })
        .map_err(watch_error)?;

        // The work tree covers `.git` too when it lives inside, which is the
        // ordinary case; the second watch is for a `.git` that does not — a
        // linked worktree, or a repository with a `.git` file.
        if let Some(work_dir) = &work_dir {
            inner
                .watch(work_dir, RecursiveMode::Recursive)
                .map_err(watch_error)?;
        }
        if !git_dir.starts_with(work_dir.as_deref().unwrap_or(Path::new("/"))) {
            inner
                .watch(&git_dir, RecursiveMode::Recursive)
                .map_err(watch_error)?;
        }

        let (tx, rx) = async_channel::unbounded();
        let classifier = Classifier::new(repo, git_dir, work_dir);
        let stop = Arc::new(AtomicBool::new(false));
        let thread = std::thread::Builder::new()
            .name("omagit-repo-watch".into())
            .spawn({
                let stop = Arc::clone(&stop);
                move || debounce_loop(raw_rx, classifier, tx, stop)
            })
            .map_err(|source| GitError::Io {
                path: PathBuf::from("omagit-repo-watch"),
                source,
            })?;

        Ok((
            Self {
                inner: Some(inner),
                stop,
                thread: Some(thread),
            },
            rx,
        ))
    }
}

/// A path with its symlinks resolved, or the path itself if it cannot be.
///
/// Falling back rather than failing: a repository whose path cannot be resolved
/// is one that has just been unmounted or deleted, and the watch that follows
/// will report that far more usefully than an error here.
fn resolved(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn watch_error(error: notify::Error) -> GitError {
    GitError::backend("watching the repository for changes", error)
}

/// Collect events until they stop arriving for [`DEBOUNCE`], then report the
/// union.
///
/// Deliberately *not* a timeout from the first event: a `cargo build` writes
/// continuously for a minute, and reporting every 150 ms throughout would be a
/// status recomputation every 150 ms for a minute. It reports when the burst
/// ends, which is when the answer is worth having.
fn debounce_loop(
    events: mpsc::Receiver<notify::Result<notify::Event>>,
    mut classifier: Classifier,
    tx: async_channel::Sender<Changes>,
    stop: Arc<AtomicBool>,
) {
    /// How long the loop sleeps when there is nothing pending.
    ///
    /// Only ever a shutdown latency: a watcher dropped now is joined within
    /// this, and the cost of the wakeup is one atomic load per half-second per
    /// open repository.
    const IDLE: Duration = Duration::from_millis(500);

    let mut pending = Changes::default();
    // Bounded so a genuinely continuous writer — a log being appended to, a
    // long build — still gets an answer rather than being starved forever.
    const MAX_HOLD: Duration = Duration::from_secs(2);
    let mut first_seen: Option<Instant> = None;

    loop {
        if stop.load(Ordering::Acquire) {
            return;
        }
        // Pending: wait only for the rest of the burst. Idle: wake often enough
        // to notice a shutdown, rarely enough not to matter.
        match events.recv_timeout(if pending.is_empty() { IDLE } else { DEBOUNCE }) {
            Ok(Ok(event)) => {
                for path in &event.paths {
                    pending.merge(classifier.classify(path));
                }
                if !pending.is_empty() {
                    let started = *first_seen.get_or_insert_with(Instant::now);
                    if started.elapsed() >= MAX_HOLD {
                        if tx.send_blocking(std::mem::take(&mut pending)).is_err() {
                            return;
                        }
                        first_seen = None;
                    }
                }
            }
            // A watcher error is not fatal: notify reports overflows and
            // transient permission problems this way, and the next event is
            // usually fine. It is logged rather than swallowed.
            Ok(Err(error)) => tracing::debug!(%error, "filesystem watch error"),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if !pending.is_empty() {
                    if tx.send_blocking(std::mem::take(&mut pending)).is_err() {
                        return;
                    }
                    first_seen = None;
                }
            }
            // The watcher was dropped.
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Turns a path into what it invalidates.
struct Classifier {
    git_dir: PathBuf,
    work_dir: Option<PathBuf>,
    /// The repository, kept to answer "is this path ignored?" — and to rebuild
    /// the exclusion stack when the index changes, since that is when
    /// `.gitignore` may have been staged.
    repo: Repository,
    excludes: Option<Excludes>,
}

impl Classifier {
    fn new(repo: &Repository, git_dir: PathBuf, work_dir: Option<PathBuf>) -> Self {
        Self {
            git_dir,
            work_dir,
            repo: repo.clone(),
            excludes: None,
        }
    }

    fn classify(&mut self, path: &Path) -> Changes {
        let mut changes = Changes::default();
        if let Ok(inside) = path.strip_prefix(&self.git_dir) {
            self.classify_git_dir(inside, &mut changes);
            return changes;
        }
        let Some(work_dir) = self.work_dir.clone() else {
            return changes;
        };
        let Ok(relative) = path.strip_prefix(&work_dir) else {
            return changes;
        };
        if relative.as_os_str().is_empty() || self.is_ignored(relative) {
            return changes;
        }
        changes.working_copy = true;
        changes
    }

    fn classify_git_dir(&mut self, inside: &Path, changes: &mut Changes) {
        let text = inside.to_string_lossy();
        // Git writes and unlinks a lock beside almost everything it touches.
        // Reacting to those is reacting to ourselves.
        if text.ends_with(".lock") {
            return;
        }
        match text.as_ref() {
            "index" => {
                changes.index = true;
                // `.gitignore` may have been staged, so the exclusion stack is
                // no longer trustworthy.
                self.excludes = None;
            }
            "HEAD" => changes.head = true,
            "packed-refs" => changes.refs = true,
            "MERGE_HEAD" | "CHERRY_PICK_HEAD" | "REVERT_HEAD" | "BISECT_LOG" | "REBASE_HEAD" => {
                changes.operation = true
            }
            _ if text.starts_with("refs/") => changes.refs = true,
            _ if text.starts_with("rebase-merge") || text.starts_with("rebase-apply") => {
                changes.operation = true;
            }
            // `objects/`, `logs/`, `hooks/`, `config`, everything else: either
            // uninteresting, or already covered by the ref or index change that
            // accompanies it.
            _ => {}
        }
    }

    /// Whether Git would ignore this path.
    ///
    /// Asked of the repository rather than pattern-matched here, so nested
    /// `.gitignore` files, the global excludes file and `.git/info/exclude` all
    /// count — the same answer `git status` gives.
    fn is_ignored(&mut self, relative: &Path) -> bool {
        if self.excludes.is_none() {
            self.excludes = Excludes::build(&self.repo);
        }
        self.excludes
            .as_mut()
            .is_some_and(|excludes| excludes.is_excluded(relative))
    }
}

/// The repository's exclusion stack, kept alive between events.
struct Excludes {
    repo: gix::Repository,
    index: gix::index::File,
}

impl Excludes {
    fn build(repo: &Repository) -> Option<Self> {
        let gix = repo.gix();
        let index = gix.index_or_empty().ok()?;
        Some(Self {
            repo: gix,
            index: (**index).clone(),
        })
    }

    fn is_excluded(&mut self, relative: &Path) -> bool {
        let Ok(mut stack) = self.repo.excludes(
            &self.index,
            None,
            gix::worktree::stack::state::ignore::Source::WorktreeThenIdMappingIfNotSkipped,
        ) else {
            return false;
        };
        let path = gix::path::into_bstr(relative);
        // The mode matters: a directory and a file with the same name can be
        // excluded by different patterns (`target` versus `target/`). The
        // filesystem is the only source for it, and by the time an event
        // arrives the entry may be gone — in which case treating it as a file
        // is the safer guess, because it errs towards *not* ignoring.
        let mode = std::fs::metadata(self.repo.workdir().unwrap_or(Path::new(".")).join(relative))
            .ok()
            .filter(std::fs::Metadata::is_dir)
            .map(|_| gix::index::entry::Mode::DIR);
        stack
            .at_entry(path.as_ref(), mode)
            .map(|platform| platform.is_excluded())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_set_says_what_has_to_be_re_read() {
        let none = Changes::default();
        assert!(none.is_empty());
        assert!(!none.invalidates_status());

        let staged = Changes {
            index: true,
            ..Changes::default()
        };
        assert!(staged.invalidates_status());
        assert!(
            !staged.invalidates_refs(),
            "staging a file does not move a branch"
        );

        let pushed = Changes {
            refs: true,
            ..Changes::default()
        };
        assert!(
            !pushed.invalidates_status(),
            "a write under .git/refs must not cost a working-copy scan — SPEC §10 names this one"
        );
        assert!(pushed.invalidates_refs());
    }

    #[test]
    fn merging_takes_the_union() {
        let mut changes = Changes {
            working_copy: true,
            ..Changes::default()
        };
        changes.merge(Changes {
            head: true,
            ..Changes::default()
        });
        assert!(changes.working_copy && changes.head);
        assert!(!changes.refs);
    }
}

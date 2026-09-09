//! The serialized write queue, one per repository.
//!
//! SPEC §10 is blunt about it: two writing Git commands must never run in
//! parallel on the same repository. They contend for `.git/index.lock`, and the
//! one that loses reports a lock failure the user did nothing to cause — or
//! worse, two commands interleave and the index ends up describing neither.
//!
//! So writes go in a queue and leave it one at a time. The queue lives on the
//! [`crate::repo_store::RepoStore`], which is already per repository, so the
//! serialization is a property of where it sits rather than a lock anyone has
//! to remember to take.
//!
//! Reads are not queued. They take no lock — `GIT_OPTIONAL_LOCKS=0` in
//! `omagit-git`'s environment scrubbing — and blocking a status behind a commit
//! would make the screen stale for no gain.

use std::collections::VecDeque;
use std::sync::Arc;

use omagit_git::cli::Git;
use omagit_git::ops::{CommitOptions, CommitOutcome};
use omagit_git::patch::Selection;
use omagit_git::{Cancel, FileDiff, GitError, RepoPath, Repository, Result};

/// One thing to do to a repository.
///
/// Carries its own copy of the diff it acts on: by the time the queue reaches
/// it, the store may have refreshed and replaced the one it was built from, and
/// applying a patch built from a diff that no longer describes the file is
/// exactly how a write lands somewhere it was not meant to. `git apply` refuses
/// a patch whose context has moved, so a stale entry fails loudly rather than
/// corrupting anything — but it fails, and the message says why.
#[derive(Clone, Debug)]
pub enum Write {
    Stage(Target),
    Unstage(Target),
    Discard {
        target: Target,
        /// An untracked file has nothing to be restored to, so discarding it
        /// means removing it. Carried rather than re-derived, because the
        /// status it came from is what knew.
        untracked: bool,
    },
    Commit {
        message: String,
        options: CommitOptions,
    },
}

/// What a write acts on.
///
/// The two are genuinely different operations, not one with a flag: a whole
/// file goes through `git add` and needs only a path, while a hunk or a set of
/// lines goes through a patch and needs the diff it was built from. Modelling
/// them as one meant a checkbox that did nothing until the file had been
/// opened, because a diff it did not need had not been read yet.
#[derive(Clone, Debug)]
pub enum Target {
    File(RepoPath),
    Part {
        file: Arc<FileDiff>,
        selection: Selection,
    },
}

impl Target {
    pub fn path(&self) -> &RepoPath {
        match self {
            Target::File(path) => path,
            Target::Part { file, .. } => &file.path,
        }
    }

    fn describe(&self) -> String {
        let what = match self {
            Target::File(_) => "le fichier".to_owned(),
            Target::Part { selection, .. } => match selection {
                Selection::File => "le fichier".to_owned(),
                Selection::Hunks(hunks) if hunks.len() == 1 => "le bloc".to_owned(),
                Selection::Hunks(hunks) => format!("{} blocs", hunks.len()),
                Selection::Lines(lines) if lines.len() == 1 => "la ligne".to_owned(),
                Selection::Lines(lines) => format!("{} lignes", lines.len()),
            },
        };
        format!("{what} · {}", self.path().display_lossy())
    }
}

impl Write {
    /// What the interface calls this, for the confirmation and the error.
    pub fn describe(&self) -> String {
        match self {
            Write::Stage(target) => format!("Indexer {}", target.describe()),
            Write::Unstage(target) => format!("Désindexer {}", target.describe()),
            Write::Discard { target, .. } => format!("Rejeter {}", target.describe()),
            Write::Commit { options, .. } if options.amend => "Corriger le commit".to_owned(),
            Write::Commit { .. } => "Commiter".to_owned(),
        }
    }

    /// Whether this can lose work, and so needs confirming before it is queued
    /// (SPEC §3 rule 7).
    pub fn is_destructive(&self) -> bool {
        match self {
            Write::Discard { .. } => true,
            Write::Commit { options, .. } => options.amend,
            Write::Stage(_) | Write::Unstage(_) => false,
        }
    }

    /// Run it. On the background executor, never on the render thread — the
    /// guard inside `omagit-git` enforces that.
    pub fn run(&self, git: &Git, repo: &Repository, cancel: &Cancel) -> Result<Done> {
        use omagit_git::ops;
        match self {
            Write::Stage(Target::File(path)) => {
                ops::stage_file(git, repo, path, cancel).map(|()| Done::Staged)
            }
            Write::Stage(Target::Part { file, selection }) => {
                ops::stage(git, repo, file, selection, cancel).map(|()| Done::Staged)
            }
            Write::Unstage(Target::File(path)) => {
                ops::unstage_file(git, repo, path, cancel).map(|()| Done::Staged)
            }
            Write::Unstage(Target::Part { file, selection }) => {
                ops::unstage(git, repo, file, selection, cancel).map(|()| Done::Staged)
            }
            Write::Discard {
                target: Target::File(path),
                untracked,
            } => ops::discard_file(git, repo, path, *untracked, cancel).map(|()| Done::Staged),
            Write::Discard {
                target: Target::Part { file, selection },
                untracked,
            } => {
                ops::discard(git, repo, file, selection, *untracked, cancel).map(|()| Done::Staged)
            }
            Write::Commit { message, options } => {
                ops::commit(git, repo, message, options, cancel).map(Done::Committed)
            }
        }
    }
}

/// What a write produced, for the caller that has to react to it.
#[derive(Clone, Debug)]
pub enum Done {
    /// The index or the working tree moved; the screen re-reads.
    Staged,
    Committed(CommitOutcome),
}

/// The queue itself: what is running, what is waiting, and what went wrong.
#[derive(Default)]
pub struct Queue {
    running: Option<Write>,
    pending: VecDeque<Write>,
    /// The last failure, kept until something succeeds. SPEC §10 wants the
    /// failed state rendered rather than swallowed, and a write that failed is
    /// not visible anywhere else — the repository simply did not change.
    failure: Option<(String, GitError)>,
}

impl Queue {
    /// Add a write. The caller always pumps afterwards; [`Queue::start`] is
    /// what decides whether anything may begin.
    pub fn push(&mut self, write: Write) {
        self.pending.push_back(write);
    }

    /// Take the next write, if nothing is running.
    pub fn start(&mut self) -> Option<Write> {
        if self.running.is_some() {
            return None;
        }
        let next = self.pending.pop_front()?;
        self.running = Some(next.clone());
        Some(next)
    }

    /// Record the outcome of the write that was running, and take it out.
    pub fn finish(&mut self, outcome: Result<Done>) -> Result<Done> {
        let write = self.running.take();
        match &outcome {
            Ok(_) => self.failure = None,
            Err(_) => {
                // Everything queued behind a failure was decided against a
                // state that no longer holds — the file the user was looking at
                // did not change the way they were told it would. Continuing
                // would apply the rest to a repository they have not seen.
                self.pending.clear();
            }
        }
        match (write, outcome) {
            (Some(write), Err(error)) => {
                let description = write.describe();
                self.failure = Some((description, error));
                Err(GitError::backend("the previous write", AlreadyReported))
            }
            (_, outcome) => outcome,
        }
    }

    pub fn running(&self) -> Option<&Write> {
        self.running.as_ref()
    }

    pub fn waiting(&self) -> usize {
        self.pending.len()
    }

    pub fn is_busy(&self) -> bool {
        self.running.is_some() || !self.pending.is_empty()
    }

    pub fn failure(&self) -> Option<(&str, &GitError)> {
        self.failure
            .as_ref()
            .map(|(description, error)| (description.as_str(), error))
    }

    pub fn clear_failure(&mut self) {
        self.failure = None;
    }
}

/// Stands in for an error already stored on the queue, so a caller that
/// propagates does not report it twice.
#[derive(Debug)]
struct AlreadyReported;

impl std::fmt::Display for AlreadyReported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("already recorded on the queue")
    }
}

impl std::error::Error for AlreadyReported {}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use std::sync::Arc;

    fn a_file() -> Arc<FileDiff> {
        Arc::new(FileDiff {
            path: RepoPath::from_bytes(b"src/main.rs".to_vec()),
            change: omagit_git::diff::FileChange::Modified,
            content: omagit_git::diff::DiffContent::Empty,
            mode: omagit_git::diff::MODE_FILE,
        })
    }

    fn a_write(n: usize) -> Write {
        Write::Commit {
            message: format!("commit {n}"),
            options: CommitOptions::default(),
        }
    }

    #[test]
    fn one_write_runs_at_a_time() {
        // The whole point: SPEC §10 forbids two writing commands at once on one
        // repository.
        let mut queue = Queue::default();
        queue.push(a_write(1));
        queue.push(a_write(2));

        let first = queue.start().expect("the first write starts");
        assert!(
            queue.start().is_none(),
            "nothing else may start while one is running"
        );
        assert_eq!(queue.waiting(), 1);

        let _ = queue.finish(Ok(Done::Staged));
        let second = queue.start().expect("the second follows");
        assert_ne!(first.describe(), "", "writes describe themselves");
        assert!(matches!(second, Write::Commit { .. }));
    }

    #[test]
    fn a_failure_drops_what_was_queued_behind_it() {
        // Everything waiting was decided against a state that no longer holds.
        let mut queue = Queue::default();
        queue.push(a_write(1));
        queue.push(a_write(2));
        queue.push(a_write(3));
        queue.start();

        let reported = queue.finish(Err(GitError::GitNotFound));
        assert!(reported.is_err());
        assert_eq!(
            queue.waiting(),
            0,
            "the rest is dropped, not applied blindly"
        );
        assert!(!queue.is_busy());
        let (description, _) = queue.failure().expect("the failure is kept for the banner");
        assert!(
            description.starts_with("Commiter"),
            "the banner names the write that failed, not the one after it"
        );
    }

    #[test]
    fn a_success_clears_the_last_failure() {
        let mut queue = Queue::default();
        queue.push(a_write(1));
        queue.start();
        let _ = queue.finish(Err(GitError::GitNotFound));
        assert!(queue.failure().is_some());

        queue.push(a_write(2));
        queue.start();
        let _ = queue.finish(Ok(Done::Staged));
        assert!(queue.failure().is_none(), "a success clears the banner");
    }

    #[test]
    fn destructive_writes_are_the_ones_that_lose_work() {
        let file = a_file();

        assert!(
            Write::Discard {
                target: Target::Part {
                    file: file.clone(),
                    selection: Selection::File
                },
                untracked: false
            }
            .is_destructive()
        );
        assert!(
            Write::Commit {
                message: String::new(),
                options: CommitOptions {
                    amend: true,
                    ..CommitOptions::default()
                }
            }
            .is_destructive(),
            "an amend replaces a commit"
        );
        assert!(
            !Write::Stage(Target::Part {
                file: file.clone(),
                selection: Selection::File
            })
            .is_destructive(),
            "staging loses nothing: the working tree is untouched"
        );
        assert!(
            !Write::Unstage(Target::Part {
                file,
                selection: Selection::File
            })
            .is_destructive()
        );
    }

    #[test]
    fn a_target_is_described_by_what_it_covers() {
        let path = RepoPath::from_bytes(b"src/main.rs".to_vec());
        assert!(
            Target::File(path.clone())
                .describe()
                .starts_with("le fichier")
        );

        let part = |selection| Target::Part {
            file: a_file(),
            selection,
        };
        assert!(part(Selection::hunk(0)).describe().starts_with("le bloc"));

        let lines = Selection::Lines([(0, 1), (0, 2), (1, 0)].into_iter().collect());
        assert!(
            part(lines).describe().starts_with("3 lignes"),
            "a confirmation has to say how much is at stake"
        );
    }

    #[test]
    fn a_whole_file_needs_no_diff_to_be_written() {
        // The bug this pins: modelling "the file" as a selection meant the
        // checkbox on a row nobody had opened had no diff to build from, and
        // did nothing.
        let target = Target::File(RepoPath::from_bytes(b"a.txt".to_vec()));
        assert_eq!(target.path().display_lossy(), "a.txt");
        assert!(!Write::Stage(target).is_destructive());
    }
}

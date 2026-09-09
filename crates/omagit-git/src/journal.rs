//! Every `git` command omagit runs, with its exact command line.
//!
//! Two requirements meet here. SPEC §11 asks the interface for an operations
//! journal showing the exact command, so that what omagit did to a repository is
//! never a matter of trust. SPEC §15 risk 5 asks that a destructive command be
//! written down **before** it runs, so that a command that loses data leaves a
//! record even when it is the thing that crashed.
//!
//! So an entry is opened before the process is spawned and closed when it ends.
//! An entry that stays open is not a bug in the journal: it is a command that
//! did not come back, which is exactly what someone reading the journal after a
//! crash needs to see.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

/// How many entries are kept. A long session runs thousands of reads; the
/// journal is for reading back what happened, not for auditing forever.
const CAPACITY: usize = 500;

/// A shared, bounded record of what omagit asked `git` to do.
///
/// Cheap to clone: the app holds one, every background task writes through it.
#[derive(Clone, Debug, Default)]
pub struct Journal {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Debug, Default)]
struct Inner {
    entries: VecDeque<Entry>,
    next: u64,
}

/// One command.
#[derive(Clone, Debug)]
pub struct Entry {
    pub id: u64,
    /// The command line as it ran — readable, not shell-quoted.
    pub command: String,
    pub work_dir: PathBuf,
    pub started: SystemTime,
    /// Whether this command can lose work. Written down before it runs, and
    /// what the interface colours differently.
    pub destructive: bool,
    pub outcome: Outcome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Spawned and not yet finished — or never finished, if omagit died.
    Running,
    Succeeded {
        duration: Duration,
        /// `git` writes progress and warnings here even when it succeeds.
        stderr: String,
    },
    Failed {
        duration: Duration,
        stderr: String,
    },
}

impl Journal {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a command that is about to run, and hand back the handle that
    /// closes it. Called before the process is spawned, not after.
    pub fn begin(&self, command: &str, work_dir: &Path, destructive: bool) -> Record {
        let mut inner = self.lock();
        let id = inner.next;
        inner.next += 1;
        if inner.entries.len() >= CAPACITY {
            inner.entries.pop_front();
        }
        inner.entries.push_back(Entry {
            id,
            command: command.to_owned(),
            work_dir: work_dir.to_owned(),
            started: SystemTime::now(),
            destructive,
            outcome: Outcome::Running,
        });
        if destructive {
            // At warn, so it survives the default filter: this is the line
            // someone reads after losing work.
            tracing::warn!(command, dir = %work_dir.display(), "running a destructive git command");
        }
        Record {
            journal: self.clone(),
            id,
        }
    }

    /// The entries, newest last.
    pub fn entries(&self) -> Vec<Entry> {
        self.lock().entries.iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn finish(&self, id: u64, outcome: Outcome) {
        let mut inner = self.lock();
        if let Some(entry) = inner.entries.iter_mut().find(|entry| entry.id == id) {
            entry.outcome = outcome;
        }
    }

    /// A poisoned journal is not worth taking the app down for: the lock is
    /// held across nothing that can panic, so recovering the guard is safe and
    /// losing the journal would be worse than a stale entry.
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// An open entry. Closing it is the caller's job; dropping it without closing
/// leaves the command marked as still running, which is the truth.
pub struct Record {
    journal: Journal,
    id: u64,
}

impl Record {
    pub fn succeeded(self, duration: Duration, stderr: &str) {
        self.journal.finish(
            self.id,
            Outcome::Succeeded {
                duration,
                stderr: stderr.to_owned(),
            },
        );
    }

    pub fn failed(self, duration: Duration, stderr: &str) {
        self.journal.finish(
            self.id,
            Outcome::Failed {
                duration,
                stderr: stderr.to_owned(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_recorded_before_it_finishes() {
        // Risk 5: the record exists while the command is still running, which
        // is what makes it useful after a crash.
        let journal = Journal::new();
        let record = journal.begin("git reset --hard", Path::new("/repo"), true);

        let open = journal.entries();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].outcome, Outcome::Running);
        assert!(open[0].destructive);
        assert_eq!(open[0].command, "git reset --hard");

        record.succeeded(Duration::from_millis(12), "");
        assert!(matches!(
            journal.entries()[0].outcome,
            Outcome::Succeeded { .. }
        ));
    }

    #[test]
    fn a_dropped_record_stays_running() {
        // A command that never came back reads as one, rather than quietly
        // becoming a success.
        let journal = Journal::new();
        drop(journal.begin("git push", Path::new("/repo"), false));
        assert_eq!(journal.entries()[0].outcome, Outcome::Running);
    }

    #[test]
    fn the_journal_is_bounded() {
        let journal = Journal::new();
        for index in 0..CAPACITY + 10 {
            journal
                .begin(&format!("git status {index}"), Path::new("/repo"), false)
                .succeeded(Duration::ZERO, "");
        }
        assert_eq!(journal.len(), CAPACITY, "old entries are dropped");
        assert!(
            journal.entries()[0].command.ends_with(&format!("{}", 10)),
            "the oldest kept entry is the eleventh, not the first"
        );
    }
}

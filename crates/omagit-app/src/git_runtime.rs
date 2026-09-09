//! The `git` binary the app writes through, and the journal it writes into.
//!
//! Detected once, at start-up, before any window exists. SPEC §8 asks for
//! exactly that — find `git`, refuse below 2.35 with a clear message, and say so
//! when it is not in `PATH` — and the reason to do it early is that every
//! answer is one the interface has to give differently: an app that only
//! discovers `git` is missing when the user presses Commit has already let them
//! write the message.
//!
//! Detection runs a process, which is Git work, but it runs before
//! `mark_render_thread` arms the guard and before there is a window to freeze.
//! It is bounded by `omagit-git`'s own probe timeout, which is what keeps a
//! `git` on a dead network mount from turning start-up into a hang.

use std::sync::Arc;

use gpui_kit::{App, Global};
use omagit_git::cli::Git;
use omagit_git::journal::Journal;

/// What the app knows about the `git` it will run.
#[derive(Clone)]
pub struct GitRuntime {
    /// `None` when `git` is missing or too old — the interface says which, and
    /// every write is refused with that reason rather than attempted.
    git: Option<Arc<Git>>,
    journal: Journal,
    unavailable: Option<String>,
}

impl Global for GitRuntime {}

impl GitRuntime {
    /// Look for `git` and describe what was found. Called from `main`, before
    /// the render thread is armed.
    pub fn detect() -> Self {
        let journal = Journal::new();
        match Git::detect() {
            Ok(git) => {
                let git = git.with_journal(journal.clone());
                tracing::info!(version = %git.version(), "git detected");
                Self {
                    git: Some(Arc::new(git)),
                    journal,
                    unavailable: None,
                }
            }
            Err(error) => {
                // Not fatal: reading a repository needs no `git` binary at all,
                // so the app still opens and shows history and diffs. Only
                // writing is refused, and it says why.
                tracing::warn!(%error, "git is unusable; writes will be refused");
                Self {
                    git: None,
                    journal,
                    unavailable: Some(error.to_string()),
                }
            }
        }
    }

    pub fn git(&self) -> Option<&Arc<Git>> {
        self.git.as_ref()
    }

    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    /// Why writing is unavailable, if it is.
    pub fn unavailable(&self) -> Option<&str> {
        self.unavailable.as_deref()
    }
}

/// The runtime, for anything that needs to write.
pub trait ActiveGit {
    fn git_runtime(&self) -> &GitRuntime;
}

impl ActiveGit for App {
    fn git_runtime(&self) -> &GitRuntime {
        self.global::<GitRuntime>()
    }
}

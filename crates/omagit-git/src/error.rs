//! Git errors are ordinary, not exceptional.
//!
//! They travel to the UI as a state, never as a panic (SPEC §3 rule 3). Every
//! variant carries enough to render a useful message without the UI having to
//! guess.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("no repository at {0}")]
    NotARepository(PathBuf),

    #[error("{0} is gone from disk")]
    RepositoryMissing(PathBuf),

    /// `git` was not found in `PATH`. Said explicitly, because the hybrid
    /// backend (SPEC §8) cannot write or reach the network without it.
    #[error("`git` was not found in PATH")]
    GitNotFound,

    /// SPEC §8 sets the floor at 2.35 and requires a clear message below it.
    #[error("git {found} is too old; omagit needs {minimum} or newer")]
    GitTooOld { found: String, minimum: String },

    #[error("git {command} failed: {stderr}")]
    CommandFailed { command: String, stderr: String },

    /// SPEC §8 requires every subprocess to carry a deadline. Naming the
    /// command matters: the useful next question is always *which* one hung.
    #[error("git {command} did not finish within {seconds:.0}s")]
    Timeout { command: String, seconds: f32 },

    /// The user moved on — switched repository, closed the tab. Not a failure,
    /// but it travels as an error because it ends the same call stack, and the
    /// UI drops it instead of rendering it.
    #[error("cancelled")]
    Cancelled,

    /// A `gix` operation failed. The variant is deliberately coarse: `gix` has
    /// one error type per operation, roughly forty of them, and mirroring that
    /// into this enum would make the UI match on the shape of a dependency.
    /// `operation` is what the user was doing, and the source chain carries the
    /// detail into the log.
    #[error("{operation} failed: {source}")]
    Backend {
        operation: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Asked for something the repository does not have: an unknown revision, a
    /// path with no such entry, a branch that was deleted between listing and
    /// reading.
    #[error("{0} not found in this repository")]
    NotFound(String),

    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, GitError>;

impl GitError {
    /// Wrap a `gix` (or other backend) error, naming the operation the user
    /// asked for.
    pub fn backend<E>(operation: &'static str, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Backend {
            operation,
            source: Box::new(source),
        }
    }

    /// True when the error is the user's own doing and the UI should stay
    /// silent rather than show a failed state.
    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

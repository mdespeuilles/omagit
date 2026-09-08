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

    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, GitError>;

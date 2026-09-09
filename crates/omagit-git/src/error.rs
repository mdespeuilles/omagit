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

    #[error("{command} failed: {stderr}")]
    CommandFailed { command: String, stderr: String },

    /// SPEC §8 requires every subprocess to carry a deadline. Naming the
    /// command matters: the useful next question is always *which* one hung.
    #[error("{command} did not finish within {seconds:.0}s")]
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

    /// The failure without the command that caused it.
    ///
    /// `Display` puts the command first — "git ls-remote --heads -- <url>
    /// failed: fatal: could not read Username" — and that is right for a log
    /// and for the journal, where *which* command failed is the question. It is
    /// wrong beside the field somebody just typed that URL into, where the
    /// command echo is two thirds of the message and says nothing they do not
    /// already see.
    ///
    /// Only `CommandFailed` has a command to drop. Everything else is already
    /// only its reason.
    pub fn reason(&self) -> String {
        match self {
            Self::CommandFailed { stderr, .. } => stderr.clone(),
            other => other.to_string(),
        }
    }

    /// True when the error is the user's own doing and the UI should stay
    /// silent rather than show a failed state.
    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_can_be_read_without_the_command_that_caused_it() {
        let failed = GitError::CommandFailed {
            command: "git ls-remote --heads -- https://example.com/repo.git".into(),
            stderr: "fatal: could not read Username".into(),
        };
        assert_eq!(failed.reason(), "fatal: could not read Username");
        // And the whole thing is still there for the log and the journal, where
        // *which* command failed is the question.
        assert!(failed.to_string().contains("ls-remote"));
    }

    #[test]
    fn everything_else_is_already_only_its_reason() {
        let timed_out = GitError::Timeout {
            command: "git fetch".into(),
            seconds: 20.0,
        };
        assert_eq!(timed_out.reason(), timed_out.to_string());
    }
}

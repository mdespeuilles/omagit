//! Opening a repository, and the two things every screen asks it first: where
//! `HEAD` is, and whether an operation is half-finished.

use std::path::Path;
use std::sync::Arc;

use crate::{GitError, ObjectId, Result, assert_off_render_thread};

/// An open repository.
///
/// Cheap to clone and safe to send between background tasks, which is what the
/// data flow of SPEC §10 needs: one `Entity<RepoStore>` hands the same handle to
/// every job it spawns.
///
/// It holds `gix`'s thread-safe half and makes a thread-local view per call.
/// That is `gix`'s own model — the local view carries mutable caches (the pack
/// window, the object buffer) that make it fast and `!Sync` — and it is the
/// reason nothing in this crate returns a borrowed `gix` object.
#[derive(Clone)]
pub struct Repository {
    inner: Arc<gix::ThreadSafeRepository>,
}

impl Repository {
    /// Open the repository at `path`, which may be a work tree or a `.git`
    /// directory. No searching upwards: use [`Repository::discover`] for that.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        assert_off_render_thread();
        let path = path.as_ref();
        // Told apart deliberately, because the two mean different things to the
        // user: a repository that was moved or unmounted keeps its entry in the
        // sidebar with a struck-through icon (DESIGN §4), while a folder that
        // simply is not a repository is a mistake in the add dialog.
        if !path.exists() {
            return Err(GitError::RepositoryMissing(path.to_owned()));
        }
        gix::ThreadSafeRepository::open(path)
            .map(|inner| Self {
                inner: Arc::new(inner),
            })
            .map_err(|error| match error {
                gix::open::Error::NotARepository { .. } => {
                    GitError::NotARepository(path.to_owned())
                }
                other => GitError::backend("opening the repository", other),
            })
    }

    /// Open the repository that contains `path`, searching upwards — what
    /// dropping a subdirectory onto the app has to do.
    pub fn discover(path: impl AsRef<Path>) -> Result<Self> {
        assert_off_render_thread();
        let path = path.as_ref();
        if !path.exists() {
            return Err(GitError::RepositoryMissing(path.to_owned()));
        }
        gix::ThreadSafeRepository::discover(path)
            .map(|inner| Self {
                inner: Arc::new(inner),
            })
            .map_err(|error| match error {
                gix::discover::Error::Discover(_) => GitError::NotARepository(path.to_owned()),
                other => GitError::backend("discovering the repository", other),
            })
    }

    /// The `.git` directory.
    pub fn git_dir(&self) -> &Path {
        self.inner.path()
    }

    /// The work tree, or `None` for a bare repository.
    pub fn work_dir(&self) -> Option<&Path> {
        self.inner.work_dir()
    }

    pub fn is_bare(&self) -> bool {
        self.inner.work_dir().is_none()
    }

    /// Where `HEAD` points.
    pub fn head(&self) -> Result<Head> {
        assert_off_render_thread();
        let repo = self.gix();
        let head = repo
            .head()
            .map_err(|error| GitError::backend("reading HEAD", error))?;
        Ok(match head.kind {
            // A repository that has been initialised but never committed. Not
            // an error and not empty-looking either: `HEAD` already names the
            // branch the first commit will create, and the UI shows that name.
            gix::head::Kind::Unborn(name) => Head::Unborn {
                branch: short_ref_name(name.as_ref().as_bstr()),
            },
            gix::head::Kind::Detached { target, peeled } => Head::Detached {
                commit: peeled.unwrap_or(target),
            },
            gix::head::Kind::Symbolic(reference) => {
                let commit = reference
                    .target
                    .try_id()
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| GitError::NotFound("HEAD's target".into()))?;
                Head::Branch {
                    branch: short_ref_name(reference.name.as_ref().as_bstr()),
                    commit,
                }
            }
        })
    }

    /// The operation the repository is in the middle of, if any.
    ///
    /// Read from the marker files in `.git`, the same way `git status` and the
    /// shell prompt do. M8 resolves these; M2 only has to see them, because
    /// every other read has to be interpreted differently while one is running —
    /// a "modified" file during a merge may be a conflict, not an edit.
    pub fn operation(&self) -> Option<Operation> {
        assert_off_render_thread();
        use gix::state::InProgress;
        Some(match self.gix().state()? {
            InProgress::Merge => Operation::Merge,
            InProgress::Rebase => Operation::Rebase { interactive: false },
            InProgress::RebaseInteractive => Operation::Rebase { interactive: true },
            InProgress::CherryPick | InProgress::CherryPickSequence => Operation::CherryPick,
            InProgress::Revert | InProgress::RevertSequence => Operation::Revert,
            InProgress::Bisect => Operation::Bisect,
            InProgress::ApplyMailbox => Operation::ApplyMailbox,
            InProgress::ApplyMailboxRebase => Operation::Rebase { interactive: false },
        })
    }

    /// A thread-local `gix` view for one operation.
    pub(crate) fn gix(&self) -> gix::Repository {
        self.inner.to_thread_local()
    }
}

impl std::fmt::Debug for Repository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Repository")
            .field("git_dir", &self.git_dir())
            .field("work_dir", &self.work_dir())
            .finish()
    }
}

/// Where `HEAD` points, in the three shapes the UI has to draw differently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Head {
    /// On a branch, the ordinary case.
    Branch { branch: String, commit: ObjectId },
    /// On a commit, with no branch to move — the checkout has to warn before a
    /// commit is made here (M5).
    Detached { commit: ObjectId },
    /// A branch that does not exist yet, in a repository with no commits.
    Unborn { branch: String },
}

impl Head {
    /// The commit `HEAD` resolves to, or `None` in an unborn repository.
    pub fn commit(&self) -> Option<&ObjectId> {
        match self {
            Head::Branch { commit, .. } | Head::Detached { commit } => Some(commit),
            Head::Unborn { .. } => None,
        }
    }

    /// What the topbar shows.
    pub fn label(&self) -> String {
        match self {
            Head::Branch { branch, .. } | Head::Unborn { branch } => branch.clone(),
            Head::Detached { commit } => format!("detached at {}", &commit.to_string()[..7]),
        }
    }
}

/// A multi-step operation the repository is in the middle of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Merge,
    Rebase { interactive: bool },
    CherryPick,
    Revert,
    Bisect,
    ApplyMailbox,
}

impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Operation::Merge => "merge",
            Operation::Rebase { interactive: false } => "rebase",
            Operation::Rebase { interactive: true } => "interactive rebase",
            Operation::CherryPick => "cherry-pick",
            Operation::Revert => "revert",
            Operation::Bisect => "bisect",
            Operation::ApplyMailbox => "am",
        })
    }
}

/// `refs/heads/feature/x` → `feature/x`, `refs/tags/v1` → `v1`.
///
/// Kept here rather than in `refs` because `HEAD` needs it before any reference
/// is listed, and because `gix`'s own `shorten()` disambiguates against the
/// other references — correct, but it means a branch can change its displayed
/// name when an unrelated tag appears, which is not what a topbar should do.
pub(crate) fn short_ref_name(full: &gix::bstr::BStr) -> String {
    let full = full.to_string();
    for prefix in ["refs/heads/", "refs/tags/", "refs/remotes/"] {
        if let Some(rest) = full.strip_prefix(prefix) {
            return rest.to_owned();
        }
    }
    full
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortens_only_the_known_prefixes() {
        assert_eq!(short_ref_name("refs/heads/main".into()), "main");
        assert_eq!(
            short_ref_name("refs/heads/feature/nested/name".into()),
            "feature/nested/name"
        );
        assert_eq!(short_ref_name("refs/tags/v1.0".into()), "v1.0");
        assert_eq!(
            short_ref_name("refs/remotes/origin/main".into()),
            "origin/main"
        );
        // Anything else keeps its full name rather than being guessed at.
        assert_eq!(short_ref_name("refs/stash".into()), "refs/stash");
        assert_eq!(short_ref_name("HEAD".into()), "HEAD");
    }

    #[test]
    fn a_missing_directory_is_missing_not_unreadable() {
        let error = Repository::open("/nonexistent/omagit/repo").expect_err("nothing is there");
        assert!(
            matches!(error, GitError::RepositoryMissing(_)),
            "got {error:?}"
        );
    }
}

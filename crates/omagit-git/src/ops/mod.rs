//! The operations that change a repository.
//!
//! Every one of them runs the `git` binary, and that is the decision of SPEC §8
//! rather than a shortcut: the semantics here are subtle — hooks, `core.
//! autocrlf`, `commit.template`, the user's own config — and reimplementing
//! them is how a Git client corrupts data. `gix` still does every read; these
//! do every write.
//!
//! ## What is not here
//!
//! No `GitBackend` trait. SPEC §8 describes one with `GixBackend` and
//! `CliBackend` behind it, so an operation can move between them as `gix`
//! matures. Today no operation has two implementations: reads are `gix`,
//! writes are the CLI, and a trait would have exactly one implementor per
//! method. SPEC §2 and §3 are the stronger rule — an abstraction without a
//! second real implementation is debt, not preparation — and the property the
//! trait was for is already held: nothing outside this crate names a backend,
//! so moving an operation is a change here and nowhere else. Recorded in
//! ARCHITECTURE.md.

pub mod branch;
pub mod commit;
pub mod conflict;
pub mod integrate;
pub mod network;
pub mod stage;
pub mod stash;
pub mod tag;

pub use branch::{checkout, checkout_detached, create, delete, merged, rename};
pub use commit::{Author, CommitOptions, CommitOutcome, commit, committer_identity, template};
pub use integrate::{MergeOptions, abort, merge, rebase, resume};
pub use network::{
    CloneOptions, Push, PushForce, Reconcile, Step, clone, directory_for, fetch, pull, push,
    reachable, reconcile_configured,
};
pub use stage::{
    discard, discard_file, stage, stage_all, stage_file, unstage, unstage_all, unstage_file,
};

use std::path::Path;

use crate::cli::{Git, Invocation};
use crate::{GitError, Repository, Result};

/// The work tree an operation runs in.
///
/// A bare repository has none, and every write here needs one — you cannot
/// stage a file into a repository that has no files. Refused with a name rather
/// than by `git` failing halfway through.
pub(crate) fn work_dir(repo: &Repository) -> Result<&Path> {
    repo.work_dir()
        .ok_or_else(|| GitError::backend("this operation", BareRepository))
}

#[derive(Debug)]
struct BareRepository;

impl std::fmt::Display for BareRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the repository has no work tree")
    }
}

impl std::error::Error for BareRepository {}

/// A `git` invocation in the repository's work tree, with the flags SPEC §8
/// requires of every one.
pub(crate) fn at<'a>(git: &'a Git, repo: &Repository) -> Result<Invocation<'a>> {
    Ok(git.at(work_dir(repo)?))
}

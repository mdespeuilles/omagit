//! Merge and rebase, and the way out of one that stopped.
//!
//! Both bring one line of history into another and both can stop half-way, on a
//! conflict, leaving the repository in a state that is neither before nor
//! after. `Repository::operation` already reports that state — it has since M2,
//! because every other read has to be interpreted differently while one is
//! running.
//!
//! ## Why `abort` is here and `continue` is not
//!
//! Resolving a conflict is M8's. But an application that can *start* a rebase
//! and cannot stop one is a trap: it leaves the user in a state they did not
//! choose, with no way out except a terminal. `abort` is the safety valve for
//! the thing this module adds, so it ships with it.

use crate::cli::Git;
use crate::repo::Operation;
use crate::{Cancel, GitError, Repository, Result};

/// How a merge should be recorded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MergeOptions {
    /// Always make a merge commit, even when a fast-forward would do.
    ///
    /// The other direction — forcing a fast-forward — is not offered. It is
    /// `merge.ff` in the user's configuration, and a client that passed
    /// `--ff-only` of its own would refuse merges the repository is set up to
    /// accept.
    pub no_fast_forward: bool,
    /// Bring the changes in without recording the other branch as a parent,
    /// leaving them staged for a commit of your own.
    pub squash: bool,
}

/// Merge `branch` into the current one.
///
/// No `--no-edit`: `GIT_EDITOR=true` covers it for every command at once
/// (`cli.rs`), which is the difference between a rule and a habit of
/// remembering a flag.
pub fn merge(
    git: &Git,
    repo: &Repository,
    branch: &str,
    options: &MergeOptions,
    cancel: &Cancel,
) -> Result<String> {
    let mut invocation = super::at(git, repo)?
        .arg("merge")
        // It rewrites the working tree and can stop on a conflict, leaving the
        // repository in a state that is neither before nor after.
        .destructive();
    if options.no_fast_forward {
        invocation = invocation.arg("--no-ff");
    }
    if options.squash {
        invocation = invocation.arg("--squash");
    }
    invocation
        .arg(branch)
        .run(cancel)
        .map(|output| output.text().trim().to_owned())
}

/// Replay the current branch's commits on top of `onto`.
///
/// Never interactive. `rebase -i` is a conversation with an editor, and there
/// is no editor here — `GIT_SEQUENCE_EDITOR=true` would accept the default todo
/// silently, which is a plain rebase wearing a misleading name.
pub fn rebase(git: &Git, repo: &Repository, onto: &str, cancel: &Cancel) -> Result<String> {
    super::at(git, repo)?
        .arg("rebase")
        // It rewrites the branch's commits: what it replaces is reachable only
        // through the reflog.
        .destructive()
        .arg(onto)
        .run(cancel)
        .map(|output| output.text().trim().to_owned())
}

/// Put the repository back where the operation found it.
///
/// The command depends on what is running, which is why this takes the
/// operation rather than a flag: `git merge --abort` on a rebase says "no merge
/// to abort", and the message would be about the wrong thing.
pub fn abort(git: &Git, repo: &Repository, operation: Operation, cancel: &Cancel) -> Result<()> {
    let command = match operation {
        Operation::Merge => "merge",
        Operation::Rebase { .. } => "rebase",
        Operation::CherryPick => "cherry-pick",
        Operation::Revert => "revert",
        Operation::ApplyMailbox => "am",
        // `git bisect` ends with `reset`, and it is not an operation this
        // starts. Refused by name rather than sent a command that does not
        // exist.
        Operation::Bisect => {
            return Err(GitError::backend("aborting", NotAbortable(operation)));
        }
    };
    super::at(git, repo)?
        .args([command, "--abort"])
        // Abandons whatever was resolved so far, which is work nobody else has.
        .destructive()
        .run(cancel)
        .map(drop)
}

#[derive(Debug)]
struct NotAbortable(Operation);

impl std::fmt::Display for NotAbortable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "a {} is ended with `git bisect reset`, not aborted",
            self.0
        )
    }
}

impl std::error::Error for NotAbortable {}

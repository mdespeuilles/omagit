//! Merge and rebase, and the way out of one that stopped.
//!
//! Both bring one line of history into another and both can stop half-way, on a
//! conflict, leaving the repository in a state that is neither before nor
//! after. `Repository::operation` already reports that state — it has since M2,
//! because every other read has to be interpreted differently while one is
//! running.
//!
//! ## The two ways out
//!
//! `abort` shipped with M7 as the safety valve: an application that can *start*
//! a rebase and cannot stop one leaves the user somewhere they did not choose,
//! with no exit but a terminal. `resume` is M8's other half — the way *forward*,
//! once the conflicts are settled — and the two take the same argument for the
//! same reason: which command to send is read from the repository, never passed
//! in by a caller whose copy may be stale.

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

/// Carry on with the operation, now that the conflicts are settled.
///
/// `git` decides whether it may: with a path still unmerged it refuses, naming
/// the file, and that refusal is better than any check this could make first —
/// the index is the truth, and anything read here would be a copy of it taken a
/// moment earlier. The interface still disables the button while conflicts are
/// counted, so the refusal is a backstop rather than the normal path.
///
/// No `--no-edit` beyond `GIT_EDITOR=true`, which covers every command at once
/// (`cli.rs`): `git merge --continue` opens the message `git` prepared, and the
/// cover is what keeps it from waiting on an editor nobody can see.
pub fn resume(
    git: &Git,
    repo: &Repository,
    operation: Operation,
    cancel: &Cancel,
) -> Result<String> {
    let command = match operation {
        Operation::Merge => "merge",
        Operation::Rebase { .. } => "rebase",
        Operation::CherryPick => "cherry-pick",
        Operation::Revert => "revert",
        Operation::ApplyMailbox => "am",
        // `git bisect` has no `--continue`: it is driven by `good` and `bad`,
        // and it never stops on a conflict to begin with.
        Operation::Bisect => {
            return Err(GitError::backend("continuing", NotResumable(operation)));
        }
    };
    let invocation = super::at(git, repo)?.args([command, "--continue"]);
    // A merge commits what is already in the index; a replay goes on rewriting
    // commits, and can stop on the next conflict.
    let invocation = match operation {
        Operation::Merge => invocation,
        _ => invocation.destructive(),
    };
    invocation
        .run(cancel)
        .map(|output| output.text().trim().to_owned())
}

#[derive(Debug)]
struct NotResumable(Operation);

impl std::fmt::Display for NotResumable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a {} is driven by `git bisect good` and `bad`", self.0)
    }
}

impl std::error::Error for NotResumable {}

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

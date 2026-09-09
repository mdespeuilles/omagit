//! Putting work on the shelf and taking it back off.
//!
//! Every one of these is addressed by `stash@{index}`, and the index is a
//! position in a log that moves: dropping `stash@{1}` renumbers everything
//! below it. So none of them takes an index from a caller who has been holding
//! one — [`crate::stash::find`] resolves a commit to the index it has now, and
//! the resolution happens under the same write lock as the command it feeds.
//!
//! Not re-exported flat into [`crate::ops`], unlike the other modules here:
//! `apply`, `pop` and `drop` are words that mean something else in nearly every
//! other context, and `ops::stash::pop` says which shelf it is popping.

use crate::cli::Git;
use crate::{Cancel, Repository, Result};

/// Put the working copy on the shelf and leave the branch clean.
///
/// Not marked destructive: what it takes off the working tree it also stores,
/// and the entry it creates is the first row of the list the moment it returns.
/// `--include-untracked` is the one that would otherwise deserve the mark — it
/// removes files `git` has never seen — and it is still storing them, in the
/// third parent [`crate::stash::diff`] reads.
///
/// Answers with `git`'s own line, because the one worth reading is the one it
/// says when there was nothing to do: `No local changes to save`.
pub fn push(
    git: &Git,
    repo: &Repository,
    message: &str,
    untracked: bool,
    cancel: &Cancel,
) -> Result<String> {
    let mut invocation = super::at(git, repo)?.args(["stash", "push"]);
    if untracked {
        invocation = invocation.arg("--include-untracked");
    }
    let message = message.trim();
    if !message.is_empty() {
        invocation = invocation.args(["--message", message]);
    }
    invocation
        .run(cancel)
        .map(|output| output.text().trim().to_owned())
}

/// Bring one back, leaving it on the shelf.
///
/// Destructive for the same reason a merge is: it rewrites the working tree and
/// can stop half-way on a conflict, in a state that is neither before nor
/// after. What it cannot lose is the stash itself.
///
/// No `--index`. Reinstating which files were staged is a second thing to want,
/// it fails outright when the index cannot be reproduced, and nothing in the
/// window asks for it yet (SPEC §2: no code without a caller).
pub fn apply(git: &Git, repo: &Repository, index: usize, cancel: &Cancel) -> Result<String> {
    super::at(git, repo)?
        .args(["stash", "apply"])
        .arg(address(index))
        .destructive()
        .run(cancel)
        .map(|output| output.text().trim().to_owned())
}

/// Bring one back and take it off the shelf.
///
/// `git stash pop` is `apply` followed by `drop`, and it keeps the entry when
/// the apply conflicts — which is the behaviour that makes it safe and the
/// reason it is not composed here out of the two calls above.
pub fn pop(git: &Git, repo: &Repository, index: usize, cancel: &Cancel) -> Result<String> {
    super::at(git, repo)?
        .args(["stash", "pop"])
        .arg(address(index))
        .destructive()
        .run(cancel)
        .map(|output| output.text().trim().to_owned())
}

/// Throw one away without applying it.
///
/// The most destructive command in this module and the least dramatic-looking:
/// the working tree does not move, and what disappears is reachable only
/// through the reflog `git stash drop` prints on its way out. That line is the
/// answer, so a caller can show it.
pub fn drop(git: &Git, repo: &Repository, index: usize, cancel: &Cancel) -> Result<String> {
    super::at(git, repo)?
        .args(["stash", "drop"])
        .arg(address(index))
        .destructive()
        .run(cancel)
        .map(|output| output.text().trim().to_owned())
}

/// `stash@{2}`.
///
/// Composed here rather than at four call sites, and never through a shell:
/// `Invocation` spawns the binary directly, so the braces reach `git` as typed
/// and no quoting rule applies.
fn address(index: usize) -> String {
    format!("stash@{{{index}}}")
}

#[cfg(test)]
mod tests {
    use super::address;

    #[test]
    fn an_index_is_written_the_way_git_reads_it() {
        assert_eq!(address(0), "stash@{0}");
        assert_eq!(address(12), "stash@{12}");
    }
}

//! Branches: switching, creating, renaming, deleting.
//!
//! All through the `git` binary, for SPEC §8's reason and one more that is
//! sharper here than for staging. A checkout touches every file in the working
//! tree and has to respect `core.autocrlf`, `.gitattributes` filters, sparse
//! checkout and the `post-checkout` hook. Reimplementing that is not a
//! performance trade-off, it is a way to corrupt a working tree.
//!
//! ## What is *not* refused here
//!
//! Switching away from uncommitted work, and deleting a branch whose commits
//! are on no other. Both are refused by `git` itself, with a message that says
//! exactly what is in the way — and `git`'s message is better than any this
//! layer could compose, because it names the files. SPEC §3 rule 3 wants Git's
//! own words shown; the caller's job is to offer the forcing flag afterwards,
//! not to pre-empt the refusal.

use crate::cli::Git;
use crate::{Cancel, Repository, Result};

/// Switch to `name`.
///
/// `git switch`, not `git checkout`. The old command means two things — move
/// `HEAD`, and restore paths — and tells them apart by guessing, so
/// `git checkout -- feature` reads `feature` as a *path* and reports that no
/// file by that name is known. `switch` only ever moves `HEAD`, so a branch
/// whose name could also be a path is unambiguous. It arrived in Git 2.23,
/// comfortably under SPEC §8's floor of 2.35.
///
/// A remote-tracking name that exists on exactly one remote — `feature/x` when
/// only `origin/feature/x` matches — creates a local branch tracking it. That
/// is `switch`'s own behaviour and not something added here: it is what the
/// name means to everyone who has used Git, and reimplementing the
/// disambiguation would be a second, subtly different rule.
pub fn checkout(git: &Git, repo: &Repository, name: &str, cancel: &Cancel) -> Result<()> {
    super::at(git, repo)?
        .arg("switch")
        .arg(name)
        .run(cancel)
        .map(drop)
}

/// Switch to a commit, leaving `HEAD` detached.
///
/// Marked destructive, which is not obvious: nothing is deleted. What it can
/// lose is *the next commit* — one made on a detached `HEAD` is reachable only
/// through the reflog once you switch away, which is the trap this warns about
/// before it is entered rather than after (SPEC §11).
pub fn checkout_detached(
    git: &Git,
    repo: &Repository,
    commit: &str,
    cancel: &Cancel,
) -> Result<()> {
    super::at(git, repo)?
        .args(["switch", "--detach"])
        .arg(commit)
        .destructive()
        .run(cancel)
        .map(drop)
}

/// Create `name` at `start`, and optionally switch to it.
///
/// `start` is anything `git` resolves — a branch, a tag, a hash, `HEAD~3`. It
/// is passed through rather than parsed: revision syntax is a language, and
/// half-implementing it would refuse expressions that work everywhere else.
pub fn create(
    git: &Git,
    repo: &Repository,
    name: &str,
    start: Option<&str>,
    switch: bool,
    cancel: &Cancel,
) -> Result<()> {
    let mut invocation = if switch {
        super::at(git, repo)?.args(["switch", "--create"])
    } else {
        super::at(git, repo)?.arg("branch")
    };
    invocation = invocation.arg(name);
    if let Some(start) = start {
        invocation = invocation.arg(start);
    }
    invocation.run(cancel).map(drop)
}

/// Rename a branch.
///
/// Not destructive: the commits are untouched and the reflog follows the name.
/// It does break anything configured against the old name — a worktree, a CI
/// filter — but nothing Git can restore is lost.
pub fn rename(git: &Git, repo: &Repository, from: &str, to: &str, cancel: &Cancel) -> Result<()> {
    super::at(git, repo)?
        .args(["branch", "--move"])
        .arg(from)
        .arg(to)
        .run(cancel)
        .map(drop)
}

/// Delete a branch.
///
/// `force` is `-D`: it deletes a branch whose commits are on no other, and
/// those commits are then reachable only through the reflog. Without it `git`
/// refuses and says which branch is unmerged, which is the message worth
/// showing — the caller offers the force afterwards rather than guessing.
pub fn delete(
    git: &Git,
    repo: &Repository,
    name: &str,
    force: bool,
    cancel: &Cancel,
) -> Result<()> {
    let invocation = super::at(git, repo)?.arg("branch");
    let invocation = if force {
        invocation.args(["--delete", "--force"]).destructive()
    } else {
        invocation.arg("--delete")
    };
    invocation.arg(name).run(cancel).map(drop)
}

/// Every branch whose commits are all on `HEAD` too.
///
/// The whole set in one call rather than a question per branch: the sidebar
/// asks this about every row it draws, and a process each would make opening a
/// repository with forty branches forty processes.
///
/// Asked *before* a delete, so the confirmation can say which of its two
/// questions it is asking — "remove a label" or "throw away four commits".
/// `git branch --delete` would answer it too, by failing, but a refusal is a
/// poor way to find out what you are about to be asked.
pub fn merged(
    git: &Git,
    repo: &Repository,
    cancel: &Cancel,
) -> Result<std::collections::BTreeSet<String>> {
    let listed = super::at(git, repo)?
        .args(["branch", "--merged", "HEAD", "--format=%(refname:short)"])
        .run(cancel)?;
    Ok(listed
        .text()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

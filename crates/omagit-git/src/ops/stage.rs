//! Staging, unstaging and discarding — by file, by hunk, by line.
//!
//! Two routes, and which one is taken matters more than it looks.
//!
//! A **whole file** goes through the command that names it: `git add`,
//! `git restore --staged`, `git restore`. They say what is meant, they handle
//! the cases a patch cannot describe — a file that is binary, oversized, a
//! symlink, a submodule — and a command that cannot half-apply cannot
//! half-fail.
//!
//! A **hunk or a set of lines** has no such command, so it goes through a patch
//! built by [`crate::patch`] and fed to `git apply` on standard input. That is
//! what `git add -p` does, with the same flags.

use crate::cli::Git;
use crate::diff::FileDiff;
use crate::patch::{Direction, Selection, build};
use crate::{Cancel, RepoPath, Repository, Result};

/// Applying to the index, with nothing loosened.
///
/// No `--unidiff-zero`: it exists to let `git apply` accept hunks with no
/// context, which is exactly the case where it can no longer check that a hunk
/// lands where it was meant to. Our patches carry their context, so the flag
/// would buy nothing and cost the check that catches a misaddressed patch.
const APPLY: [&str; 2] = ["apply", "--cached"];

/// Add a whole file to the index.
///
/// `git add` rather than a patch, and it needs only a path: it stages a
/// deletion, a mode change and a binary file, none of which a text patch
/// describes — and asking for the diff first would mean a checkbox that does
/// nothing until the file has been opened.
pub fn stage_file(git: &Git, repo: &Repository, path: &RepoPath, cancel: &Cancel) -> Result<()> {
    super::at(git, repo)?
        .args(["add", "--"])
        .arg(path.as_os_str())
        .run(cancel)
        .map(drop)
}

/// Add part of a file to the index, through a patch.
pub fn stage(
    git: &Git,
    repo: &Repository,
    file: &FileDiff,
    selection: &Selection,
    cancel: &Cancel,
) -> Result<()> {
    if matches!(selection, Selection::File) {
        return stage_file(git, repo, &file.path, cancel);
    }
    let patch = build(file, selection, Direction::Forward)
        .map_err(|error| crate::GitError::backend("building the patch", error))?;
    super::at(git, repo)?
        .args(APPLY)
        .arg("-")
        .input(patch)
        .run(cancel)
        .map(drop)
}

/// Take `selection` of `file` back out of the index.
///
/// `file` is the **staged** diff — `HEAD` against the index — because that is
/// what describes what would be removed.
pub fn unstage_file(git: &Git, repo: &Repository, path: &RepoPath, cancel: &Cancel) -> Result<()> {
    super::at(git, repo)?
        .args(["restore", "--staged", "--"])
        .arg(path.as_os_str())
        .run(cancel)
        .map(drop)
}

pub fn unstage(
    git: &Git,
    repo: &Repository,
    file: &FileDiff,
    selection: &Selection,
    cancel: &Cancel,
) -> Result<()> {
    match selection {
        Selection::File => unstage_file(git, repo, &file.path, cancel),
        partial => {
            let patch = build(file, partial, Direction::Reverse)
                .map_err(|error| crate::GitError::backend("building the patch", error))?;
            super::at(git, repo)?
                .args(APPLY)
                .args(["--reverse", "-"])
                .input(patch)
                .run(cancel)
                .map(drop)
        }
    }
}

/// Undo `selection` of `file` in the working tree.
///
/// Destructive, and the only operation here that is: what it removes was never
/// committed and is not in the reflog. Every invocation it makes is marked as
/// such, so the journal records it before it runs (SPEC §15 risk 5). The
/// confirmation SPEC §3 rule 7 requires is the caller's — this is the layer
/// that does it, not the one that asks.
pub fn discard_file(
    git: &Git,
    repo: &Repository,
    path: &RepoPath,
    untracked: bool,
    cancel: &Cancel,
) -> Result<()> {
    let invocation = if untracked {
        // `git restore` has nothing to restore an untracked file to. Removing
        // it is the only thing "discard" can mean, and `git clean` is the
        // command that says so.
        super::at(git, repo)?.args(["clean", "--force", "--"])
    } else {
        super::at(git, repo)?.args(["restore", "--worktree", "--"])
    };
    invocation
        .arg(path.as_os_str())
        .destructive()
        .run(cancel)
        .map(drop)
}

pub fn discard(
    git: &Git,
    repo: &Repository,
    file: &FileDiff,
    selection: &Selection,
    untracked: bool,
    cancel: &Cancel,
) -> Result<()> {
    if untracked || matches!(selection, Selection::File) {
        return discard_file(git, repo, &file.path, untracked, cancel);
    }

    match selection {
        Selection::File => unreachable!("handled above"),
        partial => {
            let patch = build(file, partial, Direction::Reverse)
                .map_err(|error| crate::GitError::backend("building the patch", error))?;
            super::at(git, repo)?
                .args(["apply", "--reverse", "-"])
                .input(patch)
                .destructive()
                .run(cancel)
                .map(drop)
        }
    }
}

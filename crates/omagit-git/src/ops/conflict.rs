//! Taking one side of a conflicted file, and saying it is settled.
//!
//! Two commands, and which pair runs depends on what kind of conflict it is.
//! `git checkout --ours` restores a version from the index — and half the
//! conflicts Git can produce have no version on one of the two sides. A file
//! deleted by us and modified by them has no stage 2: "keep ours" there means
//! *keep the deletion*, and `checkout --ours` fails with `path does not have
//! our version`, which is true and useless. So the kind decides, and the kind
//! is what `status` already reads.

use crate::cli::Git;
use crate::conflict::Side;
use crate::status::Conflict;
use crate::{Cancel, RepoPath, Repository, Result};

/// Resolve one file by keeping one side whole, and mark it settled.
///
/// Two commands rather than one, and never `git checkout` alone: restoring the
/// version leaves the path still unmerged in the index, so a reader would take
/// "resolved" from the screen while `git merge --continue` went on refusing.
pub fn take(
    git: &Git,
    repo: &Repository,
    path: &RepoPath,
    side: Side,
    conflict: Conflict,
    cancel: &Cancel,
) -> Result<()> {
    if keeps_nothing(side, conflict) {
        // The chosen side is the absence of the file. `--force` because `git
        // rm` refuses an unmerged path without it — the refusal exists so that
        // nobody removes a conflict by accident, and this is the caller that
        // means it.
        return super::at(git, repo)?
            .args(["rm", "--force", "--"])
            .arg(path.as_os_str())
            .destructive()
            .run(cancel)
            .map(drop);
    }

    super::at(git, repo)?
        .args(["checkout", side.flag(), "--"])
        .arg(path.as_os_str())
        // It overwrites the file in the work tree, markers and any hand
        // resolution already in it included.
        .destructive()
        .run(cancel)?;
    super::stage_file(git, repo, path, cancel)
}

/// Whether keeping `side` of this conflict means keeping no file at all.
///
/// The table is Git's own status codes read as a question: `DU` is "deleted by
/// us", so *our* side is the deletion; `UA` is "added by them", so ours is the
/// absence. Written out rather than discovered by running a command and reading
/// its failure — a resolution that fails half-way has already touched the work
/// tree.
fn keeps_nothing(side: Side, conflict: Conflict) -> bool {
    matches!(
        (side, conflict),
        (_, Conflict::BothDeleted)
            | (Side::Ours, Conflict::AddedByThem | Conflict::DeletedByUs)
            | (Side::Theirs, Conflict::AddedByUs | Conflict::DeletedByThem)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_side_without_a_version_is_the_one_that_removes_the_file() {
        // `DU` — we deleted it, they changed it. Keeping ours keeps the
        // deletion; keeping theirs brings their file back.
        assert!(keeps_nothing(Side::Ours, Conflict::DeletedByUs));
        assert!(!keeps_nothing(Side::Theirs, Conflict::DeletedByUs));

        // `UA` — only they have it.
        assert!(keeps_nothing(Side::Ours, Conflict::AddedByThem));
        assert!(!keeps_nothing(Side::Theirs, Conflict::AddedByThem));

        // Both changed it: both sides have a version.
        assert!(!keeps_nothing(Side::Ours, Conflict::BothModified));
        assert!(!keeps_nothing(Side::Theirs, Conflict::BothModified));

        // Neither has it. Either answer is the same answer.
        assert!(keeps_nothing(Side::Ours, Conflict::BothDeleted));
        assert!(keeps_nothing(Side::Theirs, Conflict::BothDeleted));
    }
}

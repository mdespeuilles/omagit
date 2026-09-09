//! The stashes: what is on the shelf, and what one holds.
//!
//! A stash is not a reference per entry. `refs/stash` is a single reference
//! whose *reflog* is the list, and `stash@{0}` is a position in that log rather
//! than a name anything holds. Everything awkward about stashes follows from
//! that: the list is read backwards, the numbering shifts as soon as one is
//! dropped, and the entry that says "made on main" says so in a log message
//! rather than in a field.
//!
//! Reads are `gix` (SPEC §8); [`crate::ops::stash`] does the writing.

use crate::history::{Signature, Time};
use crate::{
    Cancel, Diff, DiffOptions, GitError, ObjectId, Repository, Result, assert_off_render_thread,
};

/// One entry of `refs/stash`'s reflog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stash {
    /// Its position in the log — `git`'s own address, `stash@{index}`, with 0
    /// the most recent.
    ///
    /// Not an identity. Dropping an entry renumbers every one below it, so
    /// anything held across a write — a selection in a list, a command about to
    /// run — is addressed by [`Stash::commit`] and resolved to an index again
    /// immediately before use.
    pub index: usize,
    /// The commit the entry points at, which *is* stable.
    pub commit: ObjectId,
    /// The branch it was made on, or `None` for one made on a detached `HEAD`,
    /// where `git` writes `(no branch)`.
    pub branch: Option<String>,
    /// The row's text: the message given to `git stash push -m`, or the one
    /// `git` composes from the commit `HEAD` was on.
    pub message: String,
    /// When the entry was written, from the log rather than from the commit:
    /// that is the time `git stash list --date` shows, and the two differ for a
    /// stash created by a script and pushed later.
    pub when: Time,
    /// Made with `--include-untracked`, so it holds files that were on no
    /// index. They live in a third parent, which is why [`diff`] is not
    /// [`Diff::commit`].
    pub untracked: bool,
}

/// Every stash, newest first.
///
/// Newest first because that is what `git stash list` shows and what the index
/// in `stash@{0}` means — but the log is written oldest first, so this reads it
/// forward and reverses. `gix` offers a reverse iterator and says in its own
/// documentation that it is expensive in I/O and meant for the last few
/// entries; a shelf is short enough that reading it whole costs less.
pub fn list(repo: &Repository) -> Result<Vec<Stash>> {
    assert_off_render_thread();
    let gix = repo.gix();

    // A repository that has never stashed has no reference and no log. That is
    // an empty shelf, not a failure — the same answer `git stash list` gives.
    let Some(reference) = gix
        .try_find_reference("refs/stash")
        .map_err(|error| GitError::backend("reading refs/stash", error))?
    else {
        return Ok(Vec::new());
    };
    let mut platform = reference.log_iter();
    let Some(entries) = platform
        .all()
        .map_err(|error| GitError::backend("reading the stash log", error))?
    else {
        return Ok(Vec::new());
    };

    let mut stashes = Vec::new();
    for entry in entries {
        // A line that will not parse fails the whole list rather than being
        // skipped. Skipping would renumber every entry below it, and the number
        // is what `git stash drop` is given: a silently misnumbered list is a
        // destructive command aimed at the wrong stash.
        let line = entry.map_err(|error| GitError::backend("reading the stash log", error))?;
        let (branch, message) = describe(&line.message.to_string());
        stashes.push(Stash {
            index: 0,
            commit: line.new_oid(),
            branch,
            message,
            when: Signature::from_ref(line.signature).time,
            untracked: false,
        });
    }

    stashes.reverse();
    for (index, stash) in stashes.iter_mut().enumerate() {
        stash.index = index;
        // One object read per entry, which is what tells the list whether the
        // stash holds files that were never tracked. A missing object is not
        // worth failing the list for: the entry is still there and still
        // droppable, and "we could not tell" is answered by not claiming it.
        stash.untracked = crate::history::read_commit(&gix, stash.commit)
            .is_ok_and(|commit| commit.parents.len() >= 3);
    }
    Ok(stashes)
}

/// The one entry addressed by its commit, with the index it has *now*.
///
/// The resolution every write goes through. An index that crossed to the
/// interface and came back is a copy of a numbering that a drop may have
/// changed in between; the commit is what does not move.
pub fn find(repo: &Repository, commit: ObjectId) -> Result<Stash> {
    list(repo)?
        .into_iter()
        .find(|stash| stash.commit == commit)
        .ok_or_else(|| GitError::NotFound(format!("the stash {commit}")))
}

/// What one stash holds, against the commit it was made on.
///
/// Not [`Diff::commit`], which would compare against the first parent and stop
/// there. A stash made with `--include-untracked` keeps those files in a
/// *third* parent, whose tree is in neither side of that comparison — so a
/// preview built from it would tell someone their untracked files are not in
/// the stash, and they would go and delete them.
pub fn diff(
    repo: &Repository,
    commit: ObjectId,
    options: DiffOptions,
    cancel: &Cancel,
) -> Result<Diff> {
    assert_off_render_thread();
    let gix = repo.gix();
    let stash = crate::history::read_commit(&gix, commit)?;

    // The first parent is the commit `HEAD` was on when the stash was made, and
    // it stays the right side to compare against however far the branch has
    // moved since: a stash is what the working copy held *then*.
    let mut diff = Diff::between(
        repo,
        stash.parents.first().copied(),
        Some(commit),
        options,
        cancel,
    )?;

    if let Some(untracked) = stash.parents.get(2).copied() {
        // Against nothing, so every file reads as an addition — which is what
        // "was on no index" means.
        let mut extra = Diff::between(repo, None, Some(untracked), options, cancel)?;
        diff.files.append(&mut extra.files);
        diff.files.sort_by(|a, b| a.path.cmp(&b.path));
    }
    Ok(diff)
}

/// `WIP on main: 8f3c1a2 the subject` → (`main`, `8f3c1a2 the subject`).
///
/// The branch is split off rather than left in the text because the list draws
/// the two differently — the branch is what makes a shelf of five stashes
/// readable — and because `(no branch)` is a sentence `git` writes, not a
/// branch anyone can switch to.
fn describe(message: &str) -> (Option<String>, String) {
    let rest = message
        .strip_prefix("WIP on ")
        .or_else(|| message.strip_prefix("On "))
        .unwrap_or(message);
    // The first colon, which a branch name cannot contain (`git
    // check-ref-format` forbids it) — so a message of its own that holds one is
    // split in the right place.
    let Some((branch, text)) = rest.split_once(':') else {
        return (None, message.trim().to_owned());
    };
    let branch = branch.trim();
    (
        (branch != "(no branch)").then(|| branch.to_owned()),
        text.trim().to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::describe;

    #[test]
    fn a_composed_message_carries_the_branch_and_the_commit_it_was_on() {
        let (branch, message) = describe("WIP on main: 8f3c1a2 the subject");
        assert_eq!(branch.as_deref(), Some("main"));
        assert_eq!(message, "8f3c1a2 the subject");
    }

    #[test]
    fn a_message_of_ones_own_keeps_its_colons() {
        let (branch, message) = describe("On feature/theme: note: the runtime half");
        assert_eq!(branch.as_deref(), Some("feature/theme"));
        assert_eq!(message, "note: the runtime half");
    }

    #[test]
    fn a_detached_head_has_no_branch_to_name() {
        // `(no branch)` is a sentence git writes where a branch would go, and
        // drawing it as one would offer a switch to somewhere that is not there.
        let (branch, message) = describe("WIP on (no branch): 8f3c1a2 the subject");
        assert_eq!(branch, None);
        assert_eq!(message, "8f3c1a2 the subject");
    }

    #[test]
    fn a_line_in_no_known_shape_is_kept_whole() {
        let (branch, message) = describe("something nobody here wrote");
        assert_eq!(branch, None);
        assert_eq!(message, "something nobody here wrote");
    }
}

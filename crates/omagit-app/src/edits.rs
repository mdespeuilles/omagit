//! What a write actually does to a repository, with no Tauri in it.
//!
//! Split out of `commands` for one reason: the part worth testing is here. A
//! command's own job is to find the open repository, take its lock and hand
//! over — three lines that a test would exercise by building a Tauri
//! application. What is *not* three lines is deciding which side of the index a
//! patch has to be built from, and getting that wrong writes somewhere the user
//! did not look.

use omagit_git::cli::Git;
use omagit_git::diff::{DiffOptions, staged_file, unstaged_file};
use omagit_git::patch::Selection;
use omagit_git::status::{Status, StatusOptions};
use omagit_git::{Cancel, FileDiff, RepoPath, Repository, Result};

/// What part of a file an operation acts on.
///
/// Mirrors [`omagit_git::patch::Selection`], which is not deserialisable and
/// should not become so: a wire format is an interface concern (SPEC §3 rule 5).
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Target {
    /// The whole file, by path. Needs no diff — `git add` wants a path — which
    /// is what lets a checkbox work on a row nobody has opened.
    File,
    Hunks {
        hunks: Vec<usize>,
    },
    Lines {
        lines: Vec<(usize, usize)>,
    },
}

impl Target {
    /// `None` for a whole file, which goes through the command that names it
    /// rather than through a patch.
    fn selection(&self) -> Option<Selection> {
        match self {
            Target::File => None,
            Target::Hunks { hunks } => Some(Selection::Hunks(hunks.iter().copied().collect())),
            Target::Lines { lines } => Some(Selection::Lines(lines.iter().copied().collect())),
        }
    }
}

/// Move part of a file into the index, or take it back out.
pub fn stage(
    git: &Git,
    repo: &Repository,
    path: &RepoPath,
    target: &Target,
    unstage: bool,
    cancel: &Cancel,
) -> Result<()> {
    match target.selection() {
        None if unstage => omagit_git::ops::unstage_file(git, repo, path, cancel),
        None => omagit_git::ops::stage_file(git, repo, path, cancel),
        Some(selection) => {
            // The side a patch describes is the side it comes out of: unstaging
            // reverses the *staged* diff, staging applies the unstaged one.
            // Inverted, this builds a patch `git apply` will reject — or worse,
            // one it accepts against text that happens to match.
            let diff = file_side(repo, path, unstage, cancel)?;
            if unstage {
                omagit_git::ops::unstage(git, repo, &diff, &selection, cancel)
            } else {
                omagit_git::ops::stage(git, repo, &diff, &selection, cancel)
            }
        }
    }
}

/// Undo part of a file in the working tree.
///
/// Destructive: what it removes was never committed and is not in the reflog.
/// The confirmation SPEC §3 rule 7 requires belongs to the caller — by the time
/// this runs, the asking is over.
pub fn discard(
    git: &Git,
    repo: &Repository,
    path: &RepoPath,
    target: &Target,
    cancel: &Cancel,
) -> Result<()> {
    // An untracked file has no earlier version to be restored to, so discarding
    // it means removing it. Which of the two it is comes from the status rather
    // than from the caller: the front end would have to be told, and a stale
    // answer there deletes a file that only needed reverting.
    let untracked = entry(repo, path, cancel)?
        .as_ref()
        .is_some_and(omagit_git::StatusEntry::is_untracked);

    match target.selection() {
        None => omagit_git::ops::discard_file(git, repo, path, untracked, cancel),
        Some(selection) => {
            // Always the working tree's side. Discarding is about what is *not*
            // in the index; the staged side has its own, non-destructive verb.
            let diff = file_side(repo, path, false, cancel)?;
            omagit_git::ops::discard(git, repo, &diff, &selection, untracked, cancel)
        }
    }
}

/// Which version of a conflicted file to keep.
///
/// The wire's own copy of [`omagit_git::conflict::Side`], for the reason
/// [`Target`] is: a serialisation format is an interface concern (SPEC §3
/// rule 5), and the Git core keeps none.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Ours,
    Theirs,
}

impl From<Side> for omagit_git::conflict::Side {
    fn from(side: Side) -> Self {
        match side {
            Side::Ours => omagit_git::conflict::Side::Ours,
            Side::Theirs => omagit_git::conflict::Side::Theirs,
        }
    }
}

/// Which version of one conflict inside a file to keep.
///
/// The wire's copy of [`omagit_git::Choice`], for the reason [`Side`] is one.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Choice {
    Ours,
    Theirs,
    Both,
}

impl From<Choice> for omagit_git::Choice {
    fn from(choice: Choice) -> Self {
        match choice {
            Choice::Ours => omagit_git::Choice::Ours,
            Choice::Theirs => omagit_git::Choice::Theirs,
            Choice::Both => omagit_git::Choice::Both,
        }
    }
}

/// Keep one side of a conflicted file whole, and mark it settled.
///
/// Which *kind* of conflict it is comes from the status here rather than from
/// the caller, and that is the whole of what this function adds: half the
/// conflicts Git can produce have no version on one of the two sides — a file
/// we deleted and they changed has no "ours" to check out — so keeping a side
/// is sometimes `git rm` and sometimes `git checkout`. A front end holding that
/// answer would be holding a copy of the index, and a stale copy here restores
/// a file the reader asked to see deleted.
pub fn resolve(
    git: &Git,
    repo: &Repository,
    path: &RepoPath,
    side: Side,
    cancel: &Cancel,
) -> Result<()> {
    let conflict = entry(repo, path, cancel)?
        .and_then(|entry| entry.conflict)
        .ok_or_else(|| {
            omagit_git::GitError::NotFound(format!("un conflit sur {}", path.display_lossy()))
        })?;
    omagit_git::ops::conflict::take(git, repo, path, side.into(), conflict, cancel)
}

/// The diff a patch is built from: the staged side to unstage, the working
/// tree's to stage or discard.
pub fn file_side(
    repo: &Repository,
    path: &RepoPath,
    staged: bool,
    cancel: &Cancel,
) -> Result<FileDiff> {
    let entry = entry(repo, path, cancel)?.ok_or_else(|| {
        omagit_git::GitError::NotFound(format!(
            "{} n'est plus dans le statut",
            path.display_lossy()
        ))
    })?;

    let diff = if staged {
        staged_file(repo, &entry, DiffOptions::default())
    } else {
        unstaged_file(repo, &entry, DiffOptions::default())
    }?;

    diff.ok_or_else(|| {
        omagit_git::GitError::NotFound(format!(
            "{} n'a pas de diff de ce côté",
            path.display_lossy()
        ))
    })
}

fn entry(
    repo: &Repository,
    path: &RepoPath,
    cancel: &Cancel,
) -> Result<Option<omagit_git::StatusEntry>> {
    let status = Status::load(repo, StatusOptions::default(), cancel)?;
    Ok(status.entries.into_iter().find(|entry| entry.path == *path))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire, spelled the way the front end spells it.
    ///
    /// Neither of the other suites covers this: the front end's tests run
    /// against a fake that never serialises, and the integration tests build a
    /// `Target` in Rust. A tag that did not match would deserialise to the
    /// wrong variant or fail at runtime, and the first place anyone would find
    /// out is a button that does nothing.
    fn parse(json: &str) -> Target {
        serde_json::from_str(json).expect("the front end's shape")
    }

    #[test]
    fn the_wire_shapes_are_the_ones_ipc_ts_sends() {
        assert!(matches!(parse(r#"{"kind":"file"}"#), Target::File));

        let Target::Hunks { hunks } = parse(r#"{"kind":"hunks","hunks":[2]}"#) else {
            panic!("a hunk selection")
        };
        assert_eq!(hunks, vec![2]);

        let Target::Lines { lines } = parse(r#"{"kind":"lines","lines":[[0,1],[1,2]]}"#) else {
            panic!("a line selection")
        };
        assert_eq!(lines, vec![(0, 1), (1, 2)]);
    }

    #[test]
    fn a_side_is_spelled_the_way_the_front_end_spells_it() {
        assert!(matches!(
            serde_json::from_str::<Side>(r#""ours""#).expect("the front end's shape"),
            Side::Ours
        ));
        assert!(matches!(
            serde_json::from_str::<Side>(r#""theirs""#).expect("the front end's shape"),
            Side::Theirs
        ));
    }

    #[test]
    fn only_a_whole_file_skips_the_patch() {
        assert!(Target::File.selection().is_none());
        assert!(Target::Hunks { hunks: vec![0] }.selection().is_some());
        assert!(
            Target::Lines {
                lines: vec![(0, 0)]
            }
            .selection()
            .is_some()
        );
    }
}

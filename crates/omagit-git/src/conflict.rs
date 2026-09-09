//! Which side is which, while an operation is stopped on a conflict.
//!
//! `git` calls them *ours* and *theirs*, and the words are only honest for a
//! merge. During a rebase the commits being replayed are `theirs` — including
//! the ones you wrote a minute ago — and `ours` is the branch you are replaying
//! *onto*. A conflict UI that offered two buttons labelled with those words and
//! nothing else would be asking people to choose between two pronouns whose
//! meaning depends on a state they cannot see.
//!
//! So the sides are named: the branch, the tag or the short hash each one
//! actually is, read from the files `git` leaves behind for exactly this
//! purpose.
//!
//! Reads are `gix` and the filesystem (SPEC §8); [`crate::ops::conflict`] does
//! the writing.

use std::path::Path;

use crate::repo::{Operation, short_ref_name};
use crate::{ObjectId, Repository, Result, assert_off_render_thread};

/// The two versions a conflicted file has, named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sides {
    /// What `git checkout --ours` takes: the version already in place.
    pub ours: String,
    /// What `--theirs` takes: the version arriving.
    pub theirs: String,
    /// True while the arriving side is being *replayed* — a rebase, a
    /// cherry-pick, a revert, a patch series. It is the flag that tells the
    /// interface to explain itself: on a replay, "ours" is not the work you
    /// just did.
    pub replayed: bool,
}

/// Name the two sides of the operation in progress, or `None` when none is.
///
/// Never fails on a missing file: an operation whose marker is half-written is
/// still an operation, and a side that cannot be named falls back to the short
/// hash rather than taking the screen down.
pub fn sides(repo: &Repository) -> Result<Option<Sides>> {
    assert_off_render_thread();
    let Some(operation) = repo.operation() else {
        return Ok(None);
    };
    let git_dir = repo.git_dir();
    let here = repo.head()?.label();

    Ok(Some(match operation {
        // A merge keeps the words: we are on `ours`, and `MERGE_HEAD` is what
        // is coming in.
        Operation::Merge => Sides {
            ours: here,
            theirs: read_id(git_dir, "MERGE_HEAD")
                .map(|id| name_of(repo, id))
                .unwrap_or_else(|| "l'autre branche".to_owned()),
            replayed: false,
        },
        // A rebase turns them round. `head-name` is the branch being replayed —
        // yours — and `onto` is what it is being replayed on top of, which is
        // the side `--ours` takes.
        Operation::Rebase { .. } => {
            let dir = ["rebase-merge", "rebase-apply"]
                .into_iter()
                .find(|dir| git_dir.join(dir).is_dir());
            let read =
                |name: &str| dir.and_then(|dir| read_line(git_dir, &format!("{dir}/{name}")));
            Sides {
                ours: read("onto")
                    .and_then(|line| line.parse::<ObjectId>().ok())
                    .map(|id| name_of(repo, id))
                    .unwrap_or_else(|| "la base".to_owned()),
                theirs: read("head-name")
                    .map(|name| short_ref_name(name.as_str().into()))
                    .unwrap_or(here),
                replayed: true,
            }
        }
        Operation::CherryPick => Sides {
            ours: here,
            theirs: read_id(git_dir, "CHERRY_PICK_HEAD")
                .map(|id| name_of(repo, id))
                .unwrap_or_else(|| "le commit picoré".to_owned()),
            replayed: true,
        },
        Operation::Revert => Sides {
            ours: here,
            theirs: read_id(git_dir, "REVERT_HEAD")
                .map(|id| name_of(repo, id))
                .unwrap_or_else(|| "le commit annulé".to_owned()),
            replayed: true,
        },
        Operation::ApplyMailbox => Sides {
            ours: here,
            theirs: "le patch appliqué".to_owned(),
            replayed: true,
        },
        // Nothing conflicts during a bisect: it only ever checks commits out.
        Operation::Bisect => return Ok(None),
    }))
}

/// A commit as a reader knows it: the branch or tag that points at it, else its
/// short hash.
///
/// References are scanned rather than `gix`'s own `shorten()` being asked,
/// because the question here is the other way round — not "how do I write this
/// name" but "does anything I can name point at this commit".
fn name_of(repo: &Repository, id: ObjectId) -> String {
    let short = id.to_string()[..7].to_owned();
    let gix = repo.gix();
    let Ok(platform) = gix.references() else {
        return short;
    };
    let named = |kind: &str| -> Option<String> {
        let mut all = platform.prefixed(format!("refs/{kind}/").as_str()).ok()?;
        all.find_map(|reference| {
            let reference = reference.ok()?;
            (reference.target().try_id()? == id).then(|| short_ref_name(reference.name().as_bstr()))
        })
    };
    // Branches first, then the tracking branches, then tags: the order a reader
    // would recognise them in.
    named("heads")
        .or_else(|| named("remotes"))
        .or_else(|| named("tags"))
        .unwrap_or(short)
}

fn read_line(git_dir: &Path, name: &str) -> Option<String> {
    let text = std::fs::read_to_string(git_dir.join(name)).ok()?;
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_owned())
}

fn read_id(git_dir: &Path, name: &str) -> Option<ObjectId> {
    read_line(git_dir, name)?.parse().ok()
}

/// Which version of a conflicted file to keep.
///
/// No `Deserialize`: a wire format is an interface concern (SPEC §3 rule 5),
/// and the front end's copy of this lives in `omagit-app` the way `Target`
/// already does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Ours,
    Theirs,
}

impl Side {
    pub fn flag(self) -> &'static str {
        match self {
            Side::Ours => "--ours",
            Side::Theirs => "--theirs",
        }
    }
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Side::Ours => "ours",
            Side::Theirs => "theirs",
        })
    }
}

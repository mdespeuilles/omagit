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
use crate::{GitError, ObjectId, Repository, Result, assert_off_render_thread};

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

// ── The markers themselves ──────────────────────────────────────────────────
//
// A conflicted file is not a diff and not a merge to be recomputed: `git` has
// already written both versions into the file, between markers, and resolving
// is choosing which of them to keep. So this reads what is on disk rather than
// re-deriving anything — and it reads it as *bytes*, keeping every line's own
// terminator, because rebuilding the file has to give back CRLF where there was
// CRLF and a name that is not UTF-8 where there was one (SPEC §9).

/// The seven characters `git` writes. Not configurable: `merge.conflictStyle`
/// changes what goes *between* them, never the markers.
const OURS: &[u8] = b"<<<<<<<";
const BASE: &[u8] = b"|||||||";
const SPLIT: &[u8] = b"=======";
const THEIRS: &[u8] = b">>>>>>>";

/// One part of a conflicted file: agreed text, or a place the two sides differ.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Segment {
    /// Lines both sides have, with the line number the first of them is on.
    Agreed {
        start: usize,
        lines: Vec<String>,
    },
    Conflict(Region),
}

/// One conflict inside a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Region {
    /// Its position among the file's conflicts, from zero — what "conflit 2 / 3"
    /// counts and what a choice is addressed by.
    pub index: usize,
    /// The line the `<<<<<<<` marker is on, from one.
    pub start: usize,
    /// What `git` wrote after the markers: `HEAD`, a branch, a commit's
    /// subject. Shown as it is rather than interpreted — it is the one label
    /// that is certainly about *this* conflict.
    pub ours_label: String,
    pub theirs_label: String,
    pub ours: Vec<String>,
    pub theirs: Vec<String>,
    /// The common ancestor's version, present only under
    /// `merge.conflictStyle = diff3` or `zdiff3`. Parsed either way, because a
    /// resolution that dropped it would leave the base's lines in the file.
    pub base: Option<Vec<String>>,
}

/// A conflicted file, cut into what agrees and what does not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflicted {
    pub segments: Vec<Segment>,
    /// How many conflicts there are. Zero is not an error: a file resolved by
    /// hand outside omagit has none left, and the answer to that is to stage it.
    pub regions: usize,
}

/// Read one conflicted file's markers.
pub fn read(repo: &Repository, path: &crate::RepoPath) -> Result<Conflicted> {
    assert_off_render_thread();
    let work_dir = repo
        .work_dir()
        .ok_or_else(|| GitError::NotFound("a work tree".to_owned()))?;
    let full = path
        .to_absolute(work_dir)
        .ok_or_else(|| GitError::NotFound(path.display_lossy().into_owned()))?;
    let text = std::fs::read(&full).map_err(|source| GitError::Io { path: full, source })?;
    if crate::diff::is_binary(&text) {
        return Err(GitError::backend("reading the conflict", NotText));
    }

    let mut segments = Vec::new();
    let mut regions = 0;
    let mut agreed: Vec<String> = Vec::new();
    let mut agreed_from = 1;

    for piece in scan(&text)? {
        match piece {
            Piece::Agreed(range) => {
                if agreed.is_empty() {
                    agreed_from = range.line;
                }
                agreed.push(show(&text[range.bytes]));
            }
            Piece::Conflict(cut) => {
                if !agreed.is_empty() {
                    segments.push(Segment::Agreed {
                        start: agreed_from,
                        lines: std::mem::take(&mut agreed),
                    });
                }
                segments.push(Segment::Conflict(Region {
                    index: regions,
                    start: cut.line,
                    ours_label: cut.ours_label,
                    theirs_label: cut.theirs_label,
                    ours: cut
                        .ours
                        .iter()
                        .map(|range| show(&text[range.clone()]))
                        .collect(),
                    theirs: cut
                        .theirs
                        .iter()
                        .map(|range| show(&text[range.clone()]))
                        .collect(),
                    base: cut.base.as_ref().map(|lines| {
                        lines
                            .iter()
                            .map(|range| show(&text[range.clone()]))
                            .collect()
                    }),
                }));
                regions += 1;
            }
        }
    }
    if !agreed.is_empty() {
        segments.push(Segment::Agreed {
            start: agreed_from,
            lines: agreed,
        });
    }
    Ok(Conflicted { segments, regions })
}

/// Which version of one conflict to keep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Ours,
    Theirs,
    /// Both sides, ours first — board 07's third button, and the one that
    /// matters when the two changes are additions that do not exclude each
    /// other.
    Both,
}

/// Rebuild a conflicted file with one side chosen for each of its conflicts.
///
/// Byte for byte outside the markers: the agreed text is copied from the file
/// rather than reassembled from the lines this crate parsed, so a CRLF file
/// stays CRLF, a file with no final newline keeps none, and text that is not
/// UTF-8 survives a round trip that never turned it into a `String`.
///
/// The choices are matched against the file *as it is now*, not as the dialog
/// read it: a count that no longer agrees means the file changed underneath —
/// resolved by hand, or by an editor left open — and writing the old answers
/// into it would undo that silently.
pub(crate) fn rebuild(text: &[u8], choices: &[Choice]) -> Result<Vec<u8>> {
    let pieces = scan(text)?;
    let regions = pieces
        .iter()
        .filter(|piece| matches!(piece, Piece::Conflict(_)))
        .count();
    if regions != choices.len() {
        return Err(GitError::backend(
            "resolving the conflict",
            Moved {
                expected: choices.len(),
                found: regions,
            },
        ));
    }

    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    for piece in pieces {
        match piece {
            Piece::Agreed(range) => out.extend_from_slice(&text[range.bytes]),
            Piece::Conflict(cut) => {
                let take = |out: &mut Vec<u8>, lines: &[std::ops::Range<usize>]| {
                    for line in lines {
                        out.extend_from_slice(&text[line.clone()]);
                    }
                };
                match choices[at] {
                    Choice::Ours => take(&mut out, &cut.ours),
                    Choice::Theirs => take(&mut out, &cut.theirs),
                    Choice::Both => {
                        take(&mut out, &cut.ours);
                        take(&mut out, &cut.theirs);
                    }
                }
                at += 1;
            }
        }
    }
    Ok(out)
}

/// A line of the file, and where it is.
struct At {
    bytes: std::ops::Range<usize>,
    line: usize,
}

enum Piece {
    Agreed(At),
    Conflict(Cut),
}

/// One conflict, as byte ranges into the file.
struct Cut {
    line: usize,
    ours_label: String,
    theirs_label: String,
    ours: Vec<std::ops::Range<usize>>,
    base: Option<Vec<std::ops::Range<usize>>>,
    theirs: Vec<std::ops::Range<usize>>,
}

impl Cut {
    fn opening(line: usize, ours_label: String) -> Self {
        Self {
            line,
            ours_label,
            theirs_label: String::new(),
            ours: Vec::new(),
            base: None,
            theirs: Vec::new(),
        }
    }
}

/// Cut a file into agreed lines and conflicts.
///
/// A state machine over whole lines, because a marker is only a marker at the
/// start of one. Malformed sequences are refused rather than guessed at: a
/// region that never closes, a `>>>>>>>` with nothing open, a second
/// `<<<<<<<` inside one. Any of them means the file is not what `git` wrote —
/// somebody was already editing it — and rebuilding it from a misreading is how
/// a resolution eats a line nobody chose.
fn scan(text: &[u8]) -> Result<Vec<Piece>> {
    let mut pieces = Vec::new();
    let mut open: Option<(Cut, Where)> = None;

    for (number, range) in lines(text) {
        let line = &text[range.clone()];
        let starts = |marker: &[u8]| line.starts_with(marker);

        // Outside a conflict, only two lines mean anything.
        if open.is_none() {
            if starts(OURS) {
                open = Some((Cut::opening(number, label(line, OURS)), Where::Ours));
            } else if starts(THEIRS) {
                return Err(unbalanced(number, ">>>>>>>"));
            } else {
                pieces.push(Piece::Agreed(At {
                    bytes: range,
                    line: number,
                }));
            }
            continue;
        }

        if starts(OURS) {
            return Err(unbalanced(number, "<<<<<<<"));
        }
        if starts(THEIRS) {
            let (mut cut, at) = open.take().expect("a conflict is open");
            if at != Where::Theirs {
                // Closed before `=======`: there is no second side, so there is
                // nothing to choose between.
                return Err(unbalanced(number, ">>>>>>>"));
            }
            cut.theirs_label = label(line, THEIRS);
            pieces.push(Piece::Conflict(cut));
            continue;
        }

        let (cut, at) = open.as_mut().expect("a conflict is open");
        if starts(BASE) && *at == Where::Ours {
            cut.base = Some(Vec::new());
            *at = Where::Base;
        } else if starts(SPLIT) && *at != Where::Theirs {
            *at = Where::Theirs;
        } else {
            match at {
                Where::Ours => cut.ours.push(range),
                Where::Base => cut.base.get_or_insert_with(Vec::new).push(range),
                Where::Theirs => cut.theirs.push(range),
            }
        }
    }

    match open {
        None => Ok(pieces),
        // A region `git` opened and nobody closed. Refused rather than treated
        // as agreed text: what follows it is one side of a choice, and copying
        // it out whole would put half a merge in the file.
        Some((cut, _)) => Err(unbalanced(cut.line, "<<<<<<<")),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Where {
    Ours,
    Base,
    Theirs,
}

/// Every line of `text`, as a byte range including its own terminator, numbered
/// from one.
fn lines(text: &[u8]) -> Vec<(usize, std::ops::Range<usize>)> {
    let mut out = Vec::new();
    let mut start = 0;
    for (at, byte) in text.iter().enumerate() {
        if *byte == b'\n' {
            out.push((out.len() + 1, start..at + 1));
            start = at + 1;
        }
    }
    if start < text.len() {
        out.push((out.len() + 1, start..text.len()));
    }
    out
}

/// What `git` wrote after a marker: `<<<<<<< HEAD` → `HEAD`.
fn label(line: &[u8], marker: &[u8]) -> String {
    String::from_utf8_lossy(&line[marker.len()..])
        .trim()
        .to_owned()
}

/// A line for the screen. Lossy, and only ever for display: everything written
/// back comes from the original bytes.
fn show(line: &[u8]) -> String {
    let text = String::from_utf8_lossy(line);
    text.trim_end_matches('\n')
        .trim_end_matches('\r')
        .to_owned()
}

fn unbalanced(line: usize, marker: &str) -> GitError {
    GitError::backend(
        "reading the conflict",
        Unbalanced {
            line,
            marker: marker.to_owned(),
        },
    )
}

#[derive(Debug)]
struct NotText;

impl std::fmt::Display for NotText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the file is binary, so it has no markers to choose between")
    }
}

impl std::error::Error for NotText {}

#[derive(Debug)]
struct Unbalanced {
    line: usize,
    marker: String,
}

impl std::fmt::Display for Unbalanced {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the markers do not pair up: `{}` at line {}",
            self.marker, self.line
        )
    }
}

impl std::error::Error for Unbalanced {}

#[derive(Debug)]
struct Moved {
    expected: usize,
    found: usize,
}

impl std::fmt::Display for Moved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the file now holds {} conflicts, not the {} that were answered",
            self.found, self.expected
        )
    }
}

impl std::error::Error for Moved {}

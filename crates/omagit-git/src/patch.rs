//! Building a unified diff from part of a file's diff.
//!
//! This is what makes staging by hunk and by line possible: `git apply` takes a
//! patch, so selecting three lines out of a hunk means writing a patch that
//! contains exactly those three and describes the rest truthfully.
//!
//! It is also where a mistake destroys work — a patch that applies but says the
//! wrong thing silently rewrites a file — so it is a pure function over the
//! diff model, with no process, no filesystem and no repository. Everything it
//! decides is decided here and tested here; [`crate::ops`] only hands the bytes
//! to `git`.
//!
//! ## The two transformations
//!
//! A patch is applied to one of the two sides it describes, and which side
//! decides what happens to the lines nobody selected.
//!
//! **Forward** — staging, where the patch is applied to the index and the
//! index is the diff's *old* side:
//!
//! | line | selected | not selected |
//! |---|---|---|
//! | added | kept | dropped — it is not in the index, and is not going in |
//! | removed | kept | becomes context — it is in the index and stays |
//!
//! **Reverse** — unstaging and discarding, where the patch is reverse-applied
//! to the index or the working tree, which is the diff's *new* side:
//!
//! | line | selected | not selected |
//! |---|---|---|
//! | added | kept | becomes context — it is in the file and stays |
//! | removed | kept | dropped — it is not in the file, and is not coming back |
//!
//! The mirror is not a coincidence: in both cases the lines that are not
//! selected have to be described exactly as the target file already has them,
//! or `git apply` refuses the patch — which is the failure mode to want.

use std::collections::BTreeSet;

use crate::diff::{DiffContent, FileChange, FileDiff, Hunk, Line, LineKind};

/// Which part of a file's diff an operation acts on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    /// Every change in the file.
    ///
    /// Callers that can should not build a patch for this at all: `git add` and
    /// `git restore` say it directly, and a command that cannot half-apply
    /// cannot half-fail. [`build`] still honours it, for the file whose diff is
    /// the only description of it.
    File,
    /// Whole hunks, by their index in the file's diff.
    Hunks(BTreeSet<usize>),
    /// Individual changed lines, as `(hunk index, line index within the hunk)`.
    /// Context lines are never selected; including one is ignored.
    Lines(BTreeSet<(usize, usize)>),
}

impl Selection {
    pub fn hunk(index: usize) -> Self {
        Selection::Hunks(BTreeSet::from([index]))
    }

    /// Whether a changed line is in the selection.
    fn covers(&self, hunk: usize, line: usize) -> bool {
        match self {
            Selection::File => true,
            Selection::Hunks(hunks) => hunks.contains(&hunk),
            Selection::Lines(lines) => lines.contains(&(hunk, line)),
        }
    }

    /// Whether a hunk can contribute anything, so untouched hunks are skipped
    /// rather than emitted as pure context.
    fn touches(&self, hunk: usize) -> bool {
        match self {
            Selection::File => true,
            Selection::Hunks(hunks) => hunks.contains(&hunk),
            Selection::Lines(lines) => lines.iter().any(|(at, _)| *at == hunk),
        }
    }
}

/// Which side of the diff the patch will be applied to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Applied as written, to the diff's old side — `git apply --cached`,
    /// staging an unstaged change.
    Forward,
    /// Applied reversed, to the diff's new side — `git apply --reverse`,
    /// unstaging a staged change or discarding an unstaged one.
    Reverse,
}

#[derive(Debug, thiserror::Error)]
pub enum PatchError {
    /// A binary, oversized, submodule or empty diff has no lines to select
    /// from. The caller has to act on the whole file instead.
    #[error("{0} has no text diff to build a patch from")]
    NotText(String),

    /// The selection matched nothing, which means the caller would otherwise
    /// hand `git apply` an empty patch and read its refusal as a Git failure.
    #[error("nothing selected in {0}")]
    Empty(String),
}

/// Build a patch for `selection` of `file`, to be applied in `direction`.
///
/// The result is bytes, not text: paths and content come from a repository
/// nobody promised was UTF-8.
pub fn build(
    file: &FileDiff,
    selection: &Selection,
    direction: Direction,
) -> Result<Vec<u8>, PatchError> {
    let DiffContent::Text { hunks, .. } = &file.content else {
        return Err(PatchError::NotText(file.path.display_lossy().into_owned()));
    };

    let mut body = Vec::new();
    // How far the emitted synthetic side has drifted from the real one, so each
    // hunk header states a position consistent with the hunks before it.
    let mut drift: i64 = 0;

    for (index, hunk) in hunks.iter().enumerate() {
        if !selection.touches(index) {
            continue;
        }
        let Some(emitted) = emit_hunk(hunk, index, selection, direction, drift) else {
            continue;
        };
        drift += emitted.drift;
        body.extend_from_slice(&emitted.bytes);
    }

    if body.is_empty() {
        return Err(PatchError::Empty(file.path.display_lossy().into_owned()));
    }

    let mut patch = header(file);
    patch.extend_from_slice(&body);
    Ok(patch)
}

struct EmittedHunk {
    bytes: Vec<u8>,
    /// `new_lines - old_lines` for this hunk, which is what the next one's
    /// header has to account for.
    drift: i64,
}

fn emit_hunk(
    hunk: &Hunk,
    index: usize,
    selection: &Selection,
    direction: Direction,
    drift: i64,
) -> Option<EmittedHunk> {
    let mut lines: Vec<(u8, &Line)> = Vec::with_capacity(hunk.lines.len());
    let mut old_count = 0_u32;
    let mut new_count = 0_u32;
    let mut selected_any = false;

    for (at, line) in hunk.lines.iter().enumerate() {
        let sign = match line.kind {
            LineKind::Context => Some(b' '),
            kind => {
                let selected = selection.covers(index, at);
                selected_any |= selected;
                match (kind, selected, direction) {
                    // Kept as it is, on either side.
                    (LineKind::Added, true, _) => Some(b'+'),
                    (LineKind::Removed, true, _) => Some(b'-'),
                    // Forward: the index has the removals and not the additions.
                    (LineKind::Added, false, Direction::Forward) => None,
                    (LineKind::Removed, false, Direction::Forward) => Some(b' '),
                    // Reverse: the target has the additions and not the removals.
                    (LineKind::Added, false, Direction::Reverse) => Some(b' '),
                    (LineKind::Removed, false, Direction::Reverse) => None,
                    (LineKind::Context, ..) => unreachable!("context is handled above"),
                }
            }
        };

        if let Some(sign) = sign {
            match sign {
                b' ' => {
                    old_count += 1;
                    new_count += 1;
                }
                b'-' => old_count += 1,
                b'+' => new_count += 1,
                _ => unreachable!("only three signs exist"),
            }
            lines.push((sign, line));
        }
    }

    // A hunk the selection did not actually change would be emitted as pure
    // context: a patch that says nothing, which `git apply` rejects.
    if !selected_any {
        return None;
    }

    // The side being matched keeps its real position; the synthetic side is
    // placed relative to it. Forward matches the old side, reverse the new one.
    let (old_start, new_start) = match direction {
        Direction::Forward => (
            hunk.old_start,
            (hunk.old_start as i64 + drift).max(1) as u32,
        ),
        Direction::Reverse => (
            (hunk.new_start as i64 - drift).max(1) as u32,
            hunk.new_start,
        ),
    };

    let mut bytes =
        format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@\n").into_bytes();
    for (sign, line) in lines {
        bytes.push(sign);
        bytes.extend_from_slice(&line.text);
        bytes.push(b'\n');
        if line.no_newline_at_eof {
            bytes.extend_from_slice(b"\\ No newline at end of file\n");
        }
    }

    Some(EmittedHunk {
        bytes,
        drift: new_count as i64 - old_count as i64,
    })
}

/// The `diff --git` preamble.
///
/// `/dev/null` on the side a file does not exist on, so a patch that creates or
/// removes a file says so rather than relying on the hunk to imply it.
fn header(file: &FileDiff) -> Vec<u8> {
    let new = file.path.as_bytes();
    let old = match &file.change {
        FileChange::Renamed { from } | FileChange::Copied { from } => from.as_bytes(),
        _ => new,
    };

    let mut header = Vec::new();
    header.extend_from_slice(b"diff --git a/");
    header.extend_from_slice(old);
    header.extend_from_slice(b" b/");
    header.extend_from_slice(new);
    header.push(b'\n');

    // Without these, `git apply` reads `/dev/null` as a path, strips its
    // leading component and reports "dev/null does not exist in index". They
    // are what tells it a file is being created or removed, and the mode is
    // what keeps an executable executable.
    match &file.change {
        FileChange::Added => {
            header.extend_from_slice(format!("new file mode {:o}\n", file.mode).as_bytes());
        }
        FileChange::Deleted => {
            header.extend_from_slice(format!("deleted file mode {:o}\n", file.mode).as_bytes());
        }
        FileChange::Renamed { from } => {
            header.extend_from_slice(b"rename from ");
            header.extend_from_slice(from.as_bytes());
            header.extend_from_slice(b"\nrename to ");
            header.extend_from_slice(new);
            header.push(b'\n');
        }
        FileChange::Copied { from } => {
            header.extend_from_slice(b"copy from ");
            header.extend_from_slice(from.as_bytes());
            header.extend_from_slice(b"\ncopy to ");
            header.extend_from_slice(new);
            header.push(b'\n');
        }
        FileChange::Modified | FileChange::ModeChanged => {}
    }

    match file.change {
        FileChange::Added => header.extend_from_slice(b"--- /dev/null\n"),
        _ => {
            header.extend_from_slice(b"--- a/");
            header.extend_from_slice(old);
            header.push(b'\n');
        }
    }
    match file.change {
        FileChange::Deleted => header.extend_from_slice(b"+++ /dev/null\n"),
        _ => {
            header.extend_from_slice(b"+++ b/");
            header.extend_from_slice(new);
            header.push(b'\n');
        }
    }
    header
}

//! The shapes that cross to the front end.
//!
//! Deliberately not `omagit-git`'s own types with `Serialize` derived on them.
//! Two reasons, and the first is the rule that made this port possible at all:
//!
//! * **SPEC §3 rule 5.** The Git core knows nothing about the UI, and a
//!   serialisation format chosen for one transport is a UI concern. Deriving
//!   `Serialize` there would put a JSON shape in the crate that is supposed not
//!   to have one.
//! * **The wire wants a different shape.** A `FileDiff` carries both sides of
//!   the file in full so the viewer can unfold context and highlight syntax;
//!   sending that to a webview would move megabytes nobody asked for. What
//!   crosses is what is drawn.

use omagit_git::diff::{DiffContent, FileChange, FileDiff, Hunk, Line, LineKind};
use omagit_git::status::{StageChange, Status, StatusEntry, WorktreeChange};
use omagit_git::{ObjectId, Summary};

/// A hash, as the seven characters it is read by plus the full one.
#[derive(Debug, serde::Serialize)]
pub struct Oid {
    pub full: String,
    pub short: String,
}

impl From<ObjectId> for Oid {
    fn from(id: ObjectId) -> Self {
        let full = id.to_string();
        Self {
            short: full[..7.min(full.len())].to_owned(),
            full,
        }
    }
}

/// One row of the file list.
#[derive(Debug, serde::Serialize)]
pub struct StatusRow {
    /// Lossy, because the list has to draw it. The byte-exact path stays on the
    /// Rust side and is what every command is addressed by.
    pub path: String,
    pub staged: Option<&'static str>,
    pub unstaged: Option<&'static str>,
    pub conflict: Option<&'static str>,
    /// The two letters `git status --short` prints.
    pub code: String,
}

pub fn status(status: &Status) -> Vec<StatusRow> {
    status.entries.iter().map(row).collect()
}

fn row(entry: &StatusEntry) -> StatusRow {
    StatusRow {
        path: entry.path.display_lossy().into_owned(),
        staged: entry.staged.as_ref().map(stage_name),
        unstaged: entry.unstaged.as_ref().map(worktree_name),
        conflict: entry.conflict.as_ref().map(|_| "conflict"),
        code: omagit_git::status::short_code(entry),
    }
}

fn stage_name(change: &StageChange) -> &'static str {
    match change {
        StageChange::Added => "added",
        StageChange::Modified => "modified",
        StageChange::Deleted => "deleted",
        StageChange::Renamed { .. } => "renamed",
        StageChange::Copied { .. } => "copied",
        StageChange::TypeChanged => "type-changed",
    }
}

fn worktree_name(change: &WorktreeChange) -> &'static str {
    match change {
        WorktreeChange::Modified => "modified",
        WorktreeChange::Added => "added",
        WorktreeChange::Deleted => "deleted",
        WorktreeChange::TypeChanged => "type-changed",
        WorktreeChange::Untracked => "untracked",
        WorktreeChange::Ignored => "ignored",
        WorktreeChange::Renamed { .. } => "renamed",
    }
}

/// A file's diff, flattened into the rows a virtualised list draws.
///
/// Flattened here rather than in the front end because the flattening is the
/// same decision the old renderer made, and it is where "which line is this,
/// on which side" is already known.
#[derive(Debug, serde::Serialize)]
pub struct Diff {
    pub path: String,
    pub change: &'static str,
    /// `None` when there is nothing to draw: binary, oversized, a submodule.
    pub rows: Option<Vec<DiffRow>>,
    /// Why there is nothing, when there is nothing.
    pub reason: Option<String>,
    pub added: usize,
    pub removed: usize,
    pub hunks: usize,
}

#[derive(Debug, serde::Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum DiffRow {
    /// A `@@` header, and the index a hunk-level action names.
    Header { hunk: usize, text: String },
    /// Context that is not shown.
    Fold { lines: u32 },
    Line {
        hunk: usize,
        /// The line's index inside its hunk — the other half of the coordinate
        /// `omagit_git::patch::Selection` speaks in.
        index: usize,
        side: &'static str,
        old: Option<u32>,
        new: Option<u32>,
        text: String,
        /// Byte ranges inside `text` that differ from the line this one pairs
        /// with, for the word-level highlight of DESIGN-TOKENS §5.
        refined: Vec<(usize, usize)>,
        no_newline: bool,
    },
}

pub fn diff(file: &FileDiff) -> Diff {
    let change = match &file.change {
        FileChange::Added => "added",
        FileChange::Deleted => "deleted",
        FileChange::Modified => "modified",
        FileChange::ModeChanged => "mode-changed",
        FileChange::Renamed { .. } => "renamed",
        FileChange::Copied { .. } => "copied",
    };
    let path = file.path.display_lossy().into_owned();

    match &file.content {
        DiffContent::Text {
            hunks,
            added,
            removed,
            ..
        } => Diff {
            path,
            change,
            rows: Some(rows(hunks)),
            reason: None,
            added: *added,
            removed: *removed,
            hunks: hunks.len(),
        },
        DiffContent::Binary {
            old_bytes,
            new_bytes,
        } => Diff {
            path,
            change,
            rows: None,
            reason: Some(format!("binaire · {old_bytes} → {new_bytes} octets")),
            added: 0,
            removed: 0,
            hunks: 0,
        },
        DiffContent::Oversized { bytes } => Diff {
            path,
            change,
            rows: None,
            reason: Some(format!("trop volumineux · {bytes} octets")),
            added: 0,
            removed: 0,
            hunks: 0,
        },
        DiffContent::Submodule { .. } => Diff {
            path,
            change,
            rows: None,
            reason: Some("sous-module".to_owned()),
            added: 0,
            removed: 0,
            hunks: 0,
        },
        DiffContent::Empty => Diff {
            path,
            change,
            rows: None,
            reason: None,
            added: 0,
            removed: 0,
            hunks: 0,
        },
    }
}

fn rows(hunks: &[Hunk]) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    let mut previous_end: Option<u32> = None;

    for (index, hunk) in hunks.iter().enumerate() {
        // The context between two hunks, which the viewer offers to unfold.
        if let Some(end) = previous_end
            && hunk.old_start > end
        {
            rows.push(DiffRow::Fold {
                lines: hunk.old_start - end,
            });
        }
        rows.push(DiffRow::Header {
            hunk: index,
            text: hunk.header(),
        });
        for (at, line) in hunk.lines.iter().enumerate() {
            rows.push(line_row(index, at, line));
        }
        previous_end = Some(hunk.old_start + hunk.old_lines);
    }
    rows
}

fn line_row(hunk: usize, index: usize, line: &Line) -> DiffRow {
    DiffRow::Line {
        hunk,
        index,
        side: match line.kind {
            LineKind::Context => "context",
            LineKind::Added => "added",
            LineKind::Removed => "removed",
        },
        old: line.old_number,
        new: line.new_number,
        text: line.text_lossy().into_owned(),
        refined: line
            .refinements
            .iter()
            .map(|range| (range.start, range.end))
            .collect(),
        no_newline: line.no_newline_at_eof,
    }
}

/// What a repository is, at a glance.
#[derive(Debug, serde::Serialize)]
pub struct RepoSummary {
    pub path: String,
    pub name: String,
    pub head: String,
    pub modified: usize,
    pub untracked: usize,
    pub conflicted: usize,
    pub stashes: usize,
    pub committer: Option<String>,
}

pub fn summary(path: &std::path::Path, summary: &Summary) -> RepoSummary {
    RepoSummary {
        path: path.display().to_string(),
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        head: summary.head.label().to_string(),
        modified: summary.counts.modified,
        untracked: summary.counts.untracked,
        conflicted: summary.counts.conflicted,
        stashes: summary.stashes,
        committer: summary
            .committer
            .as_ref()
            .map(|who| format!("{} <{}>", who.name, who.email)),
    }
}

/// A commit that was just made.
#[derive(Debug, serde::Serialize)]
pub struct Made {
    pub id: Oid,
    /// Whatever `git` said on the way — a `pre-commit` hook's output, the
    /// summary line. Shown rather than swallowed: a hook that rewrote a file
    /// says so here and nowhere else.
    pub notes: String,
}

/// One row of the repository list.
#[derive(Debug, serde::Serialize)]
pub struct LibraryRow {
    pub group: usize,
    pub index: usize,
    pub path: String,
    pub name: String,
}

/// One line of the operations journal.
#[derive(Debug, serde::Serialize)]
pub struct JournalRow {
    pub command: String,
    /// Marked before the command runs, which is what SPEC §15 risk 5 asks for.
    pub destructive: bool,
    /// `running` while it has not come back — and a command that never comes
    /// back stays that way, which is the truth and the point.
    pub outcome: &'static str,
    pub stderr: String,
}

pub fn journal_row(entry: &omagit_git::journal::Entry) -> JournalRow {
    use omagit_git::journal::Outcome;
    let (outcome, stderr) = match &entry.outcome {
        Outcome::Running => ("running", String::new()),
        Outcome::Succeeded { stderr, .. } => ("ok", stderr.clone()),
        Outcome::Failed { stderr, .. } => ("failed", stderr.clone()),
    };
    JournalRow {
        command: entry.command.clone(),
        destructive: entry.destructive,
        outcome,
        stderr,
    }
}

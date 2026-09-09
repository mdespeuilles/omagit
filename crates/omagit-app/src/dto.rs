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

fn change_name(change: &FileChange) -> &'static str {
    match change {
        FileChange::Added => "added",
        FileChange::Deleted => "deleted",
        FileChange::Modified => "modified",
        FileChange::ModeChanged => "mode-changed",
        FileChange::Renamed { .. } => "renamed",
        FileChange::Copied { .. } => "copied",
    }
}

pub fn diff(file: &FileDiff) -> Diff {
    let change = change_name(&file.change);
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
///
/// Wide because `omagit_git::Summary` is: it was written for board 06's card
/// and reads everything that card shows in one pass. Sending a narrower shape
/// would mean the Repositories screen asking a second time for what the first
/// answer already held.
#[derive(Debug, serde::Serialize)]
pub struct RepoSummary {
    pub path: String,
    pub name: String,
    /// The branch, or `detached at 9f3c1a2`.
    pub head: String,
    /// A half-finished merge, rebase or cherry-pick. The row says so *instead*
    /// of the branch, because "on main" is misleading while a merge is stuck.
    pub operation: Option<String>,
    pub tracking: Option<Tracking>,
    pub counts: Counts,
    pub last_commit: Option<LastCommit>,
    pub stashes: usize,
    pub remotes: Vec<RemoteRow>,
    /// Commits per bucket over the activity window, and the total beside it.
    pub activity: Vec<u32>,
    pub commits: u32,
    pub committer: Option<Identity>,
}

#[derive(Debug, serde::Serialize)]
pub struct Tracking {
    pub upstream: String,
    pub ahead: usize,
    pub behind: usize,
    /// The upstream is configured but no longer exists. The card says "gone"
    /// rather than 0/0, which would read as "up to date".
    pub gone: bool,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct Counts {
    pub modified: usize,
    pub added: usize,
    pub deleted: usize,
    pub renamed: usize,
    pub untracked: usize,
    pub conflicted: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct LastCommit {
    pub id: Oid,
    pub summary: String,
    pub author: String,
    pub when: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct RemoteRow {
    pub name: String,
    pub url: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct Identity {
    pub name: String,
    pub email: String,
    /// The card's avatar: `Élodie Laurent` → `EL`.
    pub initials: String,
    /// True when it comes from the global or system configuration rather than
    /// from this repository — a per-repository identity is a deliberate act and
    /// worth distinguishing.
    pub inherited: bool,
}

pub fn summary(path: &std::path::Path, summary: &Summary) -> RepoSummary {
    RepoSummary {
        path: path.display().to_string(),
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        head: summary.head.label().to_string(),
        operation: summary.operation.map(|operation| operation.to_string()),
        tracking: summary.tracking.as_ref().map(|tracking| Tracking {
            upstream: tracking.upstream.clone(),
            ahead: tracking.ahead,
            behind: tracking.behind,
            gone: tracking.gone,
        }),
        counts: Counts {
            modified: summary.counts.modified,
            added: summary.counts.added,
            deleted: summary.counts.deleted,
            renamed: summary.counts.renamed,
            untracked: summary.counts.untracked,
            conflicted: summary.counts.conflicted,
        },
        last_commit: summary.last_commit.as_ref().map(|commit| LastCommit {
            id: commit.id.into(),
            summary: commit.summary.clone(),
            author: commit.author.name.clone(),
            when: commit.author.time.seconds,
        }),
        stashes: summary.stashes,
        remotes: summary
            .remotes
            .iter()
            .map(|remote| RemoteRow {
                name: remote.name.clone(),
                url: remote.url.clone(),
            })
            .collect(),
        activity: summary.activity.buckets.clone(),
        commits: summary.activity.total,
        committer: summary.committer.as_ref().map(|who| Identity {
            name: who.name.clone(),
            email: who.email.clone(),
            initials: who.initials(),
            inherited: who.inherited,
        }),
    }
}

/// One commit, as the detail pane draws it.
#[derive(Debug, serde::Serialize)]
pub struct CommitDetail {
    pub id: Oid,
    pub parents: Vec<Oid>,
    pub author: Who,
    /// Only when it differs from the author — which is the case worth showing:
    /// a rebase, a cherry-pick, a patch applied by someone else. Showing it
    /// always would put the same two lines on every commit.
    pub committer: Option<Who>,
    pub summary: String,
    pub body: String,
    pub files: Vec<FileRow>,
}

#[derive(Debug, serde::Serialize)]
pub struct Who {
    pub name: String,
    pub email: String,
    /// Seconds since the epoch, and the author's own offset from UTC. Both,
    /// because the detail can show the time the author saw on their own clock.
    pub when: i64,
    pub offset: i32,
}

/// One file in a commit or a comparison.
///
/// Not a [`StatusRow`]: there is no index here, so there is no staged half and
/// no two-letter code. What a reader wants instead is how much moved, which the
/// working copy's list does not show.
#[derive(Debug, serde::Serialize)]
pub struct FileRow {
    pub path: String,
    pub change: &'static str,
    pub added: usize,
    pub removed: usize,
    /// Set when there is nothing to count: binary, oversized, a submodule.
    pub reason: Option<String>,
}

pub fn commit_detail(
    commit: &omagit_git::history::Commit,
    diff: &omagit_git::Diff,
) -> CommitDetail {
    CommitDetail {
        id: commit.id.into(),
        parents: commit.parents.iter().copied().map(Oid::from).collect(),
        author: who(&commit.author),
        committer: (commit.committer.name != commit.author.name
            || commit.committer.time != commit.author.time)
            .then(|| who(&commit.committer)),
        summary: commit.summary.clone(),
        body: commit.body.clone(),
        files: diff.files.iter().map(file_row).collect(),
    }
}

fn who(signature: &omagit_git::history::Signature) -> Who {
    Who {
        name: signature.name.clone(),
        email: signature.email.clone(),
        when: signature.time.seconds,
        offset: signature.time.offset_seconds,
    }
}

/// A file's line in a commit's list — the counts, without its text.
///
/// Reads them off the content rather than going through [`diff`], which
/// flattens every hunk into rows. A commit touching two hundred files would
/// have built two hundred row vectors to throw them all away, and the list
/// shows none of them until a file is opened.
pub fn file_row(file: &FileDiff) -> FileRow {
    let (added, removed, reason) = match &file.content {
        DiffContent::Text { added, removed, .. } => (*added, *removed, None),
        DiffContent::Binary {
            old_bytes,
            new_bytes,
        } => (
            0,
            0,
            Some(format!("binaire · {old_bytes} → {new_bytes} octets")),
        ),
        DiffContent::Oversized { bytes } => {
            (0, 0, Some(format!("trop volumineux · {bytes} octets")))
        }
        DiffContent::Submodule { .. } => (0, 0, Some("sous-module".to_owned())),
        DiffContent::Empty => (0, 0, None),
    };
    FileRow {
        path: file.path.display_lossy().into_owned(),
        change: change_name(&file.change),
        added,
        removed,
        reason,
    }
}

/// One entry of the shelf, as the Stashes list draws it.
///
/// `index` is `git`'s own address — `stash@{0}` is the most recent — and it is
/// sent so the row can show it, not so a command can be given it: the numbering
/// shifts the moment one is dropped, so every write is addressed by `id` and
/// resolved to an index on the Rust side (`omagit_git::stash::find`).
#[derive(Debug, serde::Serialize)]
pub struct StashRow {
    pub index: usize,
    pub id: Oid,
    /// `None` for one made on a detached `HEAD`, where `git` writes
    /// `(no branch)` — a sentence rather than somewhere to switch to.
    pub branch: Option<String>,
    pub message: String,
    /// Seconds since the epoch, from the reflog: the time `git stash list`
    /// shows.
    pub when: i64,
    /// Holds files that were on no index, which is what makes its preview more
    /// than a commit diff.
    pub untracked: bool,
}

pub fn stash_row(stash: &omagit_git::Stash) -> StashRow {
    StashRow {
        index: stash.index,
        id: stash.commit.into(),
        branch: stash.branch.clone(),
        message: stash.message.clone(),
        when: stash.when.seconds,
        untracked: stash.untracked,
    }
}

/// Everything that differs between two commits.
///
/// Not a [`CommitDetail`]: there is no single commit here, so there is no
/// author, no message and no parent. What a comparison has instead is two ends
/// and a count, and the two ends are what the pane has to keep saying — a
/// reader who has scrolled a long file list needs to be told which way round
/// the diff is.
#[derive(Debug, serde::Serialize)]
pub struct Comparison {
    pub from: Oid,
    pub to: Oid,
    pub files: Vec<FileRow>,
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

/// Every reference, for the sidebar's tree.
///
/// Grouping on `/` is deliberately absent: `omagit_git::refs` says in as many
/// words that it is the sidebar's job, and it is — a tree is a rendering
/// decision, and the crate that reads Git has no business holding one.
#[derive(Debug, serde::Serialize)]
pub struct Refs {
    pub branches: Vec<BranchRow>,
    pub remote_branches: Vec<RemoteBranchRow>,
    pub tags: Vec<TagRow>,
    pub remotes: Vec<RemoteRow>,
}

#[derive(Debug, serde::Serialize)]
pub struct BranchRow {
    pub name: String,
    pub commit: Oid,
    pub head: bool,
    pub tracking: Option<Tracking>,
    /// Every commit of this branch is on `HEAD` too, so deleting it removes a
    /// label and nothing else. What tells the confirmation which of its two
    /// questions to ask.
    pub merged: bool,
    /// Seconds since the tip commit was authored, for the "7 mois" a stale
    /// branch carries in board 03.
    pub age: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct RemoteBranchRow {
    pub remote: String,
    pub name: String,
    pub commit: Oid,
}

#[derive(Debug, serde::Serialize)]
pub struct TagRow {
    pub name: String,
    pub commit: Oid,
    pub annotated: bool,
}

pub fn refs(
    repo: &omagit_git::Repository,
    refs: &omagit_git::refs::Refs,
    merged: &std::collections::BTreeSet<String>,
    now: i64,
) -> Refs {
    Refs {
        branches: refs
            .branches
            .iter()
            .map(|branch| BranchRow {
                name: branch.name.clone(),
                commit: branch.commit.into(),
                head: branch.is_head,
                tracking: branch.tracking.as_ref().map(|tracking| Tracking {
                    upstream: tracking.upstream.clone(),
                    ahead: tracking.ahead,
                    behind: tracking.behind,
                    gone: tracking.gone,
                }),
                merged: merged.contains(&branch.name),
                // A branch whose tip cannot be read is not an error worth
                // failing the whole sidebar for — a shallow clone has them.
                age: omagit_git::history::commit(repo, branch.commit)
                    .map(|commit| now - commit.author.time.seconds)
                    .unwrap_or(0),
            })
            .collect(),
        remote_branches: refs
            .remote_branches
            .iter()
            .map(|branch| RemoteBranchRow {
                remote: branch.remote.clone(),
                name: branch.name.clone(),
                commit: branch.commit.into(),
            })
            .collect(),
        tags: refs
            .tags
            .iter()
            .map(|tag| TagRow {
                name: tag.name.clone(),
                commit: tag.commit.into(),
                annotated: tag.annotation.is_some(),
            })
            .collect(),
        remotes: refs
            .remotes
            .iter()
            .map(|remote| RemoteRow {
                name: remote.name.clone(),
                url: remote.url.clone(),
            })
            .collect(),
    }
}

/// One row of the repository list.
///
/// What the library holds and nothing Git knows: reading a summary per row
/// would mean a status walk for every repository the user has ever added,
/// before the screen has drawn anything. The rows arrive first and fill in.
#[derive(Debug, serde::Serialize)]
pub struct LibraryRow {
    pub group: usize,
    pub index: usize,
    pub group_name: String,
    pub path: String,
    pub name: String,
    /// The user's own note. Not read from the repository: Git has no such
    /// field.
    pub description: String,
    /// Unix seconds, or `None` for a repository that has never been opened.
    pub last_opened: Option<i64>,
    /// The directory is gone. DESIGN §4: such an entry keeps its row with a
    /// struck-through icon and is *never* silently removed — a repository on an
    /// unmounted disk comes back.
    pub missing: bool,
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

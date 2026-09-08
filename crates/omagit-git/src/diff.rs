//! Diffs: which files changed, and inside each, which lines and which words.
//!
//! Three levels, each the input of the next:
//!
//! 1. **Which files** — a tree against a tree, or the index against `HEAD` or
//!    the working tree. Rename detection happens here, and it is what keeps a
//!    moved file from showing up as one deletion plus one addition.
//! 2. **Which lines** — the Myers/histogram diff of two blobs, arranged into
//!    hunks with context. `gix` re-exports `imara-diff`, including Git's own
//!    slider heuristics, so hunks land on the same lines the command line picks.
//!    Using it rather than adding a second diff crate means one engine, one
//!    behaviour, and one dependency to keep current.
//! 3. **Which words** — the intra-line refinement of DESIGN §4: the changed
//!    span inside a line that was only partly rewritten, drawn at 24% where the
//!    line itself is at 12%.
//!
//! Everything is computed per file, on demand. SPEC §12 requires it: a commit
//! that touches 900 files must render its first file immediately, and a diff
//! view only ever shows one file at a time.

use std::ops::Range;

use gix::bstr::ByteSlice;

use crate::status::{StageChange, StatusEntry, WorktreeChange};
use crate::{Cancel, GitError, ObjectId, RepoPath, Repository, Result, assert_off_render_thread};

/// How much to compute, and when to give up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffOptions {
    /// Unchanged lines kept around each change. Git's default, and adjustable
    /// from the diff view (SPEC §11).
    pub context_lines: u32,
    /// Past this size, the content is reported as [`DiffContent::Oversized`]
    /// instead of being diffed.
    ///
    /// SPEC §11 asks for a degraded display rather than a freeze on a large
    /// file. The threshold is on the larger of the two sides, before any
    /// diffing, because it is the diff that is expensive, not the read.
    pub max_bytes: u64,
    /// Detect renames and copies when listing the files of a diff.
    pub detect_renames: bool,
    /// Compute the intra-line refinement.
    pub refine: bool,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            context_lines: 3,
            // 2 MiB is about 40 000 lines of source. Past that, no one reads
            // the diff — they read the file — and the render cost stops being
            // worth paying.
            max_bytes: 2 * 1024 * 1024,
            detect_renames: true,
            refine: true,
        }
    }
}

/// Every file that differs between two states.
#[derive(Clone, Debug, Default)]
pub struct Diff {
    pub files: Vec<FileDiff>,
}

/// One file's diff.
#[derive(Clone, Debug)]
pub struct FileDiff {
    /// Where the file is *now*: the destination of a rename, the surviving side
    /// of a deletion.
    pub path: RepoPath,
    pub change: FileChange,
    pub content: DiffContent,
}

/// What happened to the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileChange {
    Added,
    Deleted,
    Modified,
    /// Content unchanged, mode changed — an executable bit, or a file that
    /// became a symlink.
    ModeChanged,
    Renamed {
        from: RepoPath,
    },
    Copied {
        from: RepoPath,
    },
}

/// The body of a file's diff, or the reason there is none to show.
#[derive(Clone, Debug)]
pub enum DiffContent {
    Text {
        hunks: Vec<Hunk>,
        added: usize,
        removed: usize,
        /// Both sides in full, as they were compared.
        ///
        /// Kept rather than dropped once the hunks are built, because two
        /// things the viewer has to do cannot be done from hunks alone:
        /// syntax highlighting needs the whole file — a parser handed a
        /// fragment of one produces nonsense — and unfolding the context
        /// between two hunks needs the lines that were left out. Bounded by
        /// [`DiffOptions::max_bytes`], and only ever for the file on screen.
        sides: Box<Sides>,
    },
    /// One side contains a NUL byte in its first 8 000 — Git's own test, kept
    /// identical so omagit and the command line never disagree about what is
    /// binary. The viewer shows sizes, and an image preview where it can
    /// (SPEC §11).
    Binary { old_bytes: u64, new_bytes: u64 },
    /// Larger than [`DiffOptions::max_bytes`].
    Oversized { bytes: u64 },
    /// A submodule, whose "content" is the commit it points at.
    Submodule {
        old: Option<ObjectId>,
        new: Option<ObjectId>,
    },
    /// Nothing to show: a mode change, or two identical sides.
    Empty,
}

/// The two sides of a text diff, in full.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sides {
    pub old: Vec<u8>,
    pub new: Vec<u8>,
}

impl Sides {
    /// One side's lines, without their terminators — what a viewer draws when
    /// it unfolds context, and what a highlighter indexes into.
    ///
    /// Line *n* here is the line the `@@` header calls *n + 1*, so a caller can
    /// index straight into it with a diff line number minus one.
    pub fn lines(text: &[u8]) -> Vec<&[u8]> {
        if text.is_empty() {
            return Vec::new();
        }
        // A trailing newline terminates the last line rather than starting an
        // empty one; a file without a final newline keeps its last line either
        // way.
        let body = text.strip_suffix(b"\n").unwrap_or(text);
        body.split(|byte| *byte == b'\n').collect()
    }
}

/// A run of lines, with its context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    /// 1-based first line on the old side, as a `@@` header states it.
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<Line>,
}

impl Hunk {
    /// The `@@ -a,b +c,d @@` header, for the CLI and for tests.
    pub fn header(&self) -> String {
        format!(
            "@@ -{},{} +{},{} @@",
            self.old_start, self.old_lines, self.new_start, self.new_lines
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

/// One line of a hunk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub kind: LineKind,
    /// 1-based line number on each side; `None` on the side the line is absent
    /// from. Both are needed: the gutter shows two columns.
    pub old_number: Option<u32>,
    pub new_number: Option<u32>,
    /// The line, without its terminator. Bytes, because a diff has to display a
    /// file whose encoding nobody declared.
    pub text: Vec<u8>,
    /// The file ends here and has no final newline — Git's
    /// `\ No newline at end of file`.
    pub no_newline_at_eof: bool,
    /// Byte ranges inside `text` that differ from the line this one is paired
    /// with. Empty unless the refinement ran and found a partial change.
    pub refinements: Vec<Refinement>,
}

impl Line {
    /// The line as text, lossily — for the CLI, for logs, for test failures.
    pub fn text_lossy(&self) -> std::borrow::Cow<'_, str> {
        self.text.to_str_lossy()
    }
}

/// A changed span inside a line, as byte offsets into [`Line::text`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Refinement {
    pub start: usize,
    pub end: usize,
}

impl Diff {
    /// What a commit changed, against its first parent.
    ///
    /// A root commit is diffed against the empty tree, so its every file shows
    /// as an addition — which is what it is.
    pub fn commit(
        repo: &Repository,
        id: ObjectId,
        options: DiffOptions,
        cancel: &Cancel,
    ) -> Result<Self> {
        assert_off_render_thread();
        let gix = repo.gix();
        let commit = crate::history::read_commit(&gix, id)?;
        let parent = commit.parents.first().copied();
        Self::between(repo, parent, Some(id), options, cancel)
    }

    /// The diff between two commits, either of which may be absent to mean "the
    /// empty tree" — a root commit's parent, or a deleted branch's tip.
    pub fn between(
        repo: &Repository,
        old: Option<ObjectId>,
        new: Option<ObjectId>,
        options: DiffOptions,
        cancel: &Cancel,
    ) -> Result<Self> {
        assert_off_render_thread();
        let gix = repo.gix();
        let old_tree = tree_of(&gix, old)?;
        let new_tree = tree_of(&gix, new)?;

        let mut diff_options = gix::diff::Options::default();
        diff_options.track_path();
        diff_options.track_rewrites(options.detect_renames.then(gix::diff::Rewrites::default));

        let changes = gix
            .diff_tree_to_tree(old_tree.as_ref(), new_tree.as_ref(), Some(diff_options))
            .map_err(|error| GitError::backend("comparing two commits", error))?;

        let mut files = Vec::with_capacity(changes.len());
        for change in changes {
            cancel.check()?;
            if let Some(file) = file_from_tree_change(&gix, change, options)? {
                files.push(file);
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Self { files })
    }

    /// What committing now would record: `HEAD` against the index.
    pub fn staged(
        repo: &Repository,
        status: &crate::Status,
        options: DiffOptions,
        cancel: &Cancel,
    ) -> Result<Self> {
        Self::of_status(repo, status, options, cancel, Side::Staged)
    }

    /// What committing now would leave behind: the index against the working
    /// tree.
    pub fn unstaged(
        repo: &Repository,
        status: &crate::Status,
        options: DiffOptions,
        cancel: &Cancel,
    ) -> Result<Self> {
        Self::of_status(repo, status, options, cancel, Side::Unstaged)
    }

    fn of_status(
        repo: &Repository,
        status: &crate::Status,
        options: DiffOptions,
        cancel: &Cancel,
        side: Side,
    ) -> Result<Self> {
        assert_off_render_thread();
        let mut files = Vec::new();
        for entry in &status.entries {
            cancel.check()?;
            let file = match side {
                Side::Staged => staged_file(repo, entry, options)?,
                Side::Unstaged => unstaged_file(repo, entry, options)?,
            };
            if let Some(file) = file {
                files.push(file);
            }
        }
        Ok(Self { files })
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

#[derive(Clone, Copy)]
enum Side {
    Staged,
    Unstaged,
}

/// One staged file: its blob in `HEAD` against its blob in the index.
///
/// Returns `None` when the entry has nothing staged, so a caller can walk a
/// whole status without filtering it first.
pub fn staged_file(
    repo: &Repository,
    entry: &StatusEntry,
    options: DiffOptions,
) -> Result<Option<FileDiff>> {
    assert_off_render_thread();
    let Some(staged) = &entry.staged else {
        return Ok(None);
    };
    let gix = repo.gix();
    let index = gix
        .index_or_empty()
        .map_err(|error| GitError::backend("reading the index", error))?;

    let (change, old_path) = match staged {
        StageChange::Added => (FileChange::Added, None),
        StageChange::Deleted => (FileChange::Deleted, Some(entry.path.clone())),
        StageChange::Modified => (FileChange::Modified, Some(entry.path.clone())),
        StageChange::TypeChanged => (FileChange::Modified, Some(entry.path.clone())),
        StageChange::Renamed { from } => (
            FileChange::Renamed { from: from.clone() },
            Some(from.clone()),
        ),
        StageChange::Copied { from } => (
            FileChange::Copied { from: from.clone() },
            Some(from.clone()),
        ),
    };

    let old = match &old_path {
        Some(path) => head_blob(&gix, path)?,
        None => None,
    };
    let new = match staged {
        StageChange::Deleted => None,
        _ => index_blob(&gix, &index, &entry.path)?,
    };

    Ok(Some(FileDiff {
        path: entry.path.clone(),
        change,
        content: content_of(old, new, entry.is_submodule, options),
    }))
}

/// One unstaged file: its blob in the index against the file on disk.
pub fn unstaged_file(
    repo: &Repository,
    entry: &StatusEntry,
    options: DiffOptions,
) -> Result<Option<FileDiff>> {
    assert_off_render_thread();
    let Some(unstaged) = &entry.unstaged else {
        return Ok(None);
    };
    // Ignored files are listed, never diffed: there is nothing to compare a
    // file Git does not track against.
    if matches!(unstaged, WorktreeChange::Ignored) {
        return Ok(None);
    }
    let gix = repo.gix();
    let index = gix
        .index_or_empty()
        .map_err(|error| GitError::backend("reading the index", error))?;

    let (change, old_path) = match unstaged {
        WorktreeChange::Untracked => (FileChange::Added, None),
        WorktreeChange::Deleted => (FileChange::Deleted, Some(entry.path.clone())),
        WorktreeChange::Renamed { from } => (
            FileChange::Renamed { from: from.clone() },
            Some(from.clone()),
        ),
        WorktreeChange::Modified | WorktreeChange::TypeChanged => {
            (FileChange::Modified, Some(entry.path.clone()))
        }
        WorktreeChange::Ignored => unreachable!("returned above"),
    };

    let old = match &old_path {
        Some(path) => index_blob(&gix, &index, path)?,
        None => None,
    };
    let new = match unstaged {
        WorktreeChange::Deleted => None,
        _ => worktree_blob(repo, &gix, &index, &entry.path)?,
    };

    Ok(Some(FileDiff {
        path: entry.path.clone(),
        change,
        content: content_of(old, new, entry.is_submodule, options),
    }))
}

fn tree_of(gix: &gix::Repository, commit: Option<ObjectId>) -> Result<Option<gix::Tree<'_>>> {
    let Some(id) = commit else {
        return Ok(None);
    };
    let tree = gix
        .find_object(id)
        .map_err(|error| GitError::backend("reading a commit", error))?
        .try_into_commit()
        .map_err(|_| GitError::NotFound(format!("commit {id}")))?
        .tree()
        .map_err(|error| GitError::backend("reading a commit's tree", error))?;
    Ok(Some(tree))
}

fn file_from_tree_change(
    gix: &gix::Repository,
    change: gix::diff::tree_with_rewrites::Change,
    options: DiffOptions,
) -> Result<Option<FileDiff>> {
    use gix::diff::tree_with_rewrites::Change;

    // A tree entry is a directory. Its contents come through as their own
    // changes, and a row for the directory itself would be a row nothing can
    // be shown for.
    let (path, change_kind, old_id, new_id, is_submodule) = match change {
        Change::Addition {
            location,
            entry_mode,
            id,
            ..
        } => {
            if entry_mode.is_tree() {
                return Ok(None);
            }
            (
                RepoPath::from_bytes(location),
                FileChange::Added,
                None,
                Some(id),
                entry_mode.is_commit(),
            )
        }
        Change::Deletion {
            location,
            entry_mode,
            id,
            ..
        } => {
            if entry_mode.is_tree() {
                return Ok(None);
            }
            (
                RepoPath::from_bytes(location),
                FileChange::Deleted,
                Some(id),
                None,
                entry_mode.is_commit(),
            )
        }
        Change::Modification {
            location,
            previous_entry_mode,
            previous_id,
            entry_mode,
            id,
        } => {
            if entry_mode.is_tree() && previous_entry_mode.is_tree() {
                return Ok(None);
            }
            let kind = if previous_id == id {
                FileChange::ModeChanged
            } else {
                FileChange::Modified
            };
            (
                RepoPath::from_bytes(location),
                kind,
                Some(previous_id),
                Some(id),
                entry_mode.is_commit() || previous_entry_mode.is_commit(),
            )
        }
        Change::Rewrite {
            source_location,
            source_entry_mode,
            source_id,
            location,
            entry_mode,
            id,
            copy,
            ..
        } => {
            // Renaming `old/name.txt` to `new/name.txt` is reported twice: once
            // for the file, and once for the directory that moved with it.
            // `relation` exists so a caller can reconstruct the directory-level
            // summary; a file list wants the file.
            if entry_mode.is_tree() || source_entry_mode.is_tree() {
                return Ok(None);
            }
            let from = RepoPath::from_bytes(source_location);
            let kind = if copy {
                FileChange::Copied { from }
            } else {
                FileChange::Renamed { from }
            };
            (
                RepoPath::from_bytes(location),
                kind,
                Some(source_id),
                Some(id),
                entry_mode.is_commit(),
            )
        }
    };

    // A gitlink's object id names a commit in *another* repository, which this
    // one does not have. Asking for it as a blob is not just wasted work, it
    // fails — so the kind is decided before anything is read.
    let content = if is_submodule {
        DiffContent::Submodule {
            old: old_id,
            new: new_id,
        }
    } else {
        let old = old_id.map(|id| read_blob(gix, id)).transpose()?.flatten();
        let new = new_id.map(|id| read_blob(gix, id)).transpose()?.flatten();
        content_of(old, new, false, options)
    };

    Ok(Some(FileDiff {
        path,
        change: change_kind,
        content,
    }))
}

/// A file's bytes on one side of a diff.
type Blob = Vec<u8>;

fn read_blob(gix: &gix::Repository, id: ObjectId) -> Result<Option<Blob>> {
    if id.is_null() {
        return Ok(None);
    }
    let object = gix
        .find_object(id)
        .map_err(|error| GitError::backend("reading a file's contents", error))?;
    Ok((object.kind == gix::object::Kind::Blob).then(|| object.data.clone()))
}

fn head_blob(gix: &gix::Repository, path: &RepoPath) -> Result<Option<Blob>> {
    let Ok(mut tree) = gix.head_tree() else {
        // Unborn HEAD: nothing is committed, so everything staged is an
        // addition against nothing.
        return Ok(None);
    };
    let entry = tree
        .peel_to_entry_by_path(gix::path::from_bstr(path.as_bstr()).as_ref())
        .map_err(|error| GitError::backend("looking a file up in HEAD", error))?;
    match entry {
        Some(entry) => read_blob(gix, entry.object_id()),
        None => Ok(None),
    }
}

fn index_blob(
    gix: &gix::Repository,
    index: &gix::index::File,
    path: &RepoPath,
) -> Result<Option<Blob>> {
    let Some(entry) = index.entry_by_path(path.as_bstr()) else {
        return Ok(None);
    };
    read_blob(gix, entry.id)
}

/// The working-tree file, converted the way Git would convert it on the way in.
///
/// Reading the file raw would be wrong for anything with a `text` attribute, a
/// declared `eol`, or a clean filter: a CRLF checkout of an LF repository would
/// then show every line as modified. `gix`'s filter pipeline applies the same
/// conversions `git add` would, which is what makes the comparison against the
/// index blob meaningful.
fn worktree_blob(
    repo: &Repository,
    gix: &gix::Repository,
    index: &gix::index::File,
    path: &RepoPath,
) -> Result<Option<Blob>> {
    let Some(work_dir) = repo.work_dir() else {
        return Ok(None);
    };
    let Some(absolute) = path.to_absolute(work_dir) else {
        return Ok(None);
    };
    let metadata = match std::fs::symlink_metadata(&absolute) {
        Ok(metadata) => metadata,
        // The file is gone between the status and the diff: a build ran, a
        // branch was switched. Not an error — the next refresh will say so.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(GitError::Io {
                path: absolute,
                source,
            });
        }
    };
    if metadata.is_symlink() {
        // Git stores a symlink's target as its content, and that is what the
        // index blob holds too.
        let target = std::fs::read_link(&absolute).map_err(|source| GitError::Io {
            path: absolute.clone(),
            source,
        })?;
        return Ok(Some(gix::path::into_bstr(target).into_owned().into()));
    }
    if metadata.is_dir() {
        // A submodule, or a directory where a file used to be.
        return Ok(None);
    }

    let file = std::fs::File::open(&absolute).map_err(|source| GitError::Io {
        path: absolute.clone(),
        source,
    })?;
    let (mut pipeline, _) = gix
        .filter_pipeline(None)
        .map_err(|error| GitError::backend("preparing the file filters", error))?;
    let relative = std::path::Path::new(
        gix::path::from_bstr(path.as_bstr())
            .into_owned()
            .as_os_str(),
    )
    .to_owned();
    let mut converted = pipeline
        .convert_to_git(file, &relative, index)
        .map_err(|error| GitError::backend("applying the file filters", error))?;
    let mut buffer = Vec::with_capacity(metadata.len() as usize);
    std::io::Read::read_to_end(&mut converted, &mut buffer).map_err(|source| GitError::Io {
        path: absolute,
        source,
    })?;
    Ok(Some(buffer))
}

/// Decide what kind of body a file has, and compute it if it is text.
fn content_of(
    old: Option<Blob>,
    new: Option<Blob>,
    is_submodule: bool,
    options: DiffOptions,
) -> DiffContent {
    if is_submodule {
        return DiffContent::Submodule {
            old: None,
            new: None,
        };
    }
    let old = old.unwrap_or_default();
    let new = new.unwrap_or_default();
    if old == new {
        return DiffContent::Empty;
    }
    let largest = old.len().max(new.len()) as u64;
    if largest > options.max_bytes {
        return DiffContent::Oversized { bytes: largest };
    }
    if is_binary(&old) || is_binary(&new) {
        return DiffContent::Binary {
            old_bytes: old.len() as u64,
            new_bytes: new.len() as u64,
        };
    }
    text_diff(&old, &new, options)
}

/// Git's binary test: a NUL byte in the first 8 000.
///
/// Not a heuristic anyone would design today, but the point is to agree with
/// `git diff` about which files it refuses to show, not to be cleverer than it.
pub fn is_binary(data: &[u8]) -> bool {
    const SNIFF: usize = 8000;
    data[..data.len().min(SNIFF)].contains(&0)
}

/// The line diff, arranged into hunks with context.
fn text_diff(old: &[u8], new: &[u8], options: DiffOptions) -> DiffContent {
    use gix::diff::blob::sources::byte_lines;
    use gix::diff::blob::{Algorithm, InternedInput};

    let input = InternedInput::new(byte_lines(old), byte_lines(new));
    let diff = gix::diff::blob::diff_with_slider_heuristics(Algorithm::Histogram, &input);

    let changes: Vec<gix::diff::blob::Hunk> = diff.hunks().collect();
    if changes.is_empty() {
        return DiffContent::Empty;
    }

    let context = options.context_lines;
    let old_count = input.before.len() as u32;
    let new_count = input.after.len() as u32;
    let line_of = |side: &[gix::diff::blob::Token], index: u32| -> &[u8] {
        input.interner[side[index as usize]]
    };

    let mut hunks: Vec<Hunk> = Vec::new();
    let mut added = 0usize;
    let mut removed = 0usize;

    // Group changes that are close enough that their context overlaps: two
    // changes three lines apart with three lines of context are one hunk, and
    // showing them as two would repeat the lines between them.
    let mut group: Vec<&gix::diff::blob::Hunk> = Vec::new();
    let mut groups: Vec<Vec<&gix::diff::blob::Hunk>> = Vec::new();
    for change in &changes {
        match group.last() {
            Some(previous) if change.before.start <= previous.before.end + context * 2 => {
                group.push(change);
            }
            Some(_) => {
                groups.push(std::mem::take(&mut group));
                group.push(change);
            }
            None => group.push(change),
        }
    }
    if !group.is_empty() {
        groups.push(group);
    }

    for group in groups {
        let first = group.first().expect("groups are never empty");
        let last = group.last().expect("groups are never empty");
        let old_from = first.before.start.saturating_sub(context);
        let old_to = (last.before.end + context).min(old_count);
        let new_from = first.after.start.saturating_sub(context);
        let new_to = (last.after.end + context).min(new_count);

        let mut lines = Vec::new();
        let mut old_cursor = old_from;
        let mut new_cursor = new_from;

        for change in group {
            // Context before the change.
            while old_cursor < change.before.start {
                lines.push(make_line(
                    LineKind::Context,
                    Some(old_cursor + 1),
                    Some(new_cursor + 1),
                    line_of(&input.before, old_cursor),
                    old_cursor + 1 == old_count && !ends_with_newline(old),
                ));
                old_cursor += 1;
                new_cursor += 1;
            }
            let removed_at = lines.len();
            for index in change.before.clone() {
                lines.push(make_line(
                    LineKind::Removed,
                    Some(index + 1),
                    None,
                    line_of(&input.before, index),
                    index + 1 == old_count && !ends_with_newline(old),
                ));
                removed += 1;
            }
            let added_at = lines.len();
            for index in change.after.clone() {
                lines.push(make_line(
                    LineKind::Added,
                    None,
                    Some(index + 1),
                    line_of(&input.after, index),
                    index + 1 == new_count && !ends_with_newline(new),
                ));
                added += 1;
            }
            if options.refine {
                refine(&mut lines, removed_at, added_at);
            }
            old_cursor = change.before.end;
            new_cursor = change.after.end;
        }

        // Context after the last change of the group.
        while old_cursor < old_to && new_cursor < new_to {
            lines.push(make_line(
                LineKind::Context,
                Some(old_cursor + 1),
                Some(new_cursor + 1),
                line_of(&input.before, old_cursor),
                old_cursor + 1 == old_count && !ends_with_newline(old),
            ));
            old_cursor += 1;
            new_cursor += 1;
        }

        hunks.push(Hunk {
            // `@@` headers are 1-based, and a zero-length side starts at 0 —
            // the one place Git's numbering is not simply "first line".
            old_start: if old_cursor > old_from {
                old_from + 1
            } else {
                0
            },
            old_lines: old_cursor - old_from,
            new_start: if new_cursor > new_from {
                new_from + 1
            } else {
                0
            },
            new_lines: new_cursor - new_from,
            lines,
        });
    }

    DiffContent::Text {
        hunks,
        added,
        removed,
        sides: Box::new(Sides {
            old: old.to_vec(),
            new: new.to_vec(),
        }),
    }
}

fn ends_with_newline(data: &[u8]) -> bool {
    data.last() == Some(&b'\n')
}

fn make_line(
    kind: LineKind,
    old_number: Option<u32>,
    new_number: Option<u32>,
    raw: &[u8],
    last_without_newline: bool,
) -> Line {
    // The tokenizer keeps the terminator on the line; the view draws its own.
    let text = raw
        .strip_suffix(b"\n")
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .unwrap_or(raw);
    Line {
        kind,
        old_number,
        new_number,
        text: text.to_vec(),
        no_newline_at_eof: last_without_newline,
        refinements: Vec::new(),
    }
}

/// Mark the words that differ between the removed and added lines of one
/// change.
///
/// Lines are paired by position, which is what the eye does too: the first line
/// removed lines up with the first line added. Where the counts differ, the
/// surplus lines have no counterpart and are left whole.
fn refine(lines: &mut [Line], removed_at: usize, added_at: usize) {
    let removed_count = added_at - removed_at;
    let added_count = lines.len() - added_at;
    for offset in 0..removed_count.min(added_count) {
        let old = lines[removed_at + offset].text.clone();
        let new = lines[added_at + offset].text.clone();
        let Some((old_spans, new_spans)) = refine_pair(&old, &new) else {
            continue;
        };
        lines[removed_at + offset].refinements = old_spans;
        lines[added_at + offset].refinements = new_spans;
    }
}

/// The changed word spans of two lines, or `None` when highlighting them would
/// be noise.
///
/// Two lines that share almost nothing are not a rewrite of each other; marking
/// nine words out of ten tells the reader less than marking none, and turns the
/// 24% intra-line surface of DESIGN §4 into a solid block. The cutoff is on the
/// share of each line left unmarked.
fn refine_pair(old: &[u8], new: &[u8]) -> Option<(Vec<Refinement>, Vec<Refinement>)> {
    use gix::diff::blob::{Algorithm, Diff, InternedInput};

    if old.is_empty() || new.is_empty() {
        return None;
    }
    let old_words = words(old);
    let new_words = words(new);
    let mut input = InternedInput::default();
    input.update_before(old_words.iter().map(|range| &old[range.clone()]));
    input.update_after(new_words.iter().map(|range| &new[range.clone()]));

    let mut diff = Diff::compute(Algorithm::Myers, &input);
    diff.postprocess_no_heuristic(&input);

    let old_spans = merge(
        (0..old_words.len())
            .filter(|index| diff.is_removed(*index as u32))
            .map(|index| old_words[index].clone()),
    );
    let new_spans = merge(
        (0..new_words.len())
            .filter(|index| diff.is_added(*index as u32))
            .map(|index| new_words[index].clone()),
    );
    if old_spans.is_empty() && new_spans.is_empty() {
        return None;
    }

    const MAX_SHARE: f32 = 0.7;
    let marked = |spans: &[Refinement], len: usize| -> f32 {
        let bytes: usize = spans.iter().map(|span| span.end - span.start).sum();
        bytes as f32 / len as f32
    };
    if marked(&old_spans, old.len()) > MAX_SHARE && marked(&new_spans, new.len()) > MAX_SHARE {
        return None;
    }
    Some((old_spans, new_spans))
}

/// Adjacent spans become one, so a changed word and the comma after it are a
/// single highlight rather than two touching ones.
fn merge(spans: impl Iterator<Item = Range<usize>>) -> Vec<Refinement> {
    let mut out: Vec<Refinement> = Vec::new();
    for span in spans {
        match out.last_mut() {
            Some(last) if last.end == span.start => last.end = span.end,
            _ => out.push(Refinement {
                start: span.start,
                end: span.end,
            }),
        }
    }
    out
}

/// Split a line into word-ish tokens, as byte ranges.
///
/// A run of letters, digits and underscores is one token; a run of spaces or
/// tabs is one token; anything else is a token of its own. That is close enough
/// to `git diff --word-diff` to feel familiar, and it keeps punctuation from
/// gluing itself to the identifier next to it — which matters, because renaming
/// `foo` to `foo_bar` should highlight `_bar`, not the whole expression.
fn words(line: &[u8]) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < line.len() {
        let start = index;
        let byte = line[index];
        if byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80 {
            // Bytes above 0x7f are treated as word material: they are the
            // continuation bytes of a UTF-8 letter, and splitting them apart
            // would cut a character in half.
            while index < line.len()
                && (line[index].is_ascii_alphanumeric()
                    || line[index] == b'_'
                    || line[index] >= 0x80)
            {
                index += 1;
            }
        } else if byte == b' ' || byte == b'\t' {
            while index < line.len() && (line[index] == b' ' || line[index] == b'\t') {
                index += 1;
            }
        } else {
            index += 1;
        }
        out.push(start..index);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(old: &str, new: &str) -> DiffContent {
        text_diff(old.as_bytes(), new.as_bytes(), DiffOptions::default())
    }

    fn hunks(content: &DiffContent) -> &[Hunk] {
        match content {
            DiffContent::Text { hunks, .. } => hunks,
            other => panic!("expected a text diff, got {other:?}"),
        }
    }

    #[test]
    fn one_changed_line_becomes_one_hunk_with_context() {
        let old = "a\nb\nc\nd\ne\nf\ng\n";
        let new = "a\nb\nc\nD\ne\nf\ng\n";
        let content = text(old, new);
        let hunks = hunks(&content);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].header(), "@@ -1,7 +1,7 @@");

        let kinds: Vec<_> = hunks[0].lines.iter().map(|line| line.kind).collect();
        assert_eq!(
            kinds,
            vec![
                LineKind::Context,
                LineKind::Context,
                LineKind::Context,
                LineKind::Removed,
                LineKind::Added,
                LineKind::Context,
                LineKind::Context,
                LineKind::Context,
            ]
        );
        let removed = &hunks[0].lines[3];
        assert_eq!(removed.text_lossy(), "d");
        assert_eq!(removed.old_number, Some(4));
        assert_eq!(removed.new_number, None);
    }

    #[test]
    fn distant_changes_are_separate_hunks() {
        let old: String = (1..=40).map(|n| format!("line {n}\n")).collect();
        let mut new = old.clone();
        new = new.replace("line 2\n", "LINE 2\n");
        new = new.replace("line 39\n", "LINE 39\n");
        let hunks = match text(&old, &new) {
            DiffContent::Text { hunks, .. } => hunks,
            other => panic!("expected text, got {other:?}"),
        };
        assert_eq!(hunks.len(), 2, "40 lines apart is not one hunk");
        assert!(hunks[0].old_start <= 2 && hunks[1].old_start >= 36);
    }

    #[test]
    fn counts_what_changed() {
        let content = text("a\nb\n", "a\nb\nc\nd\n");
        let DiffContent::Text { added, removed, .. } = content else {
            panic!("expected text");
        };
        assert_eq!((added, removed), (2, 0));
    }

    #[test]
    fn identical_content_has_no_diff() {
        assert!(matches!(
            content_of(
                Some(b"same\n".to_vec()),
                Some(b"same\n".to_vec()),
                false,
                DiffOptions::default()
            ),
            DiffContent::Empty
        ));
    }

    #[test]
    fn refines_the_words_that_changed() {
        let content = text("let x = compute(a, b);\n", "let x = compute(a, c);\n");
        let hunks = hunks(&content);
        let removed = hunks[0]
            .lines
            .iter()
            .find(|line| line.kind == LineKind::Removed)
            .expect("one line was removed");
        let added = hunks[0]
            .lines
            .iter()
            .find(|line| line.kind == LineKind::Added)
            .expect("one line was added");

        let marked = |line: &Line| -> Vec<String> {
            line.refinements
                .iter()
                .map(|span| String::from_utf8_lossy(&line.text[span.start..span.end]).into_owned())
                .collect()
        };
        assert_eq!(marked(removed), vec!["b"]);
        assert_eq!(marked(added), vec!["c"]);
    }

    #[test]
    fn does_not_refine_two_unrelated_lines() {
        // Marking almost every word says less than marking none (DESIGN §4).
        let content = text("the quick brown fox\n", "zzz yyy xxx www vvv\n");
        let hunks = hunks(&content);
        for line in &hunks[0].lines {
            assert!(
                line.refinements.is_empty(),
                "{:?} should not be refined",
                line.text_lossy()
            );
        }
    }

    #[test]
    fn splits_words_the_way_a_reader_would() {
        let line = b"foo_bar(baz, 12) ";
        let split: Vec<String> = words(line)
            .into_iter()
            .map(|range| String::from_utf8_lossy(&line[range]).into_owned())
            .collect();
        assert_eq!(split, vec!["foo_bar", "(", "baz", ",", " ", "12", ")", " "]);
    }

    #[test]
    fn keeps_multibyte_characters_whole() {
        let line = "café".as_bytes();
        let split: Vec<String> = words(line)
            .into_iter()
            .map(|range| String::from_utf8_lossy(&line[range]).into_owned())
            .collect();
        assert_eq!(split, vec!["café"], "a UTF-8 letter is not two tokens");
    }

    #[test]
    fn splits_a_side_into_the_lines_the_headers_number() {
        let lines = |text: &str| {
            Sides::lines(text.as_bytes())
                .into_iter()
                .map(|line| String::from_utf8_lossy(line).into_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(lines("a\nb\nc\n"), vec!["a", "b", "c"]);
        assert_eq!(
            lines("a\nb\nc"),
            vec!["a", "b", "c"],
            "a file with no final newline still ends on its last line"
        );
        assert_eq!(
            lines("a\n\nb\n"),
            vec!["a", "", "b"],
            "a blank line in the middle is a line"
        );
        assert_eq!(lines(""), Vec::<String>::new());
        assert_eq!(lines("\n"), vec![""], "a file that is one empty line");
    }

    #[test]
    fn detects_binary_the_way_git_does() {
        assert!(is_binary(b"\x89PNG\r\n\x1a\n\0\0\0"));
        assert!(!is_binary(b"plain text\n"));
        // A NUL past the sniffed window is not seen, exactly as in Git.
        let mut late = vec![b'a'; 9000];
        late.push(0);
        assert!(!is_binary(&late));
    }

    #[test]
    fn a_missing_final_newline_is_marked() {
        let content = text("a\nb", "a\nc");
        let hunks = hunks(&content);
        let last = hunks[0].lines.last().expect("a line");
        assert!(
            last.no_newline_at_eof,
            "the file does not end with a newline"
        );
    }

    #[test]
    fn an_oversized_file_is_reported_not_diffed() {
        let options = DiffOptions {
            max_bytes: 16,
            ..DiffOptions::default()
        };
        let content = content_of(Some(vec![b'a'; 100]), Some(vec![b'b'; 100]), false, options);
        assert!(matches!(content, DiffContent::Oversized { bytes: 100 }));
    }
}

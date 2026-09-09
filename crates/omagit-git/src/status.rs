//! The working copy: what is staged, what is not, and what is in conflict.
//!
//! Git answers this question in two halves and so does `gix`: `HEAD` against the
//! index (what a commit would contain) and the index against the working tree
//! (what a commit would leave behind). The Working Copy screen shows both as two
//! lists, and a file can appear in both at once — staged with further edits on
//! top is the ordinary state of an interactive commit.
//!
//! The two halves are computed in parallel by `gix` and arrive interleaved, so
//! they are merged here by path into one [`StatusEntry`] per file, which is what
//! a file list row is. Sorting happens at the end, once: the arrival order is a
//! function of how many threads `gix` used, and a list that reorders itself
//! between refreshes is unusable.

use std::collections::BTreeMap;

use crate::{Cancel, GitError, RepoPath, Repository, Result, assert_off_render_thread};

/// What to include in a status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusOptions {
    /// List ignored files. Off by default — the same default as `git status`,
    /// and on a repository with a `target/` directory the difference is four
    /// orders of magnitude in entries.
    pub include_ignored: bool,
    /// List untracked files. On by default: they are half of what the Working
    /// Copy screen is for.
    pub include_untracked: bool,
    /// Detect renames on both halves. On by default, and worth it: a renamed
    /// file shown as one deletion plus one addition is the single most common
    /// way a diff view becomes useless.
    pub detect_renames: bool,
}

impl Default for StatusOptions {
    fn default() -> Self {
        Self {
            include_ignored: false,
            include_untracked: true,
            detect_renames: true,
        }
    }
}

/// The working copy, one entry per path.
#[derive(Clone, Debug, Default)]
pub struct Status {
    /// Sorted by path.
    pub entries: Vec<StatusEntry>,
}

/// One file's state, on both sides of the index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusEntry {
    pub path: RepoPath,
    /// `HEAD` → index: what committing now would record.
    pub staged: Option<StageChange>,
    /// Index → working tree: what committing now would leave behind.
    pub unstaged: Option<WorktreeChange>,
    /// Set when the merge left the path with more than one stage.
    pub conflict: Option<Conflict>,
    /// The entry is a submodule (a gitlink), not a file. Out of MVP scope
    /// (SPEC §11) but it still has to be *shown*, or the file disappears from a
    /// list the user is checking for completeness.
    pub is_submodule: bool,
    /// The entry stands for a whole directory rather than one file.
    ///
    /// Only ignored entries are collapsed this way, and deliberately: listing
    /// `node_modules/` file by file is tens of thousands of rows nobody asked
    /// for, and it is what `git status --ignored` shows too. Untracked files are
    /// never collapsed — they are half of what the Working Copy screen is for.
    pub is_directory: bool,
}

impl StatusEntry {
    fn new(path: RepoPath) -> Self {
        Self {
            path,
            staged: None,
            unstaged: None,
            conflict: None,
            is_submodule: false,
            is_directory: false,
        }
    }

    /// Untracked and ignored files are not "changes" in the Git sense; the two
    /// lists of the Working Copy screen separate them from the rest.
    pub fn is_untracked(&self) -> bool {
        matches!(self.unstaged, Some(WorktreeChange::Untracked))
    }

    pub fn is_ignored(&self) -> bool {
        matches!(self.unstaged, Some(WorktreeChange::Ignored))
    }
}

/// A change between `HEAD` and the index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StageChange {
    Added,
    Modified,
    Deleted,
    /// Regular file ↔ symlink, or a file replaced by a submodule.
    TypeChanged,
    Renamed {
        from: RepoPath,
    },
    Copied {
        from: RepoPath,
    },
}

/// A change between the index and the working tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorktreeChange {
    Modified,
    /// A file the index knows the *name* of and nothing else — `git add -N`,
    /// which is how a new file is made diffable so part of it can be staged.
    ///
    /// Its own state rather than `Untracked`: the file has an index entry, so
    /// `git diff` describes it and a patch can be built against it, neither of
    /// which is true of an untracked file. `git status` prints it as ` A`, in
    /// the worktree column, for the same reason — nothing is staged yet.
    Added,
    Deleted,
    TypeChanged,
    Untracked,
    Ignored,
    /// A tracked file was moved on disk without `git mv`: the index still has
    /// it at `from`, and an untracked file with the same content appeared here.
    Renamed {
        from: RepoPath,
    },
}

/// Which side did what, for a path with conflicting stages.
///
/// Named from the merge's point of view — "us" is the branch being merged into,
/// the same convention as `git checkout --ours`. M8 resolves these; M2 reports
/// them so nothing else has to guess what a file with three stages means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conflict {
    BothModified,
    BothAdded,
    BothDeleted,
    AddedByUs,
    AddedByThem,
    DeletedByUs,
    DeletedByThem,
}

impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Conflict::BothModified => "both modified",
            Conflict::BothAdded => "both added",
            Conflict::BothDeleted => "both deleted",
            Conflict::AddedByUs => "added by us",
            Conflict::AddedByThem => "added by them",
            Conflict::DeletedByUs => "deleted by us",
            Conflict::DeletedByThem => "deleted by them",
        })
    }
}

impl Status {
    /// Read the working copy.
    ///
    /// Cancellable throughout: `gix` polls the same token from its worker
    /// threads, so a status on a large repository stops when the user switches
    /// away instead of finishing into a result nobody reads.
    pub fn load(repo: &Repository, options: StatusOptions, cancel: &Cancel) -> Result<Self> {
        assert_off_render_thread();
        if repo.is_bare() {
            // A bare repository has no working tree, so the answer is "no
            // changes" rather than an error — the Working Copy screen shows its
            // own empty state for this (DESIGN §4).
            return Ok(Self::default());
        }

        let gix = repo.gix();
        let mut platform = gix
            .status(gix::progress::Discard)
            .map_err(|error| GitError::backend("reading the working copy", error))?
            .should_interrupt_owned(cancel.as_flag())
            .index_worktree_submodules(gix::status::Submodule::AsConfigured { check_dirty: true })
            // One row per file. The default collapses an untracked directory
            // into a single entry, which is right for a terminal and wrong for
            // a list whose whole purpose is to stage individual files.
            .untracked_files(if options.include_untracked {
                gix::status::UntrackedFiles::Files
            } else {
                gix::status::UntrackedFiles::None
            });

        if options.include_ignored {
            platform = platform.dirwalk_options(|options| {
                options
                    .emit_ignored(Some(gix::dir::walk::EmissionMode::Matching))
                    // Without this, an ignored file inside an untracked
                    // directory is swallowed by the directory that collapses
                    // over it.
                    .emit_collapsed(Some(gix::dir::walk::CollapsedEntriesEmissionMode::All))
            });
        }

        if options.detect_renames {
            platform = platform
                .index_worktree_rewrites(Some(gix::diff::Rewrites::default()))
                .tree_index_track_renames(gix::status::tree_index::TrackRenames::AsConfigured);
        } else {
            platform = platform
                .index_worktree_rewrites(None)
                .tree_index_track_renames(gix::status::tree_index::TrackRenames::Disabled);
        }

        let iter = platform
            .into_iter(None)
            .map_err(|error| GitError::backend("reading the working copy", error))?;

        let mut entries: BTreeMap<RepoPath, StatusEntry> = BTreeMap::new();
        for item in iter {
            cancel.check()?;
            let item =
                item.map_err(|error| GitError::backend("reading the working copy", error))?;
            match item {
                gix::status::Item::TreeIndex(change) => absorb_staged(&mut entries, change),
                gix::status::Item::IndexWorktree(item) => absorb_unstaged(&mut entries, item),
            }
        }

        Ok(Self {
            entries: entries.into_values().collect(),
        })
    }

    pub fn is_clean(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn conflicts(&self) -> impl Iterator<Item = &StatusEntry> {
        self.entries.iter().filter(|entry| entry.conflict.is_some())
    }
}

fn entry_for(entries: &mut BTreeMap<RepoPath, StatusEntry>, path: RepoPath) -> &mut StatusEntry {
    entries
        .entry(path.clone())
        .or_insert_with(|| StatusEntry::new(path))
}

/// `HEAD` → index.
fn absorb_staged(entries: &mut BTreeMap<RepoPath, StatusEntry>, change: gix::diff::index::Change) {
    use gix::diff::index::Change;
    let (path, staged, mode) = match change {
        Change::Addition {
            location,
            entry_mode,
            ..
        } => (
            RepoPath::from_bytes(location.into_owned()),
            StageChange::Added,
            entry_mode,
        ),
        Change::Deletion {
            location,
            entry_mode,
            ..
        } => (
            RepoPath::from_bytes(location.into_owned()),
            StageChange::Deleted,
            entry_mode,
        ),
        Change::Modification {
            location,
            previous_entry_mode,
            entry_mode,
            ..
        } => {
            let change = if kind_of(previous_entry_mode) != kind_of(entry_mode) {
                StageChange::TypeChanged
            } else {
                StageChange::Modified
            };
            (
                RepoPath::from_bytes(location.into_owned()),
                change,
                entry_mode,
            )
        }
        Change::Rewrite {
            source_location,
            location,
            entry_mode,
            copy,
            ..
        } => {
            let from = RepoPath::from_bytes(source_location.into_owned());
            let change = if copy {
                StageChange::Copied { from }
            } else {
                StageChange::Renamed { from }
            };
            (
                RepoPath::from_bytes(location.into_owned()),
                change,
                entry_mode,
            )
        }
    };

    let entry = entry_for(entries, path);
    entry.staged = Some(staged);
    entry.is_submodule |= is_gitlink(mode);
}

/// Index → working tree.
fn absorb_unstaged(
    entries: &mut BTreeMap<RepoPath, StatusEntry>,
    item: gix::status::index_worktree::Item,
) {
    use gix::status::index_worktree::Item;
    use gix::status::plumbing::index_as_worktree::{Change, EntryStatus};

    match item {
        Item::Modification {
            rela_path,
            status,
            entry,
            ..
        } => {
            let path = RepoPath::from_bytes(rela_path);
            match status {
                EntryStatus::Conflict { summary, .. } => {
                    let target = entry_for(entries, path);
                    target.conflict = Some(conflict_from(summary));
                }
                EntryStatus::Change(change) => {
                    let unstaged = match change {
                        Change::Removed => WorktreeChange::Deleted,
                        Change::Type { .. } => WorktreeChange::TypeChanged,
                        Change::Modification { .. } | Change::SubmoduleModification(_) => {
                            WorktreeChange::Modified
                        }
                    };
                    let is_submodule = is_gitlink(entry.mode);
                    let target = entry_for(entries, path);
                    target.unstaged = Some(unstaged);
                    target.is_submodule |= is_submodule;
                }
                // `gix` telling us the index could be written back faster next
                // time. Not a change to anything the user did.
                EntryStatus::NeedsUpdate(_) => {}
                // A file added with `git add -N`. This used to be dropped here,
                // on the belief that its staged half arrived through the
                // tree-index comparison — it does not: `gix` reports an
                // intent-to-add entry only on this side, so the file vanished
                // from the working copy entirely. `git status` shows it as
                // ` A`, and so do we.
                EntryStatus::IntentToAdd => {
                    entry_for(entries, path).unstaged = Some(WorktreeChange::Added);
                }
            }
        }
        Item::DirectoryContents { entry, .. } => {
            let Some(unstaged) = dirwalk_change(&entry) else {
                return;
            };
            let path = RepoPath::from_bytes(entry.rela_path);
            let is_submodule = entry.disk_kind == Some(gix::dir::entry::Kind::Repository);
            let is_directory = entry.disk_kind == Some(gix::dir::entry::Kind::Directory);
            let target = entry_for(entries, path);
            target.unstaged = Some(unstaged);
            target.is_submodule |= is_submodule;
            target.is_directory |= is_directory;
        }
        Item::Rewrite {
            source,
            dirwalk_entry,
            ..
        } => {
            let from = RepoPath::from_bytes(source.rela_path().to_owned());
            let path = RepoPath::from_bytes(dirwalk_entry.rela_path);
            entry_for(entries, path).unstaged = Some(WorktreeChange::Renamed { from });
        }
    }
}

/// A gitlink — the index entry a submodule leaves behind.
///
/// `gix` spells it `DIR | SYMLINK`, which is Git's own encoding of mode
/// `160000`; the name here is the one the rest of the crate uses.
fn is_gitlink(mode: gix::index::entry::Mode) -> bool {
    mode.is_submodule()
}

/// File, symlink or gitlink — the distinction a "type changed" row reports.
/// The executable bit is deliberately not part of it: a mode change from 644 to
/// 755 is a modification, not a change of kind.
fn kind_of(mode: gix::index::entry::Mode) -> u8 {
    if mode.is_submodule() {
        2
    } else if mode == gix::index::entry::Mode::SYMLINK {
        1
    } else {
        0
    }
}

fn dirwalk_change(entry: &gix::dir::Entry) -> Option<WorktreeChange> {
    match entry.status {
        gix::dir::entry::Status::Untracked => Some(WorktreeChange::Untracked),
        gix::dir::entry::Status::Ignored(_) => Some(WorktreeChange::Ignored),
        // `Tracked` entries are emitted only when explicitly asked for, and a
        // `Pruned` one is `.git` itself or a path the pathspec excluded.
        gix::dir::entry::Status::Tracked | gix::dir::entry::Status::Pruned => None,
    }
}

fn conflict_from(summary: gix::status::plumbing::index_as_worktree::Conflict) -> Conflict {
    use gix::status::plumbing::index_as_worktree::Conflict as Summary;
    match summary {
        Summary::BothModified => Conflict::BothModified,
        Summary::BothAdded => Conflict::BothAdded,
        Summary::BothDeleted => Conflict::BothDeleted,
        Summary::AddedByUs => Conflict::AddedByUs,
        Summary::AddedByThem => Conflict::AddedByThem,
        Summary::DeletedByUs => Conflict::DeletedByUs,
        Summary::DeletedByThem => Conflict::DeletedByThem,
    }
}

/// The short letters `git status --short` uses, for the debug CLI and for logs.
///
/// Two columns: staged, then unstaged. Deliberately the same alphabet as Git's,
/// so a bug report can be compared against a terminal without translation.
pub fn short_code(entry: &StatusEntry) -> String {
    if entry.conflict.is_some() {
        return "UU".into();
    }
    let staged = match &entry.staged {
        Some(StageChange::Added) => 'A',
        Some(StageChange::Modified) => 'M',
        Some(StageChange::Deleted) => 'D',
        Some(StageChange::TypeChanged) => 'T',
        Some(StageChange::Renamed { .. }) => 'R',
        Some(StageChange::Copied { .. }) => 'C',
        None => ' ',
    };
    let unstaged = match &entry.unstaged {
        Some(WorktreeChange::Modified) => 'M',
        Some(WorktreeChange::Added) => 'A',
        Some(WorktreeChange::Deleted) => 'D',
        Some(WorktreeChange::TypeChanged) => 'T',
        Some(WorktreeChange::Renamed { .. }) => 'R',
        Some(WorktreeChange::Untracked) => '?',
        Some(WorktreeChange::Ignored) => '!',
        None => ' ',
    };
    // `??` and `!!` are Git's, and the staged column is meaningless for a file
    // Git has never seen.
    if entry.staged.is_none() && matches!(unstaged, '?' | '!') {
        return format!("{unstaged}{unstaged}");
    }
    format!("{staged}{unstaged}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(staged: Option<StageChange>, unstaged: Option<WorktreeChange>) -> StatusEntry {
        StatusEntry {
            path: RepoPath::from("f"),
            staged,
            unstaged,
            conflict: None,
            is_submodule: false,
            is_directory: false,
        }
    }

    #[test]
    fn short_codes_match_gits_alphabet() {
        assert_eq!(short_code(&entry(Some(StageChange::Added), None)), "A ");
        assert_eq!(
            short_code(&entry(None, Some(WorktreeChange::Modified))),
            " M"
        );
        assert_eq!(
            short_code(&entry(
                Some(StageChange::Modified),
                Some(WorktreeChange::Modified)
            )),
            "MM",
            "staged and then edited again is one row, not two"
        );
        assert_eq!(
            short_code(&entry(None, Some(WorktreeChange::Untracked))),
            "??"
        );
        assert_eq!(
            short_code(&entry(None, Some(WorktreeChange::Ignored))),
            "!!"
        );

        let mut conflicted = entry(Some(StageChange::Modified), Some(WorktreeChange::Modified));
        conflicted.conflict = Some(Conflict::BothModified);
        assert_eq!(
            short_code(&conflicted),
            "UU",
            "a conflict outranks whatever the two halves say"
        );
    }
}

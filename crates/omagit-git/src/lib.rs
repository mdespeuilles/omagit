//! omagit's Git core — the read path.
//!
//! No UI dependency (SPEC §3 rule 5): this crate compiles, tests and runs from a
//! command line with no window and no renderer, which is what makes the Git
//! behaviour testable at all. `omagit-git-cli` in this workspace is that command
//! line.
//!
//! The pieces, in the order a screen needs them:
//!
//! * [`repo`] — opening a repository, `HEAD`, and any half-finished operation.
//! * [`refs`] — branches, remotes and tags, with ahead/behind.
//! * [`status`] — the working copy: staged, unstaged, untracked, conflicted.
//! * [`stash`] — the shelf: what is on it, and what one entry holds.
//! * [`summary`] — the whole of one repository at a glance, in one read.
//! * [`history`] — a resumable walk over commits.
//! * [`graph`] — which lane each commit sits in, and what connects to it.
//! * [`diff`] — hunks, lines, and intra-line refinement.
//! * [`paths`] — Git paths as bytes, and the one place they meet the filesystem.
//! * [`cli`] — running the `git` binary, under the rules of SPEC §8.
//! * [`watch`] — noticing that the repository changed underneath us.
//! * [`cancel`] — the token everything long-running polls.
//! * [`thread_guard`] — the assertion that none of this runs on the render
//!   thread.
//!
//! ## Which backend does what
//!
//! SPEC §8 splits the work: `gix` reads, the `git` binary writes and talks to
//! the network. M2 is entirely reads, and the measurements in
//! `docs/notes/gitoxide-capabilities.md` say `gix` covers them — so every module
//! here is `gix`, and [`cli`] carries the start-up check that a usable `git`
//! exists for the milestones that need it. The `GitBackend` trait SPEC §8
//! describes arrives with its second implementation at M5, not before: a trait
//! with one implementation is an abstraction nobody has tested (SPEC §2).

pub mod cancel;
pub mod cli;
pub mod diff;
pub mod error;
pub mod graph;
pub mod history;
pub mod journal;
pub mod ops;
pub mod patch;
pub mod paths;
pub mod refs;
pub mod repo;
pub mod stash;
pub mod status;
pub mod summary;
pub mod thread_guard;
pub mod watch;

pub use cancel::Cancel;
pub use diff::{Diff, DiffOptions, FileDiff, Hunk, Line, LineKind, Refinement};
pub use error::{GitError, Result};
pub use graph::{Graph, Row as GraphRow};
pub use history::{Civil, Commit, HistoryQuery, Signature, Time, Walk};
pub use paths::RepoPath;
pub use refs::{Branch, Refs, Remote, Tag, Tracking};
pub use repo::{Head, Operation, Repository};
pub use stash::Stash;
pub use status::{Conflict, StageChange, Status, StatusEntry, StatusOptions, WorktreeChange};
pub use summary::{Activity, Counts, Identity, Summary};
pub use thread_guard::{assert_off_render_thread, mark_render_thread, on_render_thread};
pub use watch::{Changes, Watcher};

/// A Git object hash.
///
/// `gix`'s own type, re-exported rather than wrapped: it is already the right
/// shape (inline, `Copy`, hex on `Display`), and a newtype would only add
/// conversions at every call site. It carries no UI dependency, so re-exporting
/// it does not weaken rule 5.
pub type ObjectId = gix::ObjectId;

/// The oldest `git` omagit runs against (SPEC §8).
pub const MINIMUM_GIT_VERSION: &str = "2.35";

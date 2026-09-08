//! Walking commits, one page at a time.
//!
//! SPEC §12 asks for the first 1 000 commits of a 100 000-commit history in
//! under 250 ms, and for scrolling that never stutters. Both rule out loading
//! the history and then slicing it: the first page must not pay for the last.
//!
//! So the walk is a resumable frontier — a priority queue of the commits whose
//! parents have not been visited yet, ordered by commit time — and a page is
//! `limit` pops from it. Nothing is recomputed between pages, and the frontier
//! is small: it holds the tips of the branches currently being merged back
//! together, not the history behind them.
//!
//! `gix` has its own revision walk and it is a good one, but it borrows the
//! repository for the lifetime of the iterator, which makes a walk that outlives
//! a single background job impossible to store. Owning the traversal also means
//! M6's lane computation reads the same order the list does, which is the one
//! way a commit graph is guaranteed to line up with its rows.
//!
//! ## The order
//!
//! Commit time, newest first, with the object id breaking ties so the order is
//! total and reproducible — `git log --date-order`. Commit times are not
//! monotonic (a rebase, a wrong clock, a patch applied out of order), so this is
//! not a topological order, and a child can appear above a parent that claims to
//! be younger. Git has the same behaviour for the same reason, and matching it
//! is more useful than being right in a way no other tool is.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};

use gix::bstr::ByteSlice;

use crate::{Cancel, GitError, ObjectId, Repository, Result, assert_off_render_thread};

/// One commit, as a history row and as the header of a commit detail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub id: ObjectId,
    /// In order. The first is the one a `--first-parent` walk follows; more
    /// than one means a merge.
    pub parents: Vec<ObjectId>,
    pub author: Signature,
    pub committer: Signature,
    /// The first line of the message, which is what a row shows.
    pub summary: String,
    /// Everything after the first line, trimmed. Empty for most commits.
    pub body: String,
}

impl Commit {
    pub fn is_merge(&self) -> bool {
        self.parents.len() > 1
    }

    pub fn is_root(&self) -> bool {
        self.parents.is_empty()
    }
}

/// Who did something, and when.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    pub name: String,
    pub email: String,
    pub time: Time,
}

impl Signature {
    pub(crate) fn from_ref(signature: gix::actor::SignatureRef<'_>) -> Self {
        let signature = signature.trim();
        Self {
            name: signature.name.to_string(),
            email: signature.email.to_string(),
            time: signature
                .time()
                .map(|time| Time {
                    seconds: time.seconds,
                    offset_seconds: time.offset,
                })
                .unwrap_or_default(),
        }
    }
}

/// A commit timestamp: seconds since the epoch, plus the author's offset from
/// UTC.
///
/// Both halves are kept because both are shown: History displays local time,
/// while a commit detail can show the time the author saw on their own clock.
/// Formatting is the UI's business — this crate has no locale.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Time {
    pub seconds: i64,
    pub offset_seconds: i32,
}

/// Where a walk starts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Tips {
    /// `HEAD`, the History screen's default.
    #[default]
    Head,
    /// Every branch and tag: the "all branches" view.
    All,
    /// Named starting points — a comparison, or one branch's history.
    These(Vec<ObjectId>),
}

/// What to walk.
///
/// The filters of SPEC §11 (author, path, date range, text) arrive with the
/// History screen at M6; they belong here, on this struct, and not in the walk
/// below, which is why it takes the query rather than a list of tips.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryQuery {
    pub tips: Tips,
    /// Follow only the first parent of each merge — the "hide merged branches"
    /// view.
    pub first_parent: bool,
}

impl HistoryQuery {
    pub fn head() -> Self {
        Self::default()
    }

    pub fn all() -> Self {
        Self {
            tips: Tips::All,
            ..Self::default()
        }
    }

    pub fn from(tips: impl IntoIterator<Item = ObjectId>) -> Self {
        Self {
            tips: Tips::These(tips.into_iter().collect()),
            ..Self::default()
        }
    }

    pub fn first_parent(mut self) -> Self {
        self.first_parent = true;
        self
    }
}

/// A history walk that can be resumed.
///
/// Owns everything it needs, so it can be parked in the repository store
/// between pages and moved to whichever background thread asks for the next one.
pub struct Walk {
    repo: Repository,
    query: HistoryQuery,
    frontier: BinaryHeap<Front>,
    /// Commits already queued. Also the reason a walk of a 100 000-commit
    /// history costs a few megabytes: without it, a merge-heavy history is
    /// walked exponentially.
    seen: HashSet<ObjectId>,
    yielded: usize,
}

impl Walk {
    pub fn new(repo: &Repository, query: HistoryQuery, cancel: &Cancel) -> Result<Self> {
        assert_off_render_thread();
        let mut walk = Self {
            repo: repo.clone(),
            query: query.clone(),
            frontier: BinaryHeap::new(),
            seen: HashSet::new(),
            yielded: 0,
        };
        let gix = walk.repo.gix();
        for tip in walk.resolve_tips(&gix, &query)? {
            cancel.check()?;
            walk.push(&gix, tip)?;
        }
        Ok(walk)
    }

    /// The next `limit` commits, newest first. A short page means the walk has
    /// reached the roots.
    pub fn next_page(&mut self, limit: usize, cancel: &Cancel) -> Result<Vec<Commit>> {
        assert_off_render_thread();
        let gix = self.repo.gix();
        let mut page = Vec::with_capacity(limit.min(1024));
        while page.len() < limit {
            cancel.check()?;
            let Some(front) = self.frontier.pop() else {
                break;
            };
            let take = if self.query.first_parent {
                1.min(front.commit.parents.len())
            } else {
                front.commit.parents.len()
            };
            // Collected before pushing: `push` needs `&mut self`, and the
            // parents live in the commit this page is about to hand out.
            let parents: Vec<_> = front.commit.parents[..take].to_vec();
            for parent in parents {
                self.push(&gix, parent)?;
            }
            page.push(front.commit);
        }
        self.yielded += page.len();
        Ok(page)
    }

    /// True once every reachable commit has been handed out.
    pub fn is_done(&self) -> bool {
        self.frontier.is_empty()
    }

    /// How many commits this walk has produced so far — the row count the
    /// virtualised list scrolls through.
    pub fn yielded(&self) -> usize {
        self.yielded
    }

    fn push(&mut self, gix: &gix::Repository, id: ObjectId) -> Result<()> {
        if !self.seen.insert(id) {
            return Ok(());
        }
        // A parent can be absent from a shallow clone, and a tag can point at a
        // blob. Neither is a reason to fail the whole walk: the row is simply
        // not there, exactly as `git log` would show it.
        match read_commit(gix, id) {
            Ok(commit) => {
                self.frontier.push(Front {
                    time: commit.committer.time.seconds,
                    commit,
                });
                Ok(())
            }
            Err(error) => {
                tracing::debug!(commit = %id, %error, "skipping unreadable commit");
                Ok(())
            }
        }
    }

    fn resolve_tips(&self, gix: &gix::Repository, query: &HistoryQuery) -> Result<Vec<ObjectId>> {
        Ok(match &query.tips {
            Tips::Head => match self.repo.head()?.commit() {
                Some(commit) => vec![*commit],
                // An unborn HEAD has no history, and that is not an error: the
                // History screen shows its empty state (DESIGN §4).
                None => Vec::new(),
            },
            Tips::These(ids) => ids.clone(),
            Tips::All => {
                let platform = gix
                    .references()
                    .map_err(|error| GitError::backend("listing references", error))?;
                let mut tips = Vec::new();
                for mut reference in platform
                    .all()
                    .map_err(|error| GitError::backend("listing references", error))?
                    .filter_map(std::result::Result::ok)
                {
                    if let Ok(id) = reference.peel_to_id() {
                        tips.push(id.detach());
                    }
                }
                tips
            }
        })
    }
}

/// A queued commit, ordered so the newest pops first.
struct Front {
    time: i64,
    commit: Commit,
}

impl Ord for Front {
    fn cmp(&self, other: &Self) -> Ordering {
        // `BinaryHeap` is a max-heap, so this order is "newest is greatest".
        // The id breaks ties: without it, two commits made in the same second
        // would come out in whatever order the hash map happened to produce,
        // and the same repository would render two different histories.
        self.time
            .cmp(&other.time)
            .then_with(|| self.commit.id.cmp(&other.commit.id))
    }
}

impl PartialOrd for Front {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Front {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Front {}

/// Read one commit by id — the header of a commit detail, and what a walk
/// yields.
pub fn read_commit(gix: &gix::Repository, id: ObjectId) -> Result<Commit> {
    let object = gix
        .find_object(id)
        .map_err(|error| GitError::backend("reading a commit", error))?;
    let commit = object
        .try_into_commit()
        .map_err(|_| GitError::NotFound(format!("commit {id}")))?;
    let decoded = commit
        .decode()
        .map_err(|error| GitError::backend("decoding a commit", error))?;
    let message = decoded.message();
    Ok(Commit {
        id,
        parents: decoded.parents().collect(),
        author: Signature::from_ref(
            decoded
                .author()
                .map_err(|error| GitError::backend("decoding a commit author", error))?,
        ),
        committer: Signature::from_ref(
            decoded
                .committer()
                .map_err(|error| GitError::backend("decoding a commit committer", error))?,
        ),
        summary: message.summary().to_string(),
        body: message
            .body
            .map(|body| body.trim().to_str_lossy().into_owned())
            .unwrap_or_default(),
    })
}

/// Read one commit through the public API.
pub fn commit(repo: &Repository, id: ObjectId) -> Result<Commit> {
    assert_off_render_thread();
    read_commit(&repo.gix(), id)
}

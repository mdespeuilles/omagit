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
use std::collections::{BinaryHeap, HashMap, HashSet};

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

impl Time {
    /// The instant broken into calendar parts, as the author's own clock showed
    /// them.
    ///
    /// Decomposition, not formatting: no month names, no ordering convention,
    /// no locale. Those belong to whoever displays it — and there are two of
    /// them, the debug CLI and the app, which want very different strings from
    /// the same six numbers.
    pub fn civil(&self) -> Civil {
        let local = self.seconds + i64::from(self.offset_seconds);
        let days = local.div_euclid(86_400);
        let seconds = local.rem_euclid(86_400);
        let (year, month, day) = civil_from_days(days);
        Civil {
            year,
            month,
            day,
            hour: (seconds / 3600) as u32,
            minute: ((seconds % 3600) / 60) as u32,
            second: (seconds % 60) as u32,
        }
    }
}

/// A timestamp's calendar parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Civil {
    pub year: i64,
    /// 1–12.
    pub month: u32,
    /// 1–31.
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

/// Days since the epoch to a civil date — Howard Hinnant's `civil_from_days`,
/// exact for every year a Git commit can carry, and with no dependency behind
/// it.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    (year + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod time_tests {
    use super::*;

    fn civil(seconds: i64, offset_seconds: i32) -> Civil {
        Time {
            seconds,
            offset_seconds,
        }
        .civil()
    }

    #[test]
    fn decomposes_in_the_authors_own_offset() {
        // 2026-09-08T13:19:33Z, written by someone two hours east of UTC.
        assert_eq!(
            civil(1_788_873_573, 7200),
            Civil {
                year: 2026,
                month: 9,
                day: 8,
                hour: 15,
                minute: 19,
                second: 33
            }
        );
        assert_eq!(
            civil(0, 0),
            Civil {
                year: 1970,
                month: 1,
                day: 1,
                hour: 0,
                minute: 0,
                second: 0
            }
        );
    }

    #[test]
    fn a_negative_offset_can_cross_back_over_midnight() {
        // 2024-02-29T00:00:00Z seen from five hours west is the leap day's eve.
        assert_eq!(
            civil(1_709_164_800, -18_000),
            Civil {
                year: 2024,
                month: 2,
                day: 28,
                hour: 19,
                minute: 0,
                second: 0
            }
        );
    }

    #[test]
    fn handles_a_leap_day_and_a_century_that_is_not_one() {
        // 2024 is a leap year, 1900 was not — the two cases the algorithm is
        // chosen for.
        assert_eq!(civil(1_709_208_000, 0).day, 29, "2024-02-29 exists");
        assert_eq!(civil(1_709_208_000, 0).month, 2);
        // 1900-03-01T00:00:00Z
        assert_eq!(
            civil(-2_203_891_200, 0),
            Civil {
                year: 1900,
                month: 3,
                day: 1,
                hour: 0,
                minute: 0,
                second: 0
            }
        );
    }
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
/// How far ahead of the emitted row the walk reads.
///
/// The order below only works if a commit's children are discovered before it
/// becomes eligible, and discovery runs ahead of emission by this much. A child
/// and its parent are neighbours in time order — that is what a commit *is* —
/// so a thousand rows of slack is many orders of magnitude more than any real
/// history needs, and it is what makes this exact in practice rather than only
/// in principle.
///
/// Git is exact instead of approximate here, and pays for it: `--date-order`
/// sorts the whole range before printing a line. That is the trade this design
/// cannot make — SPEC §11 wants history paged and resumed, §12 wants the first
/// screenful of a 100 000-commit repository immediately.
const LOOKAHEAD: usize = 1024;

pub struct Walk {
    repo: Repository,
    query: HistoryQuery,
    /// What to read next, newest first. Reading is not the same thing as
    /// handing out: this heap runs ahead of what is emitted.
    frontier: BinaryHeap<Front>,
    /// Commits already queued. Also the reason a walk of a 100 000-commit
    /// history costs a few megabytes: without it, a merge-heavy history is
    /// walked exponentially.
    seen: HashSet<ObjectId>,
    /// How many discovered children each commit is still waiting for.
    ///
    /// This is what makes the order `git log --date-order`'s rather than merely
    /// "newest timestamp first". Git's guarantee is *no parent before all of
    /// its children*, and time alone does not give it: two commits made in the
    /// same second — a scripted import, a rebase, `git commit` twice in one
    /// second — tie, and a parent can win the tie against its own grandchild.
    /// The graph then draws a line upwards, into a node already passed.
    blocked: HashMap<ObjectId, usize>,
    /// Read, and every child of it already handed out: the next row can be any
    /// of these, and it is the newest.
    ready: BinaryHeap<Front>,
    /// Read, but some child of it has not been handed out yet.
    waiting: HashMap<ObjectId, Front>,
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
            blocked: HashMap::new(),
            ready: BinaryHeap::new(),
            waiting: HashMap::new(),
            yielded: 0,
        };
        let gix = walk.repo.gix();
        for tip in walk.resolve_tips(&gix, &query)? {
            cancel.check()?;
            walk.read(&gix, tip)?;
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
            // Read ahead first, so that whatever becomes eligible below has had
            // the chance to be blocked by a child.
            self.explore(&gix, limit - page.len() + LOOKAHEAD, cancel)?;
            let Some(front) = self.next() else {
                break;
            };
            // Handing this commit out releases each parent from waiting on it.
            for parent in self.followed(&front.commit) {
                self.release(parent);
            }
            page.push(front.commit);
        }
        self.yielded += page.len();
        Ok(page)
    }

    /// True once every reachable commit has been handed out.
    pub fn is_done(&self) -> bool {
        self.frontier.is_empty() && self.ready.is_empty() && self.waiting.is_empty()
    }

    /// Read until `want` commits are in hand, or the history runs out.
    fn explore(&mut self, gix: &gix::Repository, want: usize, cancel: &Cancel) -> Result<()> {
        while self.ready.len() + self.waiting.len() < want {
            cancel.check()?;
            let Some(front) = self.frontier.pop() else {
                return Ok(());
            };
            for parent in self.followed(&front.commit) {
                self.read(gix, parent)?;
            }
            self.file(front);
        }
        Ok(())
    }

    /// The parents this walk follows — one for `--first-parent`, all otherwise.
    fn followed(&self, commit: &Commit) -> Vec<ObjectId> {
        let take = if self.query.first_parent {
            1.min(commit.parents.len())
        } else {
            commit.parents.len()
        };
        commit.parents[..take].to_vec()
    }

    /// Put a commit that has been read into `ready` or `waiting`.
    fn file(&mut self, front: Front) {
        if self.blocked.get(&front.commit.id).copied().unwrap_or(0) == 0 {
            self.ready.push(front);
        } else {
            self.waiting.insert(front.commit.id, front);
        }
    }

    /// The next row: the newest commit no un-handed-out commit is waiting on.
    fn next(&mut self) -> Option<Front> {
        if let Some(front) = self.ready.pop() {
            return Some(front);
        }
        // Nothing eligible but something held back. In a directed acyclic graph
        // this cannot happen — every waiting commit waits on a commit that will
        // itself be handed out. A repository where it does happen is one Git
        // could not walk either, and stalling would look like a hang, so what
        // was held is released and only the order suffers.
        if self.waiting.is_empty() {
            return None;
        }
        tracing::warn!(
            held = self.waiting.len(),
            "the history frontier deadlocked; releasing what was held back"
        );
        let released: Vec<_> = self.waiting.drain().map(|(_, front)| front).collect();
        self.ready.extend(released);
        self.ready.pop()
    }

    /// One fewer un-handed-out child for `id`; make it eligible if that was the
    /// last one.
    fn release(&mut self, id: ObjectId) {
        if let Some(count) = self.blocked.get_mut(&id) {
            *count = count.saturating_sub(1);
            if *count == 0
                && let Some(front) = self.waiting.remove(&id)
            {
                self.ready.push(front);
            }
        }
    }

    /// How many commits this walk has produced so far — the row count the
    /// virtualised list scrolls through.
    pub fn yielded(&self) -> usize {
        self.yielded
    }

    fn read(&mut self, gix: &gix::Repository, id: ObjectId) -> Result<()> {
        if !self.seen.insert(id) {
            return Ok(());
        }
        // A parent can be absent from a shallow clone, and a tag can point at a
        // blob. Neither is a reason to fail the whole walk: the row is simply
        // not there, exactly as `git log` would show it.
        match read_commit(gix, id) {
            Ok(commit) => {
                // Every parent now has one more child that has been read and
                // not handed out, so none of them may go out before this one.
                for parent in self.followed(&commit) {
                    *self.blocked.entry(parent).or_default() += 1;
                }
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

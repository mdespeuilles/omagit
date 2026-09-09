//! The history walk, parked between pages.
//!
//! Named `log` rather than `history` because that is what it is a view of, and
//! because `omagit_git::history` already owns the word.
//!
//! Two things live here that a `#[tauri::command]` cannot hold. The first is the
//! [`Walk`], which is stateful: SPEC §11 wants history paged and resumed rather
//! than re-walked, and §12 wants a hundred thousand commits without slowing
//! down, so the frontier and the set of commits already seen have to survive
//! between two requests. The second is the [`Graph`], for the same reason and a
//! sharper one — lane assignment is incremental *by construction*
//! (`omagit_git::graph`'s own words), and restarting it per page would put a
//! long-running branch in a different column every time the list scrolled.
//!
//! The other reason for the module is the one `edits.rs` gives: what is worth
//! testing is here, where a test can reach it without building an application.

use std::collections::HashMap;

use omagit_git::graph::{Graph, Row};
use omagit_git::history::{Commit, Filter, HistoryQuery, Tips, Walk};
use omagit_git::refs::Refs;
use omagit_git::{Cancel, ObjectId, RepoPath, Repository, Result};

/// How many commits a page holds.
///
/// Not a screenful: a screenful is what the virtualised list *draws*, and
/// asking for exactly that would mean a request per scroll wheel notch. Not the
/// whole history either — the measurement in ARCHITECTURE §2.20 says a full
/// 739-commit page with its graph is 0.55 MB, which is fine once and wasteful
/// on a repository fifty times larger.
pub const PAGE: usize = 500;

/// What the front end asked to see.
///
/// Every field defaults, so the plain view is `{}` and the front end does not
/// have to know the names of things it is not asking for.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Query {
    /// Every branch and tag rather than just `HEAD`.
    pub all: bool,
    /// Follow only the first parent of each merge — the "hide merged branches"
    /// view.
    pub first_parent: bool,
    /// SPEC §11's filters. Empty strings arrive from a text box that has been
    /// typed into and cleared again, and mean "no filter" rather than "match
    /// the empty string" — which every commit would.
    pub author: String,
    pub text: String,
    pub path: String,
    /// Seconds since the epoch, inclusive at both ends. Zero is "unset": no
    /// repository's history reaches 1970, and an `Option<i64>` on the wire
    /// would mean the front end sending `null` for a date box nobody filled in.
    pub since: i64,
    pub until: i64,
}

impl Query {
    fn to_history(&self) -> HistoryQuery {
        HistoryQuery {
            tips: if self.all { Tips::All } else { Tips::Head },
            first_parent: self.first_parent,
            filter: self.filter(),
        }
    }

    fn filter(&self) -> Filter {
        Filter {
            author: some(&self.author),
            text: some(&self.text),
            path: some(&self.path).map(|path| RepoPath::from_bytes(path.into_bytes())),
            since: (self.since > 0).then_some(self.since),
            until: (self.until > 0).then_some(self.until),
        }
    }

    /// Whether anything is being filtered out.
    pub fn is_filtered(&self) -> bool {
        !self.filter().is_empty()
    }
}

/// A box that has been typed into and cleared again is not a filter.
fn some(field: &str) -> Option<String> {
    let trimmed = field.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// A walk in progress, and everything that has to survive between its pages.
pub struct Session {
    walk: Walk,
    graph: Graph,
    /// Whether a filter is narrowing the rows, and so whether the topology
    /// drawn beside them would be the repository's.
    filtered: bool,
    /// Which references point at which commit, read once when the walk starts.
    ///
    /// Once, not per page: `Refs::load` computes ahead/behind for every branch,
    /// and doing that again for each five hundred rows would make scrolling
    /// cost more than walking. It goes stale if a branch moves under the
    /// window — which is what the refresh is for, and which restarts the walk
    /// anyway.
    labels: HashMap<ObjectId, Vec<Label>>,
}

/// One reference drawn beside a commit.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Label {
    pub name: String,
    /// `head`, `branch`, `remote` or `tag` — four kinds because DESIGN §5 draws
    /// them four ways, and because "which branch am I on" is the question the
    /// screen is most often asked.
    pub kind: &'static str,
}

/// One row of the history list: a commit, and the topology drawn beside it.
#[derive(Debug, serde::Serialize)]
pub struct HistoryRow {
    pub id: crate::dto::Oid,
    pub summary: String,
    pub author: String,
    /// Seconds since the epoch. Formatting is the front end's — this crate has
    /// no locale, and the same rule that keeps `omagit-git` free of one applies
    /// here.
    pub when: i64,
    pub merge: bool,
    pub labels: Vec<Label>,
    /// The column the node sits in.
    pub lane: usize,
    pub passing: Vec<usize>,
    pub incoming: Vec<usize>,
    pub outgoing: Vec<usize>,
    /// How many lanes are open across this row, so the gutter can be sized
    /// without a second pass.
    pub width: usize,
    /// Whether the five fields above mean anything. False under a filter, where
    /// the rows are a search result rather than a history.
    pub graph: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct Page {
    pub rows: Vec<HistoryRow>,
    /// True once every reachable commit has been handed out. What tells the
    /// list to stop asking, rather than a short page — a page can be short
    /// because a filter rejected most of it.
    pub done: bool,
}

impl Session {
    /// Begin a walk.
    pub fn start(repo: &Repository, query: Query, cancel: &Cancel) -> Result<Self> {
        let walk = Walk::new(repo, query.to_history(), cancel)?;
        Ok(Self {
            walk,
            graph: Graph::new(),
            filtered: query.is_filtered(),
            labels: labels(repo, cancel)?,
        })
    }

    /// The next page, with its graph rows.
    ///
    /// **No graph while a filter is on.** The lane algorithm places a commit
    /// relative to the commits around it, and under a filter those are not its
    /// parents and children — they are the next things that matched. A line
    /// drawn between two of them would claim a relationship whose only content
    /// is the search. A filtered history is a search result, so it is a flat
    /// list, exactly as `git log --author=…` prints one.
    pub fn next(&mut self, limit: usize, cancel: &Cancel) -> Result<Page> {
        let commits = self.walk.next_page(limit, cancel)?;
        let rows = commits
            .iter()
            .map(|commit| {
                let row = (!self.filtered).then(|| self.graph.push(commit));
                self.row(commit, row.as_ref())
            })
            .collect();
        Ok(Page {
            rows,
            done: self.walk.is_done(),
        })
    }

    fn row(&self, commit: &Commit, row: Option<&Row>) -> HistoryRow {
        HistoryRow {
            id: commit.id.into(),
            summary: commit.summary.clone(),
            author: commit.author.name.clone(),
            when: commit.author.time.seconds,
            merge: commit.is_merge(),
            labels: self.labels.get(&commit.id).cloned().unwrap_or_default(),
            lane: row.map_or(0, |row| row.lane),
            passing: row.map(|row| row.passing.clone()).unwrap_or_default(),
            incoming: row.map(|row| row.incoming.clone()).unwrap_or_default(),
            outgoing: row.map(|row| row.outgoing.clone()).unwrap_or_default(),
            width: row.map_or(0, |row| row.width),
            graph: row.is_some(),
        }
    }
}

/// Every reference, grouped by the commit it points at.
///
/// Ordered so the row reads the way the question is asked: the branch `HEAD` is
/// on first, then the other local branches, then remotes, then tags. A commit
/// with fifteen tags on it would otherwise bury the one label that says where
/// you are.
fn labels(repo: &Repository, cancel: &Cancel) -> Result<HashMap<ObjectId, Vec<Label>>> {
    let refs = Refs::load(repo, cancel)?;
    let mut by_commit: HashMap<ObjectId, Vec<Label>> = HashMap::new();

    let mut push = |commit: ObjectId, label: Label| {
        by_commit.entry(commit).or_default().push(label);
    };

    for branch in refs.branches.iter().filter(|branch| branch.is_head) {
        push(
            branch.commit,
            Label {
                name: branch.name.clone(),
                kind: "head",
            },
        );
    }
    for branch in refs.branches.iter().filter(|branch| !branch.is_head) {
        push(
            branch.commit,
            Label {
                name: branch.name.clone(),
                kind: "branch",
            },
        );
    }
    for branch in &refs.remote_branches {
        push(
            branch.commit,
            Label {
                name: format!("{}/{}", branch.remote, branch.name),
                kind: "remote",
            },
        );
    }
    for tag in &refs.tags {
        push(
            tag.commit,
            Label {
                name: tag.name.clone(),
                kind: "tag",
            },
        );
    }
    Ok(by_commit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_shape_is_the_one_ipc_ts_sends() {
        let query: Query = serde_json::from_str(
            r#"{"all":true,"firstParent":true,"author":"marek","text":"","path":"src","since":1767225600,"until":0}"#,
        )
        .expect("the front end's shape");
        assert!(query.all && query.first_parent);
        assert!(query.is_filtered());

        // Every field defaults, so the front end can ask for the plain view
        // with an empty object rather than having to know the names.
        let empty: Query = serde_json::from_str("{}").expect("an empty query");
        assert_eq!(empty, Query::default());
        assert!(!empty.is_filtered());
    }

    #[test]
    fn a_box_typed_into_and_cleared_again_is_not_a_filter() {
        // It arrives as an empty string, and an empty substring matches every
        // commit there is — so a cleared box would go on filtering nothing out
        // while the screen said it was filtering.
        let cleared: Query =
            serde_json::from_str(r#"{"author":"","text":"   "}"#).expect("a cleared filter box");
        assert!(!cleared.is_filtered());
    }

    #[test]
    fn a_zero_date_is_an_unset_date() {
        let unset: Query = serde_json::from_str(r#"{"since":0,"until":0}"#).expect("no dates");
        assert!(!unset.is_filtered());
    }
}

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
use omagit_git::history::{Commit, HistoryQuery, Tips, Walk};
use omagit_git::refs::Refs;
use omagit_git::{Cancel, ObjectId, Repository, Result};

/// How many commits a page holds.
///
/// Not a screenful: a screenful is what the virtualised list *draws*, and
/// asking for exactly that would mean a request per scroll wheel notch. Not the
/// whole history either — the measurement in ARCHITECTURE §2.20 says a full
/// 739-commit page with its graph is 0.55 MB, which is fine once and wasteful
/// on a repository fifty times larger.
pub const PAGE: usize = 500;

/// What the front end asked to see.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    /// Every branch and tag rather than just `HEAD`.
    #[serde(default)]
    pub all: bool,
    /// Follow only the first parent of each merge — the "hide merged branches"
    /// view.
    #[serde(default)]
    pub first_parent: bool,
}

impl Query {
    fn to_history(&self) -> HistoryQuery {
        HistoryQuery {
            tips: if self.all { Tips::All } else { Tips::Head },
            first_parent: self.first_parent,
        }
    }
}

/// A walk in progress, and everything that has to survive between its pages.
pub struct Session {
    walk: Walk,
    graph: Graph,
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
            labels: labels(repo, cancel)?,
        })
    }

    /// The next page, with its graph rows.
    pub fn next(&mut self, limit: usize, cancel: &Cancel) -> Result<Page> {
        let commits = self.walk.next_page(limit, cancel)?;
        let rows = commits
            .iter()
            .map(|commit| {
                let row = self.graph.push(commit);
                self.row(commit, &row)
            })
            .collect();
        Ok(Page {
            rows,
            done: self.walk.is_done(),
        })
    }

    fn row(&self, commit: &Commit, row: &Row) -> HistoryRow {
        HistoryRow {
            id: commit.id.into(),
            summary: commit.summary.clone(),
            author: commit.author.name.clone(),
            when: commit.author.time.seconds,
            merge: commit.is_merge(),
            labels: self.labels.get(&commit.id).cloned().unwrap_or_default(),
            lane: row.lane,
            passing: row.passing.clone(),
            incoming: row.incoming.clone(),
            outgoing: row.outgoing.clone(),
            width: row.width,
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
        let query: Query = serde_json::from_str(r#"{"all":true,"firstParent":true}"#)
            .expect("the front end's shape");
        assert!(query.all && query.first_parent);

        // Both fields default, so the front end can ask for the plain view with
        // an empty object rather than having to know the names.
        let empty: Query = serde_json::from_str("{}").expect("an empty query");
        assert_eq!(empty, Query::default());
    }
}

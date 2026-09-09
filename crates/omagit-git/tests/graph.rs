//! Lane assignment, on the histories that break it.
//!
//! SPEC §15 asks for this one to be tested on pathological histories, so these
//! are synthetic rather than built with `git`: a fixture that has to be
//! *committed* into existence cannot cheaply have fifty roots or an eight-way
//! merge, and the algorithm takes commits, not a repository. The shapes are
//! what matter, and they are written out here so a failure names one.

mod support;

use std::collections::HashMap;

use omagit_git::graph::{Graph, Row};
use omagit_git::{Commit, HistoryQuery, ObjectId, Walk};
use support::{never, scripted};

/// A commit id from a small number, so a test reads as a shape rather than as
/// forty hex digits.
fn id(n: u8) -> ObjectId {
    let mut bytes = [0u8; 20];
    bytes[19] = n;
    ObjectId::Sha1(bytes)
}

fn commit(n: u8, parents: &[u8]) -> Commit {
    Commit {
        id: id(n),
        parents: parents.iter().copied().map(id).collect(),
        author: signature(),
        committer: signature(),
        summary: format!("commit {n}"),
        body: String::new(),
    }
}

fn signature() -> omagit_git::Signature {
    omagit_git::Signature {
        name: "Test".into(),
        email: "test@omagit.test".into(),
        time: omagit_git::Time {
            seconds: 0,
            offset_seconds: 0,
        },
    }
}

/// Place a history given newest-first, as the walk produces it.
fn lay_out(history: &[(u8, &[u8])]) -> Vec<Row> {
    let mut graph = Graph::new();
    history
        .iter()
        .map(|(id, parents)| graph.push(&commit(*id, parents)))
        .collect()
}

#[test]
fn a_straight_history_stays_in_one_lane() {
    let rows = lay_out(&[(3, &[2]), (2, &[1]), (1, &[])]);

    assert!(rows.iter().all(|row| row.lane == 0), "one line, one column");
    assert!(
        rows.iter().all(|row| row.width == 1),
        "and a gutter one lane wide"
    );
    assert!(rows[0].is_tip(), "the newest commit has nothing above it");
    assert!(rows[2].is_root(), "the oldest has no parent");
    assert!(!rows[0].is_root());
}

#[test]
fn a_branch_and_its_merge_open_and_close_one_lane() {
    //   4  merge
    //   |\
    //   3 2   two sides
    //   |/
    //   1  base
    let rows = lay_out(&[(4, &[3, 2]), (3, &[1]), (2, &[1]), (1, &[])]);

    assert_eq!(rows[0].lane, 0, "the merge keeps the lane it was found in");
    assert_eq!(
        rows[0].outgoing,
        vec![0, 1],
        "its first parent continues in its own lane, the second opens one"
    );
    assert_eq!(rows[0].width, 2);

    assert_eq!(rows[1].lane, 0, "the first parent is where it was sent");
    assert_eq!(rows[2].lane, 1, "and the second in the lane opened for it");

    // Where the two sides rejoin is worth being precise about, because it is
    // not where one would first guess. Commit 2 is drawn *after* commit 3, by
    // which time lane 0 is already waiting for their shared parent — so 2's
    // line bends into lane 0 at 2's own row, and lane 1 closes there. The
    // parent itself has a single lane arriving.
    //
    // Converging at the child rather than at the shared parent is what keeps
    // the gutter narrow: the alternative runs both lines down in parallel until
    // they meet, and on a history where a hundred commits share a base that is
    // a hundred rows of two columns saying nothing.
    assert_eq!(rows[2].lane, 1);
    assert_eq!(
        rows[2].outgoing,
        vec![0],
        "the second side bends into the lane already holding their parent"
    );
    assert_eq!(rows[3].lane, 0);
    assert_eq!(rows[3].incoming, vec![0], "one lane arrives at the parent");
    assert_eq!(rows[3].width, 1, "and the gutter closes again");
}

#[test]
fn an_octopus_merge_opens_a_lane_per_parent() {
    // Rare and legal, and the case a two-parent assumption gets wrong.
    let rows = lay_out(&[
        (9, &[4, 3, 2, 1]),
        (4, &[0]),
        (3, &[0]),
        (2, &[0]),
        (1, &[0]),
        (0, &[]),
    ]);

    assert_eq!(rows[0].outgoing, vec![0, 1, 2, 3], "four lanes leave it");
    assert_eq!(rows[0].width, 4);

    // Each side then bends back into whichever lane first claimed the shared
    // root, so the gutter closes as it goes rather than staying four wide.
    assert_eq!(rows[1].outgoing, vec![0], "the first side claims the lane");
    for row in &rows[2..5] {
        assert_eq!(
            row.outgoing,
            vec![0],
            "and every other side bends into it at its own row"
        );
    }
    assert_eq!(rows[5].incoming, vec![0], "the root has one lane arriving");
    assert!(rows[5].is_root());
}

#[test]
fn several_roots_each_end_their_own_lane() {
    // SPEC §13 asks for a history with fifty roots. Three is the same shape.
    let rows = lay_out(&[(5, &[3, 4]), (4, &[]), (3, &[1, 2]), (2, &[]), (1, &[])]);

    let roots: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.is_root())
        .map(|(index, _)| index)
        .collect();
    assert_eq!(roots, vec![1, 3, 4], "three commits with no parent");
    assert!(
        rows.iter().all(|row| row.width <= 2),
        "and the gutter never grows past what is actually open"
    );
}

#[test]
fn a_closed_lane_is_reused_rather_than_left_as_a_gap() {
    //   5      a branch that ended
    //   |
    //   4  3   3's lane closes at 2
    //   |  |
    //   2--+
    //   |
    //   1
    let rows = lay_out(&[(5, &[4]), (4, &[2]), (3, &[2]), (2, &[1]), (1, &[])]);
    assert_eq!(rows[2].lane, 1, "the second tip takes the free column");

    // After 2 absorbs both, a new tip should land back in lane 1 rather than
    // opening a third: a gutter that only ever grows is unreadable.
    let mut graph = Graph::new();
    for (id, parents) in [(5u8, &[4u8][..]), (4, &[2]), (3, &[2]), (2, &[1])] {
        graph.push(&commit(id, parents));
    }
    let later = graph.push(&commit(9, &[]));
    assert!(
        later.lane <= 1,
        "a free column has to be reused, not skipped past — landed in {}",
        later.lane
    );
}

#[test]
fn a_parent_two_lanes_are_waiting_for_is_drawn_once() {
    // Both sides of a merge lead to the same commit by different routes. It
    // must occupy one lane, with both arriving at it — drawing it twice is how
    // a graph stops being readable.
    let rows = lay_out(&[(4, &[3, 2]), (3, &[1]), (2, &[1]), (1, &[])]);
    let ones: Vec<&Row> = rows.iter().filter(|row| row.id == id(1)).collect();
    assert_eq!(ones.len(), 1, "one row per commit");
    assert_eq!(ones[0].lane, 0, "in one lane");

    // The second side never opens a lane of its own for the parent: it points
    // at the one that has it. A lane per waiting child would draw the same
    // commit's line twice.
    assert_eq!(rows[2].outgoing, vec![0]);
    assert_eq!(
        rows[2].passing,
        vec![0],
        "and that lane is what row 2 sees passing beside it"
    );
}

#[test]
fn a_commit_whose_child_was_not_drawn_starts_its_own_lane() {
    // What a second branch tip is, and what a filtered history is full of.
    let rows = lay_out(&[(9, &[8]), (7, &[6]), (8, &[]), (6, &[])]);
    assert_eq!(rows[0].lane, 0);
    assert_eq!(rows[1].lane, 1, "an unrelated tip opens its own column");
    assert!(rows[1].is_tip());
}

#[test]
fn the_state_between_pages_is_all_a_later_page_needs() {
    // SPEC §11: computed incrementally, on the background thread. Laying a
    // history out in two pages has to give the same answer as one.
    let history: Vec<(u8, &[u8])> = vec![
        (6, &[5, 4]),
        (5, &[3]),
        (4, &[3]),
        (3, &[2]),
        (2, &[1]),
        (1, &[]),
    ];
    let whole = lay_out(&history);

    let mut graph = Graph::new();
    let mut split: Vec<Row> = history[..3]
        .iter()
        .map(|(id, parents)| graph.push(&commit(*id, parents)))
        .collect();
    // Nothing is carried over but the graph itself.
    split.extend(
        history[3..]
            .iter()
            .map(|(id, parents)| graph.push(&commit(*id, parents))),
    );

    assert_eq!(whole, split, "a page boundary must not change the shape");
}

#[test]
fn the_gutter_width_is_what_the_row_actually_draws_in() {
    // The renderer sizes the gutter from this without a second pass, so it has
    // to cover every column the row touches — passing lanes and new parents
    // alike.
    let rows = lay_out(&[(4, &[3, 2]), (3, &[1]), (2, &[1]), (1, &[])]);
    for row in &rows {
        let widest = row
            .passing
            .iter()
            .chain(row.outgoing.iter())
            .chain(std::iter::once(&row.lane))
            .copied()
            .max()
            .unwrap_or(0);
        assert!(
            row.width > widest,
            "row {:?} draws in lane {widest} and reports a width of {}",
            row.id,
            row.width
        );
    }
}

#[test]
fn a_real_history_lays_out_consistently() {
    // The synthetic shapes above say the algorithm handles what it is given;
    // this says it is given what `git` actually produces. The scripted fixture
    // has merges, renames and several branches.
    let repo = scripted();
    let opened = repo.open();
    let mut walk = Walk::new(&opened, HistoryQuery::all(), &never()).expect("a walk");
    let commits = walk.next_page(1000, &never()).expect("a page");
    assert!(commits.len() > 3, "the fixture has a history to lay out");

    let mut graph = Graph::new();
    let rows: Vec<Row> = commits.iter().map(|commit| graph.push(commit)).collect();

    let at: HashMap<ObjectId, usize> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.id, index))
        .collect();
    assert_eq!(at.len(), rows.len(), "every commit gets exactly one row");

    for (index, (commit, row)) in commits.iter().zip(&rows).enumerate() {
        assert_eq!(commit.id, row.id, "rows stay in step with the walk");
        assert_eq!(
            row.outgoing.len(),
            commit.parents.len(),
            "a line leaves for each parent of {}",
            commit.id
        );
        // A parent inside the page is always further down: the walk is
        // newest-first, so a line never points upwards.
        for parent in &commit.parents {
            if let Some(parent_row) = at.get(parent) {
                assert!(
                    *parent_row > index,
                    "{} is drawn above its parent {}",
                    commit.id,
                    parent
                );
            }
        }
        assert!(
            row.width > row.lane,
            "a row has to be wide enough for its own node"
        );
        assert!(
            !row.passing.contains(&row.lane),
            "a lane cannot both hold the node and pass it by"
        );
    }
}

#[test]
fn a_hundred_thousand_commits_lay_out_without_slowing_down() {
    // SPEC §12 wants the first thousand rows of a 100 000-commit history in
    // 250ms, and the graph is one part of that budget. This is not a benchmark
    // — `benches/` is where those go — but a linear history of 100 000 laid out
    // in one pass catches the accidental quadratic that a lane search over a
    // growing vector would be.
    let history: Vec<Commit> = (0..100_000u32)
        .map(|n| Commit {
            id: wide_id(n),
            parents: if n + 1 < 100_000 {
                vec![wide_id(n + 1)]
            } else {
                Vec::new()
            },
            author: signature(),
            committer: signature(),
            summary: String::new(),
            body: String::new(),
        })
        .collect();

    let started = std::time::Instant::now();
    let mut graph = Graph::new();
    for commit in &history {
        graph.push(commit);
    }
    let elapsed = started.elapsed();

    assert_eq!(graph.rows(), 100_000);
    assert_eq!(graph.open_lanes(), 0, "a finished linear history closes");
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "laying out 100 000 rows took {elapsed:?} — that is the shape of an \
         accidental quadratic, not of a linear pass"
    );
}

fn wide_id(n: u32) -> ObjectId {
    let mut bytes = [0u8; 20];
    bytes[16..20].copy_from_slice(&n.to_be_bytes());
    ObjectId::Sha1(bytes)
}

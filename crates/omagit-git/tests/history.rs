//! The commit walk: order, pagination, and the histories that break naive
//! traversals (SPEC §13).

mod support;

use omagit_git::{HistoryQuery, ObjectId, Walk};
use support::{TestRepo, never, scripted};

/// What `git log` says, as the reference to compare against.
fn git_log(fixture: &TestRepo, args: &[&str]) -> Vec<ObjectId> {
    let mut all = vec!["log", "--format=%H"];
    all.extend_from_slice(args);
    fixture
        .git(&all)
        .lines()
        .map(|line| line.parse().expect("a hash"))
        .collect()
}

fn walk_all(fixture: &TestRepo, query: HistoryQuery, page: usize) -> Vec<ObjectId> {
    let repo = fixture.open();
    let mut walk = Walk::new(&repo, query, &never()).expect("a walk");
    let mut ids = Vec::new();
    loop {
        let commits = walk.next_page(page, &never()).expect("a page");
        if commits.is_empty() {
            break;
        }
        ids.extend(commits.into_iter().map(|commit| commit.id));
    }
    assert!(
        walk.is_done(),
        "the walk ran out of commits, not out of pages"
    );
    assert_eq!(walk.yielded(), ids.len());
    ids
}

#[test]
fn walks_head_in_the_same_order_as_git() {
    let fixture = scripted();
    assert_eq!(
        walk_all(&fixture, HistoryQuery::head(), 2),
        git_log(&fixture, &["--date-order"]),
        "including the merge and both of its sides"
    );
}

#[test]
fn pages_do_not_change_the_result() {
    let fixture = TestRepo::new();
    for n in 1..=25 {
        fixture.commit_file(
            "file.txt",
            &format!("version {n}\n"),
            &format!("commit {n}"),
        );
    }

    let whole = walk_all(&fixture, HistoryQuery::head(), 1000);
    assert_eq!(whole.len(), 25);
    for page in [1, 2, 7, 25, 26] {
        assert_eq!(
            walk_all(&fixture, HistoryQuery::head(), page),
            whole,
            "a page size of {page} changed the history"
        );
    }
}

#[test]
fn a_short_page_means_the_end() {
    let fixture = TestRepo::new();
    for n in 1..=5 {
        fixture.commit_file("file.txt", &format!("{n}\n"), &format!("commit {n}"));
    }
    let repo = fixture.open();
    let mut walk = Walk::new(&repo, HistoryQuery::head(), &never()).expect("a walk");

    assert_eq!(walk.next_page(3, &never()).expect("a page").len(), 3);
    assert!(!walk.is_done(), "two commits are left");
    assert_eq!(walk.next_page(3, &never()).expect("a page").len(), 2);
    assert!(walk.is_done());
    assert!(walk.next_page(3, &never()).expect("a page").is_empty());
}

#[test]
fn first_parent_skips_the_merged_side() {
    let fixture = scripted();
    assert_eq!(
        walk_all(&fixture, HistoryQuery::head().first_parent(), 2),
        git_log(&fixture, &["--first-parent"]),
    );
}

#[test]
fn walking_all_refs_reaches_a_branch_head_is_not_on() {
    let fixture = TestRepo::new();
    fixture.commit_file("a.txt", "a\n", "on main");
    fixture.branch("side");
    let only_on_side = fixture.commit_file("b.txt", "b\n", "only on side");
    fixture.checkout("main");

    assert!(!walk_all(&fixture, HistoryQuery::head(), 10).contains(&only_on_side));
    assert!(walk_all(&fixture, HistoryQuery::all(), 10).contains(&only_on_side));
}

#[test]
fn walks_from_an_explicit_commit() {
    let fixture = scripted();
    let first = *git_log(&fixture, &[]).last().expect("a root commit");
    let from_root = walk_all(&fixture, HistoryQuery::from([first]), 10);
    assert_eq!(from_root, vec![first], "a root commit has no ancestors");
}

#[test]
fn a_repository_with_no_commits_walks_to_nothing() {
    let fixture = TestRepo::new();
    assert!(walk_all(&fixture, HistoryQuery::head(), 10).is_empty());
}

#[test]
fn survives_a_history_with_fifty_roots() {
    // SPEC §13 asks for it by name: fifty unrelated histories grafted together
    // is where a walk that assumes one root loses commits.
    let fixture = TestRepo::new();
    fixture.commit_file("base.txt", "base\n", "root 0");
    let mut roots = vec![fixture.head()];
    for n in 1..50 {
        fixture.git(&["checkout", "--orphan", &format!("root{n}")]);
        fixture.git(&["rm", "-rf", "-q", "--ignore-unmatch", "."]);
        roots.push(fixture.commit_file(&format!("root{n}.txt"), "x\n", &format!("root {n}")));
    }
    fixture.checkout("main");

    let all = walk_all(&fixture, HistoryQuery::all(), 7);
    assert_eq!(all.len(), 50, "every root is reachable");
    for root in roots {
        assert!(all.contains(&root), "{root} is missing");
    }
}

#[test]
fn reads_a_commits_message_author_and_parents() {
    let fixture = TestRepo::new();
    fixture.commit_file("a.txt", "a\n", "first");
    fixture.write("a.txt", "b\n");
    fixture.add("a.txt");
    fixture.git(&[
        "commit",
        "-m",
        "subject line\n\nA body that explains why.\n",
    ]);
    let repo = fixture.open();

    let commit = omagit_git::history::commit(&repo, fixture.head()).expect("a commit");
    assert_eq!(commit.summary, "subject line");
    assert_eq!(commit.body, "A body that explains why.");
    assert_eq!(commit.author.name, "Test Author");
    assert_eq!(commit.author.email, "author@omagit.test");
    assert_eq!(commit.committer.name, "Test Committer");
    assert!(commit.author.time.seconds > 0);
    assert_eq!(commit.parents.len(), 1);
    assert!(!commit.is_merge() && !commit.is_root());
}

#[test]
fn a_merge_has_both_parents_and_a_root_has_none() {
    let fixture = scripted();
    let repo = fixture.open();
    let merge = omagit_git::history::commit(&repo, fixture.head()).expect("the merge");
    assert!(merge.is_merge());
    assert_eq!(merge.parents.len(), 2);

    let root_id = *git_log(&fixture, &[]).last().expect("a root");
    let root = omagit_git::history::commit(&repo, root_id).expect("the root");
    assert!(root.is_root());
}

#[test]
fn a_walk_can_be_cancelled_between_pages() {
    let fixture = scripted();
    let repo = fixture.open();
    let mut walk = Walk::new(&repo, HistoryQuery::all(), &never()).expect("a walk");
    let cancel = omagit_git::Cancel::new();
    cancel.cancel();

    let error = walk
        .next_page(10, &cancel)
        .expect_err("a cancelled page returns");
    assert!(error.is_cancelled(), "got {error:?}");
}

/// Every commit's parents, by id — what the topological guarantee is about.
fn parents_of(fixture: &TestRepo) -> std::collections::HashMap<ObjectId, Vec<ObjectId>> {
    fixture
        .git(&["log", "--all", "--format=%H %P"])
        .lines()
        .map(|line| {
            let mut ids = line
                .split_whitespace()
                .map(|id| id.parse().expect("a hash"));
            (ids.next().expect("a commit"), ids.collect())
        })
        .collect()
}

/// A history where every commit shares one timestamp, with two branches that
/// interleave and meet — the shape that made the order wrong.
///
/// ```text
///   main:    A ── B ── C ── E ── F ── M
///                       \           /
///   feature:             D1 ─ D2 ──
/// ```
fn same_second() -> TestRepo {
    let fixture = TestRepo::new();
    fixture.freeze_clock();
    for n in 1..=3 {
        fixture.commit_file("base.txt", &format!("base {n}\n"), &format!("base {n}"));
    }
    fixture.branch("feature");
    for n in 1..=2 {
        fixture.commit_file(
            "feature.txt",
            &format!("feature {n}\n"),
            &format!("feature {n}"),
        );
    }
    fixture.checkout("main");
    for n in 1..=2 {
        fixture.commit_file("main.txt", &format!("main {n}\n"), &format!("main {n}"));
    }
    fixture.git(&["merge", "--no-ff", "-m", "merge feature", "feature"]);
    fixture
}

#[test]
fn no_parent_is_walked_before_its_children_even_when_the_clock_does_not_move() {
    // `git log --date-order`'s actual guarantee, which "newest timestamp first"
    // is not: *show no parents before all of their children are shown*. Two
    // commits made in the same second tie, and a heap ordered on time alone
    // will happily hand out a grandparent before its grandchild — which draws
    // a graph line going upwards, into a node already passed.
    let fixture = same_second();
    let times: std::collections::BTreeSet<String> = fixture
        .git(&["log", "--all", "--format=%ct"])
        .lines()
        .map(ToOwned::to_owned)
        .collect();
    assert_eq!(
        times.len(),
        1,
        "the fixture has to be degenerate to prove anything"
    );

    // The property, not Git's exact sequence. When several commits become
    // eligible at once — which is what a tie *is* — any order among them is a
    // valid `--date-order`, and Git's own choice comes from the order it
    // happened to load them in. `walks_head_in_the_same_order_as_git` compares
    // sequences, on a fixture whose timestamps are distinct and where there is
    // therefore only one answer.
    let order = walk_all(&fixture, HistoryQuery::head(), 3);
    let parents = parents_of(&fixture);
    let at: std::collections::HashMap<ObjectId, usize> = order
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect();

    for (child, parents) in &parents {
        let Some(child_at) = at.get(child) else {
            continue;
        };
        for parent in parents {
            if let Some(parent_at) = at.get(parent) {
                assert!(
                    parent_at > child_at,
                    "{parent} is its child {child}'s ancestor but was walked first \
                     ({parent_at} before {child_at})"
                );
            }
        }
    }
}

#[test]
fn a_tie_is_broken_the_same_way_on_every_page_size() {
    // The order has to be a property of the history, not of how the list
    // happened to scroll.
    let fixture = same_second();
    let whole = walk_all(&fixture, HistoryQuery::head(), 1000);
    for page in [1, 2, 3, 5] {
        assert_eq!(
            walk_all(&fixture, HistoryQuery::head(), page),
            whole,
            "a page size of {page} produced a different history"
        );
    }
}

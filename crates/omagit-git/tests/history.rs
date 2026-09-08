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

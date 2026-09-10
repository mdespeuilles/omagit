//! The history walk as the window uses it: in pages, resumed.
//!
//! `omagit-git` already proves that the walk matches `git log` and that the lane
//! algorithm is right. What is proved here is the thing that only exists because
//! the front end is a webview and asks twice: a walk parked between two requests
//! has to continue rather than begin again, and the graph it carries has to
//! continue with it.

mod support;

use omagit_lib::log::{Page, Query, Session};
use support::{TestRepo, never};

/// A chain of `count` commits on one branch.
fn chain(count: usize) -> TestRepo {
    let repo = TestRepo::new();
    for n in 0..count {
        repo.commit_file(
            "file.txt",
            &format!("version {n}\n"),
            &format!("commit {n}"),
        );
    }
    repo
}

fn summaries(page: &Page) -> Vec<String> {
    page.rows.iter().map(|row| row.summary.clone()).collect()
}

#[test]
fn a_second_page_continues_the_first_rather_than_repeating_it() {
    let repo = chain(12);
    let mut session =
        Session::start(&repo.open(), Query::default(), &never()).expect("a walk starts");

    let first = session.next(5, &never()).expect("a first page");
    let second = session.next(5, &never()).expect("a second page");

    assert_eq!(first.rows.len(), 5);
    assert_eq!(second.rows.len(), 5);
    assert!(
        !first.done && !second.done,
        "twelve commits, ten handed out"
    );

    let seen: Vec<_> = first
        .rows
        .iter()
        .chain(&second.rows)
        .map(|row| row.id.full.clone())
        .collect();
    let unique: std::collections::BTreeSet<_> = seen.iter().collect();
    assert_eq!(
        unique.len(),
        seen.len(),
        "a page that restarted the walk would hand back rows the list already has"
    );
}

#[test]
fn the_walk_says_when_it_has_reached_the_roots() {
    let repo = chain(3);
    let mut session =
        Session::start(&repo.open(), Query::default(), &never()).expect("a walk starts");

    let page = session.next(10, &never()).expect("one page holds it all");
    assert_eq!(page.rows.len(), 3);
    assert!(
        page.done,
        "the list stops asking on `done`, not on a short page: a page can be \
         short because a filter rejected most of it"
    );
}

#[test]
fn a_branch_keeps_its_lane_across_a_page_boundary() {
    // The reason the graph is parked with the walk. Lane assignment is
    // incremental; a graph rebuilt per page would put this branch in lane 0
    // again and the line would jump sideways mid-scroll.
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "root");
    repo.git(&["checkout", "-b", "side"]);
    for n in 0..6 {
        repo.commit_file("side.txt", &format!("side {n}\n"), &format!("side {n}"));
    }
    repo.git(&["checkout", "main"]);
    for n in 0..6 {
        repo.commit_file("main.txt", &format!("main {n}\n"), &format!("main {n}"));
    }

    let query = Query {
        all: true,
        ..Query::default()
    };
    let mut paged = Session::start(&repo.open(), query.clone(), &never()).expect("a walk");
    let mut whole = Session::start(&repo.open(), query, &never()).expect("a walk");

    let mut in_pages = Vec::new();
    loop {
        let page = paged.next(3, &never()).expect("a page");
        let done = page.done;
        in_pages.extend(page.rows);
        if done {
            break;
        }
    }
    let at_once = whole.next(100, &never()).expect("one page");

    assert_eq!(in_pages.len(), at_once.rows.len());
    for (paged, whole) in in_pages.iter().zip(&at_once.rows) {
        assert_eq!(paged.id.full, whole.id.full, "the same commits, in order");
        assert_eq!(
            paged.lane, whole.lane,
            "commit {} lands in lane {} when paged and {} when walked at once",
            whole.id.short, paged.lane, whole.lane
        );
    }
}

#[test]
fn head_is_labelled_as_head_and_comes_before_the_other_branches() {
    // "Which branch am I on" is what the screen is most often asked. A commit
    // with a dozen references on it would otherwise bury the one that answers.
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "root");
    repo.git(&["branch", "aaa-earlier-in-the-alphabet"]);
    repo.git(&["tag", "v1"]);

    let mut session = Session::start(&repo.open(), Query::default(), &never()).expect("a walk");
    let page = session.next(10, &never()).expect("a page");
    let labels = &page.rows[0].labels;

    assert_eq!(labels[0].kind, "head");
    assert_eq!(labels[0].name, "main");
    let kinds: Vec<_> = labels.iter().map(|label| label.kind).collect();
    assert_eq!(kinds, vec!["head", "branch", "tag"]);
}

#[test]
fn first_parent_hides_what_a_merge_brought_in() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "root");
    repo.git(&["checkout", "-b", "side"]);
    repo.commit_file("side.txt", "side\n", "only on the side");
    repo.git(&["checkout", "main"]);
    repo.git(&["merge", "--no-ff", "-m", "merge side", "side"]);

    let everything = Session::start(&repo.open(), Query::default(), &never())
        .expect("a walk")
        .next(50, &never())
        .expect("a page");
    let trunk = Session::start(
        &repo.open(),
        Query {
            first_parent: true,
            ..Query::default()
        },
        &never(),
    )
    .expect("a walk")
    .next(50, &never())
    .expect("a page");

    assert!(summaries(&everything).contains(&"only on the side".to_owned()));
    assert!(!summaries(&trunk).contains(&"only on the side".to_owned()));
    assert!(summaries(&trunk).contains(&"merge side".to_owned()));
}

#[test]
fn all_reaches_a_branch_head_is_not_on() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "root");
    repo.git(&["checkout", "-b", "side"]);
    repo.commit_file("side.txt", "side\n", "only on the side");
    repo.git(&["checkout", "main"]);

    let from_head = Session::start(&repo.open(), Query::default(), &never())
        .expect("a walk")
        .next(50, &never())
        .expect("a page");
    let everything = Session::start(
        &repo.open(),
        Query {
            all: true,
            ..Query::default()
        },
        &never(),
    )
    .expect("a walk")
    .next(50, &never())
    .expect("a page");

    assert!(!summaries(&from_head).contains(&"only on the side".to_owned()));
    assert!(summaries(&everything).contains(&"only on the side".to_owned()));
}

#[test]
fn an_empty_repository_is_an_empty_history_not_a_failure() {
    // A repository that has been initialised and never committed. `HEAD` names
    // a branch that does not exist yet, and the screen shows nothing rather
    // than an error about a missing reference.
    let repo = TestRepo::new();
    let mut session =
        Session::start(&repo.open(), Query::default(), &never()).expect("an unborn walk starts");
    let page = session.next(10, &never()).expect("a page");

    assert!(page.rows.is_empty());
    assert!(page.done);
}

#[test]
fn a_filtered_history_carries_no_graph() {
    // The lane algorithm places a commit relative to the ones around it, and
    // under a filter those are the next things that matched, not its parents
    // and children. A line between two of them would claim a relationship whose
    // only content is the search.
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "core: first");
    repo.commit_file("b.txt", "two\n", "docs: second");
    repo.commit_file("c.txt", "three\n", "core: third");

    let plain = Session::start(&repo.open(), Query::default(), &never())
        .expect("a walk")
        .next(50, &never())
        .expect("a page");
    assert!(plain.rows.iter().all(|row| row.graph));

    let searched = Session::start(
        &repo.open(),
        Query {
            text: "core".to_owned(),
            ..Query::default()
        },
        &never(),
    )
    .expect("a walk")
    .next(50, &never())
    .expect("a page");

    assert_eq!(searched.rows.len(), 2);
    assert!(
        searched.rows.iter().all(|row| !row.graph),
        "a search result is a list, not a history"
    );
}

#[test]
fn a_filter_can_come_back_with_less_than_a_page_and_still_be_done() {
    let repo = TestRepo::new();
    for n in 0..10 {
        repo.commit_file("file.txt", &format!("{n}\n"), &format!("commit {n}"));
    }

    let page = Session::start(
        &repo.open(),
        Query {
            text: "commit 7".to_owned(),
            ..Query::default()
        },
        &never(),
    )
    .expect("a walk")
    .next(500, &never())
    .expect("a page");

    assert_eq!(page.rows.len(), 1);
    assert!(
        page.done,
        "the list stops on `done`; a short page means the filter rejected the rest"
    );
}

#[test]
fn a_filter_that_matches_nothing_is_an_empty_answer_not_an_error() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "only commit");

    let page = Session::start(
        &repo.open(),
        Query {
            author: "nobody at all".to_owned(),
            ..Query::default()
        },
        &never(),
    )
    .expect("a walk")
    .next(500, &never())
    .expect("a page");

    assert!(page.rows.is_empty());
    assert!(page.done);
}

#[test]
fn a_query_scoped_to_a_branch_walks_that_branch_and_not_head() {
    // SPEC §11's filter by branch, and what clicking a branch row asks for: the
    // commits reachable from *that* tip, whether or not `HEAD` can see them.
    let repo = TestRepo::new();
    repo.commit_file("README.md", "one\n", "seed");
    repo.git(&["checkout", "-b", "feature"]);
    repo.commit_file("feature.txt", "theirs\n", "only on the branch");
    repo.git(&["checkout", "main"]);
    repo.commit_file("main.txt", "ours\n", "only on main");

    let subjects = |query: Query| {
        let mut session = Session::start(&repo.open(), query, &never()).expect("a walk");
        session
            .next(50, &never())
            .expect("a page")
            .rows
            .into_iter()
            .map(|row| row.summary)
            .collect::<Vec<_>>()
    };

    assert_eq!(
        subjects(Query::default()),
        ["only on main", "seed"],
        "HEAD's history, as before"
    );
    assert_eq!(
        subjects(Query {
            branch: "feature".to_owned(),
            ..Query::default()
        }),
        ["only on the branch", "seed"],
        "the branch's own, including what HEAD cannot reach"
    );
}

#[test]
fn a_branch_that_is_gone_is_named_rather_than_walked_as_head() {
    let repo = TestRepo::new();
    repo.commit_file("README.md", "one\n", "seed");

    let started = Session::start(
        &repo.open(),
        Query {
            branch: "jamais-existe".to_owned(),
            ..Query::default()
        },
        &never(),
    );

    let Err(error) = started else {
        panic!("a branch that does not exist is not HEAD's history under another name")
    };
    assert!(error.to_string().contains("jamais-existe"), "{error}");
}

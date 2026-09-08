//! The repository summary the Repositories screen draws a row and a card from.

mod support;

use omagit_git::{Head, Operation, Summary};
use support::{TestRepo, never, scripted};

fn summary(fixture: &TestRepo) -> Summary {
    Summary::load(&fixture.open(), &never()).expect("a summary")
}

#[test]
fn summarises_a_repository_at_a_glance() {
    let fixture = scripted();
    let summary = summary(&fixture);

    assert!(matches!(&summary.head, Head::Branch { branch, .. } if branch == "main"));
    assert_eq!(summary.operation, None);
    assert!(summary.counts.is_clean());
    assert_eq!(
        summary.last_commit.as_ref().map(|c| c.summary.as_str()),
        Some("M: merge feature")
    );
    assert_eq!(summary.stashes, 0);
    assert!(summary.remotes.is_empty());
    assert!(summary.tracking.is_none(), "no upstream is configured");
}

#[test]
fn counts_the_working_copy_by_kind() {
    let fixture = TestRepo::new();
    fixture.write("kept.txt", "kept\n");
    fixture.write("edited.txt", "before\n");
    fixture.write("removed.txt", "gone soon\n");
    fixture.commit("first");

    fixture.write("edited.txt", "after\n");
    fixture.remove("removed.txt");
    fixture.write("fresh.txt", "new\n");
    fixture.write("staged.txt", "staged\n");
    fixture.add("staged.txt");

    let counts = summary(&fixture).counts;
    assert_eq!(counts.modified, 1, "edited.txt");
    assert_eq!(counts.deleted, 1, "removed.txt");
    assert_eq!(counts.added, 1, "staged.txt");
    assert_eq!(counts.untracked, 1, "fresh.txt");
    assert_eq!(counts.conflicted, 0);
    assert!(!counts.is_clean());
    assert_eq!(
        counts.tracked_changes(),
        3,
        "an untracked file is not a change to anything yet"
    );
}

#[test]
fn a_conflict_outranks_every_other_count() {
    let fixture = TestRepo::new();
    fixture.commit_file("shared.txt", "base\n", "base");
    fixture.branch("other");
    fixture.commit_file("shared.txt", "theirs\n", "theirs");
    fixture.checkout("main");
    fixture.commit_file("shared.txt", "ours\n", "ours");
    assert!(!fixture.git_allow_failure(&["merge", "other"]));

    let summary = summary(&fixture);
    assert_eq!(summary.counts.conflicted, 1);
    assert_eq!(
        summary.counts.modified, 0,
        "a conflicted file is counted once, and as a conflict"
    );
    assert_eq!(
        summary.operation,
        Some(Operation::Merge),
        "the row says 'merge', not 'on main' — being on main is not the useful fact here"
    );
}

#[test]
fn counts_stashes() {
    let fixture = TestRepo::new();
    fixture.commit_file("f.txt", "one\n", "first");
    assert_eq!(summary(&fixture).stashes, 0, "nothing stashed, no reflog");

    fixture.write("f.txt", "two\n");
    fixture.git(&["stash", "push", "-m", "first stash"]);
    fixture.write("f.txt", "three\n");
    fixture.git(&["stash", "push", "-m", "second stash"]);

    assert_eq!(summary(&fixture).stashes, 2);
}

#[test]
fn buckets_activity_over_the_last_ninety_days() {
    let fixture = TestRepo::new();
    fixture.commit_days_ago(200, "old.txt", "ancient\n", "long before the window");
    fixture.commit_days_ago(80, "a.txt", "a\n", "inside, near the start");
    fixture.commit_days_ago(2, "b.txt", "b\n", "inside, near the end");
    fixture.commit_days_ago(1, "c.txt", "c\n", "inside, yesterday");

    let activity = summary(&fixture).activity;
    assert_eq!(
        activity.total, 3,
        "the 200-day-old commit is outside the window"
    );
    assert_eq!(activity.buckets.len(), 30, "90 days, one bar per three");
    assert_eq!(
        activity.buckets.iter().sum::<u32>(),
        activity.total,
        "every counted commit landed in a bucket"
    );
    assert!(
        activity.buckets[activity.buckets.len() - 1] >= 1,
        "the last bucket is the most recent three days: {:?}",
        activity.buckets
    );
    assert!(
        activity.buckets[..4].iter().sum::<u32>() >= 1,
        "and the 80-day-old one is near the start"
    );
    assert_eq!(
        activity.peak(),
        2,
        "yesterday and two days ago share a bucket"
    );
}

#[test]
fn reads_the_committer_this_repository_would_use() {
    let fixture = TestRepo::new();
    fixture.commit_file("f.txt", "one\n", "first");

    let identity = summary(&fixture)
        .committer
        .expect("the fixture configures one");
    assert_eq!(identity.name, "Test Author");
    assert_eq!(identity.email, "author@omagit.test");
    assert!(
        !identity.inherited,
        "the fixture sets it in the repository, so the card must not claim it came from the global"
    );
    assert_eq!(identity.initials(), "TA");
}

#[test]
fn a_repository_with_no_commits_still_summarises() {
    let fixture = TestRepo::new();
    let summary = summary(&fixture);

    assert!(matches!(summary.head, Head::Unborn { .. }));
    assert!(summary.last_commit.is_none());
    assert_eq!(summary.activity.total, 0);
    assert!(summary.counts.is_clean());
}

#[test]
fn reads_the_remotes_without_counting_every_branch() {
    let origin = TestRepo::bare();
    let fixture = TestRepo::new();
    fixture.commit_file("a.txt", "a\n", "first");
    fixture.git(&["remote", "add", "origin", &origin.path().to_string_lossy()]);
    fixture.git(&["push", "-u", "origin", "main"]);
    // Twenty more branches, each with an upstream: `Refs::load` would count the
    // divergence of all of them, and a summary needs one.
    for n in 0..20 {
        fixture.git(&["branch", &format!("topic/{n}"), "main"]);
    }
    fixture.commit_file("b.txt", "b\n", "second");

    let summary = summary(&fixture);
    assert_eq!(summary.remotes.len(), 1);
    assert_eq!(summary.remotes[0].name, "origin");
    let tracking = summary.tracking.expect("main tracks origin/main");
    assert_eq!((tracking.ahead, tracking.behind), (1, 0));
}

#[test]
fn a_summary_can_be_cancelled() {
    let fixture = scripted();
    let cancel = omagit_git::Cancel::new();
    cancel.cancel();
    let error = Summary::load(&fixture.open(), &cancel).expect_err("a cancelled read returns");
    assert!(error.is_cancelled(), "got {error:?}");
}

//! Merge and rebase, and getting out of one that stopped.

mod support;

use omagit_git::cli::Git;
use omagit_git::ops::integrate::{self, MergeOptions};
use omagit_git::repo::Operation;
use support::{TestRepo, never};

fn git() -> Git {
    Git::detect().expect("git is installed; these tests need it")
}

/// `main` and `feature`, each with a commit the other does not have, in files
/// that do not touch — so they merge cleanly.
fn diverged() -> TestRepo {
    let repo = TestRepo::new();
    repo.commit_file("README.md", "one\n", "seed");
    repo.branch("feature");
    repo.commit_file("feature.txt", "theirs\n", "on the feature");
    repo.checkout("main");
    repo.commit_file("main.txt", "ours\n", "on main");
    repo
}

/// The same, but both branches change the same line.
fn conflicting() -> TestRepo {
    let repo = TestRepo::new();
    repo.commit_file("shared.txt", "original\n", "seed");
    repo.branch("feature");
    repo.commit_file("shared.txt", "theirs\n", "their version");
    repo.checkout("main");
    repo.commit_file("shared.txt", "ours\n", "our version");
    repo
}

fn head_message(repo: &TestRepo) -> String {
    repo.git(&["log", "-1", "--format=%s"])
}

#[test]
fn merging_brings_the_other_branch_in() {
    let repo = diverged();

    integrate::merge(
        &git(),
        &repo.open(),
        "feature",
        &MergeOptions::default(),
        &never(),
    )
    .expect("they do not touch");

    assert!(repo.path().join("feature.txt").exists());
    assert_eq!(
        repo.git(&["rev-list", "--count", "--merges", "HEAD"]),
        "1",
        "diverged branches make a merge commit"
    );
}

#[test]
fn a_merge_never_waits_for_an_editor() {
    // `git merge` opens one for the message. `GIT_EDITOR=true` covers it, and
    // this proves the cover holds even when the repository configures an editor
    // that would fail: `GIT_EDITOR` wins over `core.editor`.
    let repo = diverged();
    repo.git(&["config", "core.editor", "false"]);

    integrate::merge(
        &git(),
        &repo.open(),
        "feature",
        &MergeOptions {
            no_fast_forward: true,
            ..MergeOptions::default()
        },
        &never(),
    )
    .expect("no editor is consulted");

    assert!(head_message(&repo).contains("feature"));
}

#[test]
fn no_fast_forward_records_the_merge_that_would_not_have_existed() {
    let repo = TestRepo::new();
    repo.commit_file("README.md", "one\n", "seed");
    repo.branch("feature");
    repo.commit_file("feature.txt", "theirs\n", "on the feature");
    repo.checkout("main");

    integrate::merge(
        &git(),
        &repo.open(),
        "feature",
        &MergeOptions {
            no_fast_forward: true,
            ..MergeOptions::default()
        },
        &never(),
    )
    .expect("merged");

    assert_eq!(repo.git(&["rev-list", "--count", "--merges", "HEAD"]), "1");
}

#[test]
fn squashing_stages_the_changes_without_a_second_parent() {
    let repo = diverged();

    integrate::merge(
        &git(),
        &repo.open(),
        "feature",
        &MergeOptions {
            squash: true,
            ..MergeOptions::default()
        },
        &never(),
    )
    .expect("squashed");

    assert_eq!(
        repo.git(&["rev-list", "--count", "--merges", "HEAD"]),
        "0",
        "a squash records no merge"
    );
    assert!(
        repo.git(&["diff", "--cached", "--name-only"])
            .contains("feature.txt"),
        "and leaves the changes staged for a commit of your own"
    );
}

#[test]
fn a_conflicting_merge_stops_and_the_repository_says_so() {
    let repo = conflicting();

    let refused = integrate::merge(
        &git(),
        &repo.open(),
        "feature",
        &MergeOptions::default(),
        &never(),
    )
    .expect_err("both changed the same line");
    // And it says so in Git's own words. Worth pinning: `git merge` explains a
    // conflict on **stdout**, so an error carrying only `stderr` would reach
    // the user empty — which is what it did before `cli.rs` learned to fall
    // back.
    assert!(
        refused.to_string().contains("conflict") || refused.to_string().contains("CONFLICT"),
        "git says what happened: {refused}"
    );

    // The state every other read has to be interpreted against, which
    // `Repository::operation` has reported since M2.
    assert_eq!(repo.open().operation(), Some(Operation::Merge));
}

#[test]
fn aborting_puts_the_repository_back_where_the_merge_found_it() {
    let repo = conflicting();
    let before = repo.git(&["rev-parse", "HEAD"]);
    let _ = integrate::merge(
        &git(),
        &repo.open(),
        "feature",
        &MergeOptions::default(),
        &never(),
    );
    assert_eq!(repo.open().operation(), Some(Operation::Merge));

    integrate::abort(&git(), &repo.open(), Operation::Merge, &never()).expect("aborted");

    assert_eq!(repo.open().operation(), None);
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), before);
    assert_eq!(
        String::from_utf8_lossy(&repo.read("shared.txt")),
        "ours\n",
        "and our version is back, unmarked"
    );
}

#[test]
fn rebasing_replays_our_commits_on_top_of_theirs() {
    let repo = diverged();

    integrate::rebase(&git(), &repo.open(), "feature", &never()).expect("they do not touch");

    // Linear: no merge commit, and their commit is now an ancestor of ours.
    assert_eq!(repo.git(&["rev-list", "--count", "--merges", "HEAD"]), "0");
    assert_eq!(head_message(&repo), "on main");
    assert!(
        repo.git(&["log", "--format=%s"]).contains("on the feature"),
        "and theirs is underneath"
    );
}

#[test]
fn a_conflicting_rebase_stops_and_can_be_aborted() {
    let repo = conflicting();
    let before = repo.git(&["rev-parse", "HEAD"]);

    let refused =
        integrate::rebase(&git(), &repo.open(), "feature", &never()).expect_err("same line");
    assert!(
        refused.to_string().to_lowercase().contains("conflict"),
        "git says what happened: {refused}"
    );
    assert!(matches!(
        repo.open().operation(),
        Some(Operation::Rebase { .. })
    ));

    integrate::abort(
        &git(),
        &repo.open(),
        Operation::Rebase { interactive: false },
        &never(),
    )
    .expect("aborted");

    assert_eq!(repo.open().operation(), None);
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), before);
}

#[test]
fn aborting_sends_the_command_the_running_operation_answers_to() {
    // `git merge --abort` during a rebase says "no merge to abort", and the
    // message would be about the wrong thing.
    let repo = conflicting();
    let _ = integrate::rebase(&git(), &repo.open(), "feature", &never());
    let journal = omagit_git::journal::Journal::new();
    let git = git().with_journal(journal.clone());

    integrate::abort(
        &git,
        &repo.open(),
        Operation::Rebase { interactive: false },
        &never(),
    )
    .expect("aborted");

    let entries = journal.entries();
    assert!(
        entries
            .iter()
            .any(|entry| entry.command.contains("rebase --abort")),
        "{:?}",
        entries.iter().map(|e| &e.command).collect::<Vec<_>>()
    );
}

#[test]
fn a_bisect_is_refused_by_name_rather_than_sent_a_command_that_does_not_exist() {
    let repo = diverged();

    let error = integrate::abort(&git(), &repo.open(), Operation::Bisect, &never())
        .expect_err("bisect ends with reset");
    assert!(error.to_string().contains("reset"), "{error}");
}

#[test]
fn merge_and_rebase_are_recorded_as_destructive() {
    // Both rewrite the working tree and can stop half-way.
    let repo = diverged();
    let journal = omagit_git::journal::Journal::new();
    let git = git().with_journal(journal.clone());

    integrate::merge(
        &git,
        &repo.open(),
        "feature",
        &MergeOptions::default(),
        &never(),
    )
    .expect("merged");

    assert!(
        journal
            .entries()
            .iter()
            .any(|entry| entry.destructive && entry.command.contains("merge"))
    );
}

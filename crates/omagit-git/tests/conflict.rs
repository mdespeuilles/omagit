//! Stopped on a conflict: which side is which, taking one, and getting out the
//! front.

mod support;

use omagit_git::cli::Git;
use omagit_git::conflict::{self, Side};
use omagit_git::ops::{conflict as resolve, integrate};
use omagit_git::repo::Operation;
use omagit_git::status::{Conflict, Status, StatusOptions};
use support::{TestRepo, never};

fn git() -> Git {
    Git::detect().expect("git is installed; these tests need it")
}

/// `main` and `feature`, both changing the same line of the same file.
fn conflicting() -> TestRepo {
    let repo = TestRepo::new();
    repo.commit_file("shared.txt", "original\n", "seed");
    repo.branch("feature");
    repo.commit_file("shared.txt", "theirs\n", "their version");
    repo.checkout("main");
    repo.commit_file("shared.txt", "ours\n", "our version");
    repo
}

/// Stop a merge of `feature` on that conflict.
fn stopped_merge() -> TestRepo {
    let repo = conflicting();
    integrate::merge(
        &git(),
        &repo.open(),
        "feature",
        &Default::default(),
        &never(),
    )
    .expect_err("the same line");
    repo
}

fn status(repo: &TestRepo) -> Status {
    Status::load(&repo.open(), StatusOptions::default(), &never()).expect("a status")
}

fn conflicts(repo: &TestRepo) -> Vec<String> {
    status(repo)
        .conflicts()
        .map(|entry| entry.path.display_lossy().into_owned())
        .collect()
}

fn contents(repo: &TestRepo, path: &str) -> String {
    String::from_utf8_lossy(&repo.read(path)).into_owned()
}

#[test]
fn a_merge_keeps_the_words_ours_and_theirs_meaning_what_they_say() {
    let repo = stopped_merge();

    let sides = conflict::sides(&repo.open())
        .expect("read")
        .expect("a merge is running");

    assert_eq!(sides.ours, "main");
    assert_eq!(sides.theirs, "feature");
    assert!(!sides.replayed);
}

#[test]
fn a_rebase_calls_the_branch_you_are_standing_on_theirs() {
    // The reason this module exists. During a rebase `--ours` is the branch
    // being replayed *onto* and `--theirs` is your own work — two buttons
    // labelled with the pronouns alone would be asking someone to choose
    // between two meanings they cannot see.
    let repo = conflicting();
    repo.checkout("feature");
    integrate::rebase(&git(), &repo.open(), "main", &never()).expect_err("the same line");

    let sides = conflict::sides(&repo.open())
        .expect("read")
        .expect("a rebase is running");

    assert_eq!(sides.ours, "main", "the side being replayed onto");
    assert_eq!(sides.theirs, "feature", "your own commits");
    assert!(sides.replayed, "and the interface has to say so");
}

#[test]
fn a_repository_in_the_middle_of_nothing_has_no_sides() {
    let repo = conflicting();
    assert_eq!(conflict::sides(&repo.open()).expect("read"), None);
}

#[test]
fn keeping_ours_leaves_our_version_resolved() {
    let repo = stopped_merge();

    resolve::take(
        &git(),
        &repo.open(),
        &"shared.txt".into(),
        Side::Ours,
        Conflict::BothModified,
        &never(),
    )
    .expect("resolved");

    assert_eq!(contents(&repo, "shared.txt"), "ours\n");
    assert!(
        conflicts(&repo).is_empty(),
        "and it is out of the index's unmerged state, not merely restored"
    );
}

#[test]
fn keeping_theirs_leaves_their_version_resolved() {
    let repo = stopped_merge();

    resolve::take(
        &git(),
        &repo.open(),
        &"shared.txt".into(),
        Side::Theirs,
        Conflict::BothModified,
        &never(),
    )
    .expect("resolved");

    assert_eq!(contents(&repo, "shared.txt"), "theirs\n");
    assert!(conflicts(&repo).is_empty());
}

#[test]
fn keeping_our_side_of_a_file_we_deleted_keeps_the_deletion() {
    // `DU`: our side has no version at all, and `git checkout --ours` says so
    // — "does not have our version", which is true and useless. Keeping ours
    // here means keeping the deletion.
    let repo = TestRepo::new();
    repo.commit_file("gone.txt", "original\n", "seed");
    repo.branch("feature");
    repo.commit_file("gone.txt", "theirs\n", "they changed it");
    repo.checkout("main");
    repo.remove("gone.txt");
    repo.add_all();
    repo.commit_staged("we deleted it");
    integrate::merge(
        &git(),
        &repo.open(),
        "feature",
        &Default::default(),
        &never(),
    )
    .expect_err("delete against modify");

    let kind = status(&repo)
        .conflicts()
        .find_map(|entry| entry.conflict)
        .expect("a conflict");
    assert_eq!(kind, Conflict::DeletedByUs);

    resolve::take(
        &git(),
        &repo.open(),
        &"gone.txt".into(),
        Side::Ours,
        kind,
        &never(),
    )
    .expect("resolved");

    assert!(!repo.path().join("gone.txt").exists());
    assert!(conflicts(&repo).is_empty());
}

#[test]
fn keeping_their_side_of_a_file_they_deleted_removes_it() {
    let repo = TestRepo::new();
    repo.commit_file("gone.txt", "original\n", "seed");
    repo.branch("feature");
    repo.remove("gone.txt");
    repo.add_all();
    repo.commit_staged("they deleted it");
    repo.checkout("main");
    repo.commit_file("gone.txt", "ours\n", "we changed it");
    integrate::merge(
        &git(),
        &repo.open(),
        "feature",
        &Default::default(),
        &never(),
    )
    .expect_err("modify against delete");

    let kind = status(&repo)
        .conflicts()
        .find_map(|entry| entry.conflict)
        .expect("a conflict");
    assert_eq!(kind, Conflict::DeletedByThem);

    resolve::take(
        &git(),
        &repo.open(),
        &"gone.txt".into(),
        Side::Theirs,
        kind,
        &never(),
    )
    .expect("resolved");

    assert!(!repo.path().join("gone.txt").exists());
    assert!(conflicts(&repo).is_empty());
}

#[test]
fn continuing_finishes_the_merge_the_conflict_stopped() {
    let repo = stopped_merge();
    resolve::take(
        &git(),
        &repo.open(),
        &"shared.txt".into(),
        Side::Ours,
        Conflict::BothModified,
        &never(),
    )
    .expect("resolved");

    integrate::resume(&git(), &repo.open(), Operation::Merge, &never()).expect("continued");

    assert_eq!(repo.open().operation(), None);
    assert_eq!(
        repo.git(&["rev-list", "--count", "--merges", "HEAD"]),
        "1",
        "the merge commit the conflict was standing in the way of"
    );
}

#[test]
fn continuing_with_a_path_still_unmerged_is_refused_in_gits_own_words() {
    // The check is git's, not ours: the index is the truth, and anything read
    // here first would be a copy of it taken a moment earlier.
    let repo = stopped_merge();

    let refused =
        integrate::resume(&git(), &repo.open(), Operation::Merge, &never()).expect_err("unmerged");

    let said = refused.to_string();
    assert!(
        said.contains("shared.txt") || said.to_lowercase().contains("conflict"),
        "it names the file in the way: {said}"
    );
    assert_eq!(
        repo.open().operation(),
        Some(Operation::Merge),
        "and the operation is still where it was"
    );
}

#[test]
fn continuing_a_rebase_replays_what_was_left() {
    let repo = conflicting();
    repo.checkout("feature");
    integrate::rebase(&git(), &repo.open(), "main", &never()).expect_err("the same line");
    resolve::take(
        &git(),
        &repo.open(),
        &"shared.txt".into(),
        Side::Theirs,
        Conflict::BothModified,
        &never(),
    )
    .expect("resolved");

    integrate::resume(
        &git(),
        &repo.open(),
        Operation::Rebase { interactive: false },
        &never(),
    )
    .expect("continued");

    assert_eq!(repo.open().operation(), None);
    assert_eq!(repo.git(&["rev-parse", "--abbrev-ref", "HEAD"]), "feature");
    assert_eq!(
        repo.git(&["log", "--format=%s", "-2"]),
        "their version\nour version",
        "our commit replayed on top of main's"
    );
}

#[test]
fn a_bisect_is_refused_by_name_rather_than_sent_a_flag_it_has_not_got() {
    let repo = conflicting();

    let error = integrate::resume(&git(), &repo.open(), Operation::Bisect, &never())
        .expect_err("bisect has no --continue");

    assert!(error.to_string().contains("bisect good"), "{error}");
}

#[test]
fn taking_a_side_is_recorded_as_destructive_and_continuing_a_replay_too() {
    let repo = stopped_merge();
    let journal = omagit_git::journal::Journal::new();
    let git = git().with_journal(journal.clone());

    resolve::take(
        &git,
        &repo.open(),
        &"shared.txt".into(),
        Side::Ours,
        Conflict::BothModified,
        &never(),
    )
    .expect("resolved");
    integrate::resume(&git, &repo.open(), Operation::Merge, &never()).expect("continued");

    let entries = journal.entries();
    assert!(
        entries
            .iter()
            .any(|entry| entry.destructive && entry.command.contains("checkout --ours")),
        "it overwrites the file in the work tree, hand resolution included: {entries:?}"
    );
    assert!(
        entries
            .iter()
            .any(|entry| !entry.destructive && entry.command.contains("merge --continue")),
        "a merge --continue commits what is already in the index: {entries:?}"
    );
}

#[test]
fn a_stash_that_conflicts_on_the_way_back_has_no_operation_and_still_has_sides_to_take() {
    // The case the two M8 slices meet in. A `git stash pop` that conflicts
    // leaves unmerged paths behind *without* starting an operation: there is no
    // `MERGE_HEAD`, so nothing to name the sides after — and the interface falls
    // back to the words. What still works is the resolution, because the index
    // has the stages whether or not anything is half-finished.
    let repo = TestRepo::new();
    repo.commit_file("shared.txt", "original\n", "seed");
    repo.write("shared.txt", "stashed\n");
    omagit_git::ops::stash::push(&git(), &repo.open(), "shelved", false, &never())
        .expect("stashed");
    // Committed, not merely written: `git stash pop` refuses outright when the
    // change in the way is uncommitted, and refusing is not conflicting.
    repo.commit_file("shared.txt", "committed since\n", "the line moved under it");

    omagit_git::ops::stash::pop(&git(), &repo.open(), 0, &never()).expect_err("the same line");

    assert_eq!(
        conflict::sides(&repo.open()).expect("read"),
        None,
        "no operation is running: a stash pop is not one"
    );
    assert_eq!(conflicts(&repo), ["shared.txt"]);

    resolve::take(
        &git(),
        &repo.open(),
        &"shared.txt".into(),
        Side::Theirs,
        Conflict::BothModified,
        &never(),
    )
    .expect("resolved");

    assert_eq!(contents(&repo, "shared.txt"), "stashed\n");
    assert!(conflicts(&repo).is_empty());
}

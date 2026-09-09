//! Branch operations, against real repositories.
//!
//! Every assertion asks `git` what the repository looks like afterwards rather
//! than trusting the command that ran: an operation that reports success and
//! did the wrong thing is the failure worth catching.

mod support;

use omagit_git::cli::Git;
use omagit_git::ops::branch;
use support::{TestRepo, never};

fn git() -> Git {
    Git::detect().expect("git is installed; these tests need it")
}

/// A repository on `main` with one commit, and a `feature` branch ahead of it.
fn two_branches() -> TestRepo {
    let repo = TestRepo::new();
    repo.commit_file("README.md", "one\n", "seed");
    repo.branch("feature");
    repo.commit_file("feature.txt", "work\n", "only on feature");
    repo.checkout("main");
    repo
}

fn head(repo: &TestRepo) -> String {
    repo.git(&["rev-parse", "--abbrev-ref", "HEAD"])
}

fn branches(repo: &TestRepo) -> Vec<String> {
    repo.git(&["branch", "--format=%(refname:short)"])
        .lines()
        .map(ToOwned::to_owned)
        .collect()
}

#[test]
fn switching_moves_head_and_the_working_tree_with_it() {
    let repo = two_branches();
    assert_eq!(head(&repo), "main");
    assert!(!repo.path().join("feature.txt").exists());

    branch::checkout(&git(), &repo.open(), "feature", &never()).expect("feature exists");

    assert_eq!(head(&repo), "feature");
    assert!(
        repo.path().join("feature.txt").exists(),
        "a checkout brings the tree with it, which is why it is `git`'s to do"
    );
}

#[test]
fn switching_away_from_uncommitted_work_is_refused_in_gits_own_words() {
    // Not pre-empted here: `git`'s message names the file in the way, and
    // SPEC §3 rule 3 wants that message shown rather than replaced by one this
    // layer composed.
    let repo = two_branches();
    // `feature.txt` exists on `feature` and not on `main`, so an uncommitted
    // one here is what a switch would overwrite.
    repo.write("feature.txt", "written on main, never committed\n");

    let error = branch::checkout(&git(), &repo.open(), "feature", &never())
        .expect_err("git refuses to overwrite it");
    let said = error.to_string();
    assert!(
        said.contains("feature.txt"),
        "the refusal has to name what is in the way: {said}"
    );
    assert_eq!(head(&repo), "main", "and nothing moved");
}

#[test]
fn a_branch_whose_name_could_be_a_path_still_switches() {
    // `git checkout -- docs` reads `docs` as a pathspec and reports that no
    // file by that name is known. `git switch` only ever moves HEAD.
    let repo = TestRepo::new();
    repo.commit_file("docs/readme.md", "one\n", "seed");
    repo.git(&["branch", "docs"]);

    branch::checkout(&git(), &repo.open(), "docs", &never()).expect("the branch, not the folder");
    assert_eq!(head(&repo), "docs");
}

#[test]
fn creating_a_branch_leaves_head_alone_unless_asked() {
    let repo = two_branches();

    branch::create(&git(), &repo.open(), "quiet", None, false, &never()).expect("created");
    assert!(branches(&repo).contains(&"quiet".to_owned()));
    assert_eq!(head(&repo), "main", "created is not switched to");

    branch::create(&git(), &repo.open(), "loud", None, true, &never()).expect("created");
    assert_eq!(head(&repo), "loud");
}

#[test]
fn a_branch_can_start_anywhere_git_can_resolve() {
    // Revision syntax is a language; it is passed through rather than parsed,
    // so an expression that works in a terminal works here.
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "first");
    repo.commit_file("a.txt", "two\n", "second");
    repo.commit_file("a.txt", "three\n", "third");

    branch::create(
        &git(),
        &repo.open(),
        "back-then",
        Some("HEAD~2"),
        false,
        &never(),
    )
    .expect("a relative revision resolves");

    assert_eq!(
        repo.git(&["log", "-1", "--format=%s", "back-then"]),
        "first"
    );
}

#[test]
fn renaming_keeps_the_commits() {
    let repo = two_branches();

    branch::rename(&git(), &repo.open(), "feature", "feature/renamed", &never()).expect("renamed");

    let names = branches(&repo);
    assert!(names.contains(&"feature/renamed".to_owned()));
    assert!(!names.contains(&"feature".to_owned()));
    assert_eq!(
        repo.git(&["log", "-1", "--format=%s", "feature/renamed"]),
        "only on feature"
    );
}

#[test]
fn deleting_an_unmerged_branch_is_refused_until_it_is_forced() {
    let repo = two_branches();

    let error = branch::delete(&git(), &repo.open(), "feature", false, &never())
        .expect_err("its commit is on no other branch");
    assert!(
        error.to_string().contains("feature"),
        "the refusal names the branch: {error}"
    );
    assert!(branches(&repo).contains(&"feature".to_owned()));

    branch::delete(&git(), &repo.open(), "feature", true, &never()).expect("forced");
    assert!(!branches(&repo).contains(&"feature".to_owned()));
}

#[test]
fn a_forced_delete_is_recorded_as_destructive() {
    // SPEC §15 risk 5: the journal marks it before it runs. What it removes is
    // reachable only through the reflog afterwards.
    let repo = two_branches();
    let journal = omagit_git::journal::Journal::new();
    let git = git().with_journal(journal.clone());

    branch::delete(&git, &repo.open(), "feature", true, &never()).expect("forced");

    let entries = journal.entries();
    let forced = entries
        .iter()
        .find(|entry| entry.command.contains("--force"))
        .expect("the delete is in the journal");
    assert!(forced.destructive);
}

#[test]
fn merged_says_which_of_the_two_questions_a_delete_asks() {
    let repo = two_branches();
    let git = git();

    let before = branch::merged(&git, &repo.open(), &never()).expect("asked");
    assert!(
        !before.contains("feature"),
        "feature has a commit main does not"
    );
    assert!(
        before.contains("main"),
        "and a branch is merged into itself"
    );

    repo.git(&["merge", "--no-ff", "-m", "merge feature", "feature"]);
    let after = branch::merged(&git, &repo.open(), &never()).expect("asked");
    assert!(after.contains("feature"), "and now it has none");
}

#[test]
fn the_whole_merged_set_comes_back_in_one_command() {
    // The sidebar asks this about every row it draws; a process each would make
    // opening a repository with forty branches forty processes.
    let repo = two_branches();
    for name in ["one", "two", "three"] {
        repo.git(&["branch", name]);
    }
    let journal = omagit_git::journal::Journal::new();
    let git = git().with_journal(journal.clone());

    let all = branch::merged(&git, &repo.open(), &never()).expect("asked");

    assert_eq!(all.len(), 4, "main and the three that point at it: {all:?}");
    assert_eq!(journal.entries().len(), 1);
}

#[test]
fn a_detached_checkout_says_so_and_is_marked_destructive() {
    // Nothing is deleted; what can be lost is the *next* commit, which is
    // reachable only through the reflog once you switch away.
    let repo = two_branches();
    let journal = omagit_git::journal::Journal::new();
    let git = git().with_journal(journal.clone());
    let commit = repo.git(&["rev-parse", "HEAD"]);

    branch::checkout_detached(&git, &repo.open(), &commit, &never()).expect("detached");

    assert_eq!(head(&repo), "HEAD", "abbrev-ref says HEAD when detached");
    assert!(
        journal
            .entries()
            .iter()
            .any(|entry| entry.destructive && entry.command.contains("--detach"))
    );
}

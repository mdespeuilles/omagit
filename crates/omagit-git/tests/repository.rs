//! Opening repositories, and the states SPEC §13 requires the read path to
//! survive: no commits, detached `HEAD`, bare, and an operation in progress.

mod support;

use std::path::Path;

use omagit_git::{GitError, Head, Operation, Repository};
use support::TestRepo;

/// Two paths naming the same directory.
///
/// Not string equality, and macOS is why: `/var` is a symlink to `/private/var`
/// there, so a temporary directory has two true names and `gix` hands back the
/// one it was given rather than the resolved one. Comparing the strings passes
/// on Linux — where `/tmp` is a real directory — and fails on macOS, which is
/// exactly the shape of bug the two-platform matrix exists to catch (and did).
///
/// What the app actually needs is weaker than either: every path it joins or
/// strips comes from the same source, so it only has to be *consistent*. This
/// asserts the useful property instead of the incidental one.
fn assert_same_dir(actual: Option<&Path>, expected: &Path, what: &str) {
    let actual = actual.expect("a work tree");
    let resolve = |path: &Path| std::fs::canonicalize(path).expect("the path exists");
    assert_eq!(resolve(actual), resolve(expected), "{what}");
}

#[test]
fn opens_a_work_tree_and_a_git_dir() {
    let fixture = TestRepo::new();
    fixture.commit_file("a.txt", "a\n", "first");

    let from_work_tree = Repository::open(fixture.path()).expect("a repository");
    assert_same_dir(
        from_work_tree.work_dir(),
        fixture.path(),
        "the work tree is where the fixture is",
    );
    assert!(!from_work_tree.is_bare());

    let from_git_dir = Repository::open(fixture.path().join(".git")).expect("a repository");
    assert_eq!(from_git_dir.head().unwrap(), from_work_tree.head().unwrap());
}

#[test]
fn discovers_from_a_subdirectory() {
    let fixture = TestRepo::new();
    fixture.commit_file("src/deep/file.rs", "fn main() {}\n", "first");

    let repo = Repository::discover(fixture.path().join("src/deep")).expect("discovery walks up");
    assert_same_dir(
        repo.work_dir(),
        fixture.path(),
        "discovery lands on the repository root",
    );
}

#[test]
fn tells_a_missing_repository_from_a_plain_directory() {
    // Moved or unmounted: the sidebar keeps the entry and strikes it through.
    let gone = Repository::open("/nonexistent/omagit-test").expect_err("nothing there");
    assert!(
        matches!(gone, GitError::RepositoryMissing(_)),
        "got {gone:?}"
    );

    // Present, but not a repository: a mistake in the add dialog.
    let empty = tempfile::tempdir().expect("a temporary directory");
    let not_a_repo = Repository::open(empty.path()).expect_err("not a repository");
    assert!(
        matches!(not_a_repo, GitError::NotARepository(_)),
        "got {not_a_repo:?}"
    );
}

#[test]
fn a_repository_with_no_commits_has_an_unborn_head() {
    let fixture = TestRepo::new();
    let repo = fixture.open();

    match repo
        .head()
        .expect("HEAD is readable before the first commit")
    {
        Head::Unborn { branch } => assert_eq!(branch, "main"),
        other => panic!("expected an unborn HEAD, got {other:?}"),
    }
    assert_eq!(repo.head().unwrap().commit(), None);
    assert_eq!(repo.operation(), None);
}

#[test]
fn reads_a_branch_and_a_detached_head() {
    let fixture = TestRepo::new();
    let first = fixture.commit_file("a.txt", "a\n", "first");
    fixture.commit_file("b.txt", "b\n", "second");
    let repo = fixture.open();

    match repo.head().expect("HEAD") {
        Head::Branch { branch, commit } => {
            assert_eq!(branch, "main");
            assert_eq!(commit, fixture.head());
        }
        other => panic!("expected a branch, got {other:?}"),
    }

    fixture.checkout(&first.to_string());
    match fixture.open().head().expect("HEAD") {
        Head::Detached { commit } => assert_eq!(commit, first),
        other => panic!("expected a detached HEAD, got {other:?}"),
    }
    assert!(
        fixture
            .open()
            .head()
            .unwrap()
            .label()
            .starts_with("detached at"),
        "the topbar has something to show"
    );
}

#[test]
fn a_bare_repository_has_no_work_tree() {
    let fixture = TestRepo::bare();
    let repo = fixture.open();

    assert!(repo.is_bare());
    assert_eq!(repo.work_dir(), None);
    // No commits and no work tree: reading it must still answer, not fail.
    assert!(matches!(repo.head(), Ok(Head::Unborn { .. })));
}

#[test]
fn sees_a_merge_in_progress() {
    let fixture = TestRepo::new();
    fixture.commit_file("shared.txt", "base\n", "base");
    fixture.branch("other");
    fixture.commit_file("shared.txt", "theirs\n", "theirs");
    fixture.checkout("main");
    fixture.commit_file("shared.txt", "ours\n", "ours");

    let merged = fixture.git_allow_failure(&["merge", "other"]);
    assert!(!merged, "the fixture is built so the merge conflicts");

    assert_eq!(fixture.open().operation(), Some(Operation::Merge));
}

#[test]
fn sees_a_rebase_in_progress() {
    let fixture = TestRepo::new();
    fixture.commit_file("shared.txt", "base\n", "base");
    fixture.branch("topic");
    fixture.commit_file("shared.txt", "topic\n", "topic");
    fixture.checkout("main");
    fixture.commit_file("shared.txt", "main\n", "main");

    let rebased = fixture.git_allow_failure(&["rebase", "main", "topic"]);
    assert!(!rebased, "the fixture is built so the rebase stops");

    assert!(
        matches!(fixture.open().operation(), Some(Operation::Rebase { .. })),
        "a stopped rebase has to be visible, or the UI offers actions that cannot work"
    );
}

#[test]
fn a_clean_repository_is_in_no_operation() {
    let fixture = support::scripted();
    assert_eq!(fixture.open().operation(), None);
}

#[test]
fn a_repository_handle_travels_between_threads() {
    // SPEC §10: one handle, many background jobs. If this stops compiling, the
    // data flow does too.
    let fixture = support::scripted();
    let repo = fixture.open();
    let head = std::thread::spawn(move || repo.head().expect("HEAD"))
        .join()
        .expect("the thread ran");
    assert_eq!(head.commit(), Some(&fixture.head()));
}

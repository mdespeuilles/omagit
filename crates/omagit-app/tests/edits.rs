//! The write logic, against real repositories.
//!
//! `omagit-git`'s own tests already prove that `stage`, `unstage` and `discard`
//! do what they say. What is proved here is the layer above: that the target
//! the front end sent is turned into the right selection, and — the one that
//! matters — that a patch is built from the side of the index it is coming out
//! of. Inverted, that produces a patch `git apply` rejects, or one it accepts
//! against text that happens to match.

mod support;

use omagit_git::RepoPath;
use omagit_git::cli::Git;
use omagit_lib::edits::{Target, discard, file_side, stage};
use support::{TestRepo, never};

fn git() -> Git {
    Git::detect().expect("git is installed; these tests need it")
}

fn at(name: &str) -> RepoPath {
    RepoPath::from_bytes(name.as_bytes().to_vec())
}

/// The hunks of a textual diff. Anything else has none to select from.
fn hunks(diff: &omagit_git::FileDiff) -> &[omagit_git::diff::Hunk] {
    match &diff.content {
        omagit_git::diff::DiffContent::Text { hunks, .. } => hunks,
        other => panic!("the fixture is text, not {other:?}"),
    }
}

/// Everything a diff's hunks say, as one string — enough to tell the two sides
/// of the index apart.
fn text(diff: &omagit_git::FileDiff) -> String {
    hunks(diff)
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .map(|line| line.text_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Twenty lines, with the second and the nineteenth replaced — far enough apart
/// that Git makes two hunks of them.
fn twenty(second: &str, nineteenth: &str) -> String {
    let mut lines: Vec<String> = (1..=20).map(|n| format!("line {n}")).collect();
    lines[1] = second.to_owned();
    lines[18] = nineteenth.to_owned();
    format!("{}\n", lines.join("\n"))
}

fn two_hunks() -> TestRepo {
    let repo = TestRepo::new();
    repo.write("file.txt", &twenty("line 2", "line 19"));
    repo.git(&["add", "file.txt"]);
    repo.git(&["commit", "-m", "seed"]);
    repo.write("file.txt", &twenty("SECOND", "NINETEENTH"));
    repo
}

#[test]
fn one_hunk_goes_in_and_the_other_stays_out() {
    let repo = two_hunks();
    let target = Target::Hunks { hunks: vec![0] };

    stage(
        &git(),
        &repo.open(),
        &at("file.txt"),
        &target,
        false,
        &never(),
    )
    .expect("the first hunk stages");

    let indexed = repo.indexed("file.txt");
    assert!(indexed.contains("SECOND"), "the first hunk is in the index");
    assert!(
        !indexed.contains("NINETEENTH"),
        "the second hunk is not: staging one is the whole point"
    );
    assert!(
        repo.read("file.txt").contains("NINETEENTH"),
        "and the working tree keeps both"
    );
}

#[test]
fn unstaging_a_hunk_reads_the_staged_side_not_the_working_tree() {
    // The inversion this pins. Both sides have a diff for this file and they
    // are different diffs: the working tree has two hunks against HEAD, the
    // index has one. A patch built from the wrong one names lines that are not
    // where it says they are.
    let repo = two_hunks();
    let cancel = never();
    stage(
        &git(),
        &repo.open(),
        &at("file.txt"),
        &Target::Hunks { hunks: vec![0] },
        false,
        &cancel,
    )
    .expect("the first hunk stages");

    // Both sides now have exactly one hunk, and they are different hunks. The
    // count is the same, which is why counting them proves nothing — what the
    // patch is built against is *which lines* the side describes.
    let staged = file_side(&repo.open(), &at("file.txt"), true, &cancel).expect("a staged diff");
    let unstaged =
        file_side(&repo.open(), &at("file.txt"), false, &cancel).expect("an unstaged diff");
    assert!(text(&staged).contains("SECOND") && !text(&staged).contains("NINETEENTH"));
    assert!(text(&unstaged).contains("NINETEENTH") && !text(&unstaged).contains("SECOND"));

    stage(
        &git(),
        &repo.open(),
        &at("file.txt"),
        &Target::Hunks { hunks: vec![0] },
        true,
        &cancel,
    )
    .expect("the same hunk comes back out");

    assert!(
        !repo.indexed("file.txt").contains("SECOND"),
        "the index is back where it was"
    );
    assert!(
        repo.read("file.txt").contains("SECOND"),
        "unstaging never touches the working tree"
    );
}

#[test]
fn a_whole_file_needs_no_diff_at_all() {
    // The bug this pins: a checkbox on a row nobody had opened did nothing,
    // because the code asked for a diff it did not need. A deletion has no text
    // patch to build, and `git add` stages it from the path alone.
    let repo = TestRepo::new();
    repo.write("gone.txt", "one\n");
    repo.git(&["add", "gone.txt"]);
    repo.git(&["commit", "-m", "seed"]);
    std::fs::remove_file(repo.path().join("gone.txt")).expect("a removable fixture");

    stage(
        &git(),
        &repo.open(),
        &at("gone.txt"),
        &Target::File,
        false,
        &never(),
    )
    .expect("a deletion stages");

    assert_eq!(
        repo.git(&["diff", "--cached", "--name-status"]),
        "D\tgone.txt"
    );
}

#[test]
fn lines_are_addressed_by_hunk_and_position_inside_it() {
    let repo = two_hunks();
    let cancel = never();
    let diff = file_side(&repo.open(), &at("file.txt"), false, &cancel).expect("a diff");

    // The one changed line of the second hunk, found the way the front end
    // finds it: by walking the rows the interface was given.
    let (hunk, index) = hunks(&diff)
        .iter()
        .enumerate()
        .find_map(|(hunk, block)| {
            block
                .lines
                .iter()
                .position(|line| line.text_lossy().contains("NINETEENTH"))
                .map(|index| (hunk, index))
        })
        .expect("the added line is in some hunk");

    stage(
        &git(),
        &repo.open(),
        &at("file.txt"),
        &Target::Lines {
            lines: vec![(hunk, index)],
        },
        false,
        &cancel,
    )
    .expect("one line stages");

    let indexed = repo.indexed("file.txt");
    assert!(indexed.contains("NINETEENTH"), "that line went in");
    assert!(!indexed.contains("SECOND"), "and only that line did");
}

#[test]
fn discarding_an_untracked_file_removes_it_rather_than_restoring_it() {
    // There is no earlier version to restore to. Which of the two it is comes
    // from the status here rather than from the caller: a front end that was
    // told, and told a moment too early, would delete a file that only needed
    // reverting.
    let repo = TestRepo::new();
    repo.write("seed.txt", "one\n");
    repo.git(&["add", "seed.txt"]);
    repo.git(&["commit", "-m", "seed"]);
    repo.write("new.txt", "not in any commit\n");

    discard(
        &git(),
        &repo.open(),
        &at("new.txt"),
        &Target::File,
        &never(),
    )
    .expect("an untracked file is discardable");

    assert!(
        !repo.path().join("new.txt").exists(),
        "discarding an untracked file means removing it"
    );
}

#[test]
fn discarding_one_hunk_leaves_the_other_alone() {
    let repo = two_hunks();

    discard(
        &git(),
        &repo.open(),
        &at("file.txt"),
        &Target::Hunks { hunks: vec![1] },
        &never(),
    )
    .expect("the second hunk is discardable");

    let worktree = repo.read("file.txt");
    assert!(worktree.contains("SECOND"), "the first edit survives");
    assert!(
        !worktree.contains("NINETEENTH"),
        "the second is gone from the working tree"
    );
}

#[test]
fn a_file_that_has_left_the_status_is_named_rather_than_guessed_at() {
    let repo = TestRepo::new();
    repo.write("seed.txt", "one\n");
    repo.git(&["add", "seed.txt"]);
    repo.git(&["commit", "-m", "seed"]);

    let error = file_side(&repo.open(), &at("seed.txt"), false, &never())
        .expect_err("an unmodified file has no diff");
    assert!(
        error.to_string().contains("seed.txt"),
        "the message says which file: {error}"
    );
}

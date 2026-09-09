//! Patches, proved against `git apply` rather than against my expectations.
//!
//! A patch that looks right and applies wrong rewrites a file silently, so
//! these do not compare bytes to a string I wrote by hand. They build a patch,
//! hand it to the real `git`, and then ask the repository what happened —
//! which is the only check that would have caught the patch being subtly
//! misaddressed.

mod support;

use std::collections::BTreeSet;

use omagit_git::diff::{DiffOptions, unstaged_file};
use omagit_git::patch::{Direction, Selection, build};
use omagit_git::status::{Status, StatusOptions};
use omagit_git::{FileDiff, RepoPath};
use support::{TestRepo, never};

/// The unstaged diff of one file, which is what the Working Copy hands to a
/// staging operation.
fn unstaged(repo: &TestRepo, path: &str) -> FileDiff {
    let opened = repo.open();
    let status = Status::load(&opened, StatusOptions::default(), &never()).expect("a status");
    let wanted = RepoPath::from_bytes(path.as_bytes().to_vec());
    let entry = status
        .entries
        .iter()
        .find(|entry| entry.path == wanted)
        .unwrap_or_else(|| panic!("{path} is not in the status"));
    unstaged_file(&opened, entry, DiffOptions::default())
        .expect("a diff")
        .unwrap_or_else(|| panic!("{path} has no unstaged diff"))
}

/// Feed a patch to `git apply` with the given extra arguments, and fail loudly
/// with `git`'s own words if it refuses.
fn apply(repo: &TestRepo, patch: &[u8], args: &[&str]) {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    let mut child = Command::new("git")
        .arg("apply")
        .args(args)
        .arg("-")
        .current_dir(repo.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("git is on PATH");
    child
        .stdin
        .as_mut()
        .expect("a pipe")
        .write_all(patch)
        .expect("git took the patch");
    let output = child.wait_with_output().expect("git finished");
    assert!(
        output.status.success(),
        "git apply {args:?} refused the patch:\n{}\n--- patch ---\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(patch)
    );
}

/// What `git` says is staged for a path, so the assertions read the index
/// rather than trusting the patch that wrote it.
///
/// Untrimmed, unlike `TestRepo::git`: whether a file ends with a newline is one
/// of the things a patch gets wrong.
fn staged_blob(repo: &TestRepo, path: &str) -> String {
    let output = std::process::Command::new("git")
        .args(["show", &format!(":{path}")])
        .current_dir(repo.path())
        .output()
        .expect("git is on PATH");
    assert!(output.status.success(), "git show :{path} failed");
    String::from_utf8(output.stdout).expect("utf-8 in the fixtures")
}

/// Twenty lines, so two changes can be far enough apart that their three lines
/// of context do not meet and merge them into one hunk.
fn twenty_lines(second: &str, nineteenth: &str) -> String {
    let mut lines: Vec<String> = (1..=20).map(|n| format!("line {n}")).collect();
    lines[1] = second.to_owned();
    lines[18] = nineteenth.to_owned();
    format!("{}\n", lines.join("\n"))
}

/// The hunk at `index`, or a failure that says what the diff was instead.
fn text_hunk(file: &FileDiff, index: usize) -> &omagit_git::Hunk {
    match &file.content {
        omagit_git::diff::DiffContent::Text { hunks, .. } => &hunks[index],
        other => panic!("expected a text diff, got {other:?}"),
    }
}

/// How many hunks a text diff has.
fn hunk_count(file: &FileDiff) -> usize {
    match &file.content {
        omagit_git::diff::DiffContent::Text { hunks, .. } => hunks.len(),
        other => panic!("expected a text diff, got {other:?}"),
    }
}

/// Where a line with exactly this text sits in the hunk.
fn line_at(hunk: &omagit_git::Hunk, text: &[u8]) -> usize {
    hunk.lines
        .iter()
        .position(|line| line.text == text)
        .unwrap_or_else(|| panic!("{} is not in the hunk", String::from_utf8_lossy(text)))
}

/// A file with two changes far enough apart to be two hunks.
fn two_hunk_repo() -> TestRepo {
    let repo = TestRepo::new();
    repo.commit_file("file.txt", &twenty_lines("line 2", "line 19"), "base");
    repo.write("file.txt", &twenty_lines("SECOND", "NINETEENTH"));
    repo
}

#[test]
fn a_single_hunk_stages_without_touching_the_other() {
    let repo = two_hunk_repo();
    let file = unstaged(&repo, "file.txt");
    // Asserted rather than assumed: with the changes any closer their context
    // meets and git emits one hunk, and this test would quietly be testing
    // whole-file staging instead.
    assert_eq!(hunk_count(&file), 2, "the fixture has to produce two hunks");

    let patch = build(&file, &Selection::hunk(0), Direction::Forward).expect("a patch");
    apply(&repo, &patch, &["--cached"]);

    assert_eq!(
        staged_blob(&repo, "file.txt"),
        twenty_lines("SECOND", "line 19"),
        "only the first hunk should have been staged"
    );
    assert_eq!(
        String::from_utf8(repo.read("file.txt")).expect("utf-8"),
        twenty_lines("SECOND", "NINETEENTH"),
        "staging must not touch the working tree"
    );
}

#[test]
fn both_hunks_stage_with_headers_that_agree_with_each_other() {
    // Two hunks in one patch is where a header computed per-hunk rather than
    // cumulatively goes wrong, and `git apply` is the thing that notices.
    let repo = two_hunk_repo();
    let file = unstaged(&repo, "file.txt");

    assert_eq!(hunk_count(&file), 2);
    let patch = build(&file, &Selection::File, Direction::Forward).expect("a patch");
    apply(&repo, &patch, &["--cached"]);

    assert_eq!(
        staged_blob(&repo, "file.txt"),
        twenty_lines("SECOND", "NINETEENTH")
    );
}

#[test]
fn a_line_out_of_a_hunk_stages_alone() {
    // The differentiator of SPEC §11. Two separate changes inside one hunk, so
    // each is its own block and the selection is unambiguous: one is staged,
    // the other stays out of the index and stays in the working tree.
    let repo = TestRepo::new();
    repo.commit_file("file.txt", "a\nb\nc\nd\ne\n", "base");
    repo.write("file.txt", "a\nB\nc\nD\ne\n");

    let file = unstaged(&repo, "file.txt");
    let hunk = text_hunk(&file, 0);
    let selection = Selection::Lines(BTreeSet::from([
        (0, line_at(hunk, b"b")),
        (0, line_at(hunk, b"B")),
    ]));

    let patch = build(&file, &selection, Direction::Forward).expect("a patch");
    apply(&repo, &patch, &["--cached"]);

    assert_eq!(
        staged_blob(&repo, "file.txt"),
        "a\nB\nc\nd\ne\n",
        "only the b→B change should be staged"
    );
    assert_eq!(
        String::from_utf8(repo.read("file.txt")).expect("utf-8"),
        "a\nB\nc\nD\ne\n",
        "the working tree keeps both changes"
    );
}

#[test]
fn selecting_inside_a_rewritten_block_follows_gits_own_rules() {
    // Worth pinning, because the result surprises people and is not a bug.
    //
    // A unified diff groups every removal of a block before every addition, so
    // `a b c` → `A B C` arrives as `-a -b -c +A +B +C` with no record of which
    // addition replaced which removal. Selecting "remove b" and "add B" can
    // therefore only produce: keep a, remove b, keep c, then add B — because
    // "after the removals" is the only position the diff actually states.
    //
    // This is exactly the transformation `git add -p` prints when it opens a
    // hunk for manual editing ("to remove '-' lines, make them ' ' lines; to
    // remove '+' lines, delete them"), so omagit and the command line agree.
    let repo = TestRepo::new();
    repo.commit_file("file.txt", "a\nb\nc\nd\n", "base");
    repo.write("file.txt", "A\nB\nC\nd\n");

    let file = unstaged(&repo, "file.txt");
    let hunk = text_hunk(&file, 0);
    let selection = Selection::Lines(BTreeSet::from([
        (0, line_at(hunk, b"b")),
        (0, line_at(hunk, b"B")),
    ]));

    let patch = build(&file, &selection, Direction::Forward).expect("a patch");
    apply(&repo, &patch, &["--cached"]);

    assert_eq!(
        staged_blob(&repo, "file.txt"),
        "a\nc\nB\nd\n",
        "the addition lands after the block's removals, which is where the diff puts it"
    );
}

#[test]
fn a_file_with_no_final_newline_keeps_it_missing() {
    // The marker `git` writes as `\ No newline at end of file`. Dropped from a
    // patch, `git apply` silently adds a newline the file never had.
    let repo = TestRepo::new();
    repo.commit_file("file.txt", "one\ntwo\n", "base");
    repo.write_bytes("file.txt", b"one\nCHANGED");

    let file = unstaged(&repo, "file.txt");
    let patch = build(&file, &Selection::File, Direction::Forward).expect("a patch");
    apply(&repo, &patch, &["--cached"]);

    assert_eq!(
        staged_blob(&repo, "file.txt"),
        "one\nCHANGED",
        "the staged blob must end without a newline"
    );
}

#[test]
fn discarding_a_hunk_reverses_it_in_the_working_tree() {
    let repo = two_hunk_repo();
    let file = unstaged(&repo, "file.txt");

    // Reverse: the patch describes the working tree, and applying it backwards
    // undoes the selection.
    assert_eq!(hunk_count(&file), 2);
    let patch = build(&file, &Selection::hunk(1), Direction::Reverse).expect("a patch");
    apply(&repo, &patch, &["--reverse"]);

    assert_eq!(
        String::from_utf8(repo.read("file.txt")).expect("utf-8"),
        twenty_lines("SECOND", "line 19"),
        "the second change is gone, the first stays"
    );
}

#[test]
fn a_new_file_is_created_by_its_patch() {
    let repo = TestRepo::new();
    repo.commit_file("kept.txt", "kept\n", "base");
    repo.write("new.txt", "fresh\ncontent\n");

    let file = unstaged(&repo, "new.txt");
    let patch = build(&file, &Selection::File, Direction::Forward).expect("a patch");
    apply(&repo, &patch, &["--cached"]);

    assert_eq!(staged_blob(&repo, "new.txt"), "fresh\ncontent\n");
}

#[test]
fn a_selection_that_matches_nothing_is_refused_here_rather_than_by_git() {
    let repo = two_hunk_repo();
    let file = unstaged(&repo, "file.txt");

    let empty = Selection::Lines(BTreeSet::new());
    assert!(
        build(&file, &empty, Direction::Forward).is_err(),
        "an empty patch must not reach `git apply`, whose refusal reads as a Git failure"
    );

    // A hunk index that does not exist is the same mistake by another route.
    let missing = Selection::hunk(42);
    assert!(build(&file, &missing, Direction::Forward).is_err());
}

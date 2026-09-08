//! Diffs against real repositories: which files, which lines, which words, and
//! the shapes that have to degrade instead of freezing (SPEC §11, §13).

mod support;

use omagit_git::diff::{DiffContent, FileChange, staged_file, unstaged_file};
use omagit_git::{Diff, DiffOptions, LineKind, Status, StatusOptions};
use support::{TestRepo, never, scripted};

fn head_diff(fixture: &TestRepo) -> Diff {
    Diff::commit(
        &fixture.open(),
        fixture.head(),
        DiffOptions::default(),
        &never(),
    )
    .expect("a diff")
}

fn status(fixture: &TestRepo) -> Status {
    Status::load(&fixture.open(), StatusOptions::default(), &never()).expect("a status")
}

fn file<'a>(diff: &'a Diff, path: &str) -> &'a omagit_git::FileDiff {
    diff.files
        .iter()
        .find(|file| file.path.as_bytes() == path.as_bytes())
        .unwrap_or_else(|| {
            panic!(
                "no diff for {path:?}; the diff has {:?}",
                diff.files
                    .iter()
                    .map(|file| file.path.to_string())
                    .collect::<Vec<_>>()
            )
        })
}

fn lines(content: &DiffContent, kind: LineKind) -> Vec<String> {
    match content {
        DiffContent::Text { hunks, .. } => hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .filter(|line| line.kind == kind)
            .map(|line| line.text_lossy().into_owned())
            .collect(),
        other => panic!("expected a text diff, got {other:?}"),
    }
}

#[test]
fn a_root_commit_adds_every_file() {
    let fixture = TestRepo::new();
    fixture.write("a.txt", "one\ntwo\n");
    fixture.write("b.txt", "three\n");
    fixture.commit("first");

    let diff = head_diff(&fixture);
    assert_eq!(diff.files.len(), 2);
    assert_eq!(file(&diff, "a.txt").change, FileChange::Added);
    assert_eq!(
        lines(&file(&diff, "a.txt").content, LineKind::Added),
        vec!["one", "two"],
        "a root commit is diffed against the empty tree"
    );
}

#[test]
fn shows_the_lines_a_commit_changed() {
    let fixture = TestRepo::new();
    fixture.commit_file("f.txt", "a\nb\nc\nd\ne\n", "first");
    fixture.commit_file("f.txt", "a\nb\nCHANGED\nd\ne\n", "second");

    let diff = head_diff(&fixture);
    let file = file(&diff, "f.txt");
    assert_eq!(file.change, FileChange::Modified);
    assert_eq!(lines(&file.content, LineKind::Removed), vec!["c"]);
    assert_eq!(lines(&file.content, LineKind::Added), vec!["CHANGED"]);

    let DiffContent::Text {
        hunks,
        added,
        removed,
    } = &file.content
    else {
        panic!("expected text");
    };
    assert_eq!((*added, *removed), (1, 1));
    assert_eq!(hunks.len(), 1);
    let changed = hunks[0]
        .lines
        .iter()
        .find(|line| line.kind == LineKind::Added)
        .expect("one added line");
    assert_eq!(
        (changed.old_number, changed.new_number),
        (None, Some(3)),
        "the gutter needs both columns, and an added line has no old number"
    );
}

#[test]
fn a_deletion_is_a_deletion() {
    let fixture = TestRepo::new();
    fixture.commit_file("gone.txt", "content\n", "first");
    fixture.git(&["rm", "-q", "gone.txt"]);
    fixture.commit_staged("second");

    let diff = head_diff(&fixture);
    assert_eq!(file(&diff, "gone.txt").change, FileChange::Deleted);
    assert_eq!(
        lines(&file(&diff, "gone.txt").content, LineKind::Removed),
        vec!["content"]
    );
}

#[test]
fn detects_a_rename_in_a_commit() {
    let fixture = TestRepo::new();
    let text: String = (1..=20).map(|n| format!("line {n}\n")).collect();
    fixture.commit_file("old/name.txt", &text, "first");
    fixture.mkdir("new");
    fixture.git(&["mv", "old/name.txt", "new/name.txt"]);
    fixture.commit_staged("rename");

    let diff = head_diff(&fixture);
    assert_eq!(
        diff.files.len(),
        1,
        "a rename is one row — not two, and not three with the directory that moved with it"
    );
    assert_eq!(
        diff.files[0].change,
        FileChange::Renamed {
            from: "old/name.txt".into()
        }
    );
}

#[test]
fn a_mode_change_alone_has_no_lines() {
    let fixture = TestRepo::new();
    fixture.commit_file("script.sh", "#!/bin/sh\necho hi\n", "first");
    fixture.git(&["update-index", "--chmod=+x", "script.sh"]);
    fixture.commit_staged("make it executable");

    let diff = head_diff(&fixture);
    let file = file(&diff, "script.sh");
    assert_eq!(file.change, FileChange::ModeChanged);
    assert!(
        matches!(file.content, DiffContent::Empty),
        "nothing changed inside the file, so there is nothing to render"
    );
}

#[test]
fn compares_two_commits_directly() {
    let fixture = TestRepo::new();
    let first = fixture.commit_file("f.txt", "one\n", "first");
    fixture.commit_file("f.txt", "two\n", "second");
    let third = fixture.commit_file("f.txt", "three\n", "third");

    let diff = Diff::between(
        &fixture.open(),
        Some(first),
        Some(third),
        DiffOptions::default(),
        &never(),
    )
    .expect("a diff");
    assert_eq!(
        lines(&file(&diff, "f.txt").content, LineKind::Removed),
        vec!["one"]
    );
    assert_eq!(
        lines(&file(&diff, "f.txt").content, LineKind::Added),
        vec!["three"],
        "the commit in between is not shown; A↔B is a comparison, not a range"
    );
}

#[test]
fn refines_the_words_that_changed_in_a_real_file() {
    let fixture = TestRepo::new();
    fixture.commit_file(
        "src/lib.rs",
        "pub fn total(items: &[u32]) -> u32 {\n",
        "first",
    );
    fixture.commit_file(
        "src/lib.rs",
        "pub fn total(items: &[u64]) -> u64 {\n",
        "second",
    );

    let diff = head_diff(&fixture);
    let DiffContent::Text { hunks, .. } = &file(&diff, "src/lib.rs").content else {
        panic!("expected text");
    };
    let added = hunks[0]
        .lines
        .iter()
        .find(|line| line.kind == LineKind::Added)
        .expect("one added line");
    let marked: Vec<String> = added
        .refinements
        .iter()
        .map(|span| String::from_utf8_lossy(&added.text[span.start..span.end]).into_owned())
        .collect();
    assert_eq!(marked, vec!["u64", "u64"], "only the type changed");
}

#[test]
fn a_binary_file_is_reported_not_rendered() {
    let fixture = TestRepo::new();
    fixture.write_bytes("logo.png", b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR");
    fixture.commit("add a binary");
    fixture.write_bytes("logo.png", b"\x89PNG\r\n\x1a\n\0\0\0\x0eIHDR!");
    fixture.commit("change it");

    match &file(&head_diff(&fixture), "logo.png").content {
        DiffContent::Binary {
            old_bytes,
            new_bytes,
        } => assert!(*old_bytes > 0 && *new_bytes > *old_bytes),
        other => panic!("expected a binary diff, got {other:?}"),
    }
}

#[test]
fn an_oversized_file_degrades_instead_of_freezing() {
    let fixture = TestRepo::new();
    let big: String = (0..60_000).map(|n| format!("line {n}\n")).collect();
    fixture.commit_file("big.txt", &big, "first");
    fixture.commit_file("big.txt", &big.replace("line 0\n", "LINE 0\n"), "second");

    let diff = Diff::commit(
        &fixture.open(),
        fixture.head(),
        DiffOptions {
            max_bytes: 64 * 1024,
            ..DiffOptions::default()
        },
        &never(),
    )
    .expect("a diff");
    assert!(matches!(
        file(&diff, "big.txt").content,
        DiffContent::Oversized { .. }
    ));

    // And with the real threshold it is diffed normally: the degradation is a
    // limit, not a policy about large files.
    assert!(matches!(
        file(&head_diff(&fixture), "big.txt").content,
        DiffContent::Text { .. }
    ));
}

#[test]
fn a_one_megabyte_line_does_not_hang() {
    // SPEC §13 names it: a single line with no newline in a megabyte is where a
    // line-based diff that assumes short lines falls over.
    let fixture = TestRepo::new();
    let long = "x".repeat(1024 * 1024);
    fixture.commit_file("long.txt", &long, "first");
    fixture.commit_file("long.txt", &format!("{long}y"), "second");

    let started = std::time::Instant::now();
    let diff = Diff::commit(
        &fixture.open(),
        fixture.head(),
        DiffOptions {
            max_bytes: 8 * 1024 * 1024,
            ..DiffOptions::default()
        },
        &never(),
    )
    .expect("a diff");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "took {:?}",
        started.elapsed()
    );

    let file = file(&diff, "long.txt");
    let DiffContent::Text { hunks, .. } = &file.content else {
        panic!("expected text, got {:?}", file.content);
    };
    let last = hunks[0].lines.last().expect("a line");
    assert!(last.no_newline_at_eof);
}

#[test]
fn diffs_what_is_staged_and_what_is_not_separately() {
    let fixture = scripted();
    fixture.write("README.md", "staged version\n");
    fixture.add("README.md");
    fixture.write("README.md", "working version\n");

    let repo = fixture.open();
    let status = status(&fixture);
    let options = DiffOptions::default();

    let staged = Diff::staged(&repo, &status, options, &never()).expect("a staged diff");
    assert_eq!(
        lines(&file(&staged, "README.md").content, LineKind::Added),
        vec!["staged version"]
    );

    let unstaged = Diff::unstaged(&repo, &status, options, &never()).expect("an unstaged diff");
    assert_eq!(
        lines(&file(&unstaged, "README.md").content, LineKind::Removed),
        vec!["staged version"],
        "the unstaged half starts where the staged half ended"
    );
    assert_eq!(
        lines(&file(&unstaged, "README.md").content, LineKind::Added),
        vec!["working version"]
    );
}

#[test]
fn an_untracked_file_diffs_as_an_addition() {
    let fixture = scripted();
    fixture.write("new.txt", "brand new\n");

    let repo = fixture.open();
    let status = status(&fixture);
    let entry = status
        .entries
        .iter()
        .find(|entry| entry.path.as_bytes() == b"new.txt")
        .expect("the untracked file");

    let diff = unstaged_file(&repo, entry, DiffOptions::default())
        .expect("a diff")
        .expect("an untracked file has content to show");
    assert_eq!(diff.change, FileChange::Added);
    assert_eq!(lines(&diff.content, LineKind::Added), vec!["brand new"]);
}

#[test]
fn an_ignored_file_has_no_diff_to_show() {
    let fixture = TestRepo::new();
    fixture.write(".gitignore", "*.log\n");
    fixture.commit("ignore logs");
    fixture.write("run.log", "noise\n");

    let repo = fixture.open();
    let status = Status::load(
        &repo,
        StatusOptions {
            include_ignored: true,
            ..StatusOptions::default()
        },
        &never(),
    )
    .expect("a status");
    let entry = status
        .entries
        .iter()
        .find(|entry| entry.is_ignored() && entry.path.as_bytes() == b"run.log")
        .expect("the ignored file");

    assert!(
        unstaged_file(&repo, entry, DiffOptions::default())
            .expect("no error")
            .is_none(),
        "there is nothing to compare an untracked, ignored file against"
    );
}

#[test]
fn an_entry_with_nothing_staged_produces_no_staged_diff() {
    let fixture = scripted();
    fixture.write("src/main.rs", "fn main() { println!() }\n");

    let repo = fixture.open();
    let status = status(&fixture);
    let entry = &status.entries[0];
    assert!(entry.staged.is_none());
    assert!(
        staged_file(&repo, entry, DiffOptions::default())
            .expect("no error")
            .is_none(),
        "a caller can walk a whole status without filtering it first"
    );
}

#[test]
fn context_is_adjustable() {
    let fixture = TestRepo::new();
    let text: String = (1..=20).map(|n| format!("line {n}\n")).collect();
    fixture.commit_file("f.txt", &text, "first");
    fixture.commit_file("f.txt", &text.replace("line 10\n", "LINE 10\n"), "second");

    for context in [0, 1, 3, 6] {
        let diff = Diff::commit(
            &fixture.open(),
            fixture.head(),
            DiffOptions {
                context_lines: context,
                ..DiffOptions::default()
            },
            &never(),
        )
        .expect("a diff");
        let count = lines(&file(&diff, "f.txt").content, LineKind::Context).len();
        assert_eq!(
            count,
            context as usize * 2,
            "{context} lines of context on each side of the change"
        );
    }
}

#[test]
fn a_diff_can_be_cancelled() {
    let fixture = scripted();
    let cancel = omagit_git::Cancel::new();
    cancel.cancel();

    let error = Diff::commit(
        &fixture.open(),
        fixture.head(),
        DiffOptions::default(),
        &cancel,
    )
    .expect_err("a cancelled diff returns");
    assert!(error.is_cancelled(), "got {error:?}");
}

#[test]
fn hunk_headers_match_git_diff() {
    // The `@@` numbers are the one part of a diff a user can check against
    // another tool at a glance, and the empty-side case (`-0,0`) is easy to get
    // wrong. So they are compared against `git diff` itself rather than against
    // an expectation written by hand.
    let fixture = TestRepo::new();
    let twenty: String = (1..=20).map(|n| format!("line {n}\n")).collect();
    fixture.write("edited.txt", &twenty);
    fixture.write("removed-from.txt", &twenty);
    fixture.commit("first");

    fixture.write("added.txt", "brand new\nsecond line\n");
    fixture.write(
        "edited.txt",
        &twenty
            .replace("line 2\n", "LINE 2\n")
            .replace("line 18\n", "LINE 18\n"),
    );
    fixture.write("removed-from.txt", &twenty.replace("line 1\n", ""));
    fixture.commit("second");

    let from_git: Vec<String> = fixture
        .git(&["diff", "-U3", "HEAD~1", "HEAD"])
        .lines()
        .filter(|line| line.starts_with("@@"))
        // git appends the enclosing function to the header; the numbers are
        // what this compares.
        .map(|line| {
            line.split("@@")
                .nth(1)
                .unwrap_or_default()
                .trim()
                .to_owned()
        })
        .collect();

    let ours: Vec<String> = match_hunks(&head_diff(&fixture));
    assert_eq!(ours, from_git);
}

fn match_hunks(diff: &Diff) -> Vec<String> {
    let mut out = Vec::new();
    for file in &diff.files {
        if let DiffContent::Text { hunks, .. } = &file.content {
            for hunk in hunks {
                out.push(
                    hunk.header()
                        .trim_start_matches("@@")
                        .trim_end_matches("@@")
                        .trim()
                        .to_owned(),
                );
            }
        }
    }
    out
}

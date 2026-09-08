//! The questions SPEC §8 says M2 must answer before the `gix` / `git` split is
//! fixed, each answered by a test rather than by a reading of the changelog
//! (SPEC §3 rule 1).
//!
//! The answers, and what they cost, are written up in
//! `docs/notes/gitoxide-capabilities.md`. When one of these tests starts
//! failing after a `gix` bump, that note is what needs revisiting — not just
//! the test.

mod support;

use std::time::Instant;

use omagit_git::diff::{DiffContent, FileChange};
use omagit_git::{Diff, DiffOptions, Status, StatusOptions, WorktreeChange};
use support::{TestRepo, never};

fn status(fixture: &TestRepo) -> Status {
    Status::load(&fixture.open(), StatusOptions::default(), &never()).expect("a status")
}

/// Question 4a — `.gitattributes` `text` / `eol`.
///
/// A repository that checks out CRLF and stores LF must read as clean. Without
/// the conversion, every line of every file shows as modified, which is the
/// single most visible way a Git client can be wrong on a mixed-platform team.
#[test]
fn eol_conversion_is_applied_when_reading_the_working_tree() {
    let fixture = TestRepo::new();
    fixture.write(".gitattributes", "*.txt text eol=crlf\n");
    fixture.write("notes.txt", "first\nsecond\nthird\n");
    fixture.commit("stored with LF, checked out with CRLF");

    // Re-checkout so the working tree gets the CRLF the attribute asks for.
    fixture.remove("notes.txt");
    fixture.git(&["checkout", "--", "notes.txt"]);
    assert!(
        fixture
            .read("notes.txt")
            .windows(2)
            .any(|pair| pair == b"\r\n"),
        "the fixture is only meaningful if git actually wrote CRLF"
    );

    assert!(
        status(&fixture).is_clean(),
        "a CRLF working tree over an LF index is not a change"
    );
}

/// Question 4b — clean filters.
///
/// A file with a `clean` filter differs from its blob byte for byte on disk.
/// Reporting it as modified would make a repository using `git-crypt` or a
/// keyword filter permanently dirty.
#[test]
fn clean_filters_are_applied_when_reading_the_working_tree() {
    let fixture = TestRepo::new();
    fixture.git(&["config", "filter.redact.clean", "sed s/SECRET/REDACTED/"]);
    fixture.write(".gitattributes", "*.conf filter=redact\n");
    fixture.write("app.conf", "token = SECRET\n");
    fixture.commit("store the redacted form");

    assert_eq!(
        fixture.git(&["show", "HEAD:app.conf"]),
        "token = REDACTED",
        "the fixture is only meaningful if the filter actually ran"
    );
    assert!(
        String::from_utf8_lossy(&fixture.read("app.conf")).contains("SECRET"),
        "and if the working tree still holds the unfiltered text"
    );

    assert!(
        status(&fixture).is_clean(),
        "the filter's output is what the index holds, so nothing changed"
    );

    // And the unstaged diff of a file that *did* change compares filtered
    // against filtered, rather than showing the filter's own work as an edit.
    fixture.write("app.conf", "token = SECRET\nport = 8080\n");
    let repo = fixture.open();
    let status = status(&fixture);
    let diff = Diff::unstaged(&repo, &status, DiffOptions::default(), &never()).expect("a diff");
    let DiffContent::Text { hunks, .. } = &diff.files[0].content else {
        panic!("expected a text diff, got {:?}", diff.files[0].content);
    };
    let added: Vec<String> = hunks
        .iter()
        .flat_map(|hunk| &hunk.lines)
        .filter(|line| line.kind == omagit_git::LineKind::Added)
        .map(|line| line.text_lossy().into_owned())
        .collect();
    assert_eq!(added, vec!["port = 8080"], "only the real edit shows");
}

/// Question 5 — submodules.
///
/// Out of MVP scope (SPEC §11), which makes it *more* important that they are
/// reported rather than skipped: a file list that silently omits a directory is
/// worse than one that shows it and offers nothing to do with it.
#[test]
fn a_submodule_is_reported_as_its_own_kind() {
    let inner = TestRepo::new();
    inner.commit_file("inner.txt", "inner\n", "inner first");

    let outer = TestRepo::new();
    outer.commit_file("outer.txt", "outer\n", "outer first");
    // Local paths as submodule URLs are refused by default since the 2.38
    // security fixes; the tests are not a threat model.
    outer.git(&[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        "--",
        &inner.path().to_string_lossy(),
        "vendor/inner",
    ]);
    outer.commit_staged("add the submodule");

    // A commit inside the submodule leaves the superproject pointing at an
    // older commit than the one checked out.
    inner.commit_file("inner.txt", "inner, changed\n", "inner second");
    outer.git(&[
        "-c",
        "protocol.file.allow=always",
        "-C",
        "vendor/inner",
        "fetch",
        "origin",
    ]);
    outer.git(&[
        "-C",
        "vendor/inner",
        "checkout",
        "-q",
        &inner.head().to_string(),
    ]);

    let status = status(&outer);
    let entry = status
        .entries
        .iter()
        .find(|entry| entry.path.as_bytes() == b"vendor/inner")
        .unwrap_or_else(|| {
            panic!(
                "the submodule is missing from {:?}",
                status
                    .entries
                    .iter()
                    .map(|entry| entry.path.to_string())
                    .collect::<Vec<_>>()
            )
        });
    assert!(entry.is_submodule, "it is a gitlink, not a file");
    assert_eq!(entry.unstaged, Some(WorktreeChange::Modified));

    // And its diff says "a submodule", not a wall of lines from another
    // repository's files.
    let diff = Diff::commit(
        &outer.open(),
        outer.head(),
        DiffOptions::default(),
        &never(),
    )
    .expect("a diff");
    let submodule = diff
        .files
        .iter()
        .find(|file| file.path.as_bytes() == b"vendor/inner")
        .expect("the submodule was added by this commit");
    assert_eq!(submodule.change, FileChange::Added);
    assert!(matches!(submodule.content, DiffContent::Submodule { .. }));
}

/// Question 6 — Unicode normalisation.
///
/// macOS hands back decomposed names (NFD) where Git stores what it was given
/// (usually NFC). `core.precomposeUnicode` is `gix`'s to honour, and this is the
/// test that says whether it does: on macOS the fixture is genuinely
/// decomposed by the filesystem, and on Linux it is a plain byte-for-byte name.
/// Either way the entry has to match its index entry rather than showing up as
/// one deletion plus one untracked file.
#[test]
fn an_accented_name_matches_its_index_entry() {
    let fixture = TestRepo::new();
    fixture.commit_file("réservé/été.txt", "chaud\n", "first");

    assert!(
        status(&fixture).is_clean(),
        "the name written to disk is the name the index holds"
    );

    fixture.write("réservé/été.txt", "froid\n");
    let status = status(&fixture);
    assert_eq!(status.entries.len(), 1, "one modification, not a rename");
    assert_eq!(status.entries[0].unstaged, Some(WorktreeChange::Modified));
}

/// Question 2 — rename detection in diffs, and what it costs.
///
/// Renaming a hundred files at once is the shape that makes similarity
/// detection quadratic if it is done naively.
#[test]
fn rename_detection_scales_to_a_hundred_files_at_once() {
    let fixture = TestRepo::new();
    let body: String = (1..=50).map(|n| format!("line {n}\n")).collect();
    for n in 0..100 {
        fixture.write(
            &format!("before/file{n}.txt"),
            &format!("{body}unique {n}\n"),
        );
    }
    fixture.commit("a hundred files");
    fixture.mkdir("after");
    for n in 0..100 {
        fixture.git(&[
            "mv",
            &format!("before/file{n}.txt"),
            &format!("after/file{n}.txt"),
        ]);
    }
    fixture.commit_staged("move them all");

    let started = Instant::now();
    let diff = Diff::commit(
        &fixture.open(),
        fixture.head(),
        DiffOptions::default(),
        &never(),
    )
    .expect("a diff");
    let elapsed = started.elapsed();

    let renames = diff
        .files
        .iter()
        .filter(|file| matches!(file.change, FileChange::Renamed { .. }))
        .count();
    assert_eq!(
        renames, 100,
        "every move is a rename, not a delete plus an add"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "rename detection over 100 files took {elapsed:?}"
    );
}

/// Question 1 — `status` on 50 000 files, inside the 300 ms of SPEC §12.
///
/// Ignored by default: building the fixture writes 50 000 files and takes far
/// longer than the measurement itself, which would make every CI run pay for a
/// number that only matters when `gix` changes. Run it deliberately:
///
/// ```sh
/// cargo test -p omagit-git --release -- --ignored --nocapture
/// ```
///
/// The figures it printed, and on what machine, are in
/// `docs/notes/gitoxide-capabilities.md`.
#[test]
#[ignore = "writes 50 000 files; run with --ignored when checking SPEC §12"]
fn status_of_fifty_thousand_files() {
    const FILES: usize = 50_000;
    let fixture = TestRepo::new();
    for directory in 0..100 {
        for file in 0..(FILES / 100) {
            fixture.write(
                &format!("d{directory:03}/f{file:03}.txt"),
                "some content that is long enough to be worth hashing\n",
            );
        }
    }
    fixture.commit("fifty thousand files");
    let repo = fixture.open();

    // Once to warm the index and the filesystem cache — the second run is the
    // one a user experiences, the first is the one they pay for at start-up.
    let cold = Instant::now();
    let first = Status::load(&repo, StatusOptions::default(), &never()).expect("a status");
    let cold = cold.elapsed();
    assert!(first.is_clean());

    let warm = Instant::now();
    let second = Status::load(&repo, StatusOptions::default(), &never()).expect("a status");
    let warm = warm.elapsed();
    assert!(second.is_clean());

    println!("status of {FILES} files: cold {cold:?}, warm {warm:?}");
    assert!(
        warm < std::time::Duration::from_millis(300),
        "SPEC §12 budgets 300 ms for a status of 50 000 files; this took {warm:?}"
    );
}

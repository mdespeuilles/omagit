//! The working copy, in every state SPEC §11 says the screen has to show.

mod support;

use omagit_git::status::short_code;
use omagit_git::{Conflict, StageChange, Status, StatusEntry, StatusOptions, WorktreeChange};
use support::{TestRepo, never, scripted};

fn status(fixture: &TestRepo) -> Status {
    Status::load(&fixture.open(), StatusOptions::default(), &never()).expect("a status")
}

fn with(fixture: &TestRepo, options: StatusOptions) -> Status {
    Status::load(&fixture.open(), options, &never()).expect("a status")
}

fn entry<'a>(status: &'a Status, path: &str) -> &'a StatusEntry {
    status
        .entries
        .iter()
        .find(|entry| entry.path.as_bytes() == path.as_bytes())
        .unwrap_or_else(|| {
            panic!(
                "no entry for {path:?}; the status has {:?}",
                status
                    .entries
                    .iter()
                    .map(|entry| entry.path.to_string())
                    .collect::<Vec<_>>()
            )
        })
}

#[test]
fn a_committed_tree_is_clean() {
    let fixture = scripted();
    assert!(
        status(&fixture).is_clean(),
        "nothing was touched after the merge"
    );
}

#[test]
fn reports_untracked_files_one_by_one() {
    let fixture = scripted();
    fixture.write("new.txt", "new\n");
    fixture.write("dir/nested.txt", "nested\n");

    let status = status(&fixture);
    assert!(entry(&status, "new.txt").is_untracked());
    assert!(
        status
            .entries
            .iter()
            .any(|entry| entry.path.as_bytes() == b"dir/nested.txt"),
        "an untracked directory is listed as its files, not collapsed into one row"
    );
}

#[test]
fn separates_what_is_staged_from_what_is_not() {
    let fixture = scripted();
    fixture.write("README.md", "staged\n");
    fixture.add("README.md");
    fixture.write("README.md", "staged, then edited again\n");

    let status = status(&fixture);
    let readme = entry(&status, "README.md");
    assert_eq!(readme.staged, Some(StageChange::Modified));
    assert_eq!(readme.unstaged, Some(WorktreeChange::Modified));
    assert_eq!(short_code(readme), "MM", "one row, both columns");
}

#[test]
fn reports_additions_and_deletions_on_both_sides() {
    let fixture = scripted();
    fixture.write("added.txt", "added\n");
    fixture.add("added.txt");
    fixture.git(&["rm", "--cached", "-q", "README.md"]);
    fixture.remove("src/main.rs");

    let status = status(&fixture);
    assert_eq!(entry(&status, "added.txt").staged, Some(StageChange::Added));
    assert_eq!(
        entry(&status, "README.md").staged,
        Some(StageChange::Deleted),
        "removed from the index but still on disk"
    );
    assert_eq!(
        entry(&status, "src/main.rs").unstaged,
        Some(WorktreeChange::Deleted)
    );
}

#[test]
fn detects_a_staged_rename() {
    let fixture = scripted();
    fixture.git(&["mv", "src/main.rs", "src/entry.rs"]);

    let status = status(&fixture);
    let renamed = entry(&status, "src/entry.rs");
    assert_eq!(
        renamed.staged,
        Some(StageChange::Renamed {
            from: "src/main.rs".into()
        }),
        "a move is one row, not a deletion plus an addition"
    );
    assert!(
        status
            .entries
            .iter()
            .all(|entry| entry.path.as_bytes() != b"src/main.rs"),
        "the source must not also appear as a deletion"
    );
}

#[test]
fn detects_a_rename_made_outside_git() {
    let fixture = scripted();
    let contents = String::from_utf8(fixture.read("src/lib.rs")).expect("utf-8 fixture");
    fixture.remove("src/lib.rs");
    fixture.write("src/library.rs", &contents);

    let status = status(&fixture);
    assert_eq!(
        entry(&status, "src/library.rs").unstaged,
        Some(WorktreeChange::Renamed {
            from: "src/lib.rs".into()
        })
    );
}

#[test]
fn a_rename_that_changes_only_case_is_a_rename() {
    // SPEC §9: APFS is case-insensitive and Git is not. This has to work on
    // both platforms, and it is the reason path handling is centralised.
    let fixture = TestRepo::new();
    fixture.commit_file("Readme.md", "text\n", "first");
    fixture.git(&["mv", "-f", "Readme.md", "README.md"]);

    let status = status(&fixture);
    assert_eq!(
        entry(&status, "README.md").staged,
        Some(StageChange::Renamed {
            from: "Readme.md".into()
        })
    );
}

#[test]
fn handles_names_with_spaces_and_accents() {
    let fixture = TestRepo::new();
    fixture.commit_file("notes/café menu.txt", "expresso\n", "first");
    fixture.write("notes/café menu.txt", "expresso, allongé\n");

    let status = status(&fixture);
    assert_eq!(
        entry(&status, "notes/café menu.txt").unstaged,
        Some(WorktreeChange::Modified),
        "an accented name matches the index entry it came from"
    );
}

// APFS rejects file names that are not valid UTF-8, so this fixture cannot be
// created on macOS at all. The path type is byte-based everywhere regardless —
// see `paths::tests::holds_paths_that_are_not_utf8` — and this proves the whole
// read path carries those bytes through where the filesystem allows them.
#[cfg(target_os = "linux")]
#[test]
fn carries_a_name_that_is_not_utf8() {
    use std::os::unix::ffi::OsStrExt;

    let fixture = TestRepo::new();
    fixture.commit_file("anchor.txt", "anchor\n", "first");
    let name = std::ffi::OsStr::from_bytes(b"caf\xe9.txt");
    std::fs::write(fixture.path().join(name), b"latin-1\n").expect("a writable name");

    let status = status(&fixture);
    let entry = status
        .entries
        .iter()
        .find(|entry| entry.path.as_bytes() == b"caf\xe9.txt")
        .expect("the undecodable name survived the read");
    assert!(entry.is_untracked());
    assert!(
        entry.path.display_lossy().contains('\u{fffd}'),
        "it can still be named in a message"
    );
}

#[test]
fn ignored_files_are_listed_only_when_asked_for() {
    let fixture = TestRepo::new();
    fixture.write(".gitignore", "build/\n*.log\n");
    fixture.commit("ignore build output");
    fixture.write("build/artifact.bin", "binary\n");
    fixture.write("run.log", "log\n");

    let default = status(&fixture);
    assert!(
        !default.entries.iter().any(StatusEntry::is_ignored),
        "the default status is `git status`'s, which hides them"
    );

    let with_ignored = with(
        &fixture,
        StatusOptions {
            include_ignored: true,
            ..StatusOptions::default()
        },
    );
    let ignored: Vec<String> = with_ignored
        .entries
        .iter()
        .filter(|entry| entry.is_ignored())
        .map(|entry| entry.path.to_string())
        .collect();
    assert!(
        ignored.contains(&"run.log".to_string()),
        "an ignored file is listed: {ignored:?}"
    );
    // An ignored *directory* is one row, not one per file inside it — the same
    // answer `git status --ignored` gives, and the only one that survives a
    // `node_modules`.
    let build = entry(&with_ignored, "build");
    assert!(build.is_ignored() && build.is_directory);
}

#[test]
fn untracked_files_can_be_left_out() {
    let fixture = scripted();
    fixture.write("scratch.txt", "scratch\n");

    let status = with(
        &fixture,
        StatusOptions {
            include_untracked: false,
            ..StatusOptions::default()
        },
    );
    assert!(status.is_clean(), "only untracked files changed");
}

#[test]
fn reports_a_conflict_and_which_side_did_what() {
    let fixture = TestRepo::new();
    fixture.commit_file("shared.txt", "base\n", "base");
    fixture.branch("other");
    fixture.commit_file("shared.txt", "theirs\n", "theirs");
    fixture.checkout("main");
    fixture.commit_file("shared.txt", "ours\n", "ours");
    assert!(
        !fixture.git_allow_failure(&["merge", "other"]),
        "the fixture conflicts on purpose"
    );

    let status = status(&fixture);
    let conflicted = entry(&status, "shared.txt");
    assert_eq!(conflicted.conflict, Some(Conflict::BothModified));
    assert_eq!(short_code(conflicted), "UU");
    assert_eq!(status.conflicts().count(), 1);
}

#[test]
fn a_type_change_is_not_a_modification() {
    let fixture = TestRepo::new();
    fixture.commit_file("thing", "a regular file\n", "first");
    fixture.remove("thing");
    fixture.symlink("elsewhere", "thing");

    let status = status(&fixture);
    assert_eq!(
        entry(&status, "thing").unstaged,
        Some(WorktreeChange::TypeChanged),
        "a file that became a symlink has no line diff to show"
    );
}

#[test]
fn a_repository_with_no_commits_still_has_a_working_copy() {
    // SPEC §13's empty repository: there is no HEAD tree to compare the index
    // against, which is exactly where a status that assumes one falls over.
    let fixture = TestRepo::new();
    fixture.write("first.txt", "a start\n");
    fixture.write("staged.txt", "already staged\n");
    fixture.add("staged.txt");

    let status = status(&fixture);
    assert!(entry(&status, "first.txt").is_untracked());
    assert_eq!(
        entry(&status, "staged.txt").staged,
        Some(StageChange::Added),
        "the very first commit is all additions, against nothing"
    );
}

#[test]
fn a_bare_repository_reports_no_working_copy() {
    let fixture = TestRepo::bare();
    assert!(
        status(&fixture).is_clean(),
        "a bare repository has no work tree, and that is an empty state, not an error"
    );
}

#[test]
fn a_status_can_be_cancelled() {
    let fixture = scripted();
    fixture.write("new.txt", "new\n");
    let cancel = omagit_git::Cancel::new();
    cancel.cancel();

    let error = Status::load(&fixture.open(), StatusOptions::default(), &cancel)
        .expect_err("a cancelled read returns");
    assert!(error.is_cancelled(), "got {error:?}");
}

#[test]
fn an_intent_to_add_file_is_listed_the_way_git_lists_it() {
    // It used to vanish from the status entirely: `gix` reports an
    // intent-to-add entry only on the index→worktree side, and that arm dropped
    // it on the belief its staged half came through the tree→index comparison.
    // `git status` prints ` A`; so do we.
    let repo = TestRepo::new();
    repo.commit_file("kept.txt", "kept\n", "base");
    repo.write("new.txt", "fresh\ncontent\n");
    repo.git(&["add", "-N", "new.txt"]);

    let status = Status::load(&repo.open(), StatusOptions::default(), &never()).expect("a status");
    let entry = status
        .entries
        .iter()
        .find(|entry| entry.path.display_lossy() == "new.txt")
        .expect("the file has to be in the status at all");

    assert_eq!(entry.staged, None, "nothing is staged yet");
    assert_eq!(entry.unstaged, Some(WorktreeChange::Added));
    assert_eq!(short_code(entry), " A", "the same two letters git prints");
    assert!(
        !entry.is_untracked(),
        "it has an index entry, which is exactly what untracked does not"
    );
}

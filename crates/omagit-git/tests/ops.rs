//! The write operations, against real repositories.
//!
//! Every assertion here asks `git` what the repository looks like afterwards
//! rather than trusting the command that ran. A write that reports success and
//! did the wrong thing is the failure mode worth catching.

mod support;

use std::collections::BTreeSet;

use omagit_git::cli::Git;
use omagit_git::diff::{DiffOptions, staged_file, unstaged_file};
use omagit_git::journal::Outcome;
use omagit_git::ops::{
    CommitOptions, commit, committer_identity, discard, stage, stage_all, template, unstage,
    unstage_all,
};
use omagit_git::patch::Selection;
use omagit_git::status::{Status, StatusOptions};
use omagit_git::{FileDiff, RepoPath, Repository};
use support::{TestRepo, never};

fn git() -> Git {
    Git::detect().expect("git is installed; these tests need it")
}

fn entry_diff(repo: &Repository, path: &str, staged: bool) -> FileDiff {
    let status = Status::load(repo, StatusOptions::default(), &never()).expect("a status");
    let wanted = RepoPath::from_bytes(path.as_bytes().to_vec());
    let entry = status
        .entries
        .iter()
        .find(|entry| entry.path == wanted)
        .unwrap_or_else(|| panic!("{path} is not in the status"));
    let diff = if staged {
        staged_file(repo, entry, DiffOptions::default())
    } else {
        unstaged_file(repo, entry, DiffOptions::default())
    };
    diff.expect("a diff")
        .unwrap_or_else(|| panic!("{path} has no diff on that side"))
}

/// The staged content of a path, untrimmed.
fn staged_blob(repo: &TestRepo, path: &str) -> String {
    let output = std::process::Command::new("git")
        .args(["show", &format!(":{path}")])
        .current_dir(repo.path())
        .output()
        .expect("git is on PATH");
    assert!(output.status.success(), "git show :{path} failed");
    String::from_utf8(output.stdout).expect("utf-8 in the fixtures")
}

fn twenty_lines(second: &str, nineteenth: &str) -> String {
    let mut lines: Vec<String> = (1..=20).map(|n| format!("line {n}")).collect();
    lines[1] = second.to_owned();
    lines[18] = nineteenth.to_owned();
    format!("{}\n", lines.join("\n"))
}

#[test]
fn a_whole_file_stages_and_unstages() {
    let repo = TestRepo::new();
    repo.commit_file("file.txt", "one\n", "base");
    repo.write("file.txt", "two\n");
    let git = git();
    let opened = repo.open();

    let file = entry_diff(&opened, "file.txt", false);
    stage(&git, &opened, &file, &Selection::File, &never()).expect("staged");
    assert_eq!(staged_blob(&repo, "file.txt"), "two\n");

    let staged = entry_diff(&opened, "file.txt", true);
    unstage(&git, &opened, &staged, &Selection::File, &never()).expect("unstaged");
    assert_eq!(staged_blob(&repo, "file.txt"), "one\n", "back to HEAD");
    assert_eq!(
        String::from_utf8(repo.read("file.txt")).expect("utf-8"),
        "two\n",
        "unstaging never touches the working tree"
    );
}

#[test]
fn a_deletion_stages_through_git_add_rather_than_a_patch() {
    // The case a text patch cannot describe on its own, and the reason a whole
    // file goes through the command that names it.
    let repo = TestRepo::new();
    repo.commit_file("gone.txt", "content\n", "base");
    repo.remove("gone.txt");
    let git = git();
    let opened = repo.open();

    let file = entry_diff(&opened, "gone.txt", false);
    stage(&git, &opened, &file, &Selection::File, &never()).expect("staged");

    assert_eq!(
        repo.git(&["diff", "--cached", "--name-status"]),
        "D\tgone.txt",
        "the deletion has to be in the index"
    );
}

#[test]
fn one_hunk_stages_and_then_unstages_again() {
    let repo = TestRepo::new();
    repo.commit_file("file.txt", &twenty_lines("line 2", "line 19"), "base");
    repo.write("file.txt", &twenty_lines("SECOND", "NINETEENTH"));
    let git = git();
    let opened = repo.open();

    let file = entry_diff(&opened, "file.txt", false);
    stage(&git, &opened, &file, &Selection::hunk(0), &never()).expect("staged one hunk");
    assert_eq!(
        staged_blob(&repo, "file.txt"),
        twenty_lines("SECOND", "line 19")
    );

    // And back out again, from the staged side this time.
    let staged = entry_diff(&opened, "file.txt", true);
    unstage(&git, &opened, &staged, &Selection::hunk(0), &never()).expect("unstaged the hunk");
    assert_eq!(
        staged_blob(&repo, "file.txt"),
        twenty_lines("line 2", "line 19"),
        "the index is back at HEAD"
    );
    assert_eq!(
        String::from_utf8(repo.read("file.txt")).expect("utf-8"),
        twenty_lines("SECOND", "NINETEENTH"),
        "and the working tree still holds both changes"
    );
}

#[test]
fn discarding_a_hunk_leaves_the_other_alone() {
    let repo = TestRepo::new();
    repo.commit_file("file.txt", &twenty_lines("line 2", "line 19"), "base");
    repo.write("file.txt", &twenty_lines("SECOND", "NINETEENTH"));
    let git = git();
    let opened = repo.open();

    let file = entry_diff(&opened, "file.txt", false);
    discard(&git, &opened, &file, &Selection::hunk(1), false, &never()).expect("discarded");

    assert_eq!(
        String::from_utf8(repo.read("file.txt")).expect("utf-8"),
        twenty_lines("SECOND", "line 19"),
        "only the second change is gone"
    );
}

#[test]
fn discarding_an_untracked_file_removes_it() {
    // `git restore` has nothing to restore it to, so "discard" can only mean
    // removing it — and that has to be said, not implied.
    let repo = TestRepo::new();
    repo.commit_file("kept.txt", "kept\n", "base");
    repo.write("scratch.txt", "notes\n");
    let git = git();
    let opened = repo.open();

    let file = entry_diff(&opened, "scratch.txt", false);
    discard(&git, &opened, &file, &Selection::File, true, &never()).expect("discarded");

    assert!(
        !repo.path().join("scratch.txt").exists(),
        "the untracked file should be gone"
    );
    assert!(repo.path().join("kept.txt").exists(), "and nothing else");
}

#[test]
fn a_destructive_command_is_in_the_journal_before_it_runs() {
    // SPEC §15 risk 5. The entry has to exist, say what ran, and be marked.
    let repo = TestRepo::new();
    repo.commit_file("file.txt", "one\n", "base");
    repo.write("file.txt", "two\n");
    let git = git();
    let opened = repo.open();

    let file = entry_diff(&opened, "file.txt", false);
    discard(&git, &opened, &file, &Selection::File, false, &never()).expect("discarded");

    let entries = git.journal().entries();
    let destructive: Vec<_> = entries.iter().filter(|entry| entry.destructive).collect();
    assert_eq!(destructive.len(), 1, "exactly one destructive command ran");
    assert!(
        destructive[0].command.contains("restore --worktree"),
        "the journal records the exact command, not a description: {}",
        destructive[0].command
    );
    assert!(matches!(destructive[0].outcome, Outcome::Succeeded { .. }));
    assert_eq!(
        String::from_utf8(repo.read("file.txt")).expect("utf-8"),
        "one\n"
    );
}

#[test]
fn committing_records_the_message_and_the_new_head() {
    let repo = TestRepo::new();
    repo.commit_file("file.txt", "one\n", "base");
    repo.write("file.txt", "two\n");
    repo.add_all();
    let git = git();
    let opened = repo.open();

    let outcome = commit(
        &git,
        &opened,
        "subject line\n\nA body that spans\nseveral lines.\n",
        &CommitOptions::default(),
        &never(),
    )
    .expect("committed");

    assert_eq!(outcome.id, repo.head(), "the reported id is the new HEAD");
    assert_eq!(
        repo.git(&["log", "-1", "--pretty=%B"]).trim(),
        "subject line\n\nA body that spans\nseveral lines.",
        "the message survives its newlines"
    );
}

#[test]
fn amending_replaces_the_commit_and_says_so_in_the_journal() {
    let repo = TestRepo::new();
    repo.commit_file("file.txt", "one\n", "base");
    let before = repo.head();
    repo.write("file.txt", "two\n");
    repo.add_all();
    let git = git();
    let opened = repo.open();

    let outcome = commit(
        &git,
        &opened,
        "amended subject\n",
        &CommitOptions {
            amend: true,
            ..CommitOptions::default()
        },
        &never(),
    )
    .expect("amended");

    assert_ne!(outcome.id, before, "amending rewrites the commit");
    assert_eq!(
        repo.git(&["rev-list", "--count", "HEAD"]),
        "1",
        "and does not add one"
    );
    assert!(
        git.journal()
            .entries()
            .iter()
            .any(|entry| entry.destructive && entry.command.contains("--amend")),
        "an amend is destructive and the journal has to say so"
    );
}

#[test]
fn sign_off_adds_the_trailer() {
    let repo = TestRepo::new();
    repo.commit_file("file.txt", "one\n", "base");
    repo.write("file.txt", "two\n");
    repo.add_all();
    let git = git();
    let opened = repo.open();

    commit(
        &git,
        &opened,
        "signed\n",
        &CommitOptions {
            sign_off: true,
            ..CommitOptions::default()
        },
        &never(),
    )
    .expect("committed");

    assert!(
        repo.git(&["log", "-1", "--pretty=%B"])
            .contains("Signed-off-by:"),
        "sign-off has to reach the message"
    );
}

#[test]
fn no_verify_is_what_skips_a_hook_and_nothing_else_does() {
    let repo = TestRepo::new();
    repo.commit_file("file.txt", "one\n", "base");

    // A hook that always refuses, so "was it run" is unambiguous.
    let hook = repo.path().join(".git/hooks/pre-commit");
    std::fs::write(&hook, "#!/bin/sh\nexit 1\n").expect("wrote the hook");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755))
            .expect("made it executable");
    }

    repo.write("file.txt", "two\n");
    repo.add_all();
    let git = git();
    let opened = repo.open();

    let refused = commit(
        &git,
        &opened,
        "blocked\n",
        &CommitOptions::default(),
        &never(),
    );
    assert!(refused.is_err(), "the hook has to be able to stop a commit");

    let allowed = commit(
        &git,
        &opened,
        "forced\n",
        &CommitOptions {
            no_verify: true,
            ..CommitOptions::default()
        },
        &never(),
    );
    assert!(allowed.is_ok(), "--no-verify is what gets past it");
}

#[test]
fn a_missing_identity_is_refused_before_the_message_is_spent() {
    // SPEC §11 asks for the warning; the point of raising it here is that the
    // user has not yet written anything when it appears.
    let repo = TestRepo::new();
    repo.commit_file("file.txt", "one\n", "base");
    // An empty address rather than an unset one: unsetting the local value
    // falls through to the machine's global config, which on a developer's
    // machine is set. Empty is the state `git commit` itself refuses with
    // "empty ident", so it is the state worth testing.
    repo.git(&["config", "--local", "user.email", ""]);
    let git = git();
    let opened = repo.open();

    assert!(
        committer_identity(&git, &opened, &never())
            .expect("the lookup itself works")
            .is_none(),
        "half an identity is no identity"
    );

    repo.write("file.txt", "two\n");
    repo.add_all();
    let refused = commit(&git, &opened, "nope\n", &CommitOptions::default(), &never());
    assert!(
        refused.is_err(),
        "committing without an identity is refused"
    );
}

#[test]
fn a_commit_template_is_read_when_one_is_configured() {
    let repo = TestRepo::new();
    repo.commit_file("file.txt", "one\n", "base");
    let git = git();
    let opened = repo.open();

    assert_eq!(
        template(&git, &opened, &never()).expect("the lookup works"),
        None,
        "no template configured is not an error"
    );

    repo.write(".gitmessage", "# Why, not what.\n");
    repo.git(&["config", "commit.template", ".gitmessage"]);
    assert_eq!(
        template(&git, &opened, &never()).expect("the lookup works"),
        Some("# Why, not what.\n".to_owned())
    );
}

#[test]
fn staged_lines_stage_alone_through_the_operation() {
    // The end-to-end of SPEC §11's differentiator: from a status entry to an
    // index that holds one of two changes.
    let repo = TestRepo::new();
    repo.commit_file("file.txt", "a\nb\nc\nd\ne\n", "base");
    repo.write("file.txt", "a\nB\nc\nD\ne\n");
    let git = git();
    let opened = repo.open();

    let file = entry_diff(&opened, "file.txt", false);
    let hunk = match &file.content {
        omagit_git::diff::DiffContent::Text { hunks, .. } => &hunks[0],
        other => panic!("expected a text diff, got {other:?}"),
    };
    let at = |text: &[u8]| {
        hunk.lines
            .iter()
            .position(|line| line.text == text)
            .expect("the line is in the hunk")
    };
    let selection = Selection::Lines(BTreeSet::from([(0, at(b"b")), (0, at(b"B"))]));

    stage(&git, &opened, &file, &selection, &never()).expect("staged the lines");
    assert_eq!(staged_blob(&repo, "file.txt"), "a\nB\nc\nd\ne\n");
}

#[test]
fn part_of_a_new_file_can_be_staged_once_git_knows_its_name() {
    // What `git add -N` is *for*, and what the status bug made impossible: an
    // untracked file has no index entry, so nothing can be diffed against it
    // and no patch can be built. Marked intent-to-add, it can — and only some
    // of it goes in.
    let repo = TestRepo::new();
    repo.commit_file("kept.txt", "kept\n", "base");
    repo.write("new.txt", "one\ntwo\nthree\n");
    repo.git(&["add", "-N", "new.txt"]);
    let git = git();
    let opened = repo.open();

    let file = entry_diff(&opened, "new.txt", false);
    let hunk = match &file.content {
        omagit_git::diff::DiffContent::Text { hunks, .. } => &hunks[0],
        other => panic!("a new file has to have a text diff, got {other:?}"),
    };
    let second = hunk
        .lines
        .iter()
        .position(|line| line.text == b"two")
        .expect("the second line is in the hunk");

    let selection = Selection::Lines(BTreeSet::from([(0, second)]));
    stage(&git, &opened, &file, &selection, &never()).expect("staged one line of a new file");

    assert_eq!(
        staged_blob(&repo, "new.txt"),
        "two\n",
        "only the picked line should be in the index"
    );
    assert_eq!(
        String::from_utf8(repo.read("new.txt")).expect("utf-8"),
        "one\ntwo\nthree\n",
        "and the working tree keeps the whole file"
    );
}

#[test]
fn staging_everything_is_one_command_over_every_kind_of_change() {
    // The three shapes a per-file loop gets wrong or slow: a modification, a
    // deletion, and a file `git add <path>` would need to be told about.
    let repo = TestRepo::new();
    repo.commit_file("kept.txt", "one\n", "seed");
    repo.commit_file("gone.txt", "two\n", "seed the deletion");
    repo.write("kept.txt", "one changed\n");
    repo.remove("gone.txt");
    repo.write("new.txt", "three\n");
    repo.write(".gitignore", "ignored.txt\n");
    repo.write("ignored.txt", "not this one\n");

    stage_all(&git(), &repo.open(), &never()).expect("everything stages");

    let staged: BTreeSet<String> = repo
        .git(&["diff", "--cached", "--name-only"])
        .lines()
        .map(ToOwned::to_owned)
        .collect();
    assert!(staged.contains("kept.txt"), "a modification stages");
    assert!(staged.contains("gone.txt"), "a deletion stages");
    assert!(staged.contains("new.txt"), "an untracked file stages");
    assert!(
        !staged.contains("ignored.txt"),
        "an ignored file stays ignored: --all is not --force"
    );
}

#[test]
fn unstaging_everything_leaves_the_working_tree_alone() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "seed");
    repo.write("a.txt", "one changed\n");
    repo.write("b.txt", "two\n");
    repo.add_all();

    unstage_all(&git(), &repo.open(), &never()).expect("everything unstages");

    assert_eq!(
        repo.git(&["diff", "--cached", "--name-only"]),
        "",
        "the index is back at HEAD"
    );
    assert_eq!(
        String::from_utf8_lossy(&repo.read("a.txt")),
        "one changed\n",
        "the edit survives: unstaging is not discarding"
    );
    assert_eq!(
        String::from_utf8_lossy(&repo.read("b.txt")),
        "two\n",
        "the new file survives too, as untracked"
    );
}

#[test]
fn unstaging_everything_works_before_the_first_commit() {
    // The reason this is `git reset` and not `git restore --staged -- :/`:
    // there is no HEAD to restore against, and `restore` fails outright.
    let repo = TestRepo::new();
    repo.write("a.txt", "one\n");
    repo.add_all();

    unstage_all(&git(), &repo.open(), &never()).expect("an unborn repository unstages");

    assert_eq!(repo.git(&["status", "--short"]), "?? a.txt");
}

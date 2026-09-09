//! The shelf: what goes on it, what comes back off, and what one entry holds.

mod support;

use omagit_git::cli::Git;
use omagit_git::diff::DiffOptions;
use omagit_git::ops::stash as ops;
use omagit_git::stash;
use support::{TestRepo, never};

fn git() -> Git {
    Git::detect().expect("git is installed; these tests need it")
}

/// A repository with one commit and one modified file — the ordinary thing to
/// stash.
fn dirty() -> TestRepo {
    let repo = TestRepo::new();
    repo.commit_file("README.md", "one\n", "seed");
    repo.write("README.md", "one\ntwo\n");
    repo
}

fn status_is_clean(repo: &TestRepo) -> bool {
    repo.git(&["status", "--porcelain"]).is_empty()
}

fn paths(diff: &omagit_git::Diff) -> Vec<String> {
    diff.files
        .iter()
        .map(|file| file.path.display_lossy().into_owned())
        .collect()
}

#[test]
fn a_repository_that_has_never_stashed_has_an_empty_shelf() {
    // No `refs/stash` and no log at all: an empty list, not a failure. Reading
    // this as an error would put a red panel on every repository nobody has
    // ever stashed in, which is most of them.
    let repo = TestRepo::new();
    repo.commit_file("README.md", "one\n", "seed");

    assert_eq!(
        stash::list(&repo.open()).expect("no shelf is a shelf"),
        vec![]
    );
}

#[test]
fn stashing_clears_the_working_tree_and_names_the_branch_it_came_from() {
    let repo = dirty();

    ops::push(&git(), &repo.open(), "", false, &never()).expect("something to stash");

    assert!(
        status_is_clean(&repo),
        "the change is on the shelf, not here"
    );
    let shelf = stash::list(&repo.open()).expect("one entry");
    assert_eq!(shelf.len(), 1);
    assert_eq!(shelf[0].index, 0);
    assert_eq!(shelf[0].branch.as_deref(), Some("main"));
    // git composes `WIP on main: <short> <subject>`; the branch is split off and
    // what is left is what the row draws.
    assert!(shelf[0].message.contains("seed"), "{}", shelf[0].message);
    assert!(!shelf[0].untracked);
}

#[test]
fn nothing_to_stash_is_gits_own_sentence_rather_than_a_failure() {
    let repo = TestRepo::new();
    repo.commit_file("README.md", "one\n", "seed");

    let said = ops::push(&git(), &repo.open(), "", false, &never()).expect("not an error");

    assert!(said.contains("No local changes"), "{said}");
    assert_eq!(stash::list(&repo.open()).expect("still empty"), vec![]);
}

#[test]
fn the_newest_entry_is_the_first_row() {
    // The log is written oldest first and `stash@{0}` is the newest. A list in
    // file order would number every entry the wrong way round, and the number
    // is what a drop is given.
    let repo = dirty();
    ops::push(&git(), &repo.open(), "first", false, &never()).expect("stashed");
    repo.write("README.md", "one\nthree\n");
    ops::push(&git(), &repo.open(), "second", false, &never()).expect("stashed");

    let shelf = stash::list(&repo.open()).expect("two entries");

    assert_eq!(
        shelf
            .iter()
            .map(|entry| entry.message.as_str())
            .collect::<Vec<_>>(),
        ["second", "first"]
    );
    assert_eq!(shelf[0].index, 0);
    assert_eq!(
        repo.git(&["stash", "list", "--format=%gd %gs"])
            .lines()
            .next(),
        Some("stash@{0} On main: second"),
        "and git agrees about which one is zero"
    );
}

#[test]
fn a_message_of_ones_own_replaces_the_composed_one() {
    let repo = dirty();

    ops::push(
        &git(),
        &repo.open(),
        "  the runtime half  ",
        false,
        &never(),
    )
    .expect("stashed");

    let shelf = stash::list(&repo.open()).expect("one entry");
    assert_eq!(shelf[0].message, "the runtime half");
}

#[test]
fn untracked_files_reach_the_shelf_and_the_preview_says_so() {
    // The one that matters. A stash made with --include-untracked keeps those
    // files in a *third* parent, which the ordinary commit diff never looks at:
    // a preview built from `Diff::commit` would tell someone their untracked
    // files were not stashed, and they would go and delete them.
    let repo = dirty();
    repo.write("notes.scratch.md", "not tracked\n");

    ops::push(&git(), &repo.open(), "with the untracked", true, &never()).expect("stashed");

    assert!(status_is_clean(&repo), "the untracked file left too");
    assert!(!repo.path().join("notes.scratch.md").exists());

    let shelf = stash::list(&repo.open()).expect("one entry");
    assert!(shelf[0].untracked);
    let diff = stash::diff(
        &repo.open(),
        shelf[0].commit,
        DiffOptions::default(),
        &never(),
    )
    .expect("a preview");
    assert_eq!(paths(&diff), ["README.md", "notes.scratch.md"]);
}

#[test]
fn the_preview_is_against_the_commit_the_stash_was_made_on() {
    // Not against HEAD, which moves. A stash is what the working copy held
    // then, and a preview that drifted with the branch would show the commits
    // made since as part of it.
    let repo = dirty();
    ops::push(&git(), &repo.open(), "shelved", false, &never()).expect("stashed");
    repo.commit_file("unrelated.txt", "later\n", "a commit made afterwards");

    let shelf = stash::list(&repo.open()).expect("one entry");
    let diff = stash::diff(
        &repo.open(),
        shelf[0].commit,
        DiffOptions::default(),
        &never(),
    )
    .expect("a preview");

    assert_eq!(paths(&diff), ["README.md"]);
}

#[test]
fn applying_brings_it_back_and_leaves_it_on_the_shelf() {
    let repo = dirty();
    ops::push(&git(), &repo.open(), "shelved", false, &never()).expect("stashed");

    ops::apply(&git(), &repo.open(), 0, &never()).expect("applied");

    assert_eq!(
        String::from_utf8_lossy(&repo.read("README.md")),
        "one\ntwo\n"
    );
    assert_eq!(stash::list(&repo.open()).expect("still there").len(), 1);
}

#[test]
fn popping_brings_it_back_and_takes_it_off() {
    let repo = dirty();
    ops::push(&git(), &repo.open(), "shelved", false, &never()).expect("stashed");

    ops::pop(&git(), &repo.open(), 0, &never()).expect("popped");

    assert_eq!(
        String::from_utf8_lossy(&repo.read("README.md")),
        "one\ntwo\n"
    );
    assert_eq!(stash::list(&repo.open()).expect("empty"), vec![]);
}

#[test]
fn a_pop_that_conflicts_keeps_the_entry() {
    // The reason `pop` is one command rather than an apply followed by a drop:
    // git only drops when the apply succeeded. Composing the two here would
    // throw the stash away at exactly the moment its owner still needs it.
    let repo = dirty();
    ops::push(&git(), &repo.open(), "shelved", false, &never()).expect("stashed");
    repo.write("README.md", "one\nsomething else\n");

    let refused = ops::pop(&git(), &repo.open(), 0, &never()).expect_err("the same line moved");

    assert!(
        refused.to_string().to_lowercase().contains("conflict")
            || refused.to_string().contains("overwritten"),
        "git says what happened: {refused}"
    );
    assert_eq!(
        stash::list(&repo.open()).expect("still there").len(),
        1,
        "the stash is still the only copy of that work"
    );
}

#[test]
fn dropping_removes_the_entry_and_leaves_the_working_tree_alone() {
    let repo = dirty();
    ops::push(&git(), &repo.open(), "shelved", false, &never()).expect("stashed");
    repo.write("README.md", "one\nsomething else\n");

    ops::drop(&git(), &repo.open(), 0, &never()).expect("dropped");

    assert_eq!(stash::list(&repo.open()).expect("empty"), vec![]);
    assert_eq!(
        String::from_utf8_lossy(&repo.read("README.md")),
        "one\nsomething else\n",
        "what was on the shelf went; what was on the disk stayed"
    );
}

#[test]
fn an_entry_is_found_by_its_commit_because_dropping_renumbers_the_rest() {
    // The whole reason `find` exists. An index that crossed to the interface
    // and came back is a copy of a numbering that any drop invalidates: after
    // one, `stash@{1}` is a different stash from the one that was on screen.
    let repo = dirty();
    for message in ["first", "second", "third"] {
        repo.write("README.md", &format!("one\n{message}\n"));
        ops::push(&git(), &repo.open(), message, false, &never()).expect("stashed");
    }
    let shelf = stash::list(&repo.open()).expect("three entries");
    let wanted = shelf[2].commit; // "first", the oldest

    ops::drop(&git(), &repo.open(), 0, &never()).expect("dropped the newest");
    let now = stash::find(&repo.open(), wanted).expect("still on the shelf");

    assert_eq!(now.message, "first");
    assert_eq!(now.index, 1, "it moved up when the one above it went");
}

#[test]
fn a_stash_that_is_gone_is_named_rather_than_acted_on() {
    let repo = dirty();
    ops::push(&git(), &repo.open(), "shelved", false, &never()).expect("stashed");
    let shelf = stash::list(&repo.open()).expect("one entry");
    ops::drop(&git(), &repo.open(), 0, &never()).expect("dropped");

    let error = stash::find(&repo.open(), shelf[0].commit).expect_err("it is gone");

    assert!(error.to_string().contains("not found"), "{error}");
}

#[test]
fn a_stash_made_on_a_detached_head_has_no_branch_to_name() {
    let repo = dirty();
    let head = repo.git(&["rev-parse", "HEAD"]);
    repo.git(&["switch", "--detach", &head]);
    repo.write("README.md", "one\ndetached\n");

    ops::push(&git(), &repo.open(), "", false, &never()).expect("stashed");

    let shelf = stash::list(&repo.open()).expect("one entry");
    assert_eq!(
        shelf[0].branch, None,
        "`(no branch)` is a sentence, not a branch"
    );
}

#[test]
fn what_comes_back_off_the_shelf_is_recorded_as_destructive() {
    // All three rewrite something nobody else has a copy of: apply and pop the
    // working tree, drop the entry itself.
    let repo = dirty();
    let journal = omagit_git::journal::Journal::new();
    let git = git().with_journal(journal.clone());
    ops::push(&git, &repo.open(), "shelved", false, &never()).expect("stashed");
    ops::apply(&git, &repo.open(), 0, &never()).expect("applied");
    repo.git(&["checkout", "--", "README.md"]);
    ops::drop(&git, &repo.open(), 0, &never()).expect("dropped");

    let entries = journal.entries();
    let marked = |what: &str| {
        entries
            .iter()
            .any(|entry| entry.destructive && entry.command.contains(what))
    };
    assert!(marked("stash apply"), "{entries:?}");
    assert!(marked("stash drop"), "{entries:?}");
    assert!(
        entries
            .iter()
            .any(|entry| !entry.destructive && entry.command.contains("stash push")),
        "putting work on the shelf loses nothing: it is stored, and listed"
    );
}

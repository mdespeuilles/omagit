//! Fetch, pull and push, against a real remote.
//!
//! The remote is a bare repository on disk. That is a real remote to `git` —
//! the same refspec handling, the same `--prune`, the same `--force-with-lease`
//! check — and it needs no network, so these tests run on a laptop in a tunnel
//! and never flake on someone else's outage. What it does *not* exercise is
//! authentication, which is exactly the part that cannot be tested without a
//! server and a credential helper; the rule that covers it — never wait for a
//! person — is enforced in `cli.rs` and asserted there.

mod support;

use std::sync::{Arc, Mutex};

use omagit_git::cli::Git;
use omagit_git::ops::network::{self, PushForce};
use support::{TestRepo, never};

fn git() -> Git {
    Git::detect().expect("git is installed; these tests need it")
}

/// A bare repository, and a clone of it with `origin` pointing back.
struct Pair {
    remote: tempfile::TempDir,
    local: TestRepo,
}

fn pair() -> Pair {
    let remote = tempfile::tempdir().expect("a temporary directory");
    std::process::Command::new("git")
        .args(["init", "--bare", "--initial-branch=main"])
        .arg(remote.path())
        .output()
        .expect("git is installed");

    let local = TestRepo::new();
    local.commit_file("README.md", "one\n", "seed");
    local.git(&[
        "remote",
        "add",
        "origin",
        &remote.path().display().to_string(),
    ]);
    local.git(&["push", "--set-upstream", "origin", "main"]);
    Pair { remote, local }
}

/// A second clone of the same remote, for the case where someone else pushes.
fn second_clone(pair: &Pair) -> TestRepo {
    let other = TestRepo::new();
    other.git(&[
        "remote",
        "add",
        "origin",
        &pair.remote.path().display().to_string(),
    ]);
    other.git(&["fetch", "origin"]);
    other.git(&["checkout", "-B", "main", "origin/main"]);
    other
}

fn collector() -> (omagit_git::cli::Progress, Arc<Mutex<Vec<String>>>) {
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    let watch: omagit_git::cli::Progress = Arc::new(move |line: &str| {
        sink.lock().expect("not poisoned").push(line.to_owned());
    });
    (watch, seen)
}

#[test]
fn pushing_puts_the_commits_on_the_remote() {
    let pair = pair();
    pair.local.commit_file("a.txt", "new\n", "a new commit");

    network::push(
        &git(),
        &pair.local.open(),
        &network::Push {
            remote: "origin",
            branch: "main",
            force: PushForce::Never,
            set_upstream: false,
        },
        None,
        &never(),
    )
    .expect("a fast-forward push");

    let on_remote = std::process::Command::new("git")
        .args(["log", "-1", "--format=%s", "main"])
        .current_dir(pair.remote.path())
        .output()
        .expect("git");
    assert_eq!(
        String::from_utf8_lossy(&on_remote.stdout).trim(),
        "a new commit"
    );
}

#[test]
fn fetching_brings_a_branch_down_without_touching_the_working_tree() {
    let pair = pair();
    let other = second_clone(&pair);
    other.commit_file("theirs.txt", "theirs\n", "from somewhere else");
    other.git(&["push", "origin", "main"]);

    network::fetch(&git(), &pair.local.open(), Some("origin"), None, &never()).expect("fetched");

    assert_eq!(
        pair.local.git(&["log", "-1", "--format=%s", "origin/main"]),
        "from somewhere else"
    );
    assert!(
        !pair.local.path().join("theirs.txt").exists(),
        "a fetch moves refs and nothing else"
    );
}

#[test]
fn fetching_prunes_a_branch_that_is_gone_from_the_remote() {
    // A remote-tracking ref for a branch somebody deleted is a lie the sidebar
    // would keep repeating.
    let pair = pair();
    pair.local.git(&["push", "origin", "main:doomed"]);
    network::fetch(&git(), &pair.local.open(), Some("origin"), None, &never()).expect("fetched");
    assert!(pair.local.git(&["branch", "-r"]).contains("origin/doomed"));

    std::process::Command::new("git")
        .args(["branch", "-D", "doomed"])
        .current_dir(pair.remote.path())
        .output()
        .expect("git");

    network::fetch(&git(), &pair.local.open(), Some("origin"), None, &never()).expect("fetched");
    assert!(!pair.local.git(&["branch", "-r"]).contains("origin/doomed"));
}

#[test]
fn pulling_brings_the_commits_into_the_working_tree() {
    let pair = pair();
    let other = second_clone(&pair);
    other.commit_file("theirs.txt", "theirs\n", "from somewhere else");
    other.git(&["push", "origin", "main"]);

    network::pull(&git(), &pair.local.open(), None, &never()).expect("pulled");

    assert!(
        pair.local.path().join("theirs.txt").exists(),
        "a pull is a fetch that lands"
    );
}

#[test]
fn a_push_that_is_not_a_fast_forward_is_refused_until_the_lease_is_taken() {
    let pair = pair();
    let other = second_clone(&pair);
    other.commit_file("theirs.txt", "theirs\n", "somebody else's commit");
    other.git(&["push", "origin", "main"]);

    // Ours diverges: a commit of our own on top of the old tip.
    pair.local.commit_file("ours.txt", "ours\n", "our commit");

    let refused = network::push(
        &git(),
        &pair.local.open(),
        &network::Push {
            remote: "origin",
            branch: "main",
            force: PushForce::Never,
            set_upstream: false,
        },
        None,
        &never(),
    )
    .expect_err("the remote has moved");
    assert!(
        refused.to_string().contains("reject") || refused.to_string().contains("fetch first"),
        "git says why: {refused}"
    );
}

#[test]
fn force_with_lease_refuses_exactly_the_case_force_would_destroy() {
    // The whole reason `--force` is not offered. Someone else pushed while we
    // were rebasing; `--force` would overwrite their commit without a word,
    // and cannot tell that from the rebase we meant to publish.
    let pair = pair();
    let other = second_clone(&pair);
    other.commit_file("theirs.txt", "theirs\n", "somebody else's commit");
    other.git(&["push", "origin", "main"]);

    pair.local.commit_file("ours.txt", "ours\n", "our commit");

    let refused = network::push(
        &git(),
        &pair.local.open(),
        &network::Push {
            remote: "origin",
            branch: "main",
            force: PushForce::WithLease,
            set_upstream: false,
        },
        None,
        &never(),
    )
    .expect_err("the remote is not where we last saw it");
    assert!(
        refused.to_string().contains("stale info") || refused.to_string().contains("reject"),
        "git says why: {refused}"
    );

    // Once we have seen their commit, the lease is ours to take.
    network::fetch(&git(), &pair.local.open(), Some("origin"), None, &never()).expect("fetched");
    network::push(
        &git(),
        &pair.local.open(),
        &network::Push {
            remote: "origin",
            branch: "main",
            force: PushForce::WithLease,
            set_upstream: false,
        },
        None,
        &never(),
    )
    .expect("the lease holds now");
}

#[test]
fn a_forced_push_is_recorded_as_destructive() {
    let pair = pair();
    let journal = omagit_git::journal::Journal::new();
    let git = git().with_journal(journal.clone());
    pair.local.commit_file("a.txt", "new\n", "a new commit");

    network::push(
        &git,
        &pair.local.open(),
        &network::Push {
            remote: "origin",
            branch: "main",
            force: PushForce::WithLease,
            set_upstream: false,
        },
        None,
        &never(),
    )
    .expect("pushed");

    let entries = journal.entries();
    let forced = entries
        .iter()
        .find(|entry| entry.command.contains("--force-with-lease"))
        .expect("in the journal");
    assert!(forced.destructive, "it rewrites published history");
}

#[test]
fn a_new_branch_can_set_its_upstream_as_it_is_pushed() {
    // Pushing a new branch and then finding it tracks nothing is a second step
    // nobody wants; `git` itself suggests exactly this command in that case.
    let pair = pair();
    pair.local.git(&["checkout", "-b", "feature"]);
    pair.local.commit_file("f.txt", "f\n", "on the feature");

    network::push(
        &git(),
        &pair.local.open(),
        &network::Push {
            remote: "origin",
            branch: "feature",
            force: PushForce::Never,
            set_upstream: true,
        },
        None,
        &never(),
    )
    .expect("pushed");

    assert_eq!(
        pair.local
            .git(&["rev-parse", "--abbrev-ref", "feature@{upstream}"]),
        "origin/feature"
    );
}

#[test]
fn progress_arrives_while_the_command_runs_and_not_only_at_the_end() {
    // `git` overwrites its progress line with a carriage return, so a reader
    // that split on newlines alone would receive one enormous line at the end
    // and report nothing until then.
    let pair = pair();
    for n in 0..40 {
        pair.local
            .commit_file("bulk.txt", &format!("{n}\n"), &format!("commit {n}"));
    }
    let (watch, seen) = collector();

    network::push(
        &git(),
        &pair.local.open(),
        &network::Push {
            remote: "origin",
            branch: "main",
            force: PushForce::Never,
            set_upstream: false,
        },
        Some(watch),
        &never(),
    )
    .expect("pushed");

    let lines = seen.lock().expect("not poisoned");
    assert!(
        !lines.is_empty(),
        "nothing was reported while the push ran: {lines:?}"
    );
    assert!(
        lines.iter().all(|line| !line.contains('\r')),
        "a carriage return means a line was handed over whole rather than cut: {lines:?}"
    );
}

#[test]
fn a_cancelled_fetch_stops_and_says_so() {
    let pair = pair();
    let cancel = omagit_git::Cancel::new();
    cancel.cancel();

    let error = network::fetch(&git(), &pair.local.open(), Some("origin"), None, &cancel)
        .expect_err("cancelled before it began");
    assert!(
        matches!(error, omagit_git::GitError::Cancelled),
        "{error:?}"
    );
}

// ── Cloning ─────────────────────────────────────────────────────────────────
//
// The one operation with no repository to start from. Everything below clones
// the same on-disk bare repository the rest of this file pushes to, so the
// remote is real to `git` — the same transport code path, the same `--depth`
// handling — and needs no network.

#[test]
fn cloning_brings_the_history_and_a_working_tree() {
    let pair = pair();
    pair.local.commit_file("a.txt", "new\n", "a second commit");
    pair.local.git(&["push", "origin", "main"]);
    let into = tempfile::tempdir().expect("a temporary directory");

    let landed = network::clone(
        &git(),
        &pair.remote.path().display().to_string(),
        into.path(),
        "copy",
        &network::CloneOptions::default(),
        None,
        &never(),
    )
    .expect("cloned");

    assert_eq!(landed, into.path().join("copy"));
    assert!(
        landed.join("a.txt").exists(),
        "a clone lands a working tree"
    );
    // Opening it is the assertion that matters: a directory full of files is
    // not the same thing as a repository this application can read, and the
    // library adds what it clones without a second chance to notice.
    let opened = omagit_git::Repository::open(&landed).expect("a repository");
    let head = opened.head().expect("HEAD resolves");
    assert!(
        matches!(head, omagit_git::repo::Head::Branch { ref branch, .. } if branch == "main"),
        "a clone checks out the remote's default branch: {head:?}"
    );

    let counted = std::process::Command::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .current_dir(&landed)
        .output()
        .expect("git");
    assert_eq!(
        String::from_utf8_lossy(&counted.stdout).trim(),
        "2",
        "and the whole history behind it"
    );
}

#[test]
fn a_shallow_clone_stops_at_the_tip() {
    // `--depth 1` is a real trade and the test says which half you get: the
    // files, without the history behind them.
    //
    // `file://` rather than a bare path, and not for tidiness: `git` clones a
    // local path by copying object files, a transport that has no notion of
    // depth, so it *ignores* `--depth` there and says so on stderr. Only the
    // `file://` URL goes through the real protocol. That is Git's behaviour and
    // not something to route around — see the note on `CloneOptions::shallow`.
    let pair = pair();
    for n in 0..5 {
        pair.local
            .commit_file("bulk.txt", &format!("{n}\n"), &format!("commit {n}"));
    }
    pair.local.git(&["push", "origin", "main"]);
    let into = tempfile::tempdir().expect("a temporary directory");

    let landed = network::clone(
        &git(),
        &format!("file://{}", pair.remote.path().display()),
        into.path(),
        "shallow",
        &network::CloneOptions {
            shallow: true,
            submodules: false,
        },
        None,
        &never(),
    )
    .expect("cloned");

    let counted = std::process::Command::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .current_dir(&landed)
        .output()
        .expect("git");
    assert_eq!(
        String::from_utf8_lossy(&counted.stdout).trim(),
        "1",
        "the tip and nothing behind it"
    );
    assert!(landed.join("bulk.txt").exists(), "and the files are there");
}

#[test]
fn cloning_into_a_parent_that_does_not_exist_yet_makes_it() {
    // `~/src` for somebody who has never had one. `git` needs somewhere to run,
    // and refusing here would be refusing over a directory this can create.
    let pair = pair();
    let into = tempfile::tempdir().expect("a temporary directory");
    let parent = into.path().join("never").join("existed");

    let landed = network::clone(
        &git(),
        &pair.remote.path().display().to_string(),
        &parent,
        "copy",
        &network::CloneOptions::default(),
        None,
        &never(),
    )
    .expect("cloned");

    assert!(landed.join("README.md").exists());
}

#[test]
fn cloning_onto_something_that_is_already_there_is_refused_in_gits_own_words() {
    let pair = pair();
    let into = tempfile::tempdir().expect("a temporary directory");
    std::fs::create_dir(into.path().join("taken")).expect("made");
    std::fs::write(into.path().join("taken").join("mine.txt"), "mine\n").expect("written");

    let refused = network::clone(
        &git(),
        &pair.remote.path().display().to_string(),
        into.path(),
        "taken",
        &network::CloneOptions::default(),
        None,
        &never(),
    )
    .expect_err("the directory is not empty");
    assert!(
        refused.to_string().contains("taken"),
        "the refusal names the directory: {refused}"
    );
    assert!(
        into.path().join("taken").join("mine.txt").exists(),
        "and it left what was there alone"
    );
}

#[test]
fn a_clone_reports_progress_while_it_runs() {
    let pair = pair();
    for n in 0..40 {
        pair.local
            .commit_file("bulk.txt", &format!("{n}\n"), &format!("commit {n}"));
    }
    pair.local.git(&["push", "origin", "main"]);
    let into = tempfile::tempdir().expect("a temporary directory");
    let (watch, seen) = collector();

    network::clone(
        &git(),
        &pair.remote.path().display().to_string(),
        into.path(),
        "copy",
        &network::CloneOptions::default(),
        Some(watch),
        &never(),
    )
    .expect("cloned");

    let lines = seen.lock().expect("not poisoned");
    assert!(
        !lines.is_empty(),
        "a clone is the longest wait in the application: {lines:?}"
    );
}

#[test]
fn a_reachable_remote_answers_and_a_missing_one_says_so() {
    let pair = pair();
    let url = pair.remote.path().display().to_string();

    network::reachable(&git(), &url, &never()).expect("it is right there");

    let gone = pair
        .remote
        .path()
        .join("not-a-repository")
        .display()
        .to_string();
    let refused = network::reachable(&git(), &gone, &never()).expect_err("nothing is at that path");
    assert!(
        !refused.to_string().is_empty(),
        "the probe says why: {refused}"
    );
}

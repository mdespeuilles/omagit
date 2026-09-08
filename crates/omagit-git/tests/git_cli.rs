//! The `git` subprocess runner, against the real binary (SPEC §8).
//!
//! M2 calls it once, at start-up, to find out whether a usable `git` exists.
//! Everything else it promises — a deadline, cancellation that reaches the
//! helpers `git` spawns, `stderr` passed through unedited — is what M5 and M7
//! will rely on for `commit`, `push` and `fetch`, and none of that is worth
//! having unverified. These tests are the verification.

mod support;

use std::time::{Duration, Instant};

use omagit_git::cli::Git;
use omagit_git::{Cancel, GitError, MINIMUM_GIT_VERSION};
use support::{TestRepo, never};

fn git() -> Git {
    Git::detect().expect("the tests need a git in PATH")
}

#[test]
fn detects_the_installed_git_and_vets_its_version() {
    let git = git();
    let minimum = omagit_git::cli::Version::parse_number(MINIMUM_GIT_VERSION).expect("a version");
    assert!(
        git.version() >= minimum,
        "detection is supposed to refuse anything below {MINIMUM_GIT_VERSION}, and it returned {}",
        git.version()
    );
}

#[test]
fn runs_a_command_and_returns_its_output() {
    let fixture = TestRepo::new();
    fixture.commit_file("a.txt", "a\n", "first");

    let output = git()
        .at(fixture.path())
        .args(["rev-parse", "HEAD"])
        .run(&never())
        .expect("rev-parse succeeds");
    assert_eq!(output.text().trim(), fixture.head().to_string());
}

#[test]
fn a_failure_carries_gits_own_stderr() {
    let fixture = TestRepo::new();
    let error = git()
        .at(fixture.path())
        .args(["rev-parse", "--verify", "refs/heads/nope"])
        .run(&never())
        .expect_err("the reference does not exist");

    match error {
        GitError::CommandFailed { command, stderr } => {
            assert!(command.starts_with("git rev-parse"), "got {command:?}");
            assert!(
                !stderr.is_empty(),
                "git's diagnostic is better than anything written here, so it travels verbatim"
            );
        }
        other => panic!("expected a command failure, got {other:?}"),
    }
}

#[test]
fn the_command_line_is_readable_before_it_runs() {
    // SPEC §15: a destructive command is logged with its exact command line
    // *before* it runs, and SPEC §11's operations journal shows the same string.
    let git = git();
    let invocation = git
        .at(".")
        .args(["push", "--force-with-lease", "origin", "main"]);
    assert_eq!(
        invocation.command_line(),
        "git push --force-with-lease origin main"
    );
}

#[test]
fn a_timeout_kills_the_command() {
    let fixture = TestRepo::new();
    let started = Instant::now();
    let error = git()
        .at(fixture.path())
        .args(["-c", "alias.snooze=!sleep 30", "snooze"])
        .timeout(Duration::from_millis(300))
        .run(&never())
        .expect_err("the deadline passes long before the sleep does");

    assert!(matches!(error, GitError::Timeout { .. }), "got {error:?}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the deadline is only a deadline if it is enforced; took {:?}",
        started.elapsed()
    );
}

#[test]
fn cancelling_reaches_the_processes_git_spawned() {
    // The rule of SPEC §8 that is easy to implement wrongly: `git` delegates to
    // `git-remote-https`, to `ssh` and to hooks, so signalling only the leader
    // leaves those running — a "cancelled" push that pushes anyway.
    //
    // The alias runs a shell that touches a marker, sleeps, then writes to it
    // again. If the kill reached only `git`, the shell survives and the second
    // write lands.
    let fixture = TestRepo::new();
    let marker = fixture.path().join("marker");
    let script = format!(
        "!sh -c 'echo started > {0}; sleep 2; echo finished >> {0}'",
        marker.display()
    );

    let cancel = Cancel::new();
    let ticker = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(400));
        ticker.cancel();
    });

    let started = Instant::now();
    let error = git()
        .at(fixture.path())
        .args(["-c", &format!("alias.snooze={script}"), "snooze"])
        .run(&cancel)
        .expect_err("the run was cancelled");
    assert!(matches!(error, GitError::Cancelled), "got {error:?}");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "cancelling returned only after the child finished on its own"
    );

    // Long enough for the sleep to have finished, had it survived.
    std::thread::sleep(Duration::from_millis(2500));
    let contents = std::fs::read_to_string(&marker).unwrap_or_default();
    assert!(
        contents.contains("started"),
        "the fixture is only meaningful if the shell actually ran: {contents:?}"
    );
    assert!(
        !contents.contains("finished"),
        "the shell outlived the git it was spawned by"
    );
}

#[test]
fn a_read_never_takes_the_index_lock() {
    // `GIT_OPTIONAL_LOCKS=0`: running omagit while a terminal `git` holds
    // `index.lock` must not turn a read into an error.
    let fixture = TestRepo::new();
    fixture.commit_file("a.txt", "a\n", "first");
    std::fs::write(fixture.path().join(".git/index.lock"), b"held elsewhere")
        .expect("a writable fixture");

    let output = git()
        .at(fixture.path())
        .args(["status", "--porcelain=v1", "-z"])
        .run(&never())
        .expect("a read succeeds while the index is locked");
    assert!(output.text().is_empty(), "the tree is clean");
}

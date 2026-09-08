//! The repository watcher, against the real filesystem.
//!
//! SPEC §10 warns that inotify and FSEvents do not report the same events, so
//! every assertion here is about the **outcome** — what the app is told to
//! re-read — rather than about the events themselves. That is what makes the
//! same test meaningful on both platforms, and it is the same shape M1's
//! Omarchy watcher test settled on.
//!
//! These are slow by nature: each one waits out a 150 ms debounce plus whatever
//! the platform takes to deliver. They are still worth having as integration
//! tests rather than as a unit test of the classifier, because the classifier
//! being right about a path it never receives is worth nothing.

mod support;

use std::time::Duration;

use omagit_git::watch::{Changes, Watcher};
use support::TestRepo;

/// How long to wait for a notification before deciding none is coming.
///
/// Generous: a loaded CI runner delivers late, and a test that fails there and
/// passes here teaches nothing.
const TIMEOUT: Duration = Duration::from_secs(5);

/// Long enough that a notification would have arrived if one were coming.
const QUIET: Duration = Duration::from_millis(900);

fn watch(fixture: &TestRepo) -> (Watcher, async_channel::Receiver<Changes>) {
    let repo = fixture.open();
    let (watcher, changes) = Watcher::start(&repo).expect("a watcher");
    // Two waits, and both are needed.
    //
    // The platform registers asynchronously, so a change made immediately races
    // the registration. And `git commit` leaves work behind it — `gc --auto`
    // runs in the background and touches `.git` a second later — so a test that
    // starts asserting straight away is asserting about the *previous* command.
    // Settling first is what makes these tests about the change they make.
    std::thread::sleep(Duration::from_millis(300));
    settle(&changes);
    (watcher, changes)
}

/// Drain until nothing has arrived for a while.
fn settle(changes: &async_channel::Receiver<Changes>) {
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let mut quiet_since = std::time::Instant::now();
    while std::time::Instant::now() < deadline {
        if changes.try_recv().is_ok() {
            quiet_since = std::time::Instant::now();
        } else if quiet_since.elapsed() > Duration::from_millis(600) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The next change set, or a failure naming what was expected.
///
/// `async-channel` has no blocking receive with a deadline, so this polls —
/// which is what a test wants anyway: a blocking receive that never returns
/// hangs the suite instead of failing it.
fn next(changes: &async_channel::Receiver<Changes>, what: &str) -> Changes {
    let deadline = std::time::Instant::now() + TIMEOUT;
    while std::time::Instant::now() < deadline {
        if let Ok(changes) = changes.try_recv() {
            return changes;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("no notification for {what} within {TIMEOUT:?}");
}

/// Assert nothing arrives — the harder half of a watcher's contract.
fn expect_quiet(changes: &async_channel::Receiver<Changes>, why: &str) {
    std::thread::sleep(QUIET);
    if let Ok(unexpected) = changes.try_recv() {
        panic!("{why}, but the watcher reported {unexpected:?}");
    }
}

#[test]
fn editing_a_tracked_file_invalidates_the_working_copy() {
    let fixture = TestRepo::new();
    fixture.commit_file("src/main.rs", "fn main() {}\n", "first");
    let (_watcher, changes) = watch(&fixture);

    fixture.write("src/main.rs", "fn main() { println!() }\n");

    let changes = next(&changes, "an edit");
    assert!(changes.working_copy);
    assert!(changes.invalidates_status());
    assert!(
        !changes.invalidates_refs(),
        "editing a file does not move a branch, so the sidebar must not be re-read"
    );
}

#[test]
fn a_burst_of_writes_arrives_as_one_notification() {
    // The case that matters: `cargo build` writes thousands of files, and a
    // status per file is a client that does nothing else.
    let fixture = TestRepo::new();
    fixture.commit_file("anchor.txt", "anchor\n", "first");
    let (_watcher, changes) = watch(&fixture);

    for n in 0..200 {
        fixture.write(&format!("file{n}.txt"), "content\n");
    }

    let first = next(&changes, "a burst of 200 writes");
    assert!(first.working_copy);
    // Whatever the platform delivered, it coalesced: a second notification
    // would mean the debounce is not holding.
    expect_quiet(&changes, "200 writes should coalesce into one notification");
}

#[test]
fn the_index_lock_is_not_a_change() {
    // Git creates and removes it around every write it makes. Reacting to it is
    // reacting to ourselves (SPEC §10 names this one).
    let fixture = TestRepo::new();
    fixture.commit_file("a.txt", "a\n", "first");
    let (_watcher, changes) = watch(&fixture);

    let lock = fixture.path().join(".git/index.lock");
    std::fs::write(&lock, b"held").expect("a writable fixture");
    std::fs::remove_file(&lock).expect("removable");

    expect_quiet(&changes, "index.lock is Git talking to itself");
}

#[test]
fn a_branch_moving_does_not_invalidate_the_working_copy() {
    // The example SPEC §10 gives by name: a write under `.git/refs/` must not
    // cost a working-copy scan.
    let fixture = TestRepo::new();
    fixture.commit_file("a.txt", "a\n", "first");
    let (_watcher, changes) = watch(&fixture);

    fixture.git(&["branch", "a-new-branch"]);

    let changes = next(&changes, "a new branch");
    assert!(changes.invalidates_refs());
    assert!(
        !changes.working_copy,
        "nothing in the work tree was touched: {changes:?}"
    );
}

#[test]
fn writing_into_an_ignored_directory_is_silent() {
    // The other half of the build case: `target/` is where the thousands of
    // writes actually go, and Git does not care about any of them.
    let fixture = TestRepo::new();
    fixture.write(".gitignore", "target/\n*.log\n");
    fixture.commit("ignore build output");
    fixture.mkdir("target/debug");
    let (_watcher, changes) = watch(&fixture);

    for n in 0..50 {
        fixture.write(&format!("target/debug/artifact{n}.bin"), "binary\n");
    }
    fixture.write("build.log", "noise\n");

    expect_quiet(&changes, "every written path is ignored by Git");
}

#[test]
fn an_ignored_directory_does_not_hide_a_real_change_beside_it() {
    // The exclusion must not be a blanket "anything under a directory with an
    // ignored sibling", which is the easy way to write it and wrong.
    let fixture = TestRepo::new();
    fixture.write(".gitignore", "target/\n");
    fixture.commit_file("src/main.rs", "fn main() {}\n", "first");
    fixture.mkdir("target");
    let (_watcher, changes) = watch(&fixture);

    fixture.write("target/junk.bin", "junk\n");
    fixture.write("src/main.rs", "fn main() { }\n");

    let changes = next(&changes, "an edit beside an ignored directory");
    assert!(changes.working_copy);
}

#[test]
fn a_commit_arrives_as_one_notification_covering_everything_it_touched() {
    let fixture = TestRepo::new();
    fixture.commit_file("a.txt", "a\n", "first");
    let (_watcher, changes) = watch(&fixture);

    fixture.write("a.txt", "changed\n");
    fixture.commit("second");

    let changes = next(&changes, "a commit");
    assert!(
        changes.invalidates_status() && changes.invalidates_refs(),
        "a commit moves HEAD, moves a branch and rewrites the index: {changes:?}"
    );
}

#[test]
fn a_dropped_watcher_stops_the_thread() {
    let fixture = TestRepo::new();
    fixture.commit_file("a.txt", "a\n", "first");
    let (watcher, changes) = watch(&fixture);

    drop(watcher);
    fixture.write("a.txt", "changed after the watcher went away\n");

    // The channel closes rather than going quiet, which is what lets the
    // foreground task end instead of waiting for something that will never
    // arrive.
    let deadline = std::time::Instant::now() + TIMEOUT;
    while std::time::Instant::now() < deadline {
        if changes.is_closed() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the channel is still open after the watcher was dropped");
}

#[test]
fn a_repository_reached_through_a_symlink_still_reports_changes() {
    // A repository opened through a symlink still has to report its changes.
    //
    // Read this test for what it is: on Linux it passes with or without the
    // resolution in `Watcher::start`, because inotify reports paths under the
    // watch root exactly as that root was spelled. It is macOS that resolves
    // them — `/var` is a symlink to `/private/var`, FSEvents reports the
    // resolved form, and a watch registered on the unresolved one then receives
    // events whose prefix does not match and drops every one of them.
    //
    // So this asserts a property worth having and **does not** prove the fix.
    // The macOS half of the matrix is what answers for that, which is the
    // second time in this project it has caught something a green Linux run
    // called fine (`docs/ARCHITECTURE.md` §5, tenth entry).
    let fixture = TestRepo::new();
    fixture.commit_file("src/main.rs", "fn main() {}\n", "first");

    let link = fixture
        .path()
        .parent()
        .expect("a parent")
        .join("linked-repo");
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(fixture.path(), &link).expect("a symlink");

    let repo = omagit_git::Repository::open(&link).expect("a repository through the link");
    let (_watcher, changes) = Watcher::start(&repo).expect("a watcher");
    std::thread::sleep(Duration::from_millis(300));
    settle(&changes);

    fixture.write("src/main.rs", "fn main() { println!() }\n");

    let changes = next(&changes, "an edit under a symlinked repository");
    assert!(
        changes.working_copy,
        "the watch and the events have to agree on how the path is spelled"
    );

    let _ = std::fs::remove_file(&link);
}

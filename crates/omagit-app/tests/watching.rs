//! A repository opened through `AppState` reports its own changes (SPEC §10).
//!
//! `omagit-git` tests the watcher against a real filesystem, and the front end
//! tests which notifications it acts on. Between the two sits the bridge — a
//! watcher started per open repository, and a thread turning its change sets
//! into a window event — and nothing else reaches it. It is exactly the kind of
//! wiring that compiles, type-checks, and silently does nothing: `watch.rs` had
//! been written since M2 and never called at all.
//!
//! Tauri's mock runtime is what makes it testable: a real application, with a
//! real event system, and no window.

use std::sync::mpsc;
use std::time::Duration;

use omagit_lib::state::AppState;
use tauri::{Listener, Manager};

/// A repository with one commit, and the app that has it open.
fn repository() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let run = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .env("GIT_AUTHOR_NAME", "T")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "T")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .expect("git runs");
        assert!(status.status.success(), "git {args:?}: {status:?}");
    };
    run(&["init", "--initial-branch=main"]);
    std::fs::write(dir.path().join("kept.txt"), "one\n").expect("write");
    run(&["add", "."]);
    run(&["commit", "-m", "first"]);
    dir
}

#[test]
fn a_file_written_on_disk_reaches_the_window() {
    let repo = repository();
    let app = tauri::test::mock_builder()
        .manage(AppState::detect(None))
        .build(tauri::generate_context!("tauri.conf.json"))
        .expect("the mock application builds");

    // What `run()` does in `setup`, and the reason the order matters: a
    // repository opened before the window has nowhere to report to.
    app.state::<AppState>().attach(app.handle().clone());

    let (sender, events) = mpsc::channel();
    app.listen("changed", move |event| {
        let _ = sender.send(event.payload().to_owned());
    });

    app.state::<AppState>()
        .open(repo.path())
        .expect("the repository opens");

    // Something else writes into the working copy.
    std::fs::write(repo.path().join("new.txt"), "two\n").expect("write");

    // The watcher debounces at 150ms; a second is room for a slow machine
    // without making a failure take minutes to arrive.
    let payload = events
        .recv_timeout(Duration::from_secs(5))
        .expect("the window is told the repository changed");

    assert!(
        payload.contains("\"status\":true"),
        "a file in the work tree invalidates the status: {payload}"
    );
    assert!(
        payload.contains(&repo.path().display().to_string())
            || payload.contains(
                &std::fs::canonicalize(repo.path())
                    .expect("canonical")
                    .display()
                    .to_string()
            ),
        "the event names the repository it is about: {payload}"
    );
}

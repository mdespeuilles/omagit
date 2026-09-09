//! A temporary repository, built by running the real `git`.
//!
//! The same choice `omagit-git`'s fixtures make, and for the same reason: the
//! point of these tests is that omagit agrees with Git, not that it agrees with
//! itself. It also means a failure can be reproduced by hand — every helper
//! here is one command.
//!
//! Deterministic on purpose. The identity, the initial branch name and the
//! timestamps are set explicitly and the global and system configurations are
//! cut off, so a developer with `commit.gpgsign = true` or
//! `init.defaultBranch = trunk` sees what everyone else sees.

// Two test binaries share this module and neither uses all of it; Rust compiles
// it once per binary, so what one skips reads as dead code there.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use omagit_git::Repository;

pub struct TestRepo {
    /// Kept alive: dropping it deletes the repository.
    _dir: tempfile::TempDir,
    path: PathBuf,
    /// Bumped once per commit, so history order is unambiguous.
    clock: std::cell::Cell<i64>,
}

/// 2026-01-01T00:00:00Z — a fixed epoch, so a failure message shows the same
/// timestamps on every machine.
const EPOCH: i64 = 1_767_225_600;

impl TestRepo {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().to_path_buf();
        let repo = Self {
            _dir: dir,
            path,
            clock: std::cell::Cell::new(EPOCH),
        };
        repo.git(&["init", "--initial-branch=main"]);
        repo.git(&["config", "user.name", "Test Author"]);
        repo.git(&["config", "user.email", "author@omagit.test"]);
        // A signing key on the developer's machine would make every commit here
        // fail, and a merge whose message opens an editor would hang.
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo.git(&["config", "core.editor", "true"]);
        repo
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn open(&self) -> Repository {
        Repository::open(&self.path).expect("the fixture is a repository")
    }

    /// Run `git`, returning its stdout. Panics with `stderr` on failure — a
    /// fixture that did not build is a broken test, not a result.
    pub fn git(&self, args: &[&str]) -> String {
        let time = format!("{} +0000", self.clock.get());
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.path)
            .env("HOME", &self.path)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Test Author")
            .env("GIT_AUTHOR_EMAIL", "author@omagit.test")
            .env("GIT_COMMITTER_NAME", "Test Committer")
            .env("GIT_COMMITTER_EMAIL", "committer@omagit.test")
            .env("GIT_AUTHOR_DATE", &time)
            .env("GIT_COMMITTER_DATE", &time)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .expect("git is installed; these tests need it");
        assert!(
            output.status.success(),
            "git {}\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        if args.first() == Some(&"commit") || args.first() == Some(&"merge") {
            self.clock.set(self.clock.get() + 60);
        }
        String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_owned()
    }

    pub fn write(&self, name: &str, contents: &str) {
        std::fs::write(self.path.join(name), contents).expect("a writable fixture");
    }

    pub fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.path.join(name)).expect("the file exists")
    }

    pub fn remove(&self, name: &str) {
        std::fs::remove_file(self.path.join(name)).expect("the file exists");
    }

    /// Write one file and commit it: the shape most of these tests need.
    pub fn commit_file(&self, name: &str, contents: &str, message: &str) {
        self.write(name, contents);
        self.git(&["add", name]);
        self.git(&["commit", "-m", message]);
    }

    /// What `git show :<path>` prints — the index's copy, which is what every
    /// assertion about staging is really about.
    pub fn indexed(&self, name: &str) -> String {
        self.git(&["show", &format!(":{name}")])
    }
}

/// Everything omagit reads is cancellable; these tests never cancel.
pub fn never() -> omagit_git::Cancel {
    omagit_git::Cancel::new()
}

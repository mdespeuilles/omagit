//! A temporary repository with a scripted history (SPEC §13).
//!
//! Built by running the real `git`, on purpose. The point of these tests is to
//! check that omagit reads what Git wrote — not that `gix` agrees with itself —
//! so the fixtures come from the reference implementation. It also means a
//! failure can be reproduced by hand: every helper here is one command.
//!
//! Everything is deterministic. Identity, timestamps and the initial branch name
//! are set explicitly, the global and system configurations are cut off, and
//! `HOME` points inside the temporary directory: a developer with
//! `commit.gpgsign = true`, `init.defaultBranch = trunk` or a `core.excludesFile`
//! of their own must not change what these tests see.

// The helpers are shared by five test binaries and no binary uses all of them;
// Rust compiles this module once per binary, so anything one of them skips reads
// as dead code there.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use omagit_git::{ObjectId, Repository};

pub struct TestRepo {
    /// Kept alive: dropping it deletes the repository.
    _dir: tempfile::TempDir,
    path: PathBuf,
    /// Bumped once per commit so every commit has a distinct, ordered time.
    clock: std::cell::Cell<i64>,
    /// When set, the clock stops and every commit shares one timestamp.
    frozen: std::cell::Cell<bool>,
    /// Who the next commits are by. The identity is passed in the environment
    /// rather than through `git config`, so that a developer's own settings
    /// cannot leak in — which also means `git config user.name` in a test does
    /// nothing, and this is the way to change it.
    author: std::cell::RefCell<(String, String)>,
}

/// 2026-01-01T00:00:00Z — a fixed epoch, so a failure message shows the same
/// timestamps on every machine.
const EPOCH: i64 = 1_767_225_600;

impl TestRepo {
    /// An empty repository with one branch and no commits.
    pub fn new() -> Self {
        Self::init(&["init", "--initial-branch=main"])
    }

    /// A bare repository — SPEC §13 requires the read path to survive one.
    pub fn bare() -> Self {
        Self::init(&["init", "--bare", "--initial-branch=main"])
    }

    fn init(args: &[&str]) -> Self {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().to_path_buf();
        let repo = Self {
            _dir: dir,
            path,
            clock: std::cell::Cell::new(EPOCH),
            frozen: std::cell::Cell::new(false),
            author: std::cell::RefCell::new((
                "Test Author".to_owned(),
                "author@omagit.test".to_owned(),
            )),
        };
        repo.git(args);
        repo.git(&["config", "user.name", "Test Author"]);
        repo.git(&["config", "user.email", "author@omagit.test"]);
        // A signing key on the developer's machine would make every commit here
        // fail, and a merge whose message is opened in an editor would hang.
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo.git(&["config", "tag.gpgsign", "false"]);
        repo.git(&["config", "core.editor", "true"]);
        repo
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Stop the clock, so every commit made afterwards shares one timestamp.
    ///
    /// Distinct times are what most of these fixtures want — a failure message
    /// reads better when the order is unambiguous. But a repository where they
    /// are distinct is a repository that never exercises a tie, and a tie is
    /// ordinary: a scripted import, a rebase, `git commit` twice in one second.
    pub fn freeze_clock(&self) {
        self.frozen.set(true);
    }

    /// Who the commits made after this are by.
    pub fn set_author(&self, name: &str, email: &str) {
        *self.author.borrow_mut() = (name.to_owned(), email.to_owned());
    }

    /// Open it the way the app would.
    pub fn open(&self) -> Repository {
        Repository::open(&self.path).expect("the fixture is a repository")
    }

    /// Run `git`, returning its stdout. Panics with `stderr` on failure —
    /// a fixture that did not build is a broken test, not a result.
    pub fn git(&self, args: &[&str]) -> String {
        let output = self
            .command()
            .args(args)
            .output()
            .expect("git is installed; the tests need it");
        assert!(
            output.status.success(),
            "git {}\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    /// Same, but a failure is the expected outcome — a merge that conflicts.
    pub fn git_allow_failure(&self, args: &[&str]) -> bool {
        self.command()
            .args(args)
            .output()
            .expect("git is installed")
            .status
            .success()
    }

    fn command(&self) -> Command {
        let mut command = Command::new("git");
        let time = format!("{} +0000", self.clock.get());
        command
            .current_dir(&self.path)
            .env("HOME", &self.path)
            .env("XDG_CONFIG_HOME", self.path.join(".config"))
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", &self.author.borrow().0)
            .env("GIT_AUTHOR_EMAIL", &self.author.borrow().1)
            .env("GIT_COMMITTER_NAME", "Test Committer")
            .env("GIT_COMMITTER_EMAIL", "committer@omagit.test")
            .env("GIT_AUTHOR_DATE", &time)
            .env("GIT_COMMITTER_DATE", &time)
            .env("GIT_TERMINAL_PROMPT", "0");
        command
    }

    pub fn write(&self, path: &str, contents: &str) {
        self.write_bytes(path, contents.as_bytes());
    }

    pub fn write_bytes(&self, path: &str, contents: &[u8]) {
        let file = self.path.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).expect("a parent directory");
        }
        std::fs::write(&file, contents).expect("a writable fixture");
    }

    pub fn read(&self, path: &str) -> Vec<u8> {
        std::fs::read(self.path.join(path)).expect("the file exists")
    }

    pub fn mkdir(&self, path: &str) {
        std::fs::create_dir_all(self.path.join(path)).expect("a writable fixture");
    }

    pub fn remove(&self, path: &str) {
        std::fs::remove_file(self.path.join(path)).expect("the file exists");
    }

    pub fn symlink(&self, target: &str, link: &str) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, self.path.join(link)).expect("a symlink");
    }

    pub fn add(&self, path: &str) {
        self.git(&["add", "--", path]);
    }

    pub fn add_all(&self) {
        self.git(&["add", "-A"]);
    }

    /// Stage everything and commit. Each commit advances the clock by a minute,
    /// so history order is unambiguous.
    pub fn commit(&self, message: &str) -> ObjectId {
        self.add_all();
        self.commit_staged(message)
    }

    /// Commit what is already staged, without touching the working tree —
    /// needed to build a state where staged and unstaged differ.
    pub fn commit_staged(&self, message: &str) -> ObjectId {
        self.git(&["commit", "--allow-empty", "-m", message]);
        if !self.frozen.get() {
            self.clock.set(self.clock.get() + 60);
        }
        self.head()
    }

    /// Write one file and commit it: the shape most of these tests need.
    pub fn commit_file(&self, path: &str, contents: &str, message: &str) -> ObjectId {
        self.write(path, contents);
        self.add(path);
        self.commit_staged(message)
    }

    /// Commit with a timestamp `days` days before now.
    ///
    /// The fixed epoch above keeps ordering reproducible, which is all most
    /// tests need. Anything that reads commit dates *relative to the present* —
    /// the activity sparkline — needs real ones instead.
    pub fn commit_days_ago(
        &self,
        days: i64,
        path: &str,
        contents: &str,
        message: &str,
    ) -> ObjectId {
        let saved = self.clock.get();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a clock set after 1970")
            .as_secs() as i64;
        self.clock.set(now - days * 86_400);
        let id = self.commit_file(path, contents, message);
        self.clock.set(saved);
        id
    }

    pub fn head(&self) -> ObjectId {
        self.rev_parse("HEAD")
    }

    pub fn rev_parse(&self, revision: &str) -> ObjectId {
        self.git(&["rev-parse", revision])
            .parse()
            .expect("git printed a hash")
    }

    pub fn branch(&self, name: &str) {
        self.git(&["checkout", "-b", name]);
    }

    pub fn checkout(&self, revision: &str) {
        self.git(&["checkout", revision]);
    }
}

/// Everything omagit reads is cancellable; the tests never cancel.
pub fn never() -> omagit_git::Cancel {
    omagit_git::Cancel::new()
}

/// A repository with the history most read tests want: two branches, a merge, a
/// rename, and a file that stays untouched throughout.
///
/// ```text
///   main:    A ── B ────────── M
///                  \          /
///   feature:        C ── D ──
/// ```
pub fn scripted() -> TestRepo {
    let repo = TestRepo::new();
    repo.commit_file("README.md", "first\n", "A: add README");
    repo.commit_file("src/main.rs", "fn main() {}\n", "B: add main");
    repo.branch("feature");
    repo.commit_file("src/lib.rs", "pub fn one() {}\n", "C: add lib");
    repo.commit_file(
        "src/lib.rs",
        "pub fn one() {}\npub fn two() {}\n",
        "D: extend lib",
    );
    repo.checkout("main");
    repo.git(&["merge", "--no-ff", "-m", "M: merge feature", "feature"]);
    repo
}

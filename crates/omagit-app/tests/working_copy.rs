//! The Working Copy screen, driven against a real repository.
//!
//! Same reasoning as `keyboard.rs`, and the same payoff: the parts that can be
//! reasoned about are unit-tested next to the code, and what is left is the
//! *wiring* — that the screen takes the keyboard, that a key reaches the file
//! list, that selecting a file actually reaches the diff viewer. On
//! Repositories that class of bug shipped invisibly until a test like this one
//! caught it (`docs/ARCHITECTURE.md` risk 11).
//!
//! Unlike the keyboard tests, this one needs a real repository: the screen
//! shows what `git status` says, and a fixture that faked it would be testing
//! the fake.

use std::path::{Path, PathBuf};
use std::process::Command;

use gpui_kit::{AppContext, Entity, TestAppContext, TestDispatcher, VisualTestContext};
use omagit_app::repo_store::{RepoStore, Side};
use omagit_app::screens::WorkingCopyScreen;
use omagit_git::Repository;

/// A repository with three files in three different states.
struct Fixture {
    _dir: tempfile::TempDir,
    path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let fixture = Self {
            path: dir.path().to_path_buf(),
            _dir: dir,
        };
        fixture.git(&["init", "--initial-branch=main"]);
        fixture.git(&["config", "user.name", "Test"]);
        fixture.git(&["config", "user.email", "test@omagit.test"]);
        fixture.git(&["config", "commit.gpgsign", "false"]);

        fixture.write("alpha.rs", "fn alpha() {}\n");
        fixture.write("beta.rs", "fn beta() {}\n");
        fixture.git(&["add", "-A"]);
        fixture.git(&["commit", "-m", "first"]);

        // One modified, one staged, one untracked: enough for both sections and
        // for a diff on each side.
        fixture.write("alpha.rs", "fn alpha() { println!() }\n");
        fixture.write("beta.rs", "fn beta() { println!() }\n");
        fixture.git(&["add", "beta.rs"]);
        fixture.write("gamma.txt", "untracked\n");
        fixture
    }

    fn write(&self, name: &str, contents: &str) {
        std::fs::write(self.path.join(name), contents).expect("a writable fixture");
    }

    fn git(&self, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.path)
            .env("HOME", &self.path)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@omagit.test")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@omagit.test")
            .output()
            .expect("git is installed; the tests need it");
        assert!(
            output.status.success(),
            "git {}\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn screen(cx: &mut TestAppContext, path: &Path) -> (Entity<WorkingCopyScreen>, VisualTestContext) {
    cx.update(|cx| {
        gpui_omarchy::init(cx);
        omagit_ui::fonts::resolve("sans-serif", cx);
        let theme = omagit_theme::catalogue()
            .first()
            .expect("the catalogue is never empty")
            .clone();
        omagit_ui::apply(&theme, omagit_theme::DensityMode::Compact, cx);
        omagit_app::actions::bind(cx);
    });

    let path = path.to_path_buf();
    let window = cx.add_window(|_window, cx| {
        let repo = Repository::open(&path).expect("the fixture is a repository");
        let store = cx.new(|cx| RepoStore::new(repo, path.clone(), cx));
        WorkingCopyScreen::new(store, cx)
    });
    let view = window.root(cx).expect("the window has a root");
    let visual = VisualTestContext::from_window(window.into(), cx);
    // The status is read on the background executor; this is where it lands.
    visual.run_until_parked();
    (view, visual)
}

#[test]
fn the_file_list_shows_both_sides_of_the_index() {
    let fixture = Fixture::new();
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("working-copy"));
    let (screen, visual) = screen(&mut cx, &fixture.path);

    let selected = screen
        .read_with(&visual, |screen, _| screen.selected())
        .expect("the screen opens on a file rather than on an empty panel");
    assert_eq!(
        selected.0,
        Side::Staged,
        "the staged section comes first, so its first file is the one shown"
    );
    assert_eq!(selected.1.to_string(), "beta.rs");
}

#[test]
fn j_walks_the_files_and_the_diff_follows() {
    let fixture = Fixture::new();
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("working-copy"));
    let (screen, mut visual) = screen(&mut cx, &fixture.path);

    // The diff of the opening selection is read in the background too.
    visual.run_until_parked();
    assert_eq!(
        screen
            .read_with(&visual, |screen, cx| screen.shown(cx))
            .map(|path| path.to_string()),
        Some("beta.rs".into()),
        "selecting a file has to reach the diff viewer, not just the list"
    );

    visual.simulate_keystrokes("j");
    visual.run_until_parked();

    let selected = screen
        .read_with(&visual, |screen, _| screen.selected())
        .expect("still on a file");
    assert_eq!(
        (selected.0, selected.1.to_string()),
        (Side::Unstaged, "alpha.rs".into()),
        "down from the last staged file crosses into the unstaged section"
    );
    assert_eq!(
        screen
            .read_with(&visual, |screen, cx| screen.shown(cx))
            .map(|path| path.to_string()),
        Some("alpha.rs".into()),
        "and the diff follows the selection"
    );
}

#[test]
fn k_stops_at_the_top_rather_than_wrapping() {
    let fixture = Fixture::new();
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("working-copy"));
    let (screen, mut visual) = screen(&mut cx, &fixture.path);

    visual.simulate_keystrokes("k k k");
    visual.run_until_parked();
    assert_eq!(
        screen
            .read_with(&visual, |screen, _| screen.selected())
            .map(|(side, path)| (side, path.to_string())),
        Some((Side::Staged, "beta.rs".into())),
        "a list that wraps loses the reader's place"
    );
}

#[test]
fn an_untracked_file_is_listed_and_diffs_as_an_addition() {
    let fixture = Fixture::new();
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("working-copy"));
    let (screen, mut visual) = screen(&mut cx, &fixture.path);

    // Staged beta, then unstaged alpha, then untracked gamma.
    visual.simulate_keystrokes("j j");
    visual.run_until_parked();
    assert_eq!(
        screen
            .read_with(&visual, |screen, _| screen.selected())
            .map(|(side, path)| (side, path.to_string())),
        Some((Side::Unstaged, "gamma.txt".into())),
        "an untracked file is a row like any other — it is half of what the screen is for"
    );
    assert_eq!(
        screen
            .read_with(&visual, |screen, cx| screen.shown(cx))
            .map(|path| path.to_string()),
        Some("gamma.txt".into()),
        "and it has a diff: everything in it is an addition"
    );
}

//! The History screen, against a real repository.
//!
//! The lane algorithm has its own tests in `omagit-git`; these are about the
//! wiring — that opening the screen actually walks the history, that the pages
//! land, and that the rows carry the graph the gutter draws from.

use std::path::{Path, PathBuf};
use std::process::Command;

use gpui_kit::{AppContext, Entity, TestAppContext, TestDispatcher, VisualTestContext};
use omagit_app::repo_store::RepoStore;
use omagit_app::screens::HistoryScreen;
use omagit_git::Repository;

/// A history with a branch and a merge, so the graph has something to say.
struct Fixture {
    _dir: tempfile::TempDir,
    path: PathBuf,
    /// Commits get distinct, increasing timestamps. Without that they all land
    /// in the same second and the walk's date ordering has nothing to sort by,
    /// so which tip comes first is arbitrary — a real history does not have
    /// that problem, and a test that pretends otherwise is testing a tie-break.
    clock: std::cell::Cell<i64>,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let fixture = Self {
            path: dir.path().to_path_buf(),
            _dir: dir,
            clock: std::cell::Cell::new(1_700_000_000),
        };
        fixture.git(&["init", "--initial-branch=main"]);
        fixture.git(&["config", "user.name", "Test"]);
        fixture.git(&["config", "user.email", "test@omagit.test"]);
        fixture.git(&["config", "commit.gpgsign", "false"]);

        fixture.commit("one.txt", "one\n", "first");
        fixture.git(&["checkout", "-q", "-b", "side"]);
        fixture.commit("side.txt", "side\n", "on the branch");
        fixture.git(&["checkout", "-q", "main"]);
        fixture.commit("two.txt", "two\n", "on main");
        fixture.git(&["merge", "--no-ff", "-m", "merge side", "side"]);
        fixture
    }

    fn commit(&self, name: &str, contents: &str, message: &str) {
        std::fs::write(self.path.join(name), contents).expect("a writable fixture");
        self.git(&["add", "-A"]);
        self.git(&["commit", "-m", message]);
    }

    fn tick(&self) -> String {
        self.clock.set(self.clock.get() + 100);
        format!("{} +0000", self.clock.get())
    }

    fn git(&self, args: &[&str]) {
        let when = self.tick();
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.path)
            .env("GIT_AUTHOR_DATE", &when)
            .env("GIT_COMMITTER_DATE", &when)
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

fn screen(
    cx: &mut TestAppContext,
    path: &Path,
) -> (Entity<HistoryScreen>, Entity<RepoStore>, VisualTestContext) {
    cx.update(|cx| {
        gpui_omarchy::init(cx);
        omagit_ui::fonts::resolve("sans-serif", cx);
        let theme = omagit_theme::catalogue()
            .first()
            .expect("the catalogue is never empty")
            .clone();
        omagit_ui::apply(&theme, omagit_theme::DensityMode::Compact, cx);
        omagit_app::actions::bind(cx);
        cx.set_global(omagit_app::git_runtime::GitRuntime::detect());
    });

    let path = path.to_path_buf();
    let mut held: Option<Entity<RepoStore>> = None;
    let window = cx.add_window(|_window, cx| {
        let repo = Repository::open(&path).expect("the fixture is a repository");
        let store = cx.new(|cx| RepoStore::new(repo, path.clone(), cx));
        held = Some(store.clone());
        HistoryScreen::new(store, cx)
    });
    let view = window.root(cx).expect("the window has a root");
    let store = held.expect("the store was built with the screen");
    let visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    (view, store, visual)
}

#[test]
fn opening_the_screen_walks_the_history_and_lays_it_out() {
    let fixture = Fixture::new();
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("history"));
    let (_screen, store, visual) = screen(&mut cx, &fixture.path);

    let (rows, complete) = store.read_with(&visual, |store, _| {
        let history = store.history();
        (
            history
                .rows
                .iter()
                .map(|row| (row.commit.summary.clone(), row.graph.lane, row.graph.width))
                .collect::<Vec<_>>(),
            history.complete,
        )
    });

    assert_eq!(rows.len(), 4, "four commits: two lines and their merge");
    assert!(complete, "a four-commit history is read in one page");
    assert_eq!(rows[0].0, "merge side", "newest first");
    assert_eq!(rows[0].1, 0, "the merge sits in the first lane");
    assert_eq!(rows[0].2, 2, "and opens a second for its other parent");

    let root = rows.last().expect("a root");
    assert_eq!(root.0, "first");
    assert_eq!(root.2, 1, "by the root the gutter has closed again");
}

#[test]
fn the_keyboard_walks_the_list() {
    let fixture = Fixture::new();
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("history"));
    let (screen, _store, mut visual) = screen(&mut cx, &fixture.path);

    assert_eq!(
        screen.read_with(&visual, |screen, cx| screen.selected(cx)),
        None,
        "nothing is selected until something asks for it"
    );

    visual.simulate_keystrokes("j");
    let first = screen
        .read_with(&visual, |screen, cx| screen.selected(cx))
        .expect("the first press selects the newest commit");

    visual.simulate_keystrokes("j");
    let second = screen
        .read_with(&visual, |screen, cx| screen.selected(cx))
        .expect("and the next moves down");
    assert_ne!(first, second, "j has to move");

    visual.simulate_keystrokes("k k k");
    assert_eq!(
        screen.read_with(&visual, |screen, cx| screen.selected(cx)),
        Some(first),
        "and it stops at the top rather than wrapping"
    );
}

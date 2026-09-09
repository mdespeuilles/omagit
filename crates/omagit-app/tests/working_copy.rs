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

impl Fixture {
    /// What `git` says is staged for a path — the question a staging test is
    /// actually asking.
    fn staged(&self, name: &str) -> String {
        let output = Command::new("git")
            .args(["show", &format!(":{name}")])
            .current_dir(&self.path)
            .output()
            .expect("git is on PATH");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.path.join(name)).expect("the file is there")
    }
}

/// Twenty lines with two of them replaced, far enough apart to be two hunks.
fn twenty(second: &str, nineteenth: &str) -> String {
    let mut lines: Vec<String> = (1..=20).map(|n| format!("line {n}")).collect();
    lines[1] = second.to_owned();
    lines[18] = nineteenth.to_owned();
    format!("{}\n", lines.join("\n"))
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
        // The same runtime `main` installs: the screen asks it whether writing
        // is possible at all, and a test that skipped it would be exercising a
        // state the app never reaches.
        cx.set_global(omagit_app::git_runtime::GitRuntime::detect());
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

// ── Writing (M5) ────────────────────────────────────────────────────────────
//
// The Git operations themselves are proved in `omagit-git`'s own tests, against
// real repositories. What these add is the wiring: that a key reaches the right
// action, that the action aims at what the diff cursor is on, and that the write
// actually reaches the repository. That is the class of bug this project has
// shipped before.

/// A repository whose one modified file has two separate hunks.
fn two_hunk_fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let fixture = Fixture {
        path: dir.path().to_path_buf(),
        _dir: dir,
    };
    fixture.git(&["init", "--initial-branch=main"]);
    fixture.git(&["config", "user.name", "Test"]);
    fixture.git(&["config", "user.email", "test@omagit.test"]);
    fixture.git(&["config", "commit.gpgsign", "false"]);
    fixture.write("file.txt", &twenty("line 2", "line 19"));
    fixture.git(&["add", "-A"]);
    fixture.git(&["commit", "-m", "base"]);
    fixture.write("file.txt", &twenty("SECOND", "NINETEENTH"));
    fixture
}

#[test]
fn alt_s_stages_the_hunk_the_diff_cursor_is_in() {
    let fixture = two_hunk_fixture();
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("writes"));
    let (_screen, mut visual) = screen(&mut cx, &fixture.path);

    // The screen opens on the only file, and its diff loads.
    visual.run_until_parked();

    // The cursor starts on the first hunk's header.
    visual.simulate_keystrokes("alt-s");
    visual.run_until_parked();

    assert_eq!(
        fixture.staged("file.txt"),
        twenty("SECOND", "line 19"),
        "only the hunk under the cursor should have reached the index"
    );
    assert_eq!(
        fixture.read("file.txt"),
        twenty("SECOND", "NINETEENTH"),
        "and the working tree keeps both changes"
    );
}

#[test]
fn the_second_hunk_is_reachable_and_stages_on_its_own() {
    // Proves the cursor actually moves: without `alt-shift-j` doing anything,
    // this would stage the first hunk and the assertion would catch it.
    let fixture = two_hunk_fixture();
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("writes"));
    let (_screen, mut visual) = screen(&mut cx, &fixture.path);
    visual.run_until_parked();

    visual.simulate_keystrokes("alt-shift-j");
    visual.simulate_keystrokes("alt-s");
    visual.run_until_parked();

    assert_eq!(
        fixture.staged("file.txt"),
        twenty("line 2", "NINETEENTH"),
        "the second hunk, not the first"
    );
}

#[test]
fn discarding_asks_before_it_removes_anything() {
    // SPEC §3 rule 7. The key must not act on its own, and Escape must answer.
    let fixture = two_hunk_fixture();
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("writes"));
    let (screen, mut visual) = screen(&mut cx, &fixture.path);
    visual.run_until_parked();

    visual.simulate_keystrokes("alt-d");
    visual.run_until_parked();
    assert!(
        screen.read_with(&visual, |screen, _| screen.is_confirming()),
        "a destructive key opens a question, not a command"
    );
    assert_eq!(
        fixture.read("file.txt"),
        twenty("SECOND", "NINETEENTH"),
        "and nothing has happened yet"
    );

    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    assert!(
        !screen.read_with(&visual, |screen, _| screen.is_confirming()),
        "Escape answers the question"
    );
    assert_eq!(
        fixture.read("file.txt"),
        twenty("SECOND", "NINETEENTH"),
        "answering no changes nothing"
    );
}

#[test]
fn a_whole_file_stages_without_its_diff_ever_being_read() {
    // The gesture board 03 draws as a checkbox, and the bug it used to have:
    // modelling "the file" as a kind of selection meant it needed the diff, so
    // the checkbox on a row nobody had opened did nothing.
    let fixture = Fixture::new();
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("writes"));
    let (screen, mut visual) = screen(&mut cx, &fixture.path);
    visual.run_until_parked();

    // `alpha.rs` is modified and unstaged. The screen opens on some file; this
    // one is deliberately acted on without selecting it first.
    let path = omagit_git::RepoPath::from_bytes(b"alpha.rs".to_vec());
    screen.update(&mut visual, |screen, cx| {
        screen.stage_whole_file(Side::Unstaged, path, cx);
    });
    visual.run_until_parked();

    assert_eq!(
        fixture.staged("alpha.rs"),
        "fn alpha() { println!() }\n",
        "the file has to reach the index without its diff being read first"
    );
}

#[test]
fn a_key_stages_every_unstaged_file_at_once() {
    // `a`, from DESIGN board 09.
    let fixture = Fixture::new();
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("writes"));
    let (_screen, mut visual) = screen(&mut cx, &fixture.path);
    visual.run_until_parked();

    visual.simulate_keystrokes("a");
    visual.run_until_parked();

    assert_eq!(
        fixture.staged("alpha.rs"),
        "fn alpha() { println!() }\n",
        "the modified file is staged"
    );
    assert_eq!(
        fixture.staged("gamma.txt"),
        "untracked\n",
        "and so is the untracked one, which is what `git add` is for"
    );
}

#[test]
fn a_commit_template_fills_the_empty_message_box() {
    // SPEC §11. The reader was written and never called; this is what says it
    // is called.
    let fixture = Fixture::new();
    fixture.write(".gitmessage", "# Pourquoi, pas quoi.\n");
    fixture.git(&["config", "commit.template", ".gitmessage"]);

    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("writes"));
    let (screen, mut visual) = screen(&mut cx, &fixture.path);
    visual.run_until_parked();
    // The template lands on the render after the read, so force the frame.
    visual.refresh().expect("a frame");
    visual.run_until_parked();

    assert_eq!(
        screen.read_with(&visual, |screen, cx| screen.message_for_test(cx)),
        "# Pourquoi, pas quoi.\n",
        "the template has to reach the box"
    );
}

#[test]
fn no_template_leaves_the_box_empty() {
    let fixture = Fixture::new();
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("writes"));
    let (screen, mut visual) = screen(&mut cx, &fixture.path);
    visual.run_until_parked();
    visual.refresh().expect("a frame");
    visual.run_until_parked();

    assert_eq!(
        screen.read_with(&visual, |screen, cx| screen.message_for_test(cx)),
        "",
        "a repository without a template gets nothing put in its box"
    );
}

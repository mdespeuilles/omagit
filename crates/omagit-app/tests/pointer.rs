//! The Repositories screen, driven from the pointer.
//!
//! `keyboard.rs` covers the keys. These cover what a hand does with a mouse,
//! which is where four bugs got through in one session: a row that answered a
//! click with nothing, a trail that led nowhere, a screen that could not be
//! left, and a crash on adding a repository.
//!
//! ## What these cannot cover, and why it is written here rather than assumed
//!
//! The crash was Git work running on the render thread, caught in production by
//! `omagit_git::assert_off_render_thread`. That guard cannot fire in a test:
//! GPUI's `TestDispatcher` is deterministic and runs background tasks on the
//! test's own thread, so marking that thread as the render thread would make
//! correct code panic too. The threading rule is enforced at runtime, by the
//! guard, on a real executor — not here. What is covered here is the flow
//! itself, end to end, which is what nobody had exercised.

use gpui_kit::{
    AppContext, Entity, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point,
    TestAppContext, TestDispatcher, VisualTestContext,
};
use omagit_app::actions::AddLocalRepository;
use omagit_app::screens::RepositoriesScreen;
use omagit_app::store::Store;
use omagit_settings::Location;

/// One repository, so the row under test is unambiguous.
const LIBRARY: &str = r#"
[[group]]
name = "Récents"
collapsed = false

[[group.repository]]
path = "/nowhere/alpha"
name = "alpha"
"#;

/// The screen, the store behind it, and the window to send events to. The store
/// is handed back rather than reached through the screen: what these tests
/// assert is what the library ends up holding, which is the store's business.
fn screen(
    cx: &mut TestAppContext,
) -> (Entity<RepositoriesScreen>, Entity<Store>, VisualTestContext) {
    let dir = tempfile::tempdir().expect("a temporary config directory");
    std::fs::write(dir.path().join("repositories.toml"), LIBRARY).expect("a writable fixture");

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

    // The directory outlives the handle: the store writes to it whenever the
    // library changes.
    let path = dir.keep();
    let mut held: Option<Entity<Store>> = None;
    let window = cx.add_window(|window, cx| {
        let store = cx.new(|cx| Store::new(Some(path), cx));
        held = Some(store.clone());
        RepositoriesScreen::new(store, window, cx)
    });
    let view = window.root(cx).expect("the window has a root");
    let store = held.expect("the store was built with the screen");
    let visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    (view, store, visual)
}

/// The middle of a row, found by the selector the row carries rather than by a
/// coordinate someone worked out once and that the next layout change breaks.
fn row_centre(visual: &mut VisualTestContext, at: Location) -> Point<Pixels> {
    let selector: &'static str =
        Box::leak(format!("repository:{}:{}", at.group, at.index).into_boxed_str());
    let bounds = visual
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("the row {selector} is on screen"));
    bounds.center()
}

/// A click carrying `count`, which is what tells a second click from the first.
/// `VisualTestContext::simulate_click` always sends 1.
fn click(visual: &mut VisualTestContext, position: Point<Pixels>, count: usize) {
    visual.simulate_event(MouseDownEvent {
        position,
        modifiers: Modifiers::default(),
        button: MouseButton::Left,
        click_count: count,
        first_mouse: false,
    });
    visual.simulate_event(MouseUpEvent {
        position,
        modifiers: Modifiers::default(),
        button: MouseButton::Left,
        click_count: count,
    });
    visual.run_until_parked();
}

#[test]
fn a_first_click_selects_and_a_second_opens() {
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("pointer"));
    let (screen, store, mut visual) = screen(&mut cx);
    let at = Location { group: 0, index: 0 };

    let opened = |visual: &VisualTestContext| {
        store.read_with(visual, |store, _| store.open_repository().is_some())
    };
    assert!(
        !opened(&visual),
        "nothing is open before anything is clicked"
    );

    let centre = row_centre(&mut visual, at);

    // One click selects. The card is a surface meant to be read about a
    // repository without committing to switching to it.
    click(&mut visual, centre, 1);
    assert_eq!(
        screen.read_with(&visual, |screen, _| screen.selected()),
        Some(at)
    );
    assert!(
        !opened(&visual),
        "reading a repository's card must not open it"
    );

    // The second opens, the way every list-and-detail does.
    click(&mut visual, centre, 2);
    assert!(
        opened(&visual),
        "a second click on a row has to open the repository"
    );
}

#[test]
fn adding_a_repository_goes_from_the_picker_to_the_list() {
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("pointer"));
    let (_screen, store, mut visual) = screen(&mut cx);

    // A real repository, because the screen keeps only what opens as one.
    let repository = tempfile::tempdir().expect("a temporary repository");
    let status = std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(repository.path())
        .status()
        .expect("git is on PATH");
    assert!(status.success(), "git init failed");
    let path = std::fs::canonicalize(repository.path()).expect("a real path");

    // The action first, then the answer: the prompt has to be open before it can
    // be responded to. Dispatched rather than typed — which key opens it is
    // `keyboard.rs`'s business, and it differs by platform.
    visual.dispatch_action(AddLocalRepository);
    visual.run_until_parked();
    assert!(
        visual.did_prompt_for_paths(),
        "adding a repository has to ask where"
    );

    let answer = path.clone();
    visual.simulate_path_prompt_response(move |_| Some(vec![answer]));
    visual.run_until_parked();

    let found = store.read_with(&visual, |store, _| store.library().find(&path).is_some());
    assert!(found, "the picked repository has to reach the library");
}

#[test]
fn a_folder_that_is_not_a_repository_is_kept_out() {
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("pointer"));
    let (_screen, store, mut visual) = screen(&mut cx);

    let plain = tempfile::tempdir().expect("a temporary folder");
    let path = std::fs::canonicalize(plain.path()).expect("a real path");

    let before = store.read_with(&visual, |store, _| store.library().len());

    visual.dispatch_action(AddLocalRepository);
    visual.run_until_parked();
    let answer = path.clone();
    visual.simulate_path_prompt_response(move |_| Some(vec![answer]));
    visual.run_until_parked();

    let after = store.read_with(&visual, |store, _| store.library().len());
    assert_eq!(before, after, "a plain folder must not be added");
    assert!(store.read_with(&visual, |store, _| store.library().find(&path).is_none()));
}

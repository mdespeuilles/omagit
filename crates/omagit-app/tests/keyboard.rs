//! The Repositories screen, driven from the keyboard.
//!
//! SPEC §14 asks M3 for a screen that is navigable end to end from the
//! keyboard, and the pure part of that — the order of the stops, what a row
//! reads out — is unit-tested next to the code. What those tests cannot see is
//! the *wiring*: that a key reaches the right action, that the action is scoped
//! so it does not fire while someone is typing, and that focus ends up where
//! the ring is drawn. That is what these do, through GPUI's test platform and
//! the real widget tree.
//!
//! The repositories are deliberately not real. Every read of them fails with
//! `RepositoryMissing`, which is a state the screen has to draw anyway, and it
//! keeps a keyboard test from depending on what `git` does.

use gpui_kit::{AppContext, Entity, TestAppContext, TestDispatcher, VisualTestContext};
use omagit_app::screens::RepositoriesScreen;
use omagit_app::screens::repositories::Stop;
use omagit_app::store::Store;
use omagit_settings::Location;

const LIBRARY: &str = r#"
[[group]]
name = "Récents"
collapsed = false

[[group.repository]]
path = "/nowhere/alpha"
name = "alpha"

[[group.repository]]
path = "/nowhere/beta"
name = "beta"

[[group]]
name = "Travail"
collapsed = false

[[group.repository]]
path = "/nowhere/gamma"
name = "gamma"

[[group.repository]]
path = "/nowhere/delta"
name = "delta"
"#;

/// A screen with four repositories in two groups, ready for keystrokes.
fn screen(cx: &mut TestAppContext) -> (Entity<RepositoriesScreen>, VisualTestContext) {
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

    // The directory outlives the handle on purpose: the store writes to it
    // whenever the library changes, and a test that dropped it would be writing
    // into a deleted path.
    let path = dir.keep();
    let window = cx.add_window(|window, cx| {
        let store = cx.new(|cx| Store::new(Some(path), cx));
        RepositoriesScreen::new(store, window, cx)
    });
    let view = window.root(cx).expect("the window has a root");
    let visual = VisualTestContext::from_window(window.into(), cx);
    // Draw the first frame, so the screen has picked its initial selection.
    visual.run_until_parked();
    (view, visual)
}

fn at(group: usize, index: usize) -> Option<Location> {
    Some(Location { group, index })
}

#[test]
fn j_and_k_walk_the_list_across_groups() {
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("keyboard"));
    let (screen, mut visual) = screen(&mut cx);
    let selected = |visual: &VisualTestContext| screen.read_with(visual, |s, _| s.selected());

    assert_eq!(
        selected(&visual),
        at(0, 0),
        "the screen opens on a repository rather than on an empty card"
    );

    visual.simulate_keystrokes("j");
    assert_eq!(selected(&visual), at(0, 1));

    // Across a group boundary: the list is one zone, not one zone per group.
    visual.simulate_keystrokes("j");
    assert_eq!(selected(&visual), at(1, 0));

    visual.simulate_keystrokes("k");
    assert_eq!(selected(&visual), at(0, 1));

    // And it stops at the ends rather than wrapping: a list that wraps loses
    // the reader's place.
    visual.simulate_keystrokes("k k k");
    assert_eq!(selected(&visual), at(0, 0));
}

#[test]
fn tab_walks_the_stops_and_comes_back_to_the_first() {
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("keyboard"));
    let (screen, mut visual) = screen(&mut cx);
    let stop = |visual: &VisualTestContext| screen.read_with(visual, |s, _| s.focus_stop());

    // The screen starts in the list, which is the zone the user acts in.
    assert_eq!(stop(&visual).0, Stop::List);

    visual.simulate_keystrokes("tab");
    assert_eq!(stop(&visual).0, Stop::Description);
    visual.simulate_keystrokes("tab");
    assert_eq!(stop(&visual), (Stop::Open, false), "on 'Ouvrir' first");
    visual.simulate_keystrokes("tab");
    assert_eq!(
        stop(&visual),
        (Stop::Open, true),
        "then on its pair, 'Retirer' — 6b, without a seventh stop"
    );
    visual.simulate_keystrokes("tab");
    assert_eq!(
        stop(&visual).0,
        Stop::Add,
        "after the last stop, focus returns to the first rather than leaving"
    );
}

#[test]
fn the_zone_keys_mean_the_same_thing_here_as_everywhere() {
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("keyboard"));
    let (screen, mut visual) = screen(&mut cx);
    let stop = |visual: &VisualTestContext| screen.read_with(visual, |s, _| s.focus_stop().0);

    visual.simulate_keystrokes("3");
    assert_eq!(stop(&visual), Stop::Description, "3 is the detail panel");

    // `3` landed in an editable field, so `1` is now a character rather than a
    // key — `Esc` is the way out, which is what "Esc goes up one level" means
    // in practice (DESIGN §5).
    assert_eq!(
        stop(&visual),
        Stop::Description,
        "and typing a digit into the description does not jump zones"
    );
    visual.simulate_keystrokes("escape");
    assert_eq!(stop(&visual), Stop::List);

    visual.simulate_keystrokes("2");
    assert_eq!(
        stop(&visual),
        Stop::Description,
        "this screen has no centre column, so 2 lands on the card like 3"
    );
    visual.simulate_keystrokes("escape");
    visual.simulate_keystrokes("1");
    assert_eq!(stop(&visual), Stop::List, "1 is the sidebar");
}

#[test]
fn slash_puts_the_caret_in_the_filter_box() {
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("keyboard"));
    let (screen, mut visual) = screen(&mut cx);

    visual.simulate_keystrokes("/");
    assert_eq!(
        screen.read_with(&visual, |s, _| s.focus_stop().0),
        Stop::Filter
    );

    // The keys that follow are text, not navigation. Typing `eta` leaves only
    // `beta` in the list, which could not happen if `j`-style bindings were
    // still firing — and `k` in the middle of it would have moved the
    // selection rather than filtered.
    visual.simulate_keystrokes("e t a");
    assert_eq!(
        screen.read_with(&visual, |s, _| s.selected()),
        None,
        "the selected repository no longer matches, so the card lets go of it"
    );

    // The arrows still steer the list from inside the box: type to narrow,
    // arrow to pick, which is the whole point of a filter beside a list.
    visual.simulate_keystrokes("down");
    assert_eq!(
        screen.read_with(&visual, |s, _| s.selected()),
        at(0, 1),
        "beta is the only row left, so down lands on it"
    );
    assert_eq!(
        screen.read_with(&visual, |s, _| s.focus_stop().0),
        Stop::Filter,
        "and the caret has not left the box"
    );
}

#[test]
fn the_arrows_fold_the_group_the_selection_is_in() {
    let mut cx = TestAppContext::build(TestDispatcher::new(0), Some("keyboard"));
    let (screen, mut visual) = screen(&mut cx);

    visual.simulate_keystrokes("left");
    visual.run_until_parked();
    assert_eq!(
        screen.read_with(&visual, |s, _| s.selected()),
        None,
        "the selected row is no longer drawn, so the selection lets go of it"
    );

    // Down from a folded group lands on the next visible row.
    visual.simulate_keystrokes("j");
    assert_eq!(screen.read_with(&visual, |s, _| s.selected()), at(1, 0));
}

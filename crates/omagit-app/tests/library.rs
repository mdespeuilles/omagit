//! Filing repositories into folders, through the commands the window calls.
//!
//! `omagit-settings` tests the model — what a move does to the list, what a
//! group deletion keeps. What is tested here is the *view* the window is sent,
//! and one line of it in particular: the folders are read from the folders and
//! not derived from the rows. Derived, a group with nothing in it would vanish
//! the instant it was made, and the button that made it would read as broken.
//!
//! Tauri's mock runtime is what gives a `State<'_, AppState>` outside a window.

use omagit_lib::commands;
use omagit_lib::state::AppState;
use tauri::Manager;

/// An application whose settings live in a temporary directory, so the library
/// is written and read back rather than held in memory.
fn app(dir: &std::path::Path) -> tauri::App<tauri::test::MockRuntime> {
    tauri::test::mock_builder()
        .manage(AppState::detect(Some(dir.to_owned())))
        .build(tauri::generate_context!("tauri.conf.json"))
        .expect("the mock application builds")
}

/// A repository the library can hold: `add_repository` opens it to find out
/// whether it is one, so an empty directory will not do.
fn repository(at: &std::path::Path) {
    std::fs::create_dir_all(at).expect("mkdir");
    let status = std::process::Command::new("git")
        .args(["init", "--initial-branch=main"])
        .current_dir(at)
        .output()
        .expect("git runs");
    assert!(status.status.success(), "git init: {status:?}");
}

#[test]
fn a_folder_with_nothing_in_it_is_still_sent_to_the_window() {
    let dir = tempfile::tempdir().expect("tempdir");
    let app = app(dir.path());
    let state = app.state::<AppState>();

    let alpha = dir.path().join("alpha");
    repository(&alpha);
    commands::add_repository(state.clone(), alpha.display().to_string()).expect("a repository");

    let made = commands::create_group(state.clone(), "  Travail  ".into());
    assert_eq!(made, 1, "after the one the first repository created");

    let view = commands::repositories(state.clone());
    assert_eq!(view.groups.len(), 2, "the empty one is there too");
    assert_eq!(view.groups[1].name, "Travail", "trimmed");
    assert!(!view.groups[1].collapsed);
    assert_eq!(view.rows.len(), 1, "and it holds nothing yet");
    assert_eq!(view.rows[0].group, 0);
}

#[test]
fn a_repository_moves_into_a_folder_and_stays_there() {
    let dir = tempfile::tempdir().expect("tempdir");
    let app = app(dir.path());
    let state = app.state::<AppState>();

    let alpha = dir.path().join("alpha");
    repository(&alpha);
    commands::add_repository(state.clone(), alpha.display().to_string()).expect("a repository");
    let work = commands::create_group(state.clone(), "Travail".into());

    // The row's own path, which is what the window has and not what was typed
    // into the picker: the library stores the canonicalised one, and on macOS
    // a repository under `$TMPDIR` is listed as `/private/var/…`.
    let path = commands::repositories(state.clone()).rows[0].path.clone();
    commands::move_repository(state.clone(), path.clone(), work, 0);
    commands::collapse_group(state.clone(), work, true);

    let view = commands::repositories(state.clone());
    assert_eq!(view.rows[0].group, work);
    assert_eq!(view.rows[0].group_name, "Travail");
    assert!(view.groups[work].collapsed);

    // And it is on disk: this list is the one thing the window remembers
    // between launches, so an arrangement that does not survive a restart is
    // not an arrangement.
    let reread = omagit_settings::Library::load(dir.path());
    assert_eq!(
        reread.groups[work].repositories[0].path,
        std::path::PathBuf::from(&path)
    );
    assert!(reread.groups[work].collapsed);
}

#[test]
fn deleting_a_folder_hands_its_repositories_back() {
    let dir = tempfile::tempdir().expect("tempdir");
    let app = app(dir.path());
    let state = app.state::<AppState>();

    let alpha = dir.path().join("alpha");
    repository(&alpha);
    commands::add_repository(state.clone(), alpha.display().to_string()).expect("a repository");
    let work = commands::create_group(state.clone(), "Travail".into());
    let path = commands::repositories(state.clone()).rows[0].path.clone();
    commands::move_repository(state.clone(), path, work, 0);

    commands::remove_group(state.clone(), work);

    let view = commands::repositories(state.clone());
    assert_eq!(view.groups.len(), 1);
    assert_eq!(view.rows.len(), 1, "the repository is still listed");
    assert_eq!(view.rows[0].group, 0, "back in the folder above");
}

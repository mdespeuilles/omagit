//! The native menu bar (SPEC §9: « barre de menus native obligatoire. Sans
//! elle, l'app paraît cassée »).
//!
//! **The items come from the front end, not from here.** `web/src/keymap.ts`
//! holds every action the window answers, and the menu bar is its fourth reader
//! after the key handler, the palette and the `?` sheet. A menu declared in Rust
//! would be a second list of what the app can do, and this project has the
//! receipt for what a second list does — `KEYMAP.md` spent three milestones
//! describing a build that had been deleted (§5, defect 12). So the front end
//! sends the table at start-up and this file arranges it.
//!
//! What is *not* in the table is here: the predefined items — Quitter, Masquer,
//! Annuler, Copier, Coller, Tout sélectionner, Réduire, Plein écran. They are
//! not omagit actions; they are the platform's, and on macOS the Édition menu is
//! what makes `⌘Z`, `⌘A` and `⌘C` work at all inside a WKWebView. An app without
//! it has a commit box you cannot undo in.

use tauri::menu::{
    AboutMetadata, HELP_SUBMENU_ID, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu,
    WINDOW_SUBMENU_ID,
};
use tauri::{AppHandle, Runtime};

/// One command, as the front end's table describes it.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Entry {
    pub id: String,
    pub label: String,
    /// The binding in the front end's spelling: `Primary+F`, `Shift+Primary+N`.
    /// Translated by [`accelerator`], which is also where the rule about what
    /// may *become* an accelerator lives.
    pub binding: String,
    pub menu: Place,
    /// Whether the item is offered. Coarse on purpose — see [`install`].
    pub enabled: bool,
}

/// The words for everything in the bar that is *not* an omagit action.
///
/// Quitter, Masquer, Annuler, Coller, Plein écran and the menu titles are the
/// platform's items, not the table's — and they still have to be said in the
/// language the window is in, which this side does not know. So the front end
/// sends them with the actions, from the same catalogue.
///
/// A map rather than twenty fields: the set will change with the bar, and a
/// missing entry falls back to English rather than refusing to build a menu.
#[derive(Debug, Default, Clone, serde::Deserialize)]
pub struct Labels(std::collections::BTreeMap<String, String>);

impl Labels {
    fn get<'a>(&'a self, key: &str, fallback: &'a str) -> &'a str {
        self.0.get(key).map(String::as_str).unwrap_or(fallback)
    }
}

/// Which menu an action belongs under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Place {
    App,
    File,
    View,
    Repository,
    Help,
}

/// The binding as a menu accelerator, when it may be one.
///
/// **Only a binding with `Primary` becomes an accelerator**, and that is a rule
/// with a reason rather than a limitation. A menu accelerator is answered by the
/// window system *before* the web view sees the key, so a binding registered
/// here can never reach a text field. `⌘F` is meant to fetch while you are
/// typing (KEYMAP.md), so it belongs here; `⇧?` opens the shortcut sheet and
/// must yield to anyone typing a question mark into a commit message, so it does
/// not. The item is still in the menu, still runs on a click, and the key still
/// works — through the front end, which knows about the caret.
pub fn accelerator(binding: &str) -> Option<String> {
    let mut parts: Vec<&str> = binding.split('+').collect();
    let key = parts.pop()?;
    if key.is_empty() || !parts.contains(&"Primary") {
        return None;
    }
    let mut spelled = String::new();
    if parts.contains(&"Shift") {
        spelled.push_str("Shift+");
    }
    if parts.contains(&"Alt") {
        spelled.push_str("Alt+");
    }
    // `CmdOrCtrl` is the accelerator dialect's own name for what SPEC §9 calls
    // the primary modifier, and it resolves per platform exactly as we do.
    spelled.push_str("CmdOrCtrl+");
    spelled.push_str(key);
    Some(spelled)
}

/// Build the menu bar and hang it on the application.
///
/// `enabled` comes from the front end and is deliberately coarse: an item is
/// greyed when the *screen* cannot do it — no repository open — and not when a
/// fetch happens to be running. The fine-grained refusal stays where it already
/// is, in the action's own `enabled()`, because the alternative is rebuilding
/// the menu bar on every state change, including while one of its menus is
/// pulled down.
pub fn install<R: Runtime>(
    app: &AppHandle<R>,
    entries: &[Entry],
    labels: &Labels,
) -> tauri::Result<()> {
    let menu = build(app, entries, labels)?;
    app.set_menu(menu)?;
    Ok(())
}

fn build<R: Runtime>(
    app: &AppHandle<R>,
    entries: &[Entry],
    labels: &Labels,
) -> tauri::Result<Menu<R>> {
    let info = app.package_info();
    let about = AboutMetadata {
        name: Some(info.name.clone()),
        version: Some(info.version.to_string()),
        copyright: app.config().bundle.copyright.clone(),
        ..Default::default()
    };

    // Each block owns its items for as long as the submenu is built from them:
    // `Submenu::with_items` takes references, so the vectors have to outlive the
    // call rather than be built inline.
    let own = |place: Place| -> tauri::Result<Vec<MenuItem<R>>> { items(app, entries, place) };

    let app_own = own(Place::App)?;
    let file_own = own(Place::File)?;
    let view_own = own(Place::View)?;
    let repository_own = own(Place::Repository)?;
    let help_own = own(Place::Help)?;

    let separator = PredefinedMenuItem::separator(app)?;

    let mut app_items: Vec<&dyn IsMenuItem<R>> = Vec::new();
    let about_item = PredefinedMenuItem::about(
        app,
        Some(labels.get("menu.about", "About omagit")),
        Some(about),
    )?;
    let services =
        PredefinedMenuItem::services(app, Some(labels.get("menu.services", "Services")))?;
    let hide = PredefinedMenuItem::hide(app, Some(labels.get("menu.hide", "Hide omagit")))?;
    let hide_others =
        PredefinedMenuItem::hide_others(app, Some(labels.get("menu.hideOthers", "Hide others")))?;
    let show_all = PredefinedMenuItem::show_all(app, Some(labels.get("menu.showAll", "Show all")))?;
    let quit = PredefinedMenuItem::quit(app, Some(labels.get("menu.quit", "Quit omagit")))?;
    app_items.push(&about_item);
    app_items.push(&separator);
    push(&mut app_items, &app_own);
    app_items.push(&separator);
    app_items.push(&services);
    app_items.push(&separator);
    app_items.push(&hide);
    app_items.push(&hide_others);
    app_items.push(&show_all);
    app_items.push(&separator);
    app_items.push(&quit);

    let close = PredefinedMenuItem::close_window(
        app,
        Some(labels.get("menu.closeWindow", "Close window")),
    )?;
    let mut file_items: Vec<&dyn IsMenuItem<R>> = Vec::new();
    push(&mut file_items, &file_own);
    file_items.push(&separator);
    file_items.push(&close);

    // Nothing of ours in Édition: these are the platform's own, and that is the
    // point of the menu — without it the web view has no undo and no select-all.
    let undo = PredefinedMenuItem::undo(app, Some(labels.get("menu.undo", "Undo")))?;
    let redo = PredefinedMenuItem::redo(app, Some(labels.get("menu.redo", "Redo")))?;
    let cut = PredefinedMenuItem::cut(app, Some(labels.get("menu.cut", "Cut")))?;
    let copy = PredefinedMenuItem::copy(app, Some(labels.get("menu.copy", "Copy")))?;
    let paste = PredefinedMenuItem::paste(app, Some(labels.get("menu.paste", "Paste")))?;
    let select_all =
        PredefinedMenuItem::select_all(app, Some(labels.get("menu.selectAll", "Select all")))?;
    let edit = Submenu::with_items(
        app,
        labels.get("menu.edit", "Edit"),
        true,
        &[
            &undo,
            &redo,
            &separator,
            &cut,
            &copy,
            &paste,
            &separator,
            &select_all,
        ],
    )?;

    let fullscreen =
        PredefinedMenuItem::fullscreen(app, Some(labels.get("menu.fullscreen", "Full screen")))?;
    let mut view_items: Vec<&dyn IsMenuItem<R>> = Vec::new();
    push(&mut view_items, &view_own);
    view_items.push(&separator);
    view_items.push(&fullscreen);

    let mut repository_items: Vec<&dyn IsMenuItem<R>> = Vec::new();
    push(&mut repository_items, &repository_own);

    // The two ids macOS looks for: the Window menu is where the system puts its
    // own window list, and the Help menu is where its search field goes.
    let minimize =
        PredefinedMenuItem::minimize(app, Some(labels.get("menu.minimize", "Minimise")))?;
    let maximize = PredefinedMenuItem::maximize(app, Some(labels.get("menu.zoom", "Zoom")))?;
    let window = Submenu::with_id_and_items(
        app,
        WINDOW_SUBMENU_ID,
        labels.get("menu.window", "Window"),
        true,
        &[&minimize, &maximize, &separator, &close],
    )?;

    let mut help_items: Vec<&dyn IsMenuItem<R>> = Vec::new();
    push(&mut help_items, &help_own);
    let help = Submenu::with_id_and_items(
        app,
        HELP_SUBMENU_ID,
        labels.get("menu.help", "Help"),
        true,
        &help_items,
    )?;

    Menu::with_items(
        app,
        &[
            // macOS ignores this title and prints the bundle name, which is why
            // it is the application's own name here rather than a word.
            &Submenu::with_items(app, &info.name, true, &app_items)?,
            &Submenu::with_items(app, labels.get("menu.file", "File"), true, &file_items)?,
            &edit,
            &Submenu::with_items(app, labels.get("menu.view", "View"), true, &view_items)?,
            &Submenu::with_items(
                app,
                labels.get("menu.repository", "Repository"),
                true,
                &repository_items,
            )?,
            &window,
            &help,
        ],
    )
}

/// The entries of one menu, in the order the table gave them.
fn items<R: Runtime>(
    app: &AppHandle<R>,
    entries: &[Entry],
    place: Place,
) -> tauri::Result<Vec<MenuItem<R>>> {
    entries
        .iter()
        .filter(|entry| entry.menu == place)
        .map(|entry| {
            MenuItem::with_id(
                app,
                &entry.id,
                &entry.label,
                entry.enabled,
                accelerator(&entry.binding).as_deref(),
            )
        })
        .collect()
}

fn push<'a, R: Runtime>(into: &mut Vec<&'a dyn IsMenuItem<R>>, items: &'a [MenuItem<R>]) {
    into.extend(items.iter().map(|item| item as &dyn IsMenuItem<R>));
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn a_primary_binding_becomes_an_accelerator() {
        assert_eq!(accelerator("Primary+F").as_deref(), Some("CmdOrCtrl+F"));
        assert_eq!(
            accelerator("Shift+Primary+N").as_deref(),
            Some("Shift+CmdOrCtrl+N")
        );
        assert_eq!(accelerator("Primary+,").as_deref(), Some("CmdOrCtrl+,"));
    }

    #[test]
    fn the_wire_shape_is_the_one_menu_ts_sends() {
        // The half no front-end test can reach: `menu.ts` builds these objects
        // by hand, and a field renamed on one side of the wire is a menu bar
        // that silently fails to build at start-up.
        let sent = r#"[
            {
              "id": "network.fetch",
              "label": "Fetch",
              "binding": "Primary+F",
              "menu": "repository",
              "enabled": true
            },
            {
              "id": "help.shortcuts",
              "label": "Raccourcis clavier",
              "binding": "Shift+?",
              "menu": "help",
              "enabled": true
            }
        ]"#;

        let entries: Vec<Entry> = serde_json::from_str(sent).expect("the shape menu.ts sends");
        assert_eq!(entries[0].menu, Place::Repository);
        assert_eq!(
            accelerator(&entries[0].binding).as_deref(),
            Some("CmdOrCtrl+F")
        );
        assert_eq!(entries[1].menu, Place::Help);
        assert_eq!(accelerator(&entries[1].binding), None);
    }

    #[test]
    fn a_binding_that_must_reach_a_text_field_does_not() {
        // The whole rule in one assertion: an accelerator is answered before the
        // web view, so `⇧?` as one would mean nobody could type a question mark
        // into a commit message. The item still exists in the menu; only the key
        // stays with the front end.
        assert_eq!(accelerator("Shift+?"), None);
        assert_eq!(accelerator("g"), None);
        assert_eq!(accelerator(""), None);
    }
}

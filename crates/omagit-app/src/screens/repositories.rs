//! The Repositories screen — DESIGN.md board 06.
//!
//! A sidebar of repositories filed into collapsible, drag-reorderable groups,
//! and a card that reads out everything known about the selected one. It is the
//! first screen that is navigable end to end from the keyboard (SPEC §14, M3),
//! which is the part that shapes the code: six tab stops in a fixed order, one
//! zone per stop, arrows inside a zone, and `1`/`3` to jump between them.
//!
//! Focus is held here rather than delegated to the renderer's tab traversal.
//! DESIGN §5 asks for six stops with a defined order, two of which hold a pair
//! of buttons reached with the arrows, and for Tab to wrap back to the first
//! rather than escape into the window decoration. That is a specific behaviour,
//! not the generic one, so it is written out.
//!
//! What this screen deliberately does **not** do yet, each because its
//! milestone has not landed: cloning (M7, with the network and its progress
//! overlay), the command palette (M9), and opening a repository into a Working
//! Copy screen (M4) — "Ouvrir" records the repository as open and the topbar
//! names it, which is as far as there is anywhere to go.

use std::path::PathBuf;

use gpui_kit::base::input::InputState;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ClickEvent, ClipboardItem, Context, Entity, ExternalPaths, FocusHandle,
    Focusable, FontWeight, IntoElement, PathPromptOptions, SharedString, Subscription, Window, div,
    px,
};

use omagit_git::{Counts, Head, Summary};
use omagit_settings::{Entry, Location};
use omagit_theme::Rgb;
use omagit_ui::primitives::{
    ButtonKind, Pip, button, chip, field, fields, pip, section, shortcut, sparkline,
};
use omagit_ui::{ActiveFonts, ActivePalette, Fonts, Icon, Palette, hsla};

use crate::actions::*;
use crate::async_state::AsyncState;
use crate::platform::{self, Platform};
use crate::store::Store;
use crate::time;

/// Board 06: the sidebar is 300px, the card takes the rest and refuses to go
/// under 480.
const SIDEBAR_WIDTH: f32 = 300.0;
const CARD_MIN_WIDTH: f32 = 480.0;
const ROW_HEIGHT: f32 = 36.0;
const GROUP_HEADER_HEIGHT: f32 = 22.0;

/// The six tab stops of DESIGN §5, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// 1 — "Ajouter un dépôt local", and 1b "Cloner…".
    Add,
    /// 2 — the command palette trigger.
    Search,
    /// 3 — the sidebar's filter box.
    Filter,
    /// 4 — the repository list.
    List,
    /// 5 — the card's User Description, editable in place.
    Description,
    /// 6 — "Ouvrir", and 6b "Retirer de la liste".
    Open,
}

impl Stop {
    const ORDER: [Stop; 6] = [
        Stop::Add,
        Stop::Search,
        Stop::Filter,
        Stop::List,
        Stop::Description,
        Stop::Open,
    ];

    /// The next stop, wrapping. DESIGN §5: "Après le dernier arrêt, Tab revient
    /// à 1 — le focus ne sort jamais vers la décoration système."
    fn next(self) -> Self {
        let index = Self::ORDER
            .iter()
            .position(|stop| *stop == self)
            .unwrap_or(0);
        Self::ORDER[(index + 1) % Self::ORDER.len()]
    }

    fn previous(self) -> Self {
        let index = Self::ORDER
            .iter()
            .position(|stop| *stop == self)
            .unwrap_or(0);
        Self::ORDER[(index + Self::ORDER.len() - 1) % Self::ORDER.len()]
    }

    /// Whether this stop holds a pair of controls the arrows move between.
    fn has_pair(self) -> bool {
        matches!(self, Stop::Add | Stop::Open)
    }
}

impl Focusable for RepositoriesScreen {
    /// The screen's own handle, not a control's: the text fields take focus
    /// while they are the current stop, and everything else is drawn focus over
    /// this one.
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// A row of the sidebar, once groups and the filter have been applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Group(usize),
    Repository(Location),
}

/// What a drag carries.
#[derive(Clone, Debug)]
struct DraggedRow(Location);

pub struct RepositoriesScreen {
    store: Entity<Store>,
    platform: &'static dyn Platform,
    focus: FocusHandle,
    filter: Entity<InputState>,
    description: Entity<InputState>,
    stop: Stop,
    /// Which of a paired stop's two controls is focused: `false` is the primary
    /// one (Ajouter, Ouvrir), `true` the secondary (Cloner, Retirer).
    paired: bool,
    selected: Option<Location>,
    /// The row being dragged, and the row the insertion line is drawn above.
    dragging: Option<Location>,
    drop_before: Option<Location>,
    /// The description the card is showing, so the editor is only reloaded when
    /// the selection actually changes rather than on every frame.
    description_of: Option<Location>,
    /// Whether the opening selection has been made. Once, not on every frame:
    /// otherwise clearing the selection — by folding the group it is in — is
    /// undone by the next render.
    landed: bool,
    _subscriptions: Vec<Subscription>,
}

impl RepositoriesScreen {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| {
            InputState::new(window, cx).placeholder(SharedString::from("Filtrer les dépôts"))
        });
        let description = cx.new(|cx| {
            InputState::new(window, cx).placeholder(SharedString::from("Ajouter une description…"))
        });

        let mut subscriptions = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            // Typing in the filter changes which rows exist, so the list has to
            // be rebuilt — but nothing is stored: the text lives in the editor.
            cx.subscribe(
                &filter,
                |screen, editor, event: &gpui_kit::base::input::InputEvent, cx| {
                    if !matches!(event, gpui_kit::base::input::InputEvent::Change) {
                        return;
                    }
                    // The selection survives a keystroke that does not hide it.
                    // Dropping it on every change — or worse, on focus — makes
                    // the card blink out as soon as the caret lands in the box.
                    let filter = editor.read(cx).value().trim().to_owned();
                    let still_shown = screen
                        .selected
                        .and_then(|at| screen.store.read(cx).library().get(at).cloned())
                        .is_some_and(|entry| matches(&entry, &filter));
                    if !still_shown {
                        screen.selected = None;
                        screen.description_of = None;
                    }
                    cx.notify();
                },
            ),
        ];
        subscriptions.push(cx.subscribe(
            &description,
            |screen, editor, event: &gpui_kit::base::input::InputEvent, cx| {
                use gpui_kit::base::input::InputEvent;
                // Committed when the field is left, not on every keystroke: the
                // library is a file on disk, and a write per character is a
                // write per character.
                if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. })
                    && let Some(at) = screen.selected
                {
                    let text = editor.read(cx).value().to_string();
                    screen.store.update(cx, |store, cx| {
                        store.set_description(at, text, cx);
                    });
                }
            },
        ));

        let focus = cx.focus_handle();
        // Without this the screen renders correctly and answers no key at all:
        // GPUI dispatches actions along the focus path, and a window that has
        // focused nothing has no path. Found by `tests/keyboard.rs`, which is
        // the whole reason that file exists.
        window.focus(&focus, cx);

        Self {
            store,
            platform: platform::current(),
            focus,
            filter,
            description,
            stop: Stop::List,
            paired: false,
            selected: None,
            dragging: None,
            drop_before: None,
            description_of: None,
            landed: false,
            _subscriptions: subscriptions,
        }
    }

    /// Which repository the card is showing. Public because the tests drive the
    /// screen from outside, and because the router of M4 asks the same question
    /// when it opens one.
    pub fn selected(&self) -> Option<Location> {
        self.selected
    }

    /// Which tab stop has the keyboard, and whether it is on the second of a
    /// pair.
    pub fn focus_stop(&self) -> (Stop, bool) {
        (self.stop, self.paired)
    }

    // ── Focus ────────────────────────────────────────────────────────────

    fn go_to(&mut self, stop: Stop, window: &mut Window, cx: &mut Context<Self>) {
        self.stop = stop;
        self.paired = false;
        match stop {
            Stop::Filter => self.filter.update(cx, |state, cx| state.focus(window, cx)),
            Stop::Description => {
                self.load_description(window, cx);
                self.description
                    .update(cx, |state, cx| state.focus(window, cx));
            }
            // Everything else is drawn focus, not text focus: the screen keeps
            // the keyboard so its own bindings stay live.
            _ => window.focus(&self.focus, cx),
        }
        cx.notify();
    }

    /// Put the selected repository's note into the editor, once per selection.
    fn load_description(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.description_of == self.selected {
            return;
        }
        let text = self
            .selected
            .and_then(|at| {
                self.store
                    .read(cx)
                    .library()
                    .get(at)
                    .map(|entry| entry.description.clone())
            })
            .unwrap_or_default();
        self.description_of = self.selected;
        self.description
            .update(cx, |state, cx| state.set_value(text, window, cx));
    }

    fn focused(&self, stop: Stop, paired: bool) -> bool {
        self.stop == stop && (!stop.has_pair() || self.paired == paired)
    }

    // ── Rows ─────────────────────────────────────────────────────────────

    fn filter_text(&self, cx: &App) -> String {
        self.filter.read(cx).value().trim().to_owned()
    }

    /// The sidebar's rows, after the filter and the collapsed groups.
    ///
    /// A filter overrides collapsing: hiding a match inside a folded group
    /// would make the search look broken.
    fn rows(&self, cx: &App) -> Vec<Row> {
        let store = self.store.read(cx);
        let filter = self.filter_text(cx);
        let mut rows = Vec::new();
        for (index, group) in store.library().groups.iter().enumerate() {
            let matching: Vec<Location> = group
                .repositories
                .iter()
                .enumerate()
                .filter(|(_, entry)| matches(entry, &filter))
                .map(|(position, _)| Location {
                    group: index,
                    index: position,
                })
                .collect();
            if !filter.is_empty() && matching.is_empty() {
                continue;
            }
            rows.push(Row::Group(index));
            if group.collapsed && filter.is_empty() {
                continue;
            }
            rows.extend(matching.into_iter().map(Row::Repository));
        }
        rows
    }

    fn repositories(&self, cx: &App) -> Vec<Location> {
        self.rows(cx)
            .into_iter()
            .filter_map(|row| match row {
                Row::Repository(at) => Some(at),
                Row::Group(_) => None,
            })
            .collect()
    }

    fn select_relative(&mut self, delta: isize, cx: &mut Context<Self>) {
        let visible = self.repositories(cx);
        if visible.is_empty() {
            self.selected = None;
            return;
        }
        let current = self
            .selected
            .and_then(|at| visible.iter().position(|other| *other == at));
        let next = match current {
            Some(index) => (index as isize + delta).clamp(0, visible.len() as isize - 1) as usize,
            // Entering the list from nowhere lands on the first row going down
            // and the last going up, which is what a list does.
            None if delta > 0 => 0,
            None => visible.len() - 1,
        };
        self.selected = Some(visible[next]);
        self.description_of = None;
        cx.notify();
    }

    /// Pick a repository to show when the screen opens: the one most recently
    /// opened, or the first in the list.
    ///
    /// The alternative is a card that says "select something" on every launch,
    /// which is a screen asking to be dismissed before it is useful.
    fn select_initial(&mut self, cx: &mut Context<Self>) {
        if self.landed {
            return;
        }
        self.landed = true;
        // Most recently opened wins; among never-opened ones, the first in the
        // list does. `max_by_key` alone would pick the *last* of the ties,
        // which on a fresh install is the bottom of the sidebar.
        let best = self
            .store
            .read(cx)
            .library()
            .iter()
            .enumerate()
            .max_by_key(|(position, (_, entry))| {
                (
                    entry.last_opened.unwrap_or(i64::MIN),
                    std::cmp::Reverse(*position),
                )
            })
            .map(|(_, (at, _))| at);
        if let Some(at) = best {
            self.selected = Some(at);
            self.description_of = None;
        }
    }

    fn selected_entry(&self, cx: &App) -> Option<Entry> {
        self.selected
            .and_then(|at| self.store.read(cx).library().get(at).cloned())
    }

    // ── Actions ──────────────────────────────────────────────────────────

    fn add_local(&mut self, _: &AddLocalRepository, _: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: true,
            prompt: Some("Ajouter".into()),
        });
        cx.spawn(async move |screen, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            screen
                .update(cx, |screen, cx| screen.add_paths(paths, cx))
                .ok();
        })
        .detach();
    }

    /// Add whatever was picked or dropped, keeping only what is a repository.
    ///
    /// A folder that is not one is reported rather than silently ignored: the
    /// user meant something by dropping it.
    ///
    /// Deciding *whether* a folder is a repository is Git work — it stats the
    /// tree and opens the object database — so it happens on the background
    /// executor and comes back as a message (SPEC §3 rule 2). Both callers
    /// reach this from the render thread: the file picker's continuation and
    /// the drop handler.
    fn add_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        // Taken now, not when the check comes back: it is the group that was
        // selected when the user dropped, which is what they meant.
        let group = self.selected.map(|at| at.group);
        cx.spawn(async move |screen, cx| {
            let checked = cx
                .background_spawn(async move {
                    paths
                        .into_iter()
                        .map(|path| {
                            let outcome = omagit_git::Repository::open(&path).map(|_| ());
                            (path, outcome)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            screen
                .update(cx, |screen, cx| screen.apply_added(checked, group, cx))
                .ok();
        })
        .detach();
    }

    /// Record the folders that turned out to be repositories, and report the
    /// ones that did not. Back on the render thread, holding no Git work.
    fn apply_added(
        &mut self,
        checked: Vec<(PathBuf, omagit_git::Result<()>)>,
        group: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        let mut added = None;
        for (path, outcome) in checked {
            match outcome {
                Ok(()) => {
                    added = Some(
                        self.store
                            .update(cx, |store, cx| store.add(path, group, cx)),
                    );
                }
                Err(error) => {
                    tracing::warn!(path = %path.display(), %error, "not added");
                }
            }
        }
        if let Some(at) = added {
            self.selected = Some(at);
            self.description_of = None;
        }
        cx.notify();
    }

    fn confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        match self.stop {
            Stop::Add if !self.paired => self.add_local(&AddLocalRepository, window, cx),
            Stop::Add => self.not_yet("Le clonage arrive au jalon M7."),
            Stop::Search => self.not_yet("La palette de commandes arrive au jalon M9."),
            Stop::Open if self.paired => self.remove_selected(&RemoveSelected, window, cx),
            // In a text field, Enter finishes the edit — the description has
            // already been committed by its own event — and hands the keyboard
            // back to the list. Opening a repository from inside a text field
            // would be the screen acting on a keystroke meant for the field.
            Stop::Filter | Stop::Description => self.go_to(Stop::List, window, cx),
            Stop::List | Stop::Open => self.open_selected(window, cx),
        }
    }

    fn open_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(at) = self.selected else { return };
        self.store.update(cx, |store, cx| store.mark_opened(at, cx));
        // The shell owns which screen is showing, so this asks rather than
        // switches — the same way the topbar asks for "add a repository".
        window.dispatch_action(Box::new(ShowWorkingCopy), cx);
        cx.notify();
    }

    fn remove_selected(&mut self, _: &RemoveSelected, _: &mut Window, cx: &mut Context<Self>) {
        let Some(at) = self.selected else { return };
        // No confirmation: this takes an entry out of a list and destroys
        // nothing on disk. SPEC §3 rule 7 is about destruction.
        self.store.update(cx, |store, cx| {
            store.remove(at, cx);
        });
        self.selected = None;
        self.description_of = None;
        cx.notify();
    }

    fn collapse(&mut self, _: &CollapseGroup, _: &mut Window, cx: &mut Context<Self>) {
        let Some(at) = self.selected else { return };
        let collapsed = self
            .store
            .read(cx)
            .library()
            .groups
            .get(at.group)
            .is_some_and(|group| group.collapsed);
        if !collapsed {
            self.store
                .update(cx, |store, cx| store.toggle_group(at.group, cx));
            // The selection would otherwise sit on a row that is no longer
            // drawn; it moves to the group it is now inside.
            self.selected = None;
            cx.notify();
        }
    }

    fn expand(&mut self, _: &ExpandGroup, _: &mut Window, cx: &mut Context<Self>) {
        let Some(at) = self.selected else { return };
        let collapsed = self
            .store
            .read(cx)
            .library()
            .groups
            .get(at.group)
            .is_some_and(|group| group.collapsed);
        if collapsed {
            self.store
                .update(cx, |store, cx| store.toggle_group(at.group, cx));
            cx.notify();
        }
    }

    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        // Esc goes up one level (DESIGN §5). There is no overlay on this screen
        // yet, so: a filter clears, then focus returns to the sidebar.
        if !self.filter.read(cx).value().is_empty() {
            self.filter
                .update(cx, |state, cx| state.set_value("", window, cx));
            cx.notify();
            return;
        }
        self.go_to(Stop::List, window, cx);
    }

    fn new_group(&mut self, _: &NewGroup, _: &mut Window, cx: &mut Context<Self>) {
        // Named by position rather than prompted: a dialog for a label the user
        // can rename in place (M9) is a dialog for nothing.
        let index = self.store.read(cx).library().groups.len() + 1;
        self.store.update(cx, |store, cx| {
            store.add_group(format!("Groupe {index}"), cx)
        });
    }

    fn not_yet(&self, what: &str) {
        // Deliberately a log line and not a dialog: an alert box that only ever
        // says "not yet" trains people to dismiss alert boxes.
        tracing::info!("{what}");
    }

    fn drop_row(&mut self, dragged: Location, before: Option<Location>, cx: &mut Context<Self>) {
        let target = before.unwrap_or(Location {
            group: dragged.group,
            index: usize::MAX,
        });
        let moved = self
            .store
            .update(cx, |store, cx| store.move_entry(dragged, target, cx));
        self.selected = moved;
        self.dragging = None;
        self.drop_before = None;
        cx.notify();
    }
}

/// Does this repository match what was typed in the filter box?
///
/// Case is folded on both sides here rather than once by the caller. That is an
/// allocation per row per keystroke, which on a list of tens of repositories is
/// invisible — and the alternative is a function that silently requires its
/// argument to be lowercased already, which is the kind of contract that holds
/// until someone else calls it.
fn matches(entry: &Entry, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let filter = filter.to_lowercase();
    entry.name.to_lowercase().contains(&filter)
        // The path too: two clones of the same project share a name and are
        // told apart only by where they are.
        || entry.path.to_string_lossy().to_lowercase().contains(&filter)
}

// ── Rendering ────────────────────────────────────────────────────────────

impl Render for RepositoriesScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette().clone();
        let fonts = cx.fonts().clone();
        let t = palette.tokens;

        self.select_initial(cx);

        // A selection that pointed at a row which has since moved or gone.
        if let Some(at) = self.selected
            && self.store.read(cx).library().get(at).is_none()
        {
            self.selected = None;
        }

        div()
            .key_context(crate::actions::CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::add_local))
            .on_action(cx.listener(|screen, _: &CloneRepository, _, _| {
                screen.not_yet("Le clonage arrive au jalon M7.")
            }))
            .on_action(cx.listener(|screen, _: &OpenCommandPalette, _, _| {
                screen.not_yet("La palette de commandes arrive au jalon M9.")
            }))
            .on_action(cx.listener(|screen, _: &FocusFilter, window, cx| {
                screen.go_to(Stop::Filter, window, cx)
            }))
            .on_action(cx.listener(|screen, _: &FocusSidebar, window, cx| {
                screen.go_to(Stop::List, window, cx)
            }))
            .on_action(cx.listener(|screen, _: &FocusCentre, window, cx| {
                screen.go_to(Stop::Description, window, cx)
            }))
            .on_action(cx.listener(|screen, _: &FocusDetail, window, cx| {
                screen.go_to(Stop::Description, window, cx)
            }))
            .on_action(cx.listener(|screen, _: &FocusNext, window, cx| {
                // Inside a paired stop, Tab moves to its second control before
                // leaving; that is what makes "1b" and "6b" reachable without
                // adding two stops to a six-stop screen.
                if screen.stop.has_pair() && !screen.paired {
                    screen.paired = true;
                    cx.notify();
                } else {
                    screen.go_to(screen.stop.next(), window, cx);
                }
            }))
            .on_action(cx.listener(|screen, _: &FocusPrevious, window, cx| {
                if screen.stop.has_pair() && screen.paired {
                    screen.paired = false;
                    cx.notify();
                } else {
                    let previous = screen.stop.previous();
                    screen.go_to(previous, window, cx);
                    screen.paired = previous.has_pair();
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|screen, _: &SelectNext, _, cx| screen.select_relative(1, cx)))
            .on_action(
                cx.listener(|screen, _: &SelectPrevious, _, cx| screen.select_relative(-1, cx)),
            )
            .on_action(cx.listener(Self::collapse))
            .on_action(cx.listener(Self::expand))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::remove_selected))
            .on_action(cx.listener(Self::new_group))
            .on_action(cx.listener(|screen, _: &RefreshAll, _, cx| {
                screen.store.update(cx, |store, cx| store.refresh_all(cx));
            }))
            // A folder dragged in from the file manager. DESIGN §4's empty
            // state promises it works, so it is wired on the whole screen and
            // not only on the sidebar.
            .on_drop(cx.listener(|screen, paths: &ExternalPaths, _, cx| {
                screen.add_paths(paths.paths().to_vec(), cx);
            }))
            .flex()
            .size_full()
            .min_h_0()
            .bg(hsla(t.bg))
            .text_color(hsla(t.text))
            .font_family(fonts.ui.clone())
            .text_size(px(13.0))
            .child(self.sidebar(&palette, &fonts, cx))
            .child(self.card(&palette, &fonts, window, cx))
    }
}

impl RepositoriesScreen {
    fn sidebar(
        &mut self,
        palette: &Palette,
        fonts: &Fonts,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = palette.tokens;
        let rows = self.rows(cx);
        let store = self.store.read(cx);
        let empty = store.library().is_empty();

        let mut list = div().flex().flex_col().flex_1().min_h_0().overflow_hidden();
        for row in rows {
            match row {
                Row::Group(index) => {
                    list = list.child(self.group_header(index, palette, fonts, cx));
                }
                Row::Repository(at) => {
                    list = list.child(self.repository_row(at, palette, fonts, cx));
                }
            }
        }
        if empty {
            list = list.child(
                div()
                    .p(px(10.0))
                    .text_size(px(12.0))
                    .text_color(hsla(t.text_dim))
                    .child("Aucun dépôt"),
            );
        }

        div()
            .flex()
            .flex_col()
            .w(px(SIDEBAR_WIDTH))
            .flex_none()
            .min_h_0()
            .bg(hsla(t.surface))
            .border_r_1()
            .border_color(hsla(t.border))
            .child(
                div().p(px(8.0)).flex_none().child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .h(px(26.0))
                        .px(px(8.0))
                        .bg(hsla(t.bg))
                        .border_1()
                        .border_color(if self.focused(Stop::Filter, false) {
                            hsla(t.border_focus)
                        } else {
                            hsla(t.border)
                        })
                        .text_color(hsla(t.text_dim))
                        .child(Icon::Search.render(px(14.0), hsla(t.text_dim)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(12.0))
                                .text_color(hsla(t.text))
                                .child(gpui_kit::base::Input::new(&self.filter)),
                        )
                        .child(
                            div()
                                .font_family(fonts.mono.clone())
                                .text_size(px(11.0))
                                .text_color(hsla(t.text_dim))
                                .child("/"),
                        ),
                ),
            )
            .child(list)
            .child(
                // "+ Nouveau groupe" sits on the sidebar's own footer, where it
                // is out of the drag path of the rows above it.
                div()
                    .id("new-group")
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .flex_none()
                    .h(px(26.0))
                    .px(px(10.0))
                    .border_t_1()
                    .border_color(hsla(t.border))
                    .text_size(px(12.0))
                    .text_color(hsla(t.text_muted))
                    .hover(|style| style.bg(hsla(t.surface_hover)))
                    .on_click(
                        cx.listener(|screen, _, window, cx| {
                            screen.new_group(&NewGroup, window, cx)
                        }),
                    )
                    .child(Icon::Plus.render(px(12.0), hsla(t.text_muted)))
                    .child("Nouveau groupe")
                    .child(div().flex_1())
                    .child(
                        div()
                            .font_family(fonts.mono.clone())
                            .text_size(px(11.0))
                            .text_color(hsla(t.text_dim))
                            .child("glisser pour ranger"),
                    ),
            )
    }

    fn group_header(
        &self,
        index: usize,
        palette: &Palette,
        fonts: &Fonts,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = palette.tokens;
        let store = self.store.read(cx);
        let group = &store.library().groups[index];
        let collapsed = group.collapsed;
        let count = group.repositories.len();
        let name = group.name.to_uppercase();

        div()
            .id(("group", index))
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(GROUP_HEADER_HEIGHT))
            .px(px(10.0))
            .when(index > 0, |element| element.mt(px(8.0)))
            .text_size(px(11.0))
            .text_color(hsla(t.text_muted))
            .hover(|style| style.bg(hsla(t.surface_hover)))
            .on_click(cx.listener(move |screen, _, _, cx| {
                screen
                    .store
                    .update(cx, |store, cx| store.toggle_group(index, cx));
            }))
            // Dropping onto a group header files the row at the top of it,
            // which is the only way to reach an empty group.
            .on_drop(cx.listener(move |screen, dragged: &DraggedRow, _, cx| {
                screen.drop_row(
                    dragged.0,
                    Some(Location {
                        group: index,
                        index: 0,
                    }),
                    cx,
                );
            }))
            .child(
                if collapsed {
                    Icon::ChevronRight
                } else {
                    Icon::ChevronDown
                }
                .render(px(10.0), hsla(t.text_muted)),
            )
            .child(SharedString::from(name))
            .child(div().flex_1())
            .child(
                div()
                    .font_family(fonts.mono.clone())
                    .text_size(px(11.0))
                    .text_color(hsla(t.text_dim))
                    .child(SharedString::from(count.to_string())),
            )
    }

    fn repository_row(
        &self,
        at: Location,
        palette: &Palette,
        fonts: &Fonts,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = palette.tokens;
        let store = self.store.read(cx);
        let entry = store
            .library()
            .get(at)
            .expect("the row came from the library");
        let state = store.summary(&entry.path);
        let selected = self.selected == Some(at);
        let focused = selected && self.stop == Stop::List;
        let missing = is_missing(state);
        let line = subtitle(state, palette);

        // DESIGN §1 is explicit and board 06 disagrees with it: a selected row
        // takes `surface_raised`, not `accent`. The rule is followed, and the
        // divergence is recorded in DESIGN.md §6 — "Selection stays legible
        // because it changes *surface* and carries a ring, not because its hue
        // differs", which is what keeps it readable on Matte Black, where
        // `accent` and `border` nearly collapse.
        let icon_color = if missing { t.text_dim } else { t.text_muted };

        div()
            .id(("repository", at.group * 1000 + at.index))
            .flex()
            .items_center()
            .gap(px(8.0))
            .h(px(ROW_HEIGHT))
            .pl(px(20.0))
            .pr(px(10.0))
            .when(selected, |element| element.bg(hsla(t.surface_raised)))
            .when(!selected, |element| {
                element.hover(|style| style.bg(hsla(t.surface_hover)))
            })
            // Selected and focused are different things (DESIGN §1): the ring
            // is inset so the row does not change size, and it goes when focus
            // leaves the zone while the selection stays.
            .when(focused, |element| {
                element
                    .border_1()
                    .border_color(hsla(t.border_focus))
                    .pl(px(19.0))
                    .pr(px(9.0))
            })
            .when(self.dragging == Some(at), |element| element.opacity(0.6))
            .when(self.drop_before == Some(at), |element| {
                element.border_t_2().border_color(hsla(t.accent))
            })
            .text_color(hsla(t.text))
            .on_click(cx.listener(move |screen, event: &ClickEvent, window, cx| {
                screen.selected = Some(at);
                screen.description_of = None;
                screen.go_to(Stop::List, window, cx);
                // A second click opens it, the way every list-and-detail does.
                // DESIGN §5 gives the keyboard path (⏎) and board 06 the button
                // in the card; the pointer had no gesture of its own, so a row
                // could be clicked all day without anything happening.
                //
                // The first click stays a selection on purpose: the card is a
                // surface meant to be read — location, last commit, identity,
                // remotes — without committing to switching to it.
                if event.click_count() >= 2 {
                    screen.open_selected(window, cx);
                }
            }))
            .on_drag(DraggedRow(at), {
                let name = SharedString::from(entry.name.clone());
                move |_, _, _, cx| {
                    let name = name.clone();
                    cx.new(|_| DragGhost(name))
                }
            })
            .drag_over::<DraggedRow>(move |style, _, _, _| style)
            .on_drop(cx.listener(move |screen, dragged: &DraggedRow, _, cx| {
                screen.drop_row(dragged.0, Some(at), cx);
            }))
            .child(
                if missing {
                    Icon::RepositoryMissing
                } else {
                    Icon::Repository
                }
                .render(px(16.0), hsla(icon_color)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .flex_1()
                    .child(
                        div()
                            .truncate()
                            .when(selected, |element| element.font_weight(FontWeight::MEDIUM))
                            .child(SharedString::from(entry.name.clone())),
                    )
                    .child(
                        div()
                            .font_family(fonts.mono.clone())
                            .text_size(px(10.5))
                            .truncate()
                            .text_color(hsla(line.1))
                            .child(line.0),
                    ),
            )
            .children(status_pip(state, palette))
    }

    fn card(
        &mut self,
        palette: &Palette,
        fonts: &Fonts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let body = match self.selected_entry(cx) {
            None if self.store.read(cx).library().is_empty() => {
                self.empty_state(palette, fonts, cx).into_any_element()
            }
            None => self
                .placeholder("Sélectionne un dépôt dans la liste.", palette)
                .into_any_element(),
            Some(entry) => self
                .repository_card(entry, palette, fonts, window, cx)
                .into_any_element(),
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(CARD_MIN_WIDTH))
            .min_h_0()
            .child(body)
            .child(self.statusbar(palette, fonts, cx))
    }

    fn placeholder(&self, text: &'static str, palette: &Palette) -> impl IntoElement {
        div()
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .text_size(px(12.0))
            .text_color(hsla(palette.tokens.text_dim))
            .child(text)
    }

    fn empty_state(
        &self,
        palette: &Palette,
        fonts: &Fonts,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = palette.tokens;
        let modifier = self.platform.primary_modifier();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .items_center()
            .justify_center()
            .gap(px(12.0))
            .p(px(32.0))
            .child(
                div()
                    .text_size(px(16.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Aucun dépôt pour l'instant"),
            )
            .child(
                div()
                    .max_w(px(420.0))
                    .text_center()
                    .text_size(px(12.5))
                    .text_color(hsla(t.text_muted))
                    .child(
                        "Ajoute un dossier déjà versionné, ou clone une URL. \
                         Le glisser-déposer d'un dossier dans cette fenêtre fonctionne aussi.",
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .mt(px(4.0))
                    .child(
                        button(
                            "Ajouter un dépôt local",
                            palette,
                            ButtonKind::Primary,
                            self.focused(Stop::Add, false),
                        )
                        .id("empty-add")
                        .on_click(cx.listener(|screen, _, window, cx| {
                            screen.add_local(&AddLocalRepository, window, cx)
                        }))
                        .child(shortcut(
                            modifier.shortcut("O"),
                            palette,
                            &fonts.mono,
                            true,
                        )),
                    )
                    .child(
                        button("Cloner…", palette, ButtonKind::Neutral, false)
                            .text_color(hsla(t.text_dim))
                            .border_color(hsla(t.border))
                            .child(shortcut("M7", palette, &fonts.mono, false)),
                    ),
            )
    }

    fn repository_card(
        &mut self,
        entry: Entry,
        palette: &Palette,
        fonts: &Fonts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = palette.tokens;
        let at = self.selected.expect("an entry means a selection");
        let path = entry.path.clone();
        let state = self.store.read(cx).summary(&path);
        let summary = state.value().cloned();
        let missing = is_missing(state);
        let loading = state.is_blank();

        self.load_description(window, cx);

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(self.card_header(&entry, summary.as_ref(), missing, palette, fonts, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .px(px(20.0))
                    .when(loading, |element| {
                        element.child(
                            div()
                                .pt(px(14.0))
                                .text_size(px(12.0))
                                .text_color(hsla(t.text_dim))
                                .child("Lecture du dépôt…"),
                        )
                    })
                    .when(missing, |element| {
                        element.child(self.missing_notice(at, palette, cx))
                    })
                    .child(section("Repository", palette))
                    .child(fields(self.repository_fields(
                        &entry,
                        summary.as_ref(),
                        palette,
                        fonts,
                        cx,
                    )))
                    .when(!missing, |element| {
                        element
                            .child(section("Working Copy", palette))
                            .child(fields(working_copy_fields(
                                summary.as_ref(),
                                palette,
                                fonts,
                            )))
                            .when_some(
                                summary
                                    .as_ref()
                                    .filter(|summary| !summary.remotes.is_empty())
                                    .cloned(),
                                |element, summary| {
                                    element
                                        .child(section("Remotes", palette))
                                        .child(fields(remote_fields(&summary, palette, fonts, cx)))
                                },
                            )
                    }),
            )
    }

    fn card_header(
        &self,
        entry: &Entry,
        summary: Option<&Summary>,
        missing: bool,
        palette: &Palette,
        fonts: &Fonts,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = palette.tokens;
        let counts = summary.map(|summary| summary.counts).unwrap_or_default();

        div()
            .flex()
            .items_start()
            .gap(px(20.0))
            .flex_none()
            .p(px(16.0))
            .px(px(20.0))
            .border_b_1()
            .border_color(hsla(t.border))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(SharedString::from(entry.name.clone())),
                            )
                            .children(if missing {
                                Some(chip("introuvable", t.danger, t.danger).into_any_element())
                            } else {
                                summary.map(|_| {
                                    let (label, color) = counts_label(&counts, palette);
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(5.0))
                                        .flex_none()
                                        .h(px(18.0))
                                        .px(px(6.0))
                                        .border_1()
                                        .border_color(hsla(color))
                                        .text_size(px(11.0))
                                        .text_color(hsla(color))
                                        .child(pip(Pip::Filled, color, 6.0))
                                        .child(label)
                                        .into_any_element()
                                })
                            })
                            .children(summary.and_then(|summary| {
                                summary
                                    .tracking
                                    .as_ref()
                                    // A chip reading "↑0 ↓0" costs a glance and
                                    // says what its absence already said.
                                    .filter(|tracking| {
                                        !tracking.gone && tracking.ahead + tracking.behind > 0
                                    })
                                    .map(|tracking| {
                                        div().font_family(fonts.mono.clone()).child(chip(
                                            format!("↑{} ↓{}", tracking.ahead, tracking.behind),
                                            t.border,
                                            t.text_muted,
                                        ))
                                    })
                            })),
                    )
                    // The path is two lines below, under Location. Repeating it
                    // here when there is no description would be the card
                    // filling a gap with something the reader already has.
                    .children((!entry.description.is_empty()).then(|| {
                        div()
                            .font_family(fonts.mono.clone())
                            .text_size(px(12.0))
                            .text_color(hsla(t.text_muted))
                            .truncate()
                            .child(SharedString::from(entry.description.clone()))
                    })),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(px(8.0))
                    .flex_none()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                button(
                                    "Retirer de la liste",
                                    palette,
                                    ButtonKind::Danger,
                                    self.focused(Stop::Open, true),
                                )
                                .id("remove")
                                .on_click(cx.listener(
                                    |screen, _, window, cx| {
                                        screen.remove_selected(&RemoveSelected, window, cx)
                                    },
                                )),
                            )
                            .child(
                                button(
                                    "Ouvrir",
                                    palette,
                                    ButtonKind::Primary,
                                    self.focused(Stop::Open, false),
                                )
                                .id("open")
                                .on_click(cx.listener(|screen, _, window, cx| {
                                    screen.open_selected(window, cx)
                                }))
                                .child(shortcut(
                                    "⏎",
                                    palette,
                                    &fonts.mono,
                                    true,
                                )),
                            ),
                    )
                    .children(summary.map(|summary| {
                        div()
                            .flex()
                            .items_end()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(hsla(t.text_dim))
                                    .child("90 JOURS"),
                            )
                            .child(sparkline(
                                &summary.activity.buckets,
                                summary.activity.peak(),
                                palette,
                            ))
                            .child(
                                div()
                                    .font_family(fonts.mono.clone())
                                    .text_size(px(11.0))
                                    .text_color(hsla(t.text_muted))
                                    .child(SharedString::from(format!(
                                        "{} commits",
                                        summary.activity.total
                                    ))),
                            )
                    })),
            )
    }

    fn missing_notice(
        &self,
        at: Location,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = palette.tokens;
        div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .mt(px(14.0))
            .p(px(10.0))
            .border_1()
            .border_color(hsla(t.danger))
            .child(Icon::Alert.render(px(16.0), hsla(t.danger)))
            .child(
                div()
                    .flex_1()
                    .text_size(px(12.5))
                    .child("Le dossier n'existe plus à l'emplacement enregistré."),
            )
            .child(
                button("Localiser…", palette, ButtonKind::Neutral, false)
                    .id("locate")
                    .on_click(cx.listener(move |screen, _, _, cx| screen.locate(at, cx))),
            )
    }

    fn locate(&mut self, at: Location, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Localiser".into()),
        });
        cx.spawn(async move |screen, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            screen
                .update(cx, |screen, cx| {
                    screen
                        .store
                        .update(cx, |store, cx| store.relocate(at, path, cx));
                })
                .ok();
        })
        .detach();
    }

    fn repository_fields(
        &self,
        entry: &Entry,
        summary: Option<&Summary>,
        palette: &Palette,
        fonts: &Fonts,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let t = palette.tokens;
        let path = entry.path.clone();
        let mut rows = vec![
            field(
                "Location",
                palette,
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .min_w_0()
                    .child(
                        div()
                            .font_family(fonts.mono.clone())
                            .text_size(px(12.5))
                            .truncate()
                            .child(SharedString::from(time::tildify(&path))),
                    )
                    .child(
                        chip("Révéler dans le gestionnaire", t.border, t.text_muted)
                            .id("reveal")
                            .hover(|style| style.bg(hsla(t.surface_hover)))
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.reveal_path(&path);
                            })),
                    ),
            )
            .into_any_element(),
            field(
                "Last Opened",
                palette,
                div()
                    .font_family(fonts.mono.clone())
                    .text_size(px(12.5))
                    .child(SharedString::from(match entry.last_opened {
                        Some(seconds) => time::opened(seconds),
                        None => "jamais".to_owned(),
                    })),
            )
            .into_any_element(),
        ];

        rows.push(
            field(
                "Last Commit",
                palette,
                match summary.and_then(|summary| summary.last_commit.as_ref()) {
                    Some(commit) => div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .min_w_0()
                        .font_family(fonts.mono.clone())
                        .text_size(px(12.5))
                        .child(
                            div()
                                .flex_none()
                                .text_color(hsla(t.text_muted))
                                .child(SharedString::from(short_id(&commit.id))),
                        )
                        .child(
                            div()
                                .truncate()
                                .child(SharedString::from(commit.summary.clone())),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(11.0))
                                .text_color(hsla(t.text_dim))
                                .child(SharedString::from(time::ago(
                                    commit.committer.time.seconds,
                                ))),
                        )
                        .into_any_element(),
                    None => div()
                        .text_size(px(12.5))
                        .text_color(hsla(t.text_dim))
                        .child("indisponible")
                        .into_any_element(),
                },
            )
            .into_any_element(),
        );

        rows.push(
            field(
                "User Description",
                palette,
                // The field draws only at focus (DESIGN §4): a border and a
                // caret when it is the current stop, plain text otherwise. No
                // dialog, no "edit" button.
                div()
                    .flex()
                    .items_center()
                    .h(px(24.0))
                    .px(px(6.0))
                    .ml(px(-7.0))
                    .min_w_0()
                    .flex_1()
                    .text_size(px(12.5))
                    .when(self.focused(Stop::Description, false), |element| {
                        element.border_1().border_color(hsla(t.border_focus))
                    })
                    .child(gpui_kit::base::Input::new(&self.description)),
            )
            .into_any_element(),
        );

        if let Some(identity) = summary.and_then(|summary| summary.committer.as_ref()) {
            rows.push(
                field(
                    "Committer Identity",
                    palette,
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .w(px(20.0))
                                .h(px(20.0))
                                .flex_none()
                                .bg(hsla(t.surface_raised))
                                .border_1()
                                .border_color(hsla(t.border))
                                .font_family(fonts.mono.clone())
                                .text_size(px(10.0))
                                .child(SharedString::from(identity.initials())),
                        )
                        .child(SharedString::from(identity.name.clone()))
                        .child(
                            div()
                                .font_family(fonts.mono.clone())
                                .text_size(px(12.0))
                                .text_color(hsla(t.text_muted))
                                .child(SharedString::from(format!("<{}>", identity.email))),
                        )
                        .when(identity.inherited, |element| {
                            element.child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(hsla(t.text_dim))
                                    .child("hérité du global"),
                            )
                        }),
                )
                .into_any_element(),
            );
        }

        rows
    }

    fn statusbar(
        &self,
        palette: &Palette,
        fonts: &Fonts,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = palette.tokens;
        let store = self.store.read(cx);
        let repositories = store.library().len();
        let groups = store.library().groups.len();
        let missing: Vec<String> = store
            .missing()
            .map(|(_, entry)| entry.name.clone())
            .collect();

        div()
            .flex()
            .items_center()
            .gap(px(14.0))
            .flex_none()
            .h(px(24.0))
            .px(px(20.0))
            .border_t_1()
            .border_color(hsla(t.border))
            .bg(hsla(t.surface))
            .font_family(fonts.mono.clone())
            .text_size(px(11.5))
            .child(
                div()
                    .text_color(hsla(t.text_muted))
                    .child(SharedString::from(format!(
                        "{repositories} dépôt{} · {groups} groupe{}",
                        plural(repositories),
                        plural(groups)
                    ))),
            )
            .child(div().flex_1())
            .when(!missing.is_empty(), |element| {
                element.child(
                    div()
                        .text_color(hsla(t.danger))
                        .child(SharedString::from(format!(
                            "! {} introuvable{}",
                            missing.join(", "),
                            plural(missing.len())
                        ))),
                )
            })
    }
}

/// The ghost that follows the cursor during a drag.
///
/// It names the row being moved rather than showing a generic marker: with
/// several rows under the cursor and an insertion line between two of them, the
/// question the user has is *which* one they picked up.
struct DragGhost(SharedString);

impl Render for DragGhost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette().clone();
        let t = palette.tokens;
        div()
            .h(px(ROW_HEIGHT))
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .bg(hsla(t.surface_raised))
            .border_1()
            .border_color(hsla(t.accent))
            .opacity(0.9)
            .text_size(px(12.0))
            .text_color(hsla(t.text))
            .child(Icon::Repository.render(px(16.0), hsla(t.text_muted)))
            .child(self.0.clone())
    }
}

// ── Small read-outs shared by the row and the card ───────────────────────

fn is_missing(state: &AsyncState<Summary>) -> bool {
    matches!(
        state.error(),
        Some(omagit_git::GitError::RepositoryMissing(_) | omagit_git::GitError::NotARepository(_))
    )
}

/// The second line of a sidebar row: what this repository is doing.
fn subtitle(state: &AsyncState<Summary>, palette: &Palette) -> (SharedString, Rgb) {
    let t = palette.tokens;
    match state {
        _ if is_missing(state) => ("introuvable sur le disque".into(), t.danger),
        AsyncState::Failed(error) => (SharedString::from(error.to_string()), t.danger),
        _ => match state.value() {
            None => ("lecture…".into(), t.text_dim),
            Some(summary) => {
                let head = match &summary.head {
                    Head::Branch { branch, .. } => elide(branch),
                    Head::Unborn { branch } => format!("{} (vide)", elide(branch)),
                    Head::Detached { commit } => format!("HEAD détaché · {}", short_id(commit)),
                };
                if let Some(operation) = summary.operation {
                    let conflicts = summary.counts.conflicted;
                    return (
                        SharedString::from(if conflicts > 0 {
                            format!("{operation} · {conflicts} conflit{}", plural(conflicts))
                        } else {
                            format!("{operation} en cours")
                        }),
                        t.danger,
                    );
                }
                let (state_label, color) = counts_label(&summary.counts, palette);
                let tracking = summary
                    .tracking
                    .as_ref()
                    .filter(|tracking| tracking.ahead + tracking.behind > 0)
                    .map(|tracking| format!(" ↑{} ↓{}", tracking.ahead, tracking.behind))
                    .unwrap_or_default();
                (
                    SharedString::from(format!("{head} · {state_label}{tracking}")),
                    color,
                )
            }
        },
    }
}

/// `6 modifiés`, `propre`, `2 conflits` — and the colour that goes with it.
fn counts_label(counts: &Counts, palette: &Palette) -> (SharedString, Rgb) {
    let t = palette.tokens;
    if counts.conflicted > 0 {
        return (
            SharedString::from(format!(
                "{} conflit{}",
                counts.conflicted,
                plural(counts.conflicted)
            )),
            t.danger,
        );
    }
    let changed = counts.tracked_changes();
    if changed > 0 {
        return (
            SharedString::from(format!("{changed} modifié{}", plural(changed))),
            t.warning,
        );
    }
    if counts.untracked > 0 {
        return (
            SharedString::from(format!(
                "{} non suivi{}",
                counts.untracked,
                plural(counts.untracked)
            )),
            t.text_muted,
        );
    }
    ("propre".into(), t.success)
}

/// The square status pip: filled for a working copy, hollow for a detached
/// `HEAD` (DESIGN §4 — the shape carries as much as the colour).
fn status_pip(state: &AsyncState<Summary>, palette: &Palette) -> Option<impl IntoElement> {
    let summary = state.value()?;
    let (_, color) = counts_label(&summary.counts, palette);
    Some(pip(
        if matches!(summary.head, Head::Detached { .. }) {
            Pip::Hollow
        } else {
            Pip::Filled
        },
        color,
        7.0,
    ))
}

fn working_copy_fields(
    summary: Option<&Summary>,
    palette: &Palette,
    fonts: &Fonts,
) -> Vec<AnyElement> {
    let t = palette.tokens;
    let Some(summary) = summary else {
        return Vec::new();
    };

    let branch = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            div()
                .font_family(fonts.mono.clone())
                .text_size(px(12.5))
                .child(SharedString::from(summary.head.label())),
        )
        .children(summary.tracking.as_ref().map(|tracking| {
            div().font_family(fonts.mono.clone()).child(chip(
                SharedString::from(tracking.upstream.clone()),
                if tracking.gone { t.danger } else { t.info },
                if tracking.gone { t.danger } else { t.info },
            ))
        }))
        .children(
            summary
                .tracking
                .as_ref()
                .filter(|tracking| !tracking.gone)
                .map(|tracking| {
                    div()
                        .font_family(fonts.mono.clone())
                        .text_size(px(11.0))
                        .text_color(hsla(t.text_muted))
                        .child(SharedString::from(format!(
                            "↑{} ↓{}",
                            tracking.ahead, tracking.behind
                        )))
                }),
        );

    let counts = summary.counts;
    let parts = [
        (counts.modified, "modifié", t.warning),
        (counts.added, "ajouté", t.success),
        (counts.deleted, "supprimé", t.danger),
        (counts.renamed, "renommé", t.info),
        (counts.conflicted, "conflit", t.danger),
        (counts.untracked, "non suivi", t.text_dim),
    ];
    let status = div()
        .flex()
        .items_center()
        .gap(px(10.0))
        .font_family(fonts.mono.clone())
        .text_size(px(12.5))
        .when(counts.is_clean(), |element| {
            element.child(div().text_color(hsla(t.success)).child("propre"))
        })
        .children(
            parts
                .into_iter()
                .filter(|(n, _, _)| *n > 0)
                .map(|(n, label, color)| {
                    div()
                        .text_color(hsla(color))
                        .child(SharedString::from(format!("{n} {label}{}", plural(n))))
                }),
        );

    vec![
        field("Current Branch", palette, branch).into_any_element(),
        field("Status", palette, status).into_any_element(),
        field(
            "Stashes",
            palette,
            div()
                .font_family(fonts.mono.clone())
                .text_size(px(12.5))
                .text_color(hsla(if summary.stashes == 0 {
                    t.text_dim
                } else {
                    t.text
                }))
                .child(SharedString::from(summary.stashes.to_string())),
        )
        .into_any_element(),
    ]
}

fn remote_fields(
    summary: &Summary,
    palette: &Palette,
    fonts: &Fonts,
    cx: &mut Context<RepositoriesScreen>,
) -> Vec<AnyElement> {
    let t = palette.tokens;
    summary
        .remotes
        .iter()
        .enumerate()
        .map(|(index, remote)| {
            let url = remote.url.clone().unwrap_or_default();
            let copyable = !url.is_empty();
            div()
                .flex()
                .items_center()
                .gap(px(omagit_ui::primitives::FIELD_GAP))
                .w_full()
                .min_h(px(20.0))
                .child(
                    div()
                        .w(px(omagit_ui::primitives::FIELD_LABEL_WIDTH))
                        .flex_none()
                        .text_right()
                        .font_family(fonts.mono.clone())
                        .text_size(px(12.5))
                        .text_color(hsla(t.text_muted))
                        .child(SharedString::from(remote.name.clone())),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .min_w_0()
                        .child(
                            div()
                                .font_family(fonts.mono.clone())
                                .text_size(px(12.5))
                                .truncate()
                                .text_color(hsla(if copyable { t.text } else { t.text_dim }))
                                .child(SharedString::from(if copyable {
                                    url.clone()
                                } else {
                                    "(sans URL)".to_owned()
                                })),
                        )
                        .when(copyable, |element| {
                            element.child(
                                chip("copier", t.border, t.text_muted)
                                    .id(("copy", index))
                                    .hover(|style| style.bg(hsla(t.surface_hover)))
                                    .on_click(cx.listener(move |_, _, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            url.clone(),
                                        ));
                                    })),
                            )
                        }),
                )
                .into_any_element()
        })
        .collect()
}

fn short_id(id: &omagit_git::ObjectId) -> String {
    id.to_string()[..7].to_owned()
}

/// Shorten a branch name so the state beside it survives.
///
/// A row that truncates as a whole loses its right-hand end, which is where the
/// answer is: `feature/end-client-can-save-…` tells the reader nothing that
/// `feature/end-client-ca… · propre` does not, and costs them the `propre`.
fn elide(branch: &str) -> String {
    const MAX: usize = 22;
    let mut characters = branch.chars();
    let head: String = characters.by_ref().take(MAX).collect();
    if characters.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

/// French plural: everything here is a count of nouns that take a bare `s`.
fn plural(count: usize) -> &'static str {
    if count > 1 { "s" } else { "" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omagit_git::{Tracking, summary::Activity};

    /// A real palette, built the way `omagit-ui` builds it, so the colour a
    /// read-out picks can be compared against a token rather than a literal.
    fn palette() -> Palette {
        let theme = omagit_theme::catalogue()
            .first()
            .expect("the catalogue is never empty")
            .clone();
        Palette {
            name: theme.name.clone().into(),
            mode: theme.mode(),
            tokens: theme.tokens(),
            density: omagit_theme::DensityMode::Compact.metrics(),
            lanes: Vec::new(),
        }
    }

    fn counts(modified: usize, untracked: usize, conflicted: usize) -> Counts {
        Counts {
            modified,
            untracked,
            conflicted,
            ..Counts::default()
        }
    }

    #[test]
    fn tab_wraps_back_to_the_first_stop() {
        // DESIGN §5: focus never escapes into the window decoration.
        let mut stop = Stop::Add;
        for _ in 0..Stop::ORDER.len() {
            stop = stop.next();
        }
        assert_eq!(stop, Stop::Add, "six steps forward is a full turn");

        assert_eq!(Stop::Add.previous(), Stop::Open, "and backwards wraps too");
        assert_eq!(Stop::Open.next(), Stop::Add);
    }

    #[test]
    fn only_two_stops_hold_a_pair_of_controls() {
        // 1b is "Cloner…", 6b is "Retirer de la liste"; the rest are single.
        let paired: Vec<Stop> = Stop::ORDER
            .into_iter()
            .filter(|stop| stop.has_pair())
            .collect();
        assert_eq!(paired, vec![Stop::Add, Stop::Open]);
    }

    #[test]
    fn the_state_read_out_names_the_worst_thing_first() {
        let palette = palette();
        let t = palette.tokens;

        let label = |counts: Counts| {
            let (text, color) = counts_label(&counts, &palette);
            (text.to_string(), color)
        };

        assert_eq!(label(counts(0, 0, 0)), ("propre".into(), t.success));
        assert_eq!(label(counts(1, 0, 0)), ("1 modifié".into(), t.warning));
        assert_eq!(label(counts(6, 0, 0)), ("6 modifiés".into(), t.warning));
        assert_eq!(
            label(counts(0, 2, 0)),
            ("2 non suivis".into(), t.text_muted),
            "untracked files are not modifications, and read in a quieter colour"
        );
        assert_eq!(
            label(counts(6, 3, 2)),
            ("2 conflits".into(), t.danger),
            "a conflict outranks everything else on the row"
        );
    }

    #[test]
    fn a_row_says_what_the_repository_is_doing() {
        let palette = palette();
        let line = |state: &AsyncState<Summary>| subtitle(state, &palette).0.to_string();

        assert_eq!(line(&AsyncState::Idle), "lecture…");
        assert_eq!(
            line(&AsyncState::Failed(
                omagit_git::GitError::RepositoryMissing("/gone".into())
            )),
            "introuvable sur le disque"
        );

        let summary = |head: Head, operation, counts, tracking| Summary {
            head,
            operation,
            tracking,
            counts,
            last_commit: None,
            stashes: 0,
            remotes: Vec::new(),
            activity: Activity::default(),
            committer: None,
        };
        let ready = |summary| AsyncState::Ready(summary, crate::async_state::Generation(1));
        // A hash the app can name without reaching for `gix`: the Git core
        // re-exports the type, and `FromStr` is all a fixture needs.
        let commit: omagit_git::ObjectId = "25e5b7862d4f1a0c9b3e8d7f6a5c4b3e2d1f0a9b"
            .parse()
            .expect("a valid hash");
        let on_main = move || Head::Branch {
            branch: "main".into(),
            commit,
        };

        assert_eq!(
            line(&ready(summary(on_main(), None, counts(0, 0, 0), None))),
            "main · propre"
        );
        assert_eq!(
            line(&ready(summary(
                on_main(),
                None,
                counts(3, 0, 0),
                Some(Tracking {
                    upstream: "origin/main".into(),
                    ahead: 8,
                    behind: 2,
                    gone: false,
                }),
            ))),
            "main · 3 modifiés ↑8 ↓2"
        );
        assert_eq!(
            line(&ready(summary(
                on_main(),
                Some(omagit_git::Operation::Merge),
                counts(0, 0, 2),
                None,
            ))),
            "merge · 2 conflits",
            "a half-finished merge replaces the branch: 'on main' is not the useful fact"
        );
        assert_eq!(
            line(&ready(summary(
                on_main(),
                Some(omagit_git::Operation::Rebase { interactive: true }),
                counts(0, 0, 0),
                None,
            ))),
            "interactive rebase en cours"
        );
    }

    #[test]
    fn a_long_branch_name_keeps_the_state_beside_it_visible() {
        assert_eq!(elide("main"), "main");
        assert_eq!(
            elide("feature/end-client-can-save-block-html"),
            "feature/end-client-can…"
        );
        // Cut on characters, not bytes: a name with accents must not be split
        // mid-codepoint.
        assert_eq!(elide("é".repeat(30).as_str()).chars().count(), 23);
    }

    #[test]
    fn the_filter_matches_names_and_paths() {
        let entry = Entry {
            path: "/home/u/work/api-gateway".into(),
            name: "api-gateway".into(),
            description: String::new(),
            last_opened: None,
        };
        assert!(matches(&entry, ""), "an empty filter matches everything");
        assert!(matches(&entry, "gate"));
        assert!(matches(&entry, "API"), "case is ignored");
        assert!(
            matches(&entry, "work"),
            "the path is searched too: two clones share a name and differ by path"
        );
        assert!(!matches(&entry, "billing"));
    }
}

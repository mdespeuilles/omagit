//! The Working Copy screen — DESIGN.md board 03, in read.
//!
//! Three columns: a sidebar naming the repository and where you can go from it,
//! a file list split into what is staged and what is not, and the diff viewer,
//! which board 03 calls "la pièce maîtresse" and which is most of the work
//! (`omagit_ui::diff_view`).
//!
//! **Nothing here writes.** M4 is the read half: staging a file, a hunk or a
//! line, committing, discarding and the commit-message editor are all M5, and
//! the space board 03 gives them above the file list is left to them rather
//! than filled with controls that would do nothing. The checkboxes on the
//! board's rows are part of that, so they are not drawn — an inert checkbox is
//! a promise the screen cannot keep.
//!
//! What *is* live is the reading: the status refreshes itself when the
//! repository changes underneath, because the store's watcher tells it what
//! changed and it re-reads only that.

use gpui_kit::prelude::*;
use gpui_kit::{
    App, Context, Entity, FocusHandle, Focusable, FontWeight, IntoElement, SharedString,
    Subscription, Window, div, px,
};

use omagit_git::status::short_code;
use omagit_git::{Head, RepoPath, Status, StatusEntry};
use omagit_theme::Rgb;
use omagit_ui::diff_view::{DiffView, Mode};
use omagit_ui::primitives::{Pip, chip, pip};
use omagit_ui::{ActiveFonts, ActivePalette, Fonts, Icon, Palette, hsla};

use crate::actions::*;
use crate::repo_store::{RepoStore, Side};
use crate::time;

/// Board 03: the sidebar is 260px, the file column 320px, the diff takes the
/// rest and refuses to go under 420.
const SIDEBAR_WIDTH: f32 = 260.0;
const FILES_WIDTH: f32 = 320.0;
const DIFF_MIN_WIDTH: f32 = 420.0;
const ROW_HEIGHT: f32 = 24.0;

/// One row of the file column.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Row {
    Section(Side, usize),
    File { side: Side, path: RepoPath },
}

pub struct WorkingCopyScreen {
    store: Entity<RepoStore>,
    diff: Entity<DiffView>,
    focus: FocusHandle,
    selected: Option<(Side, RepoPath)>,
    /// Whether the screen has taken the keyboard yet. Once, not on every
    /// render: after that the focus belongs to whatever the user put it on.
    landed: bool,
    _subscriptions: Vec<Subscription>,
}

impl WorkingCopyScreen {
    /// No `window`: the screen is built from a background task's result, which
    /// has no frame to focus into. It takes the keyboard on its first render
    /// instead — see `landed` — which is the same guarantee a beat later.
    pub fn new(store: Entity<RepoStore>, cx: &mut Context<Self>) -> Self {
        let diff = cx.new(|_| DiffView::new());
        let focus = cx.focus_handle();

        let subscriptions = vec![cx.observe(&store, |screen, _, cx| {
            // The store changed: the selection may point at a file that is no
            // longer in the status, and the open diff may have been re-read.
            screen.reconcile(cx);
            cx.notify();
        })];

        Self {
            store,
            diff,
            focus,
            selected: None,
            landed: false,
            _subscriptions: subscriptions,
        }
    }

    /// Which file the diff panel is showing. Public for the tests, and for the
    /// commit column of M5, which acts on the same selection.
    pub fn selected(&self) -> Option<(Side, RepoPath)> {
        self.selected.clone()
    }

    /// The file the diff viewer actually has — which is not the same question
    /// as what is selected, because the diff is read in the background.
    pub fn shown(&self, cx: &App) -> Option<RepoPath> {
        self.diff.read(cx).path().cloned()
    }

    /// The rows of the file column, in the order they are drawn.
    fn rows(&self, cx: &App) -> Vec<Row> {
        let store = self.store.read(cx);
        let Some(status) = store.status().value() else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for side in [Side::Staged, Side::Unstaged] {
            let files: Vec<&StatusEntry> = status
                .entries
                .iter()
                .filter(|entry| has(entry, side))
                .collect();
            if files.is_empty() {
                continue;
            }
            rows.push(Row::Section(side, files.len()));
            rows.extend(files.into_iter().map(|entry| Row::File {
                side,
                path: entry.path.clone(),
            }));
        }
        rows
    }

    fn files(&self, cx: &App) -> Vec<(Side, RepoPath)> {
        self.rows(cx)
            .into_iter()
            .filter_map(|row| match row {
                Row::File { side, path } => Some((side, path)),
                Row::Section(..) => None,
            })
            .collect()
    }

    /// Keep the selection and the diff view pointing at something real.
    fn reconcile(&mut self, cx: &mut Context<Self>) {
        let files = self.files(cx);
        if let Some(selected) = &self.selected
            && !files.contains(selected)
        {
            // The file was staged, committed or reverted while it was open.
            self.selected = None;
        }
        if self.selected.is_none() {
            self.selected = files.first().cloned();
        }
        self.load_selected(cx);
    }

    fn load_selected(&mut self, cx: &mut Context<Self>) {
        let Some((side, path)) = self.selected.clone() else {
            self.diff.update(cx, |view, cx| view.show(None, cx));
            return;
        };
        self.store
            .update(cx, |store, cx| store.open_diff(path.clone(), side, cx));
        let file = self.store.read(cx).diff(&path, side).value().cloned();
        self.diff.update(cx, |view, cx| view.show(file, cx));
    }

    fn select(&mut self, side: Side, path: RepoPath, cx: &mut Context<Self>) {
        if self.selected.as_ref() == Some(&(side, path.clone())) {
            return;
        }
        self.selected = Some((side, path));
        self.load_selected(cx);
        cx.notify();
    }

    fn select_relative(&mut self, delta: isize, cx: &mut Context<Self>) {
        let files = self.files(cx);
        if files.is_empty() {
            return;
        }
        let current = self
            .selected
            .as_ref()
            .and_then(|selected| files.iter().position(|file| file == selected));
        let next = match current {
            Some(index) => (index as isize + delta).clamp(0, files.len() as isize - 1) as usize,
            None if delta > 0 => 0,
            None => files.len() - 1,
        };
        let (side, path) = files[next].clone();
        self.select(side, path, cx);
    }
}

/// Does this entry have something to show on `side`?
fn has(entry: &StatusEntry, side: Side) -> bool {
    match side {
        Side::Staged => entry.staged.is_some(),
        // Ignored files are listed by the status but have nothing to diff, so
        // they are not rows here (the Working Copy screen is about changes).
        Side::Unstaged => entry.unstaged.is_some() && !entry.is_ignored(),
    }
}

impl Focusable for WorkingCopyScreen {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for WorkingCopyScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette().clone();
        let fonts = cx.fonts().clone();
        let t = palette.tokens;

        if !self.landed {
            // Without this the screen renders and answers no key at all — the
            // bug `tests/keyboard.rs` found on Repositories at M3.
            self.landed = true;
            window.focus(&self.focus, cx);
        }
        if self.selected.is_none() {
            self.reconcile(cx);
        }

        div()
            .key_context(crate::actions::CONTEXT_WORKING_COPY)
            .track_focus(&self.focus)
            .on_action(cx.listener(|screen, _: &SelectNext, _, cx| screen.select_relative(1, cx)))
            .on_action(
                cx.listener(|screen, _: &SelectPrevious, _, cx| screen.select_relative(-1, cx)),
            )
            .on_action(cx.listener(|screen, _: &RefreshAll, _, cx| {
                screen.store.update(cx, |store, cx| store.refresh(cx));
            }))
            .on_action(cx.listener(|screen, _: &ToggleDiffMode, _, cx| {
                screen.diff.update(cx, |view, cx| {
                    let next = match view.mode() {
                        Mode::Unified => Mode::SideBySide,
                        Mode::SideBySide => Mode::Unified,
                    };
                    view.set_mode(next, cx);
                });
            }))
            .flex()
            .size_full()
            .min_h_0()
            .bg(hsla(t.bg))
            .text_color(hsla(t.text))
            .font_family(fonts.ui.clone())
            .text_size(px(13.0))
            .child(self.sidebar(&palette, &fonts, cx))
            .child(self.files_column(&palette, &fonts, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(DIFF_MIN_WIDTH))
                    .min_h_0()
                    .child(self.diff.clone())
                    .child(self.statusbar(&palette, &fonts, cx)),
            )
    }
}

impl WorkingCopyScreen {
    fn sidebar(
        &self,
        palette: &Palette,
        fonts: &Fonts,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = palette.tokens;
        let store = self.store.read(cx);
        let name = store
            .path()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let location = time::tildify(store.path());
        let summary = store.summary().value();
        let head = summary
            .map(|summary| summary.head.label())
            .unwrap_or_else(|| "…".into());
        let changed = self
            .store
            .read(cx)
            .status()
            .value()
            .map(|status| status.entries.len())
            .unwrap_or(0);

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
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .flex_none()
                    .p(px(10.0))
                    .border_b_1()
                    .border_color(hsla(t.border))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(SharedString::from(name)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .font_family(fonts.mono.clone())
                            .text_size(px(11.0))
                            .text_color(hsla(t.text_muted))
                            .child(div().truncate().child(SharedString::from(location)))
                            .child("·")
                            .child(SharedString::from(head)),
                    ),
            )
            // Where you can go from here. Only one of these is a screen yet, and
            // the others say which milestone they are waiting for rather than
            // pretending to be disabled for some other reason.
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .py(px(6.0))
                    .child(nav_header("Workspace", palette))
                    .child(nav_row(
                        "Working Copy",
                        Some(changed.to_string()),
                        true,
                        None,
                        palette,
                        fonts,
                    ))
                    .child(nav_row("History", None, false, Some("M6"), palette, fonts))
                    .child(nav_row("Stashes", None, false, Some("M8"), palette, fonts))
                    .child(nav_row("Branches", None, false, Some("M7"), palette, fonts)),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("back-to-repositories")
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
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(ShowRepositories), cx);
                    })
                    .child(Icon::ChevronRight.render(px(12.0), hsla(t.text_dim)))
                    .child("Tous les dépôts")
                    .child(div().flex_1())
                    .child(
                        div()
                            .font_family(fonts.mono.clone())
                            .text_size(px(11.0))
                            .text_color(hsla(t.text_dim))
                            .child("Esc"),
                    ),
            )
    }

    fn files_column(
        &self,
        palette: &Palette,
        fonts: &Fonts,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = palette.tokens;
        let state = self.store.read(cx).status();
        let rows = self.rows(cx);

        let mut list = div().flex().flex_col().flex_1().min_h_0().overflow_hidden();
        if state.is_blank() {
            list = list.child(hint("Lecture de la copie de travail…", palette));
        } else if let Some(error) = state.error() {
            list = list.child(hint_owned(error.to_string(), palette, t.danger));
        } else if rows.is_empty() {
            list = list.child(hint("Rien à valider — la copie est propre.", palette));
        }
        for row in rows {
            list = match row {
                Row::Section(side, count) => list.child(section_row(side, count, palette, fonts)),
                Row::File { side, path } => {
                    list.child(self.file_row(side, path, palette, fonts, cx))
                }
            };
        }

        div()
            .flex()
            .flex_col()
            .w(px(FILES_WIDTH))
            .flex_none()
            .min_h_0()
            .bg(hsla(t.surface))
            .border_r_1()
            .border_color(hsla(t.border))
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_none()
                    .h(px(26.0))
                    .px(px(10.0))
                    .border_b_1()
                    .border_color(hsla(t.border))
                    .text_size(px(11.0))
                    .text_color(hsla(t.text_muted))
                    .child("STATUS"),
            )
            .child(list)
    }

    fn file_row(
        &self,
        side: Side,
        path: RepoPath,
        palette: &Palette,
        fonts: &Fonts,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = palette.tokens;
        let status = self.store.read(cx).status();
        let entry = status
            .value()
            .and_then(|status| status.entries.iter().find(|entry| entry.path == path));
        let selected = self.selected.as_ref() == Some(&(side, path.clone()));
        let code = entry.map(short_code).unwrap_or_default();
        let letter = match side {
            Side::Staged => code.chars().next().unwrap_or(' '),
            Side::Unstaged => code.chars().nth(1).unwrap_or(' '),
        };
        let (color, conflicted) = match entry {
            Some(entry) if entry.conflict.is_some() => (t.danger, true),
            _ => (letter_color(letter, palette), false),
        };

        let text = path.to_string();
        let (directory, name) = match text.rsplit_once('/') {
            Some((directory, name)) => (format!("{directory}/"), name.to_owned()),
            None => (String::new(), text),
        };
        let click_path = path.clone();

        div()
            .id(SharedString::from(format!(
                "file:{}:{}",
                side.label(),
                path
            )))
            .flex()
            .items_center()
            .gap(px(8.0))
            .h(px(ROW_HEIGHT))
            .pl(px(18.0))
            .pr(px(10.0))
            .when(selected, |element| element.bg(hsla(t.surface_raised)))
            .when(!selected, |element| {
                element.hover(|style| style.bg(hsla(t.surface_hover)))
            })
            .on_click(
                cx.listener(move |screen, _, _, cx| screen.select(side, click_path.clone(), cx)),
            )
            .child(
                div()
                    .w(px(10.0))
                    .flex_none()
                    .font_family(fonts.mono.clone())
                    .text_size(px(11.0))
                    .text_color(hsla(color))
                    .child(SharedString::from(letter.to_string())),
            )
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .min_w_0()
                    .flex_1()
                    .font_family(fonts.mono.clone())
                    .text_size(px(12.0))
                    .child(
                        div()
                            .text_color(hsla(t.text_dim))
                            .truncate()
                            .child(SharedString::from(directory)),
                    )
                    .child(div().truncate().child(SharedString::from(name))),
            )
            .when(conflicted, |element| {
                element.child(chip("conflit", t.danger, t.danger))
            })
    }

    fn statusbar(
        &self,
        palette: &Palette,
        fonts: &Fonts,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = palette.tokens;
        let store = self.store.read(cx);
        let status = store.status().value();
        let staged = status
            .map(|status| {
                status
                    .entries
                    .iter()
                    .filter(|e| has(e, Side::Staged))
                    .count()
            })
            .unwrap_or(0);
        let unstaged = status
            .map(|status| {
                status
                    .entries
                    .iter()
                    .filter(|e| has(e, Side::Unstaged))
                    .count()
            })
            .unwrap_or(0);
        let conflicts = status.map(|status| status.conflicts().count()).unwrap_or(0);
        let summary = store.summary().value();

        div()
            .flex()
            .items_center()
            .gap(px(14.0))
            .flex_none()
            .h(px(24.0))
            .px(px(12.0))
            .border_t_1()
            .border_color(hsla(t.border))
            .bg(hsla(t.surface))
            .font_family(fonts.mono.clone())
            .text_size(px(11.5))
            .children(summary.map(|summary| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_color(hsla(t.text_muted))
                    .child(pip(
                        if matches!(summary.head, Head::Detached { .. }) {
                            Pip::Hollow
                        } else {
                            Pip::Filled
                        },
                        t.text_muted,
                        6.0,
                    ))
                    .child(SharedString::from(summary.head.label()))
                    .children(
                        summary
                            .tracking
                            .as_ref()
                            .filter(|t| !t.gone)
                            .map(|tracking| {
                                SharedString::from(format!(
                                    "↑{} ↓{}",
                                    tracking.ahead, tracking.behind
                                ))
                            }),
                    )
            }))
            .child(
                div()
                    .text_color(hsla(t.text_muted))
                    .child(SharedString::from(format!(
                        "{staged} indexé{} · {unstaged} non indexé{}",
                        plural(staged),
                        plural(unstaged)
                    ))),
            )
            .child(div().flex_1())
            .when(conflicts > 0, |element| {
                element.child(
                    div()
                        .text_color(hsla(t.danger))
                        .child(SharedString::from(format!(
                            "! {conflicts} conflit{} non résolu{}",
                            plural(conflicts),
                            plural(conflicts)
                        ))),
                )
            })
            // Live updates are the screen's promise; if they are off, it says
            // so rather than looking stale.
            .children(store.watch_error().map(|error| {
                div()
                    .text_color(hsla(t.warning))
                    .child(SharedString::from(format!(
                        "mises à jour live indisponibles — {error}"
                    )))
            }))
    }
}

fn nav_header(label: &'static str, palette: &Palette) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .h(px(22.0))
        .px(px(10.0))
        .text_size(px(11.0))
        .text_color(hsla(palette.tokens.text_muted))
        .child(SharedString::from(label.to_uppercase()))
}

fn nav_row(
    label: &'static str,
    count: Option<String>,
    active: bool,
    milestone: Option<&'static str>,
    palette: &Palette,
    fonts: &Fonts,
) -> impl IntoElement {
    let t = palette.tokens;
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .h(px(ROW_HEIGHT))
        .pl(px(18.0))
        .pr(px(10.0))
        .when(active, |element| element.bg(hsla(t.surface_raised)))
        .text_size(px(12.5))
        .text_color(hsla(if active {
            t.text
        } else if milestone.is_some() {
            t.text_dim
        } else {
            t.text_muted
        }))
        .child(label)
        .child(div().flex_1())
        .children(count.map(|count| {
            div()
                .font_family(fonts.mono.clone())
                .text_size(px(11.0))
                .text_color(hsla(t.text_muted))
                .child(SharedString::from(count))
        }))
        .children(milestone.map(|milestone| {
            div()
                .font_family(fonts.mono.clone())
                .text_size(px(11.0))
                .text_color(hsla(t.text_dim))
                .child(milestone)
        }))
}

fn section_row(side: Side, count: usize, palette: &Palette, fonts: &Fonts) -> impl IntoElement {
    let t = palette.tokens;
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .h(px(22.0))
        .px(px(10.0))
        .mt(px(6.0))
        .text_size(px(11.0))
        .text_color(hsla(t.text_muted))
        .child(SharedString::from(side.label().to_uppercase()))
        .child(div().flex_1())
        .child(
            div()
                .font_family(fonts.mono.clone())
                .text_size(px(11.0))
                .text_color(hsla(t.text_dim))
                .child(SharedString::from(count.to_string())),
        )
}

/// The colour of a `git status --short` letter.
fn letter_color(letter: char, palette: &Palette) -> Rgb {
    let t = palette.tokens;
    match letter {
        'A' => t.success,
        'D' => t.danger,
        'R' | 'C' => t.info,
        'U' => t.danger,
        '?' => t.text_dim,
        'M' | 'T' => t.warning,
        _ => t.text_muted,
    }
}

fn hint(text: &'static str, palette: &Palette) -> impl IntoElement {
    hint_owned(text.to_owned(), palette, palette.tokens.text_dim)
}

fn hint_owned(text: String, palette: &Palette, color: Rgb) -> impl IntoElement {
    let _ = palette;
    div()
        .p(px(10.0))
        .text_size(px(12.0))
        .text_color(hsla(color))
        .child(SharedString::from(text))
}

fn plural(count: usize) -> &'static str {
    if count > 1 { "s" } else { "" }
}

/// How many files a status has on each side — used by the shell's topbar.
pub fn counts(status: &Status) -> (usize, usize) {
    (
        status
            .entries
            .iter()
            .filter(|e| has(e, Side::Staged))
            .count(),
        status
            .entries
            .iter()
            .filter(|e| has(e, Side::Unstaged))
            .count(),
    )
}

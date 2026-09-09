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

use std::sync::Arc;

use gpui_kit::base::input::TextareaState;
use gpui_kit::prelude::*;
use gpui_kit::{
    App, Context, Entity, FocusHandle, Focusable, FontWeight, IntoElement, SharedString,
    Subscription, Window, div, px,
};

use omagit_git::ops::CommitOptions;
use omagit_git::patch::Selection;
use omagit_git::status::short_code;
use omagit_git::{FileDiff, Head, RepoPath, Status, StatusEntry};
use omagit_theme::Rgb;
use omagit_ui::diff_view::{DiffView, Mode};
use omagit_ui::primitives::{Pip, chip, pip};
use omagit_ui::{ActiveFonts, ActivePalette, Fonts, Icon, Palette, hsla};

use crate::actions::*;
use crate::git_runtime::ActiveGit;
use crate::repo_store::{RepoStore, Side};
use crate::time;
use crate::writes::{Target, Write};

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
    /// The commit message being written, and the three flags.
    ///
    /// Built on the first render rather than in `new`: a textarea needs a
    /// `Window`, and this screen is built from a background task's result,
    /// which has none. Same beat as `landed`.
    commit: Option<Entity<TextareaState>>,
    options: CommitOptions,
    /// A destructive write waiting on an answer (SPEC §3 rule 7). The dialog is
    /// what the user sees; this is what it is about.
    confirming: Option<Write>,
    /// Whether the operations journal is showing.
    journal_open: bool,
    /// A `commit.template` that has been read and not yet put in the box.
    /// Applied on the next render, which is where a `Window` exists — the
    /// editor needs one to take a value, and the background read has none.
    pending_template: Option<String>,
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
            commit: None,
            options: CommitOptions::default(),
            confirming: None,
            journal_open: false,
            pending_template: None,
            _subscriptions: subscriptions,
        }
    }

    // ── Writing ────────────────────────────────────────────────────────────

    /// What the diff cursor is aimed at, as the patch layer speaks of it.
    ///
    /// Picked lines if there are any, else the hunk the cursor is in, else the
    /// whole file. That order is the point: the narrowest thing the user has
    /// actually expressed wins, and "nothing picked" means the file rather than
    /// nothing at all.
    fn aim(&self, cx: &App) -> Option<Selection> {
        let view = self.diff.read(cx);
        if view.has_selection() {
            return Some(Selection::Lines(view.selection().clone()));
        }
        view.cursor_hunk().map(Selection::hunk)
    }

    /// The diff the write will be built from, and whether its file is untracked.
    fn target(&self, cx: &App) -> Option<(Arc<FileDiff>, bool)> {
        let (side, path) = self.selected.clone()?;
        let store = self.store.read(cx);
        let file = store.diff(&path, side).value()?;
        let untracked = store
            .status()
            .value()
            .and_then(|status| status.entries.iter().find(|entry| entry.path == path))
            .is_some_and(|entry| entry.is_untracked());
        Some((Arc::new(file.clone()), untracked))
    }

    /// Move a whole file across the index, whichever side it is on.
    ///
    /// The one gesture that acts on a file rather than a hunk: `alt-S` aims at
    /// what the diff cursor is in, which for a text file is always a hunk.
    pub fn stage_whole_file(&mut self, side: Side, path: RepoPath, cx: &mut Context<Self>) {
        self.toggle_file(side, path, false, cx);
    }

    fn toggle_file(&mut self, side: Side, path: RepoPath, untracked: bool, cx: &mut Context<Self>) {
        let _ = untracked;
        // No diff needed: a whole file moves through `git add` / `git restore`,
        // which want a path. Requiring the diff would mean a checkbox that does
        // nothing on a row nobody has opened yet.
        let write = match side {
            Side::Unstaged => Write::Stage(Target::File(path)),
            Side::Staged => Write::Unstage(Target::File(path)),
        };
        self.queue(write, cx);
    }

    /// Stage every unstaged file, which board 09 binds to `a`.
    fn stage_all(&mut self, _: &StageAll, _: &mut Window, cx: &mut Context<Self>) {
        let store = self.store.read(cx);
        let files: Vec<(RepoPath, bool)> = store
            .status()
            .value()
            .map(|status| {
                status
                    .entries
                    .iter()
                    .filter(|entry| entry.unstaged.is_some())
                    .map(|entry| (entry.path.clone(), entry.is_untracked()))
                    .collect()
            })
            .unwrap_or_default();

        for (path, untracked) in files {
            self.toggle_file(Side::Unstaged, path, untracked, cx);
        }
    }

    fn stage_picked(&mut self, _: &StagePicked, _: &mut Window, cx: &mut Context<Self>) {
        let Some((file, _)) = self.target(cx) else {
            return;
        };
        let selection = self.aim(cx).unwrap_or(Selection::File);
        // Staging from the staged side would mean nothing; the file column's
        // selection is what says which direction this is.
        let target = Target::Part { file, selection };
        match self.selected.as_ref().map(|(side, _)| *side) {
            Some(Side::Unstaged) => self.queue(Write::Stage(target), cx),
            Some(Side::Staged) => self.queue(Write::Unstage(target), cx),
            None => {}
        }
    }

    fn unstage_picked(&mut self, _: &UnstagePicked, _: &mut Window, cx: &mut Context<Self>) {
        let Some((file, _)) = self.target(cx) else {
            return;
        };
        if self.selected.as_ref().map(|(side, _)| *side) != Some(Side::Staged) {
            return;
        }
        let selection = self.aim(cx).unwrap_or(Selection::File);
        self.queue(Write::Unstage(Target::Part { file, selection }), cx);
    }

    fn discard_picked(&mut self, _: &DiscardPicked, _: &mut Window, cx: &mut Context<Self>) {
        let Some((file, untracked)) = self.target(cx) else {
            return;
        };
        if self.selected.as_ref().map(|(side, _)| *side) != Some(Side::Unstaged) {
            // Discarding what is staged would mean unstaging *and* undoing, two
            // decisions dressed as one.
            return;
        }
        let selection = self.aim(cx).unwrap_or(Selection::File);
        // Not queued: asked first (SPEC §3 rule 7).
        self.confirming = Some(Write::Discard {
            target: Target::Part { file, selection },
            untracked,
        });
        cx.notify();
    }

    fn confirm_write(&mut self, cx: &mut Context<Self>) {
        if let Some(write) = self.confirming.take() {
            self.queue(write, cx);
        }
    }

    fn cancel_write(&mut self, cx: &mut Context<Self>) {
        self.confirming = None;
        cx.notify();
    }

    /// Fill an empty message box from `commit.template`, if the repository
    /// configures one (SPEC §11).
    ///
    /// Once, when the box is built, and only while it is empty: a template that
    /// overwrote something already typed would be worse than no template. The
    /// read is a `git config` and a file, so it goes to the background executor
    /// like every other Git call.
    fn load_template(&mut self, cx: &mut Context<Self>) {
        let Some(git) = cx.git_runtime().git().cloned() else {
            return;
        };
        let repo = self.store.read(cx).repository().clone();

        cx.spawn(async move |screen, cx| {
            let template = cx
                .background_spawn(async move {
                    omagit_git::ops::template(&git, &repo, &omagit_git::Cancel::new())
                })
                .await;
            let Ok(Some(text)) = template else {
                return;
            };
            screen
                .update(cx, |screen, cx| {
                    screen.pending_template = Some(text);
                    cx.notify();
                })
                .ok();
        })
        .detach();
    }

    /// What is in the message box. For the tests, which have no other way to
    /// see it.
    pub fn message_for_test(&self, cx: &App) -> String {
        self.message(cx)
    }

    /// What is in the message box, or nothing if it has not been built yet.
    fn message(&self, cx: &App) -> String {
        self.commit
            .as_ref()
            .map(|state| state.read(cx).value().to_string())
            .unwrap_or_default()
    }

    fn commit(&mut self, _: &Commit, _: &mut Window, cx: &mut Context<Self>) {
        let message = self.message(cx);
        if message.trim().is_empty() {
            return;
        }
        let write = Write::Commit {
            message,
            options: self.options.clone(),
        };
        if write.is_destructive() {
            self.confirming = Some(write);
            cx.notify();
        } else {
            self.queue(write, cx);
        }
    }

    fn queue(&mut self, write: Write, cx: &mut Context<Self>) {
        let Some(git) = cx.git_runtime().git().cloned() else {
            // Refused with a reason rather than attempted: the runtime already
            // knows `git` is unusable, and the banner says so.
            tracing::warn!("no usable git; the write was not queued");
            return;
        };
        self.store
            .update(cx, |store, cx| store.submit(write, git, cx));
        self.diff.update(cx, |view, cx| view.clear_selection(cx));
        cx.notify();
    }

    /// Which file the diff panel is showing. Public for the tests, and for the
    /// commit column of M5, which acts on the same selection.
    pub fn selected(&self) -> Option<(Side, RepoPath)> {
        self.selected.clone()
    }

    /// Whether a destructive write is waiting on an answer. For the tests, and
    /// for the shell, which must not let Escape leave a screen that is asking
    /// a question.
    pub fn is_confirming(&self) -> bool {
        self.confirming.is_some()
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
        if self.commit.is_none() {
            let box_state = cx.new(|cx| {
                TextareaState::new(window, cx).placeholder(SharedString::from("Message du commit"))
            });
            self.commit = Some(box_state);
            self.load_template(cx);
        }
        if let Some(text) = self.pending_template.take()
            && let Some(state) = self.commit.clone()
            // Still empty? The read took a moment, and the user may have
            // started typing. A template that overwrote that would be worse
            // than no template.
            && state.read(cx).value().is_empty()
        {
            state.update(cx, |state, cx| state.set_value(text, window, cx));
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
            // Moving in the diff.
            .on_action(cx.listener(|screen, _: &NextDiffLine, _, cx| {
                screen.diff.update(cx, |view, cx| view.move_cursor(1, cx));
            }))
            .on_action(cx.listener(|screen, _: &PreviousDiffLine, _, cx| {
                screen.diff.update(cx, |view, cx| view.move_cursor(-1, cx));
            }))
            .on_action(cx.listener(|screen, _: &NextHunk, _, cx| {
                screen.diff.update(cx, |view, cx| view.move_to_hunk(1, cx));
            }))
            .on_action(cx.listener(|screen, _: &PreviousHunk, _, cx| {
                screen.diff.update(cx, |view, cx| view.move_to_hunk(-1, cx));
            }))
            // Picking.
            .on_action(cx.listener(|screen, _: &ToggleLinePick, _, cx| {
                screen
                    .diff
                    .update(cx, |view, cx| view.toggle_line_at_cursor(cx));
            }))
            .on_action(cx.listener(|screen, _: &ToggleHunkPick, _, cx| {
                screen.diff.update(cx, |view, cx| {
                    if let Some(hunk) = view.cursor_hunk() {
                        view.toggle_hunk(hunk, cx);
                    }
                });
            }))
            // Writing.
            .on_action(cx.listener(Self::stage_picked))
            .on_action(cx.listener(Self::unstage_picked))
            .on_action(cx.listener(Self::discard_picked))
            .on_action(cx.listener(Self::stage_all))
            .on_action(cx.listener(Self::commit))
            .on_action(cx.listener(|screen, _: &ToggleAmend, _, cx| {
                screen.options.amend = !screen.options.amend;
                cx.notify();
            }))
            .on_action(cx.listener(|screen, _: &ToggleSignOff, _, cx| {
                screen.options.sign_off = !screen.options.sign_off;
                cx.notify();
            }))
            .on_action(cx.listener(|screen, _: &ToggleNoVerify, _, cx| {
                screen.options.no_verify = !screen.options.no_verify;
                cx.notify();
            }))
            .on_action(cx.listener(|screen, _: &ToggleJournal, _, cx| {
                screen.journal_open = !screen.journal_open;
                cx.notify();
            }))
            // Escape answers the confirmation before it leaves the screen: a
            // dialog that ignores Escape is the one thing every dialog must not
            // do (DESIGN §5, board 07).
            .on_action(cx.listener(|screen, _: &ShowRepositories, window, cx| {
                if screen.confirming.is_some() {
                    screen.cancel_write(cx);
                    cx.stop_propagation();
                } else {
                    let _ = window;
                }
            }))
            .flex()
            .size_full()
            .min_h_0()
            .bg(hsla(t.bg))
            .text_color(hsla(t.text))
            .font_family(fonts.ui.clone())
            .text_size(px(13.0))
            .child(self.sidebar(&palette, &fonts, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(px(FILES_WIDTH))
                    .flex_none()
                    .min_h_0()
                    .border_r_1()
                    .border_color(hsla(t.border))
                    .child(self.commit_box(&palette, &fonts, cx))
                    .child(self.files_column(&palette, &fonts, cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(DIFF_MIN_WIDTH))
                    .min_h_0()
                    .child(self.diff.clone())
                    .children(self.journal_panel(&palette, &fonts, cx))
                    .child(self.statusbar(&palette, &fonts, cx)),
            )
            .children(self.confirmation(&palette, &fonts, cx))
    }
}

impl WorkingCopyScreen {
    /// The commit column of board 03: the message, the flags, the identity,
    /// and the one button that acts.
    fn commit_box(
        &self,
        palette: &Palette,
        fonts: &Fonts,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = palette.tokens;
        let store = self.store.read(cx);
        let staged = store
            .status()
            .value()
            .map(|status| counts(status).0)
            .unwrap_or(0);
        let identity = store.summary().value().and_then(|s| s.committer.clone());
        let unavailable = cx.git_runtime().unavailable().map(ToOwned::to_owned);
        let busy = store.writes().is_busy();
        let failure = store
            .writes()
            .failure()
            .map(|(what, error)| format!("{what} — {error}"));

        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .flex_none()
            .p(px(10.0))
            .border_b_1()
            .border_color(hsla(t.border))
            // Why writing is impossible, before anything is typed rather than
            // after (SPEC §8).
            .children(
                unavailable.map(|reason| {
                    banner(&format!("git indisponible — {reason}"), t.danger, palette)
                }),
            )
            .children(failure.map(|text| banner(&text, t.danger, palette)))
            // The identity warning of SPEC §11, in the same place and for the
            // same reason: before the message is written, not after.
            .children(identity.is_none().then(|| {
                banner(
                    "Aucune identité de committer configurée",
                    t.warning,
                    palette,
                )
            }))
            .child(
                div()
                    .h(px(96.0))
                    .flex_none()
                    .border_1()
                    .border_color(hsla(t.border))
                    .children(self.commit.clone()),
            )
            .children(subject_counter(&self.message(cx), palette, fonts))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(flag("Amend", self.options.amend, palette, fonts))
                    .child(flag("Sign Off", self.options.sign_off, palette, fonts))
                    .child(flag("No Verify", self.options.no_verify, palette, fonts))
                    .child(div().flex_1())
                    .child(
                        div()
                            .font_family(fonts.ui.clone())
                            .text_size(px(11.0))
                            .text_color(hsla(if busy { t.warning } else { t.text_dim }))
                            .child(SharedString::from(if busy {
                                "en cours…".to_owned()
                            } else {
                                format!("{staged} indexé{}", plural(staged))
                            })),
                    ),
            )
    }

    /// The confirmation SPEC §3 rule 7 requires before anything destructive.
    ///
    /// It names what will go and says it cannot be undone, because what discard
    /// removes was never committed and is not in the reflog.
    fn confirmation(
        &self,
        palette: &Palette,
        fonts: &Fonts,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let t = palette.tokens;
        let write = self.confirming.as_ref()?;
        let what = write.describe();

        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(hsla(t.bg).opacity(0.7))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .w(px(460.0))
                        .p(px(16.0))
                        .bg(hsla(t.surface_raised))
                        .border_1()
                        .border_color(hsla(t.border))
                        .child(
                            div()
                                .text_size(px(16.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(SharedString::from(what)),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(hsla(t.text_muted))
                                .child(
                                    "Ce qui disparaît n'a jamais été commité : ce n'est pas dans le reflog, et rien ne le rendra.",
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .flex_1()
                                        .font_family(fonts.mono.clone())
                                        .text_size(px(11.0))
                                        .text_color(hsla(t.text_dim))
                                        .child("Esc pour annuler"),
                                )
                                .child(
                                    div()
                                        .id("cancel-write")
                                        .px(px(12.0))
                                        .py(px(4.0))
                                        .border_1()
                                        .border_color(hsla(t.border))
                                        .cursor_pointer()
                                        .hover(|style| style.bg(hsla(t.surface_hover)))
                                        .on_click(cx.listener(|screen, _, _, cx| {
                                            screen.cancel_write(cx)
                                        }))
                                        .child("Annuler"),
                                )
                                .child(
                                    div()
                                        .id("confirm-write")
                                        .px(px(12.0))
                                        .py(px(4.0))
                                        .bg(hsla(t.danger))
                                        .text_color(hsla(t.bg))
                                        .cursor_pointer()
                                        .on_click(cx.listener(|screen, _, _, cx| {
                                            screen.confirm_write(cx)
                                        }))
                                        .child("Rejeter"),
                                ),
                        ),
                ),
        )
    }

    /// The operations journal of SPEC §11: the exact command, not a summary.
    fn journal_panel(
        &self,
        palette: &Palette,
        fonts: &Fonts,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        if !self.journal_open {
            return None;
        }
        let t = palette.tokens;
        let entries = cx.git_runtime().journal().entries();

        Some(
            div()
                .flex()
                .flex_col()
                .h(px(160.0))
                .flex_none()
                .border_t_1()
                .border_color(hsla(t.border))
                .bg(hsla(t.surface))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .h(px(20.0))
                        .px(px(8.0))
                        .text_size(px(11.0))
                        .text_color(hsla(t.text_muted))
                        .child("JOURNAL DES OPÉRATIONS"),
                )
                .child(div().flex().flex_col().flex_1().overflow_hidden().children(
                    entries.iter().rev().map(|entry| {
                        let (colour, mark) = match &entry.outcome {
                            omagit_git::journal::Outcome::Running => (t.warning, "…"),
                            omagit_git::journal::Outcome::Succeeded { .. } => (t.text_dim, "ok"),
                            omagit_git::journal::Outcome::Failed { .. } => (t.danger, "!"),
                        };
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(8.0))
                            .h(px(18.0))
                            .font_family(fonts.mono.clone())
                            .text_size(px(11.0))
                            .child(
                                div()
                                    .w(px(16.0))
                                    .flex_none()
                                    .text_color(hsla(colour))
                                    .child(SharedString::from(mark)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .truncate()
                                    .text_color(hsla(if entry.destructive {
                                        t.danger
                                    } else {
                                        t.text
                                    }))
                                    .child(SharedString::from(entry.command.clone())),
                            )
                    }),
                )),
        )
    }

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
        let toggle_path = path.clone();
        let untracked = entry.is_some_and(StatusEntry::is_untracked);

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
            // Board 03 draws a checkbox on every file row, and it is the only
            // gesture that moves a *whole* file: the diff keys always aim at a
            // hunk, so without this a text file could not be staged in one go.
            .child(
                div()
                    .id(SharedString::from(format!(
                        "toggle:{}:{}",
                        side.label(),
                        path
                    )))
                    .w(px(12.0))
                    .h(px(12.0))
                    .flex_none()
                    .border_1()
                    .border_color(hsla(if side == Side::Staged {
                        t.accent
                    } else {
                        t.border
                    }))
                    .when(side == Side::Staged, |element| element.bg(hsla(t.accent)))
                    .cursor_pointer()
                    .hover(|style| style.border_color(hsla(t.accent)))
                    .on_click(cx.listener(move |screen, _, _, cx| {
                        screen.toggle_file(side, toggle_path.clone(), untracked, cx);
                    })),
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

/// The subject length, once it is worth saying.
///
/// Board 03: it appears only past 50 characters, in `text_dim`, and turns
/// `warning` past 72. Silent below 50 on purpose — a counter that is always
/// there is a counter nobody reads.
fn subject_counter(message: &str, palette: &Palette, fonts: &Fonts) -> Option<gpui_kit::Div> {
    let t = palette.tokens;
    let subject = message.lines().next().unwrap_or_default();
    let length = subject.chars().count();
    if length <= 50 {
        return None;
    }
    Some(
        div()
            .flex()
            .justify_end()
            .font_family(fonts.mono.clone())
            .text_size(px(11.0))
            .text_color(hsla(if length > 72 { t.warning } else { t.text_dim }))
            .child(SharedString::from(format!("{length} / 72"))),
    )
}

/// A one-line warning above the message box.
fn banner(text: &str, colour: Rgb, palette: &Palette) -> gpui_kit::Div {
    div()
        .px(px(8.0))
        .py(px(4.0))
        // The status-tint scale of DESIGN-TOKENS §5, not an opacity invented
        // here: `tint_medium` is what a status chip uses.
        .bg(hsla(omagit_theme::mix(
            colour,
            palette.tokens.bg,
            omagit_theme::Percent::new(omagit_theme::Tint::Medium.percent()),
        )))
        .border_l_2()
        .border_color(hsla(colour))
        .text_size(px(11.0))
        .text_color(hsla(colour))
        .child(SharedString::from(text.to_owned()))
}

/// One of the three commit flags. A checkbox that says what it does when it is
/// on, since two of the three change what the commit *is*.
fn flag(label: &'static str, on: bool, palette: &Palette, fonts: &Fonts) -> gpui_kit::Div {
    let t = palette.tokens;
    div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .font_family(fonts.ui.clone())
        .text_size(px(11.0))
        .text_color(hsla(if on { t.text } else { t.text_dim }))
        .child(
            div()
                .w(px(10.0))
                .h(px(10.0))
                .flex_none()
                .border_1()
                .border_color(hsla(if on { t.accent } else { t.border }))
                .when(on, |element| element.bg(hsla(t.accent))),
        )
        .child(SharedString::from(label))
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

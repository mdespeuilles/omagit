//! The History screen — DESIGN.md board 05.
//!
//! A virtualised list of commits with the graph drawn in its gutter, and a
//! detail panel for the selected one. SPEC §12 asks for a hundred thousand
//! commits without slowing down, which shapes the whole screen: the list is
//! virtualised, the history arrives a thousand rows at a time, and the lane
//! assignment for each row was computed once when its page landed
//! (`omagit-git::graph`) rather than per frame.
//!
//! The gutter draws what that assignment says and decides nothing itself. Lane
//! colours come from the palette, which generated them without reading the
//! theme (DESIGN-TOKENS §6); a lane's colour is arbitrary and only has to
//! differ from its neighbour's.

use gpui_kit::base::{VirtualListScrollHandle, v_virtual_list};
use gpui_kit::prelude::*;
use gpui_kit::{
    App, Context, Entity, FocusHandle, Focusable, IntoElement, SharedString, Size, Subscription,
    Window, div, px,
};

use omagit_git::ObjectId;
use omagit_ui::{ActiveFonts, ActivePalette, Fonts, Palette, hsla};

use crate::actions::*;
use crate::repo_store::{HistoryRow, RepoStore};
use crate::time;

/// Board 05: the graph gutter is 100px, and 60px on a narrow window.
const GUTTER_WIDTH: f32 = 100.0;
/// The horizontal distance between two lanes inside the gutter.
const LANE_STEP: f32 = 12.0;
const ROW_HEIGHT: f32 = 28.0;
/// The node: a filled disc for an ordinary commit, hollow for a merge.
const NODE_RADIUS: f32 = 3.0;
/// How close to the end the list gets before the next page is asked for. One
/// screenful, so the rows arrive before the scroll reaches them.
const PREFETCH_ROWS: usize = 60;

pub struct HistoryScreen {
    store: Entity<RepoStore>,
    focus: FocusHandle,
    selected: Option<usize>,
    scroll: VirtualListScrollHandle,
    landed: bool,
    _subscriptions: Vec<Subscription>,
}

impl HistoryScreen {
    pub fn new(store: Entity<RepoStore>, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        let subscriptions = vec![cx.observe(&store, |_, _, cx| cx.notify())];
        store.update(cx, |store, cx| store.load_more_history(cx));

        Self {
            store,
            focus,
            selected: None,
            scroll: VirtualListScrollHandle::new(),
            landed: false,
            _subscriptions: subscriptions,
        }
    }

    /// Which commit the detail panel is about.
    pub fn selected(&self, cx: &App) -> Option<ObjectId> {
        let history = self.store.read(cx).history();
        history.rows.get(self.selected?).map(|row| row.commit.id)
    }

    fn rows(&self, cx: &App) -> usize {
        self.store.read(cx).history().rows.len()
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.rows(cx);
        if count == 0 {
            return;
        }
        let next = match self.selected {
            None => 0,
            Some(current) => (current as isize + delta).clamp(0, count as isize - 1) as usize,
        };
        self.selected = Some(next);
        self.scroll
            .scroll_to_item(next, gpui_kit::ScrollStrategy::Center);
        // Walking to the end is how the next page gets asked for, so the list
        // extends under the keyboard as well as under the scroll wheel.
        if next + PREFETCH_ROWS >= count {
            self.store
                .update(cx, |store, cx| store.load_more_history(cx));
        }
        cx.notify();
    }
}

impl Focusable for HistoryScreen {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for HistoryScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette().clone();
        let fonts = cx.fonts().clone();
        let t = palette.tokens;

        if !self.landed {
            self.landed = true;
            window.focus(&self.focus, cx);
        }

        div()
            .key_context(crate::actions::CONTEXT_HISTORY)
            .track_focus(&self.focus)
            .on_action(cx.listener(|screen, _: &SelectNext, _, cx| screen.move_selection(1, cx)))
            .on_action(
                cx.listener(|screen, _: &SelectPrevious, _, cx| screen.move_selection(-1, cx)),
            )
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .bg(hsla(t.bg))
            .text_color(hsla(t.text))
            .font_family(fonts.ui.clone())
            .text_size(px(13.0))
            .child(self.header(&palette, &fonts, cx))
            .child(self.list(&palette, &fonts, cx))
    }
}

impl HistoryScreen {
    fn header(&self, palette: &Palette, fonts: &Fonts, cx: &mut Context<Self>) -> impl IntoElement {
        let t = palette.tokens;
        let history = self.store.read(cx).history();
        let loaded = history.rows.len();

        div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .flex_none()
            .h(px(28.0))
            .px(px(12.0))
            .border_b_1()
            .border_color(hsla(t.border))
            .overflow_hidden()
            .child(
                div()
                    .flex_none()
                    .font_family(fonts.mono.clone())
                    .text_size(px(11.0))
                    .text_color(hsla(t.text_muted))
                    .child(SharedString::from(format!(
                        "{loaded} commit{}",
                        if loaded > 1 { "s" } else { "" }
                    ))),
            )
            // "loaded so far" and "all of them" are different facts, and a list
            // that does not say which it is showing invites the wrong
            // conclusion from a short one.
            .child(
                div()
                    .flex_none()
                    .text_size(px(11.0))
                    .text_color(hsla(t.text_dim))
                    .child(SharedString::from(if history.complete {
                        "· tout l'historique".to_owned()
                    } else if history.loading {
                        "· lecture…".to_owned()
                    } else {
                        "· et plus au défilement".to_owned()
                    })),
            )
            .child(div().flex_1())
            .children(history.failure.as_ref().map(|error| {
                div()
                    .flex_none()
                    .text_size(px(11.0))
                    .text_color(hsla(t.danger))
                    .child(SharedString::from(error.to_string()))
            }))
    }

    fn list(
        &mut self,
        palette: &Palette,
        fonts: &Fonts,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let count = self.rows(cx);
        if count == 0 {
            let t = palette.tokens;
            return div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_size(px(12.0))
                .text_color(hsla(t.text_dim))
                .child(if self.store.read(cx).history().loading {
                    "Lecture de l'historique…"
                } else {
                    "Aucun commit"
                })
                .into_any_element();
        }

        let sizes = std::rc::Rc::new(vec![
            Size {
                width: px(0.0),
                height: px(ROW_HEIGHT),
            };
            count
        ]);
        let palette = palette.clone();
        let fonts = fonts.clone();
        let selected = self.selected;

        div()
            .flex_1()
            .min_h_0()
            .child(
                v_virtual_list(
                    cx.entity(),
                    "history-rows",
                    sizes,
                    move |screen, range, _, cx| {
                        let history = screen.store.read(cx).history();
                        history.rows[range.clone()]
                            .iter()
                            .enumerate()
                            .map(|(offset, row)| {
                                commit_row(
                                    row,
                                    selected == Some(range.start + offset),
                                    &palette,
                                    &fonts,
                                )
                            })
                            .collect::<Vec<_>>()
                    },
                )
                .track_scroll(&self.scroll)
                .w_full()
                .h_full(),
            )
            .into_any_element()
    }
}

/// One commit, with its slice of the graph beside it.
fn commit_row(
    row: &HistoryRow,
    selected: bool,
    palette: &Palette,
    fonts: &Fonts,
) -> gpui_kit::AnyElement {
    let t = palette.tokens;
    let short = row.commit.id.to_string()[..7].to_owned();
    let when = time::ago(row.commit.author.time.seconds);

    div()
        .flex()
        .items_center()
        .gap(px(10.0))
        .h(px(ROW_HEIGHT))
        .w_full()
        .px(px(4.0))
        .when(selected, |element| element.bg(hsla(t.surface_raised)))
        .when(!selected, |element| {
            element.hover(|style| style.bg(hsla(t.surface_hover)))
        })
        .child(gutter(row, palette))
        .child(
            div()
                .w(px(64.0))
                .flex_none()
                .font_family(fonts.mono.clone())
                .text_size(px(11.5))
                .text_color(hsla(t.text_dim))
                .child(SharedString::from(short)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_family(fonts.mono.clone())
                .text_size(px(12.5))
                .child(SharedString::from(row.commit.summary.clone())),
        )
        .child(
            div()
                .w(px(140.0))
                .flex_none()
                .truncate()
                .text_size(px(11.0))
                .text_color(hsla(t.text_muted))
                .child(SharedString::from(row.commit.author.name.clone())),
        )
        .child(
            div()
                .w(px(90.0))
                .flex_none()
                .text_size(px(11.0))
                .text_color(hsla(t.text_dim))
                .child(SharedString::from(when)),
        )
        .into_any_element()
}

/// The graph, for one row.
///
/// Absolutely positioned inside a fixed-width column: a lane is a column
/// number, and turning it into an x is the only arithmetic here. Everything
/// about *which* lanes are drawn was decided when the page was laid out.
fn gutter(row: &HistoryRow, palette: &Palette) -> gpui_kit::Div {
    let lane_x = |lane: usize| px(6.0 + lane as f32 * LANE_STEP);
    let colour = |lane: usize| hsla(palette.lane(lane));

    let mut column = div()
        .relative()
        .w(px(GUTTER_WIDTH))
        .h_full()
        .flex_none()
        .overflow_hidden();

    // Lines that pass this row untouched: a straight vertical, full height.
    for lane in &row.graph.passing {
        column = column.child(
            div()
                .absolute()
                .left(lane_x(*lane))
                .top(px(0.0))
                .w(px(1.5))
                .h(px(ROW_HEIGHT))
                .bg(colour(*lane)),
        );
    }

    // Lines arriving from above, and leaving below. Drawn as an L rather than a
    // curve for now: the vertical half in its own lane, the horizontal half at
    // the node's height. Board 05 asks for a quarter-circle, which is a shape
    // this element tree cannot express — recorded rather than faked with a
    // diagonal that would read as a different kind of link.
    let node_x = lane_x(row.graph.lane);
    for (lane, top_half) in row
        .graph
        .incoming
        .iter()
        .map(|lane| (*lane, true))
        .chain(row.graph.outgoing.iter().map(|lane| (*lane, false)))
    {
        let x = lane_x(lane);
        column = column.child(
            div()
                .absolute()
                .left(x)
                .top(if top_half {
                    px(0.0)
                } else {
                    px(ROW_HEIGHT / 2.0)
                })
                .w(px(1.5))
                .h(px(ROW_HEIGHT / 2.0))
                .bg(colour(lane)),
        );
        if x != node_x {
            let (left, width) = if x < node_x {
                (x, node_x - x)
            } else {
                (node_x, x - node_x)
            };
            column = column.child(
                div()
                    .absolute()
                    .left(left)
                    .top(px(ROW_HEIGHT / 2.0 - 0.75))
                    .w(width)
                    .h(px(1.5))
                    .bg(colour(lane)),
            );
        }
    }

    // The node last, so nothing is drawn over it.
    let merge = row.graph.outgoing.len() > 1;
    column.child(
        div()
            .absolute()
            .left(node_x - px(NODE_RADIUS))
            .top(px(ROW_HEIGHT / 2.0 - NODE_RADIUS))
            .w(px(NODE_RADIUS * 2.0))
            .h(px(NODE_RADIUS * 2.0))
            .rounded_full()
            // Hollow for a merge, filled otherwise (board 05). The hollow one is
            // filled with the background rather than left transparent, so a line
            // passing underneath does not show through it.
            .bg(hsla(if merge {
                palette.tokens.bg
            } else {
                palette.lane(row.graph.lane)
            }))
            .border_1()
            .border_color(colour(row.graph.lane)),
    )
}

//! The chrome both repository screens sit inside.
//!
//! Boards 03 and 05 draw the same sidebar and the same statusbar around the
//! Working Copy and around History. They belonged to the Working Copy, which is
//! why opening History left a window with no way out of it but `Esc` and no
//! sign of where you were — the panels did not "disappear", they were never
//! that screen's to draw.
//!
//! So the shell owns the chrome and a screen owns its body, which is what the
//! boards show and what keeps a third screen from having to reimplement either.

use gpui_kit::prelude::*;
use gpui_kit::{App, Entity, FontWeight, SharedString, div, px};

use omagit_git::Head;
use omagit_ui::primitives::{Pip, pip};
use omagit_ui::{Fonts, Icon, Palette, hsla};

use crate::actions::{ShowHistory, ShowRepositories, ShowWorkingCopy};
use crate::repo_store::{RepoStore, Side};
use crate::screens::working_copy::{SIDEBAR_WIDTH, has, nav_header, nav_row, plural};
use crate::time;

/// Which screen the sidebar should mark as the one you are on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Active {
    WorkingCopy,
    History,
}

pub fn sidebar(
    store: &Entity<RepoStore>,
    active: Active,
    palette: &Palette,
    fonts: &Fonts,
    cx: &App,
) -> gpui_kit::Div {
    let t = palette.tokens;
    let store = store.read(cx);
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
    let changed = store
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
                .child(
                    div()
                        .id("nav-working-copy")
                        .cursor_pointer()
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(ShowWorkingCopy), cx);
                        })
                        .child(nav_row(
                            "Working Copy",
                            Some(changed.to_string()),
                            active == Active::WorkingCopy,
                            None,
                            palette,
                            fonts,
                        )),
                )
                .child(
                    div()
                        .id("nav-history")
                        .cursor_pointer()
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(ShowHistory), cx);
                        })
                        .child(nav_row(
                            "History",
                            None,
                            active == Active::History,
                            None,
                            palette,
                            fonts,
                        )),
                )
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

pub fn statusbar(
    store: &Entity<RepoStore>,
    palette: &Palette,
    fonts: &Fonts,
    cx: &App,
) -> gpui_kit::Div {
    let t = palette.tokens;
    let store = store.read(cx);
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
                            SharedString::from(format!("↑{} ↓{}", tracking.ahead, tracking.behind))
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

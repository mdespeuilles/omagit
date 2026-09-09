//! The main window: the topbar, and whichever screen is showing under it.
//!
//! Board 02 is the authority on the topbar and it says one thing loudly:
//! **content and order never change between platforms — only the edge reserves
//! do**, and a reserve is a flex spacer, never a conditional padding. That is
//! why the leading and trailing spacers are always there and only their width
//! differs.
//!
//! There is one screen so far (M3). The router that swaps Working Copy and
//! History in arrives with them; until then the topbar names the screen and the
//! open repository, and nothing else routes.

use gpui_kit::prelude::*;
use gpui_kit::{
    App, Bounds, Context, Entity, FontWeight, IntoElement, Pixels, SharedString, Subscription,
    Window, WindowBounds, WindowOptions, div, px, size,
};

use omagit_git::Repository;
use omagit_theme::DensityMode;
use omagit_ui::{ActiveFonts, ActivePalette, Fonts, Palette, hsla};

use crate::actions::{ShowHistory, ShowRepositories, ShowWorkingCopy};
use crate::platform::{self, Platform, TOPBAR_HEIGHT_COMFORTABLE, TOPBAR_HEIGHT_COMPACT};
use crate::repo_store::RepoStore;
use crate::screens::{HistoryScreen, RepositoriesScreen, WorkingCopyScreen, chrome};
use crate::store::Store;

/// The reference window of the mock-ups is 1600×1000; below 1100px of usable
/// width the layout collapses to tabs, which is why the minimum sits here.
const DEFAULT_SIZE: (f32, f32) = (1600.0, 1000.0);
const MIN_SIZE: (f32, f32) = (900.0, 600.0);

/// Which screen is showing.
///
/// An enum rather than a stack: the three screens of SPEC §1 are peers, not a
/// navigation history, and `Esc` goes *up a level* (DESIGN §5) rather than back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Repositories,
    WorkingCopy,
    History,
}

/// What the topbar's trail shows.
///
/// A decision worth its own name, because it was wrong: the trail was keyed on
/// whether the store had a repository *open* rather than on which screen is
/// *showing*. Coming back to the list does not close the repository — the
/// Working Copy keeps its state and its watcher, which is what makes returning
/// to it instant — so the list drew a segment for a repository it was not
/// showing, offered a way back to the screen already under the pointer, and
/// named `Esc` for a key that does something else there.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Trail {
    /// The repository segment, and with it the way back and the `Esc` hint.
    /// `None` on the list, where "Dépôts" is the name of where you are.
    repository: Option<String>,
}

impl Trail {
    fn of(screen: Screen, open: Option<String>) -> Self {
        Self {
            repository: match screen {
                Screen::Repositories => None,
                Screen::WorkingCopy | Screen::History => open,
            },
        }
    }
}

pub struct Shell {
    density: DensityMode,
    platform: &'static dyn Platform,
    store: Entity<Store>,
    repositories: Entity<RepositoriesScreen>,
    screen: Screen,
    /// Built when a repository is opened, and kept afterwards: it owns the
    /// filesystem watcher, so keeping it means coming back to a screen that is
    /// already up to date rather than one that has to re-read.
    working_copy: Option<(std::path::PathBuf, Entity<WorkingCopyScreen>)>,
    /// Built the first time History is asked for, and kept: its walk and its
    /// lane assignment are what make coming back to it instant.
    history: Option<(std::path::PathBuf, Entity<HistoryScreen>)>,
    /// Held for its lifetime: dropping it stops the window following the
    /// system's light/dark preference.
    _appearance: Subscription,
    _store_changed: Subscription,
}

impl Shell {
    pub fn new(
        density: DensityMode,
        appearance: Subscription,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let config_dir = platform::current().config_dir();
        let store = cx.new(|cx| Store::new(config_dir, cx));
        let repositories = cx.new(|cx| RepositoriesScreen::new(store.clone(), window, cx));
        let store_changed = cx.observe(&store, |_, _, cx| cx.notify());
        Self {
            density,
            platform: platform::current(),
            store,
            repositories,
            screen: Screen::Repositories,
            working_copy: None,
            history: None,
            _appearance: appearance,
            _store_changed: store_changed,
        }
    }

    fn topbar_height(&self) -> Pixels {
        px(match self.density {
            DensityMode::Compact => TOPBAR_HEIGHT_COMPACT,
            DensityMode::Comfortable => TOPBAR_HEIGHT_COMFORTABLE,
        })
    }

    fn topbar(&self, palette: &Palette, fonts: &Fonts, cx: &App) -> impl IntoElement {
        let t = palette.tokens;
        let reserve = self.platform.topbar_reserve();
        let modifier = self.platform.primary_modifier();
        let showing_repository = Trail::of(
            self.screen,
            self.store
                .read(cx)
                .open_repository()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned()),
        )
        .repository;

        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .w_full()
            .h(self.topbar_height())
            .flex_none()
            .px(px(8.0))
            .bg(hsla(t.bg))
            .border_b_1()
            .border_color(hsla(t.border))
            // Leading reserve: on macOS the traffic lights are drawn here by
            // the system, and nothing of ours may sit under them.
            .child(div().w(reserve.leading).flex_none())
            .child(
                div()
                    .pl(px(4.0))
                    .font_family(fonts.mono.clone())
                    .text_size(px(12.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(t.text))
                    .child("omagit"),
            )
            .child(separator(palette))
            // A trail reads as a trail, so it has to behave like one: from a
            // repository this is the way back to the list, and it was inert —
            // the ones that worked were Escape and a row pinned to the bottom
            // of the sidebar, neither of which you find by looking. On the list
            // itself it is where you already are, so it is a label.
            .child({
                let crumb = div().px(px(4.0)).text_size(px(12.5));
                match showing_repository.is_some() {
                    false => crumb
                        .text_color(hsla(t.text))
                        .child("Dépôts")
                        .into_any_element(),
                    true => crumb
                        .id("back-to-repositories")
                        .text_color(hsla(t.text_muted))
                        .cursor_pointer()
                        .hover(|style| style.bg(hsla(t.surface_hover)).text_color(hsla(t.text)))
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(ShowRepositories), cx);
                        })
                        .child("Dépôts")
                        .into_any_element(),
                }
            })
            .children(showing_repository.map(|name| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(separator(palette))
                    .child(
                        div()
                            .font_family(fonts.mono.clone())
                            .text_size(px(12.5))
                            .text_color(hsla(t.text))
                            .child(SharedString::from(name)),
                    )
                    // The keyboard way back, named where the pointer way back
                    // is. Board 02 puts a shortcut next to the action it
                    // belongs to; this is the same idea one level up.
                    .child(
                        div()
                            .font_family(fonts.mono.clone())
                            .text_size(px(11.0))
                            .text_color(hsla(t.text_dim))
                            .child("Esc"),
                    )
            }))
            .child(div().flex_1())
            .child(
                topbar_button(
                    "Ajouter un dépôt local",
                    modifier.shortcut("O"),
                    palette,
                    fonts,
                    true,
                )
                .id("add-local")
                .hover(|style| style.bg(hsla(t.surface_hover)))
                // Dispatched as the action rather than called directly: the
                // screen owns what "add a repository" does, the topbar only
                // asks for it — and M9's palette and the macOS menu bar will
                // ask for it the same way.
                .on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(crate::actions::AddLocalRepository), cx);
                }),
            )
            // Cloning is the network, which arrives at M7. Drawn in the
            // disabled state of board 01 rather than hidden: the topbar's
            // content is fixed (board 02), and an action that will exist is
            // better shown as not-yet than silently absent.
            .child(topbar_button(
                "Cloner…",
                "M7".to_owned(),
                palette,
                fonts,
                false,
            ))
            .child(separator(palette))
            .child(topbar_button(
                "Rechercher",
                "M9".to_owned(),
                palette,
                fonts,
                false,
            ))
            .child(div().w(reserve.trailing).flex_none())
    }
}

impl Shell {
    /// Show the repository the Repositories screen just marked as open.
    ///
    /// Opening it is Git work, so it happens on the background executor and the
    /// screen appears when the repository is actually open — not before, and
    /// not by blocking a frame on it.
    /// Show the History of the repository already open.
    ///
    /// It shares the Working Copy's store rather than opening a second one: one
    /// `Entity<RepoStore>` per repository is what SPEC §10 asks for, and two
    /// would mean two watchers and two answers to the same question.
    fn open_history(&mut self, _: &ShowHistory, _: &mut Window, cx: &mut Context<Self>) {
        let Some((path, working_copy)) = self.working_copy.clone() else {
            return;
        };
        if !matches!(&self.history, Some((open, _)) if *open == path) {
            let store = working_copy.read(cx).store().clone();
            let screen = cx.new(|cx| HistoryScreen::new(store, cx));
            self.history = Some((path, screen));
        }
        self.screen = Screen::History;
        cx.notify();
    }

    fn open_working_copy(&mut self, _: &ShowWorkingCopy, _: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.store.read(cx).open_repository().map(ToOwned::to_owned) else {
            return;
        };
        // Already open: switch to it rather than rebuilding, which would drop a
        // watcher only to start the same one again.
        if matches!(&self.working_copy, Some((open, _)) if *open == path) {
            self.screen = Screen::WorkingCopy;
            cx.notify();
            return;
        }

        cx.spawn(async move |shell, cx| {
            let opened = {
                let path = path.clone();
                cx.background_spawn(async move { Repository::open(&path) })
                    .await
            };
            shell
                .update(cx, |shell, cx| match opened {
                    Ok(repo) => {
                        // Replacing the previous pair drops the old store, and
                        // with it the old watcher.
                        let store = cx.new(|cx| RepoStore::new(repo, path.clone(), cx));
                        let screen = cx.new(|cx| WorkingCopyScreen::new(store, cx));
                        shell.working_copy = Some((path, screen));
                        shell.screen = Screen::WorkingCopy;
                        cx.notify();
                    }
                    Err(error) => {
                        // The Repositories screen already draws this repository
                        // as missing; there is nothing to add but a log line.
                        tracing::warn!(%error, "could not open the repository");
                    }
                })
                .ok();
        })
        .detach();
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette().clone();
        let fonts = cx.fonts().clone();
        let t = palette.tokens;

        // The chrome boards 03 and 05 draw around both repository screens: the
        // sidebar that says where you are and how to leave, and the statusbar
        // that reports the repository. It belonged to the Working Copy, so
        // opening History left a window with no way out of it but `Esc`.
        let open_store = match (self.screen, &self.working_copy, &self.history) {
            (Screen::WorkingCopy, Some((_, screen)), _) => {
                Some((screen.read(cx).store().clone(), chrome::Active::WorkingCopy))
            }
            (Screen::History, _, Some((_, screen))) => {
                Some((screen.read(cx).store().clone(), chrome::Active::History))
            }
            _ => None,
        };

        let body = match (self.screen, &self.working_copy, &self.history) {
            (Screen::WorkingCopy, Some((_, screen)), _) => screen.clone().into_any_element(),
            (Screen::History, _, Some((_, screen))) => screen.clone().into_any_element(),
            _ => self.repositories.clone().into_any_element(),
        };

        let body = match &open_store {
            Some((store, active)) => div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_h_0()
                        .child(chrome::sidebar(store, *active, &palette, &fonts, cx))
                        .child(body),
                )
                .child(chrome::statusbar(store, &palette, &fonts, cx))
                .into_any_element(),
            None => body,
        };

        div()
            .on_action(cx.listener(Self::open_working_copy))
            .on_action(cx.listener(Self::open_history))
            .on_action(cx.listener(|shell, _: &ShowRepositories, _, cx| {
                shell.screen = Screen::Repositories;
                cx.notify();
            }))
            .flex()
            .flex_col()
            .size_full()
            .bg(hsla(t.bg))
            .text_color(hsla(t.text))
            .font_family(fonts.ui.clone())
            .text_size(px(13.0))
            .child(self.topbar(&palette, &fonts, cx))
            .child(div().flex().flex_1().min_h_0().child(body))
    }
}

/// The 1px × 18px rule between topbar groups (board 06).
fn separator(palette: &Palette) -> impl IntoElement {
    div()
        .w(px(1.0))
        .h(px(18.0))
        .flex_none()
        .mx(px(4.0))
        .bg(hsla(palette.tokens.border))
}

/// A topbar control. `available` false is board 01's disabled state: the label
/// dims and the shortcut is replaced by the milestone it is waiting for.
fn topbar_button(
    label: &'static str,
    hint: String,
    palette: &Palette,
    fonts: &Fonts,
    available: bool,
) -> gpui_kit::Div {
    let t = palette.tokens;
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .flex_none()
        .h(px(24.0))
        .px(px(10.0))
        .border_1()
        .border_color(hsla(t.border))
        .text_size(px(12.0))
        .text_color(hsla(if available { t.text } else { t.text_dim }))
        .child(label)
        .child(
            div()
                .font_family(fonts.mono.clone())
                .text_size(px(11.0))
                .text_color(hsla(t.text_dim))
                .child(SharedString::from(hint)),
        )
}

/// Window options for the main window, with the platform's decoration rules.
pub fn options(platform: &dyn Platform, cx: &mut App) -> WindowOptions {
    let bounds = Bounds::centered(None, size(px(DEFAULT_SIZE.0), px(DEFAULT_SIZE.1)), cx);
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: platform.titlebar(),
        window_decorations: platform.window_decorations(),
        window_min_size: Some(size(px(MIN_SIZE.0), px(MIN_SIZE.1))),
        app_id: Some("dev.omagit.omagit".into()),
        // The app draws its own topbar and owns dragging from it, so AppKit
        // neither drags nor delays clicks in that band.
        app_owns_titlebar_drag: true,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    fn open() -> Option<String> {
        Some("app.duodeal.com".to_owned())
    }

    #[test]
    fn the_list_names_where_you_are_rather_than_where_you_were() {
        // The bug: a repository stays open behind the list, and the trail was
        // reading that instead of the screen.
        assert_eq!(
            Trail::of(Screen::Repositories, open()),
            Trail { repository: None },
            "no repository segment, so no way back to the screen you are on"
        );
    }

    #[test]
    fn a_repository_on_screen_gets_its_segment() {
        assert_eq!(
            Trail::of(Screen::WorkingCopy, open()),
            Trail { repository: open() }
        );
    }

    #[test]
    fn the_working_copy_without_a_repository_shows_no_segment() {
        // Not a state the shell reaches — it only shows the Working Copy once a
        // repository is open — but the trail must not invent a name for one.
        assert_eq!(
            Trail::of(Screen::WorkingCopy, None),
            Trail { repository: None }
        );
    }
}

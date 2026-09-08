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

use omagit_theme::DensityMode;
use omagit_ui::{ActiveFonts, ActivePalette, Fonts, Palette, hsla};

use crate::platform::{self, Platform, TOPBAR_HEIGHT_COMFORTABLE, TOPBAR_HEIGHT_COMPACT};
use crate::screens::RepositoriesScreen;
use crate::store::Store;

/// The reference window of the mock-ups is 1600×1000; below 1100px of usable
/// width the layout collapses to tabs, which is why the minimum sits here.
const DEFAULT_SIZE: (f32, f32) = (1600.0, 1000.0);
const MIN_SIZE: (f32, f32) = (900.0, 600.0);

pub struct Shell {
    density: DensityMode,
    platform: &'static dyn Platform,
    store: Entity<Store>,
    repositories: Entity<RepositoriesScreen>,
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
        let open = self
            .store
            .read(cx)
            .open_repository()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned());

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
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(hsla(t.text_muted))
                    .child("Dépôts"),
            )
            .children(open.map(|name| {
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

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette().clone();
        let fonts = cx.fonts().clone();
        let t = palette.tokens;

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(hsla(t.bg))
            .text_color(hsla(t.text))
            .font_family(fonts.ui.clone())
            .text_size(px(13.0))
            .child(self.topbar(&palette, &fonts, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.repositories.clone()),
            )
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

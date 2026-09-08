# gpui-kit — what it actually provides

| | |
|---|---|
| Version resolved | **0.6.0**, published 2026-09-03 — the only published version |
| GPUI underneath | the `gpui-pre-*` family at **0.3.4** |
| Verified on | macOS (aarch64), rustc 1.96.0 — 2026-09-08 |
| Build time, cold | ≈ 55 s for the dependency tree |

## Shape of the facade

`gpui_kit::*` re-exports GPUI itself, so omagit never names `gpui` (SPEC §4).
The layers are reachable by name: `gpui_kit::base` (`gpui-base`),
`gpui_kit::platform`, `gpui_kit::component` (feature `component`, on by default)
and `gpui_kit::assets` (feature `assets`).

`gpui-omarchy` depends on it with `default-features = false, features =
["assets"]`, so **`component` is off** in this workspace and `gpui_kit::init`
resolves to `gpui_base::init`. omagit matches that feature set; turning
`component` on would pull a second, competing component library.

`gpui_kit::actions!` exists because GPUI's own macro spells its derive as
`gpui::Action`, which does not resolve through the facade. Use the facade's.

## What was confirmed for M0

- `WindowOptions` carries `titlebar`, `window_decorations`, `window_min_size`,
  `app_id`, `app_owns_titlebar_drag`. All four platform requirements of SPEC §9
  are expressible without a `cfg` in the UI layer.
- `TitlebarOptions::appears_transparent` hides the system titlebar on macOS so
  the app can draw its own topbar under the traffic lights.
- `WindowDecorations::{Server, Client}` is Wayland/X11 only, as the design
  expects for Hyprland CSD.
- `app_owns_titlebar_drag` exists and is documented as the way to avoid AppKit's
  titlebar click delay — needed once the topbar becomes draggable.
- `cx.text_system().all_font_names()` enumerates installed families. It does
  **not** list the platform UI face, which is why `Platform::system_ui_family`
  supplies `.SystemUIFont` on macOS (see `omagit-ui::fonts`).
- `gpui_kit::base::Theme::global_mut(cx)` gives write access to
  `tokens.colors` and `tokens.typography`, which is how the theme bridge lands
  the palette.

## Confirmed for M1

- `Window::observe_window_appearance` returns a `Subscription` and fires on the
  real macOS light/dark switch — verified by toggling the system setting with
  the app running. `WindowAppearance` has four variants; the vibrant pair maps
  to the same modes as the plain pair.
- `App::window_appearance()` reports the appearance before any window exists,
  which is what lets the theme resolve at start-up rather than after first
  paint.
- **`AsyncApp` is not `Send`** — it holds `Weak<AppCell>` over an `Rc`. Anything
  that needs to reach the app from another thread has to hand its result to a
  foreground task through a `Send` channel. This is the constraint that shapes
  the Omarchy watcher.
- `AsyncApp::update` returns `R`, not `Result<R>`, and panics if the app is
  already gone. Safe from a task the app owns, since dropping the task is what
  ends it.
- `BackgroundExecutor::spawn` requires `Future: Send`; `ForegroundExecutor::spawn`
  does not. Blocking work belongs in the former, app access in the latter.

## Confirmed for M3

- **`background_spawn` is the right home for Git work**, and the type system
  says so: it requires `Future: Send`, which is exactly why `omagit_git::Repository`
  is a handle over `gix`'s thread-safe half rather than a live view. The pattern
  the store uses — `cx.spawn` for the foreground half, `cx.background_spawn`
  inside it for the blocking read, `this.update` to land the result — runs a
  status, a reference read and a ninety-day history walk per repository without
  the window dropping a frame. The render-thread guard has not fired once.
- **Wayland decorations under Hyprland work as M0 assumed.** The app runs on
  Hyprland with `WindowDecorations::Client` and draws its own topbar; the
  compositor tiles it without a titlebar of its own. *(Closes the M0 follow-up.)*
  Worth knowing: Hyprland tiles the window by default, so the 1600×1000 of the
  mock-ups is only what a floating window gets — the layout has to hold at
  whatever width the compositor hands it, which is what the 1100px collapse of
  DESIGN §4 is for.
- **`Window::focus_next`/`focus_prev` and `tab_index`/`tab_stop` exist**, and are
  deliberately *not* used. DESIGN §5 wants six stops in a fixed order, two of
  which hold a pair of controls reached with `Tab`, and wrapping that never
  leaves for the window decoration. That is a specific behaviour rather than the
  generic one, so the screen holds its own focus state.
- **`test-support` works, and it is worth the dev-dependency.** `TestAppContext::build(TestDispatcher::new(seed), name)` plus `VisualTestContext::from_window` gives a real widget tree and `simulate_keystrokes`, with no window and no GPU. The `#[gpui::test]` attribute is *not* usable through the facade — its expansion names `gpui::` paths, the same reason `gpui_kit::actions!` exists — so the context is built by hand, which is three lines. It found two bugs in M3 on the first run; see `docs/ARCHITECTURE.md` risk 11.
- **Key-binding contexts take predicates**: `"Repositories && !Input"` is what
  keeps a single-character binding from firing while someone is typing.
  `Input` is the context `gpui-base`'s editor puts on itself.
- **There is no letter-spacing.** `TextStyleRefinement` has no tracking, which
  is why the 0.08em on board 06's uppercase labels is dropped rather than faked
  (DESIGN §6).
- **`svg().data()` takes raw bytes**, so icons can be `include_bytes!`d with no
  asset source at all. An `svg` with no text colour paints nothing — not an
  error, a blank square — so every icon sets one.
- **`gpui_kit::base::input::InputState`** is the text editor: `new(window, cx)`,
  `placeholder`, `value`, `set_value`, `focus`, and an `InputEvent` stream of
  `Change` / `PressEnter` / `Focus` / `Blur`. It is used directly rather than
  through `gpui_omarchy::input`, whose frame has its own fixed padding and
  border that board 06's 26px filter box and in-place description field do not
  match.
- **`prompt_for_paths`, `reveal_path` and `write_to_clipboard`** all exist on
  `App` and cover the folder picker, "Révéler dans le gestionnaire" and the
  remote-URL copy without a platform `cfg`.

## Still to verify, and when

| Question | Milestone |
|---|---|
| `virtual_list` at 100 000 rows, 120 fps (SPEC §12) | M6 |
| tree-sitter integration for diff syntax highlighting | M4 |
| Simulating a *drag* on the test platform | M4 |

## Note

`block 0.1.6`, deep in the Apple dependency chain, emits a
future-incompatibility warning. Not actionable here — it is upstream of
`gpui-pre-apple`. Worth re-checking at each gpui-kit bump.

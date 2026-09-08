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

## Still to verify, and when

| Question | Milestone |
|---|---|
| `virtual_list` at 100 000 rows, 120 fps (SPEC §12) | M6 |
| Test support (`features = ["test-support"]`) for interaction tests | M4 |
| Behaviour of `background_spawn` under the executor for Git work | M2 |
| tree-sitter integration for diff syntax highlighting | M4 |
| Wayland/X11 decorations in practice under Hyprland | M0 follow-up on Linux |

## Note

`block 0.1.6`, deep in the Apple dependency chain, emits a
future-incompatibility warning. Not actionable here — it is upstream of
`gpui-pre-apple`. Worth re-checking at each gpui-kit bump.

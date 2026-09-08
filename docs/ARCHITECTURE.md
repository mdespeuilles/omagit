# ARCHITECTURE.md

Maintained as the project goes (SPEC §7). It records the decisions that are
expensive to reverse and the risks that have to be re-read at each milestone.

**Status: M0 — skeleton.** What exists is listed under "What M0 actually
ships"; everything else here is the shape later milestones fill in, not code
that is present.

---

## 1. Workspace

```
omagit/
├── crates/
│   ├── omagit-git/        Git core — no UI dependency
│   ├── omagit-theme/      DESIGN-TOKENS.md, implemented — no UI dependency
│   ├── omagit-settings/   TOML preferences
│   ├── omagit-ui/         application components on gpui-omarchy
│   └── omagit-app/        binary `omagit`: windows, platform layer, global state
├── vendor/gpui-omarchy/   vendored design system — see vendor/README.md
├── scripts/               check.sh (the milestone gate), sync-vendor.sh
└── docs/                  SPEC · DESIGN-TOKENS · DESIGN · this file · notes/
```

Dependency direction, strictly one-way:

```
omagit-app ──▶ omagit-ui ──▶ omagit-theme
     │              │
     │              └──▶ gpui-omarchy ──▶ gpui-kit
     ├──▶ omagit-git      (no UI dependency, ever)
     └──▶ omagit-settings ──▶ omagit-theme
```

`omagit-git` and `omagit-theme` compile and test with no window and no
renderer. That is what makes them testable, and it is a rule, not a
coincidence (SPEC §3 rules 5 and 6).

## 2. Decisions

### 2.1 The theme vocabulary is ours, not the design system's

`gpui_omarchy::Theme` predates `DESIGN-TOKENS.md` and names things differently
(`inset`, `bright`, `secondary`, `on_accent`), has no `surface_hover`, no
`text_dim`, no `info`, no diff surfaces, no lanes, no per-theme mixing
coefficients, and no OKLCH derivation. Full comparison in
`docs/notes/gpui-omarchy-capabilities.md`.

So: **`omagit-theme` owns the vocabulary**, and `gpui_omarchy::Theme` is a
projection target. `omagit-ui::theme_bridge` fills it in so the vendored
components sit in the same palette, and publishes the canonical tokens as a GPUI
global that omagit's own components read. The mapping is lossy in one direction
only, and never read back.

Consequence to hold: a component that reaches for `gpui_omarchy::Theme` instead
of `omagit_ui::Palette` will silently lose four tokens. Reviews should catch it.

### 2.2 Typography stacks are resolved, not requested

DESIGN-TOKENS §8 specifies stacks the way CSS does. The renderer's
`font_family` takes one family and does no walking, so `omagit-ui::fonts`
resolves each stack against the installed families once at start-up.

`system-ui` and `-apple-system` are generics the font list never enumerates:
the platform supplies its own name through `Platform::system_ui_family`
(`.SystemUIFont` on macOS, `sans-serif` on Linux). Resolving it inside
`omagit-ui` would need a `cfg(target_os)` in the UI layer, which SPEC §3 rule 6
forbids — hence the trait method.

### 2.3 Edge reserves are flex spacers

The macOS 78px traffic-light band is laid out as a spacer, never as a
conditional padding (DESIGN-TOKENS §9). Padding would move everything else when
the platform changes; a spacer does not. The Linux caption reserve (115px, three
38px buttons plus a separator) works the same way and arrives with the topbar.

### 2.4 Git stays off the render thread, and it is asserted

`omagit_git::thread_guard` registers the render thread at start-up. Every entry
point into the Git core opens with `assert_off_render_thread`, which panics in
debug and costs one relaxed atomic load in release. SPEC §15 names a blocked UI
thread as the most likely failure mode; a panic pointing at the offending call
site is much easier to diagnose than a frozen window.

### 2.5 Hybrid Git backend — decided, not yet built

`gix` for reads, the `git` binary for writes and the network, behind a
`GitBackend` trait with a `HybridBackend` routing between them (SPEC §8). Three
reasons, all about not corrupting data: merge/rebase semantics are subtle
(hooks, strategies, `rerere`, user config); the user's hooks must run, or team
workflows break; and existing credential helpers (`libsecret`, `osxkeychain`)
then work with nothing reimplemented.

The split is **provisional until M2**, which must measure what the resolved
`gix` actually covers — see `docs/notes/gitoxide-capabilities.md` for the six
questions it has to answer with a test each.

### 2.6 Lane colours never read the theme

Eight evenly spaced hues, `oklch(lane_lightness, lane_chroma, 20° + i × 45°)`.
A lane's colour is arbitrary and carries no meaning; it only has to differ from
its neighbour, and no theme palette can guarantee eight separable hues — Matte
Black has zero. Only `lane_lightness` and `lane_chroma` vary by theme, so lanes
stay readable against the background. Arrives at M1.

### 2.7 The vendored design system is not linted

`vendor/gpui-omarchy/src/` is byte-identical to the published crate, and stays
that way: it is what lets `scripts/sync-vendor.sh` tell an upstream change from
one of ours. Clippy is silenced from the vendored manifest rather than from the
sources. Details and the divergence list in `vendor/README.md`.

## 3. Data flow (from M2 onwards)

```
┌─────────────┐   command     ┌──────────────┐  background  ┌────────────┐
│  Component  │──────────────▶│  RepoStore   │─────spawn───▶│ omagit-git │
│    GPUI     │◀──cx.notify()─│ Entity<Repo> │◀────result───│            │
└─────────────┘               └──────────────┘              └────────────┘
                                     ▲
                              targeted invalidation
                              ┌──────────────┐
                              │  FS watcher  │
                              └──────────────┘
```

`Entity<RepoStore>` is the single source of truth per repository. Components
observe it and hold view state only — scroll, selection, folding — never
duplicated Git state. Each sub-state is an `AsyncState<T>` (`Idle | Loading |
Ready(T, generation) | Failed(GitError)`) and the UI renders all four cases
explicitly. Stale results are dropped by generation number. All mutating
operations go through a per-repository serial queue: two writing Git commands
must never run at once on the same repository.

## 4. What M0 actually ships

- The workspace, the pinned toolchain, and `scripts/check.sh` as the gate.
- `gpui-omarchy` 0.1.1 vendored, with its divergences recorded and a sync script.
- `omagit-theme`: the token vocabulary under canonical names, the sRGB `mix`
  derivation of DESIGN-TOKENS §4.1 and §5, the density and typography scales,
  two embedded themes.
- `omagit-git`: the error type and the render-thread guard.
- `omagit-settings`: TOML preferences that degrade to defaults rather than fail.
- `omagit-ui`: the theme bridge and the font-stack resolution.
- `omagit-app`: the `Platform` trait with Linux and macOS implementations, a
  decorated window honouring the platform's edge reserves, rotating file logs.
- CI running the gate on Linux and macOS.

Deliberately **not** shipped, to keep M0 a skeleton: OKLCH derivation, contrast
correction, lane generation, the full catalogue, Omarchy tracking, macOS
appearance tracking (all M1), and anything that reads a repository (M2).

## 5. Risks

Re-read at every milestone (SPEC §15).

| # | Risk | State at M0 |
|---|---|---|
| 1 | **The UI thread blocks.** The most likely failure mode. | Guard in place and tested (`omagit_git::thread_guard`), armed from the app's start-up. Holds only if every Git entry point calls it — a review item from M2. |
| 2 | **The `gix` / CLI split lands wrong.** | Not yet exercised. The six questions M2 must answer are written down in `docs/notes/gitoxide-capabilities.md`. Rule: when `gix` does not cover a read case, move it to the CLI and write down why — never work around it. |
| 3 | **`gpui-omarchy` is incomplete.** | Confirmed: no diff view, no graph, no palette, no file tree, and a theme vocabulary that does not match ours. Mitigated by vendoring and by owning the tokens. Its `virtual_list`, `resizable` and `tree` look reusable — to be confirmed against 100 000 rows at M6. |
| 4 | **The commit graph.** The hardest algorithm here. | Not started. It stays in `omagit-git/graph.rs`, computing topology only, never colour, and never coupled to rendering. |
| 5 | **Data loss.** | No mutating operation exists yet. The rule stands: every destructive command is logged with its exact command line **before** it runs, and when a Git semantic is in doubt, the CLI decides. |
| 6 | **macOS distribution cost.** | Unchanged and recurring: Apple developer account, signing, notarisation, a macOS CI runner. Budget it now, not at M10. |

A seventh, found while building M0 and worth watching: `block 0.1.6`, deep under
`gpui-pre-apple`, emits a future-incompatibility warning. Not actionable from
here; re-check at each `gpui-kit` bump.

# gpui-omarchy — what it actually provides

Verified against the resolved version, not the README (SPEC §3 rule 1).

| | |
|---|---|
| Version resolved | **0.1.1**, published 2026-09-08 — the brief was written against 0.1.0 |
| Vendored at | `vendor/gpui-omarchy`, upstream commit `e991669` |
| Pins | `gpui-kit = "=0.6.0"`, hard |
| Verified on | macOS (aarch64), rustc 1.96.0 — 2026-09-08 |

## Components that exist

`button`, `button_group` / `tab_list`, `calendar`, `color_picker`, `date_picker`,
`dialog`, `dock`, `focus_scope`, `hover_card`, `icon`, `input` / `textarea` /
`number_input`, `link`, `list` (`virtual_list`, `scrollbar`), `menu`,
`navigation`, `otp_input`, `popover`, `resizable`, `select` / `combobox`,
`sheet`, `slider`, `surface`, `table`, `text` (markdown / html), `tooltip`,
`tree`. Its 39 tests and 5 integration tests pass here unmodified.

`virtual_list`, `resizable` and `tree` cover three of the pieces omagit would
otherwise have had to write: the virtualisation SPEC §12 requires, the split
panes of every screen, and the branch tree of the sidebar. Worth confirming
against a 100 000-row list at M6 before relying on it.

## What is missing for omagit

Nothing resembling a diff viewer, a commit graph, a command palette or a file
tree with Git status. Those are `omagit-ui`'s job, composed on top (SPEC §15
risk 3).

## The theme vocabulary does not match ours — this is the important one

`gpui_omarchy::Theme` predates `DESIGN-TOKENS.md` and names things differently:

| gpui-omarchy | Nearest canonical token | Note |
|---|---|---|
| `background` | `bg` | same |
| `surface` | `surface` | same |
| `inset` | — | no canonical equivalent; backs gpui-base's `muted` |
| `foreground` | `text` | same |
| `secondary` | `text_muted` | fixed 55% opacity upstream, not a per-theme coefficient |
| `bright` | — | a second foreground; omagit has one |
| `selection` | `surface_raised` | same role |
| `border` | `border` | same |
| `accent` / `on_accent` | `accent` / — | `on_accent` is a projection detail |
| `danger` / `warning` / `success` | same | — |
| — | `surface_hover` | **absent**: hover would have to reuse selection |
| — | `text_dim` | absent |
| — | `info` | absent |
| — | the four diff surfaces | absent |
| — | lanes, density, the tint scale | absent |

It also has **no** OKLCH derivation, no contrast correction, and no per-theme
mixing coefficients: `hover_fill`, `divider` and friends are fixed opacities on
`foreground`.

**Conclusion.** `omagit-theme` owns the token vocabulary; `gpui_omarchy::Theme`
is a *projection target*, filled in by `omagit-ui::theme_bridge` so the vendored
components sit in the same palette. The mapping is lossy in one direction only,
and omagit's own components never read it.

## Behaviours to know about

1. **`init` starts a poller.** `gpui_omarchy::init` ends with
   `Theme::follow_system`, which spawns a task that stats
   `$HOME/.local/state/omarchy/current` **once a second, forever** — on macOS
   too, where nothing will ever be there. Applying an explicit theme calls
   `stop_following`, so `omagit_ui::apply` right after `init` cancels it. Do not
   remove that ordering.
2. **It supports legacy Omarchy layouts.** It falls back to
   `~/.config/omarchy/current`. SPEC §6.1 explicitly does **not** support legacy
   layouts, which is one reason omagit reads Omarchy itself at M1 rather than
   delegating.
3. **Its guaranteed-key set is smaller than ours.** It requires only
   `background`, `foreground` and `accent`, and infers `mode` from luminance
   when absent. DESIGN-TOKENS §2.1 guarantees six keys and falls the whole
   palette back if any is missing. Different contracts — another reason to own
   the reading.
4. **It sets the mono family to the UI family** (`type_scale.mono = self.font`).
   DESIGN-TOKENS §8 puts every Git literal in a real monospace, so
   `omagit-ui::theme_bridge::apply` corrects it after applying.
5. **All radii are zero.** Matches the design's square corners; nothing to do.
6. **macOS behaves correctly with no Omarchy present**: `system_from_home`
   returns Tokyo Night rather than panicking, and there is no hardcoded Linux
   path outside that lookup. Verified by running the app on macOS — see the
   start-up log, which resolves the embedded theme without touching Omarchy.

## A packaging trap

Upstream ships this in its own manifest:

```toml
[profile.release.package.gpui-pre-macros]
debug-assertions = true
```

Cargo **ignores `[profile]` in a workspace member**, so vendoring it silently
drops the workaround, and nothing warns.

Upstream blames `gpui-pre-macros` 0.3.1. The version resolved here is 0.3.4, and
a release build of `gpui-omarchy` with the override removed **succeeds**
(verified 2026-09-08), so the workaround is not carried: an unexplained profile
override is a magic value, and the failure it guards against is a build error
that CI catches on the spot, not a silent runtime bug.

`Cargo.lock` is committed, which is what keeps `gpui-pre-macros` at 0.3.4. If a
release build ever fails inside that crate after a bump, restoring the override
at the **workspace root** — not in the vendored manifest — is the fix. The
comment in `Cargo.toml` says so.

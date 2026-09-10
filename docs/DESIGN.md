# DESIGN.md — authority on visual usage and interface states

> Authority order (SPEC, preamble): **DESIGN-TOKENS.md** on token names and
> formulas · **this document** on visual usage and interface states ·
> **SPEC.md** on everything else.

The design was produced as a Claude Design canvas. The nine boards under
`docs/design/` are the deliverable itself and stay the reference for anything
this document does not spell out; open them in a browser.

| Board | Covers |
|---|---|
| `01 Tokens & Components.dc.html` | the semantic tokens, the two absolute rules, type and spacing scales, twelve atomic components in five states, hierarchy without hue |
| `02 Topbars.dc.html` | Linux / macOS / Windows topbar variants, common states, tab order |
| `03 Working Copy.dc.html` | the Working Copy screen at 1600×1000, and its 1280×800 collapse |
| `04 Palettes.dc.html` | the same markup on four palettes, with measured contrast |
| `05 History.dc.html` | the History screen, the eight graph lanes, commit detail, A↔B comparison |
| `06 Repositories.dc.html` | the Repositories screen, repository card, empty / missing / cloning states |
| `07 Palette et Dialogs.dc.html` | command palette, branch / clone / delete dialogs, progress overlay, conflict resolution |
| `08 Densites.dc.html` | compact vs comfortable, and what density does *not* change |
| `09 Focus clavier.dc.html` | tab stops per screen, focus vs selection, zone shortcuts |

Every board carries a live theme switcher (Tokyo Night, Gruvbox, Matte Black,
light) and a density switcher; board 01 adds a "test without colour" toggle and
board 02 a dimension overlay.

---

## 1. The rules the boards exist to prove

**Hierarchy never rests on hue.** Three value levels (`bg`, `surface`,
`surface_raised`), 1px borders, density and weight carry the structure. Board 01
has a greyscale toggle: everything must stay readable through it. Additions and
deletions are separated by the `+` / `−` sign, the gutter and the position — not
only by colour.

**Two tokens may collapse.** On Matte Black, `accent` and `border` come close.
Selection stays legible because it changes *surface* and carries a ring, not
because its hue differs.

**Square, always.** No radii anywhere. Borders are 1px and never scale with
density. One shadow is tolerated in the whole app: under a modal overlay
(`0 2px 16px -8px`).

**Selected ≠ focused.** A selected row takes `surface_raised`; a focused row
adds a 1px `border_focus` ring — inset on full-width rows, offset by 2px on
buttons. Never a drop shadow, never a size change. When focus leaves a zone the
ring goes and the selection stays.

## 2. Chrome

Topbar 48px comfortable / 40px compact; statusbar 24px / 22px. These are the
only two bars density affects.

Content and order never change between platforms — **only the edge reserves do**,
and a reserve is a flex spacer, never a conditional padding.

| Platform | Leading | Trailing |
|---|---|---|
| Linux, tiled (Hyprland) | 0 | 0 |
| Linux, floating / classic desktop | 0 | 115 (3 × 38 + 1px separator) |
| macOS | **78 — forbidden zone, traffic lights** | 0 |

Windows is out of scope (SPEC §2). Board 02's Windows variant is documentation,
not a target.

> **Amendment (2026-09-09).** With the move to Tauri, Windows is no longer
> excluded by construction — the runtime runs there. It is still not supported,
> and board 02's Windows variant is still documentation: supporting it means the
> caption buttons, the 138px reserve, the `HTMAXBUTTON` region for Snap Layouts,
> packaging and testing. That is a milestone nobody has decided on (SPEC §2's
> amendment).

Linux caption buttons, when drawn, are the app's own: 16px linear glyphs at
1.5px stroke, 38×48 each, square. Minimise and maximise hover to
`surface_raised`, close to full `danger`. Order and side follow
`gtk-decoration-layout`.

Collapse order below 1100px: action labels → Fetch/Pull into a `⋯` menu →
search shrinks to its icon. **Push stays visible**, being the most frequent
action.

Inactive window: everything drops one step — text to `text_dim`, borders to 55%,
accent gone. Never a global opacity.

Shortcuts show one modifier, never both: `⌘K` on macOS, `Ctrl K` elsewhere.

## 3. Typography

Two tracks, per DESIGN-TOKENS §8. Sizes: 16/600 dialog title, 15/500 panel
title, 13/500 emphasised label, 13/400 interface body, 11/500 section header,
11/400 metadata. Mono runs at 12.5px. Weights 400/500/600 only.

Spacing is a 4px base: 4, 8, 12, 16, 24, 32, 48.

## 4. Screens

**Repositories** — a repository sidebar with collapsible, drag-reorderable
groups and a detail card. A dragged row goes to 60% opacity with a 2px accent
insertion rule. A repository is a bound rectangle, a group a folder; a missing
repository keeps its icon struck through and is **never silently removed**. The
card is a reading surface, not a form: only User Description and Committer
Identity are editable, in place, with a `border_focus` ring and no dialog.
Activity sparkline over 90 days, one bar per three days, last three in accent.

**Working Copy** — sidebar, commit column, diff panel. Subject counter appears
past 50 characters in `text_dim`, turns `warning` past 72; the 72-column rule is
the field's right edge, never a floating marker mid-text. Hunk buttons show at
35% opacity at rest and never disappear entirely, so they stay keyboard
reachable. Diff backgrounds at 12%, intra-line at 24%, syntax highlighting over
them using only existing tokens: keywords `info`, types `warning`, functions
`accent`, strings `success`, comments `text_dim`.

**History** — graph column and detail panel. Nodes: 6px filled disc for an
ordinary commit, hollow circle filled with the background for a merge; 1.5px
stroke, quarter-circle branches, never a soft curve. Two selected commits carry
`A` and `B` badges in inverted value, not in colour. Below 1280px the gutter
goes from 100px to 60px and lanes past the fourth fold into a `+2` indicator
rather than eating into the commit message.

Under 1100px of usable width, the centre column and the detail panel become two
tabs — on every screen.

## 5. Keyboard

One zone is one tab stop. Tab enters a zone on its current element; movement
inside is arrows or vim letters. Working Copy has 11 stops, History 7,
Repositories 6. `1` `2` `3` jump to the sidebar, centre column and detail panel
with the same meaning on all three screens. `Esc` goes up one level: overlay →
zone → column 1. After the last stop, Tab returns to 1 — focus never escapes
into the window decoration. Window buttons and macOS traffic lights are never in
the tab order.

## 6. Divergences resolved

Three places where the boards and `DESIGN-TOKENS.md` disagree. The tokens
document wins (SPEC preamble); recorded here so nobody re-derives them from the
boards.

1. **Mix direction.** Board 01's table reads `text_muted = mix(fg, bg, 35% dark
   · 10% light)`, the complement of DESIGN-TOKENS §2.3's 65% / 90%. The boards'
   own CSS says `color-mix(in srgb, var(--fg) 65%, var(--bg))`, which agrees
   with the tokens document. The table's phrasing is the stale one; `mix(A, B,
   p)` is **p% of A**.
2. **Thirteen tokens or fourteen.** Board 01 titles "the thirteen semantic
   tokens" and omits `surface_hover`. DESIGN-TOKENS §4.1 adds it, and is right:
   without it, hovering a row would reuse `surface_raised`, which means
   selected. **Fourteen.**
3. **16% or 18%.** The boards use `danger 16%` for badges and `danger 18%` for
   square chips. Both are `tint_medium` = 16% (DESIGN-TOKENS §5). A real
   badge/chip distinction goes through border or size, not two points of
   opacity.

One more, from the boards' own copy: the mock-ups list `'Segoe UI Variable
Text'` in the UI stack. Windows is out of scope, so it is dropped in code.

### Four more, found on board 06 while building M3

Same rule, one level up: where **this document** states something and a board
contradicts it, this document wins — it is the authority on interface states
(SPEC preamble).

1. **A selected row takes `surface_raised`, not `accent`.** §1 states the rule
   and gives the reason — "Selection stays legible because it changes *surface*
   and carries a ring, not because its hue differs" — and board 06 paints the
   selected repository row in `accent` with `bg` text. The rule wins. It matters
   most on Matte Black, where §1 warns that `accent` and `border` come close: a
   selection that reads as a hue has nothing left to say there. Following the
   rule also keeps the row's status colour, which the accent fill overwrote.
2. **A repository row always uses the repository glyph.** The board's own copy
   says "Le dépôt est un rectangle à reliure, le groupe un dossier", and then
   draws the rows under *Travail* with the folder. The copy wins: shape is
   carrying meaning here, so it cannot mean two things.
3. **Rows are indented under their group header, uniformly.** The board indents
   the rows of two groups out of three. Indentation is structure, and §1 puts
   structure on value, borders and position — so it applies everywhere or
   nowhere. Everywhere.
4. **The 0.08em tracking on 11px uppercase labels is dropped.** The renderer has
   no letter-spacing. Faking it by inserting spaces between characters would
   break selection and copy, which is a worse trade than a label set slightly
   tighter than the mock-up.

## 7. Not yet designed

Called out by the boards themselves, so they are not mistaken for oversights:
side-by-side diff, drag line-selection in the gutter, visible whitespace,
binary / image / rename / oversized-file diffs, the Preferences screen, and the
`?` shortcut sheet.

## 8. Contract points resolved during implementation

Recorded here because they changed how the tokens are computed, and the next
person to read §4.3 will otherwise wonder why the code does not match it.

**A failed operation has a band, not a modal.** Nothing in the boards draws one:
the status bar was carrying `git`'s refusals, which run to a paragraph, in
22 pixels. The band sits above the status bar, wraps, and holds `git`'s words
verbatim; the modal shape stays reserved for the confirmations of SPEC §3
rule 7, so that it goes on meaning "answer this before something is lost".
ARCHITECTURE §2.40 has the argument.

**The correction target is `surface`, not `bg`.** DESIGN-TOKENS §4.3 says
`bg`. Text is also drawn on `surface`, which is 4% closer to the foreground and
therefore always the harder background; correcting against `bg` alone leaves the
`surface` pairs at 4.1–4.4:1 on Gruvbox, Nord, Everforest and both light themes,
which §10 test 1 rejects. Targeting `surface` satisfies both and is never weaker.

**`text_dim` is held to 3:1, not 4.5:1.** It is the disabled/placeholder token
(board 01), and WCAG 1.4.3 exempts inactive components. Measured on the
delivered palettes it sits at ~3.1:1 by construction — the per-mode coefficients
of §2.3 are what carry `text_muted` over 4.5:1, which is exactly the mechanism
board 01 describes. Forcing `text_dim` to the same floor would push it past
`text_muted` and invert the hierarchy, so it gets the non-text floor plus an
ordering invariant that keeps it below `text_muted` on every theme.

**"Minimal at 5 keys" means five colours plus `mode`.** §10 test 2 says a
palette "minimal at 5 keys" must produce a valid result, while §2.1 guarantees
six entries. The six are five colours and `mode`, so the minimal valid file is
those six; the fallback test covers it, and a file missing `mode` falls back
whole like any other incomplete palette.

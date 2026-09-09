# ARCHITECTURE.md

Maintained as the project goes (SPEC §7). It records the decisions that are
expensive to reverse and the risks that have to be re-read at each milestone.

**Status: M4 — the Working Copy, in read.** What exists is listed under "What is
built"; everything else here is the shape later milestones fill in, not code
that is present.

---

## 1. Workspace

```
omagit/
├── crates/
│   ├── omagit-git/        Git core — no UI dependency
│   ├── omagit-git-cli/    `omagit-git-cli`: the Git core, without a window
│   ├── omagit-theme/      DESIGN-TOKENS.md, implemented — no UI dependency
│   ├── omagit-settings/   TOML preferences
│   ├── omagit-ui/         palette, icons, the atoms of board 01
│   └── omagit-app/        binary `omagit`: windows, screens, state, platform layer
├── assets/icons/          the SVG glyphs, compiled into the binary
├── vendor/gpui-omarchy/   vendored design system — see vendor/README.md
├── scripts/               check.sh (the milestone gate), sync-vendor.sh
└── docs/                  SPEC · DESIGN-TOKENS · DESIGN · this file · notes/
```

Dependency direction, strictly one-way:

```
omagit-app ──▶ omagit-ui ──▶ omagit-theme
     │              │
     │              └──▶ gpui-omarchy ──▶ gpui-kit
     ├──▶ omagit-git ──▶ gix        (no UI dependency, ever)
     └──▶ omagit-settings ──▶ omagit-theme

omagit-git-cli ──▶ omagit-git
```

`omagit-git` and `omagit-theme` compile and test with no window and no
renderer. That is what makes them testable, and it is a rule, not a
coincidence (SPEC §3 rules 5 and 6). `omagit-git-cli` is that rule made
executable: a binary that drives every read from a terminal cannot compile if
the Git core has grown a UI dependency.

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

### 2.2 Contrast correction targets `surface`, not `bg`

DESIGN-TOKENS §4.3 words the correction target as `bg`. Implemented literally,
that leaves the `surface` pairs at 4.1–4.4:1 on four of the eight embedded
themes, which §10 test 1 rejects — because text is drawn on `surface` too, and
`surface = mix(fg, bg, 4%)` is always the harder of the two backgrounds.

So the correction targets `surface`. It is a strictly stronger guarantee: any
colour legible on `surface` is legible on `bg`. Measured before deciding; the
failing pairs are in the commit that introduced the tests.

`text_dim` is the one token held to a lower floor — 3:1, WCAG 1.4.11 — and the
reasoning is in `omagit_theme::theme::DIM_MIN_CONTRAST`. In short: it is the
disabled/placeholder token, WCAG 1.4.3 exempts inactive components, and forcing
it to 4.5:1 would push it past `text_muted`, inverting the hierarchy the two
tokens exist to express. An ordering invariant enforces that it never does.

### 2.3 Typography stacks are resolved, not requested

DESIGN-TOKENS §8 specifies stacks the way CSS does. The renderer's
`font_family` takes one family and does no walking, so `omagit-ui::fonts`
resolves each stack against the installed families once at start-up.

`system-ui` and `-apple-system` are generics the font list never enumerates:
the platform supplies its own name through `Platform::system_ui_family`
(`.SystemUIFont` on macOS, `sans-serif` on Linux). Resolving it inside
`omagit-ui` would need a `cfg(target_os)` in the UI layer, which SPEC §3 rule 6
forbids — hence the trait method.

### 2.4 Edge reserves are flex spacers

The macOS 78px traffic-light band is laid out as a spacer, never as a
conditional padding (DESIGN-TOKENS §9). Padding would move everything else when
the platform changes; a spacer does not. The Linux caption reserve (115px, three
38px buttons plus a separator) works the same way and arrives with the topbar.

### 2.5 Git stays off the render thread, and it is asserted

`omagit_git::thread_guard` registers the render thread at start-up. Every entry
point into the Git core opens with `assert_off_render_thread`, which panics in
debug and costs one relaxed atomic load in release. SPEC §15 names a blocked UI
thread as the most likely failure mode; a panic pointing at the offending call
site is much easier to diagnose than a frozen window.

### 2.6 Hybrid Git backend — built, and without a trait

`gix` reads, the `git` binary writes and talks to the network (SPEC §8). Three
reasons, all about not corrupting data: merge and rebase semantics are subtle
(hooks, strategies, `rerere`, user config); the user's hooks must run, or team
workflows break; and existing credential helpers then work with nothing
reimplemented.

**No `GitBackend` trait.** SPEC §8 describes one with `GixBackend` and
`CliBackend` behind it, so an operation can move between them as `gix` matures.
Today no operation has two implementations — reads are `gix`, writes are the
CLI — so the trait would have exactly one implementor per method, which SPEC §2
and §3 call debt rather than preparation. The property the trait was for is
already held another way: nothing outside `omagit-git` names a backend, so
moving an operation is a change inside this crate and nowhere else. The trait
becomes worth writing the day an operation genuinely has two implementations.

**Every write is journalled, before it runs.** `cli::Invocation::run` opens a
journal entry before the process is spawned and closes it when it ends, so a
command that never returns leaves a record of having started — which is what
SPEC §15 risk 5 asks for, and what is worth reading after a crash. Because the
hook is in the one place a `git` process is created, no call site can forget it.
Commands that can lose work are marked `destructive()` and logged at `warn`.

### 2.7 Lane colours never read the theme

Eight evenly spaced hues, `oklch(lane_lightness, lane_chroma, 20° + i × 45°)`.
A lane's colour is arbitrary and carries no meaning; it only has to differ from
its neighbour, and no theme palette can guarantee eight separable hues — Matte
Black has zero. Only `lane_lightness` and `lane_chroma` vary by theme, so lanes
stay readable against the background.

Two tests hold it: neighbouring lanes stay a minimum perceptual distance apart
in Oklab, and every lane clears 3:1 against its theme's background — the half
that actually depends on the background, and the reason lane lightness is a
per-theme field. Out-of-gamut requests give up **chroma**, never hue: clamping
channels would shift hues and could bring two lanes closer than 45° apart.

### 2.8 Theme sources are persisted, tracking is not

What is stored is the *source* (`ThemeSource`), never the resolved palette: a
stored palette goes stale the moment the system theme changes. `Sources::resolve`
turns a source plus the machine's facts into a `Resolved`, whose `source` field
is the **effective** one — `Automatic` is substituted before resolving, never
reported back, or the caller would conclude there is nothing to follow on
exactly the machines where there is.

A source that cannot work here is not offered rather than offered and broken: no
Omarchy entry on macOS, and none on a Linux box whose Quattro state is missing or
invalid. When a requested source cannot be honoured, `fell_back_from` records it
and the UI says so instead of quietly showing a different theme.

Live tracking runs off the render thread by construction. Omarchy is watched on a
dedicated thread that owns the `notify` watcher and pushes real changes through
an `async-channel` to a foreground task — `AsyncApp` is not `Send`, which is what
forces the split. Every event goes through `omarchy::Tracker`, which compares
*parsed palettes*: filesystem watchers fire several times for one logical change
and report differently on inotify and FSEvents, and the tracker makes that
irrelevant. A read failure is deliberately not a change, so a `colors.toml`
caught mid-save does not bounce the app to the fallback theme and back.

### 2.9 The Omarchy key is `muted`, and the documents said `color8`

Found at M3 by running the app on an actual Omarchy machine, which is the first
time anyone had. SPEC §6.2 and DESIGN-TOKENS §2.1 both name the fifth guaranteed
key `color8`; **no Omarchy theme has ever had one**. All 23 themes installed
spell it `muted`, and the values are the same thing under a different name:
Omarchy's `tokyo-night` has `muted = "#414868"`, and `embedded::TOKYO_NIGHT`
carries `bright_black: 0x414868`.

The consequence was total rather than partial, because §2.1's own rule made it
so: a missing guaranteed key rejects the *whole* palette. So every Omarchy
palette was rejected, and the source SPEC §6.1 makes the default on Linux fell
back to an embedded theme on every launch. The app worked, looked deliberate,
and ignored the system theme.

**How it survived a milestone whose subject was themes.** M1 was written and
verified on macOS, where the state directory never exists, so the reader was
only ever exercised against fixtures — and the fixtures were written from the
prose above. A fixture written from a document can only ever confirm the
document. `tests/fixtures/omarchy/` now holds three real `colors.toml` files
copied verbatim from `/usr/share/omarchy/themes/`, and
`omagit-theme/tests/omarchy_palettes.rs` reads them; one of its assertions is
the equality with the embedded palette above, which is a fact rather than an
opinion.

`color8` is deliberately **not** accepted as an alias. A fallback for a spelling
that exists in no file is code with no caller (SPEC §2), and keeping it would
preserve the illusion that the documents were right.

### 2.10 The history walk is ours, and it is resumable

`gix` has a revision walk, and it is a good one, but it borrows the repository
for the lifetime of the iterator — so a walk cannot outlive the background job
that created it, and the History screen needs exactly that: a walk parked
between pages while the user scrolls.

`omagit_git::history::Walk` owns its traversal instead: a priority queue of
commits whose parents have not been visited, ordered by commit time, and a page
is *n* pops. Pages cost nothing to resume, the frontier stays small, and the
order is `git log --date-order` — compared against `git log` itself in
`tests/history.rs` rather than against an expectation written by hand.

Two consequences worth holding. The `seen` set makes memory grow with the
commits *visited*, not with the frontier — a few megabytes over 100 000 commits,
and the price of not walking a merge-heavy history exponentially. And ties are
broken by object id, so two commits made in the same second always come out in
the same order; without it the same repository would render two different
histories on two runs.

M6's lane computation reads this same order, which is the only way a graph is
guaranteed to line up with the rows beside it.

### 2.11 Paths are bytes, and nothing here normalises them

Git paths are byte strings; a file committed under a Latin-1 name is a valid Git
path no `String` can hold. `omagit_git::RepoPath` carries bytes end to end and
becomes an OS path in exactly one function, which refuses anything that would
leave the work tree — an absolute path or a `..` in a corrupt index must not let
a client write outside the repository it was told to open.

SPEC §9 asks for this to be centralised for a macOS reason, and the measurement
at M2 changed what the module does about it: **`gix` already honours
`core.precomposeUnicode`**, so the names arriving here are in the form the index
holds. Normalising again is how a name stops matching its own entry, so
`paths.rs` deliberately does not. Case is left alone for the same reason:
`Readme.md` and `README.md` are two distinct Git paths, and reconciling them on
a case-insensitive volume is `core.ignorecase`'s job.

### 2.12 A diff is computed per file, and refined only where it helps

Three levels — which files, which lines, which words — each the input of the
next, and all computed on demand: a commit touching 900 files must render its
first file immediately (SPEC §12), and a diff view only ever shows one.

The line diff is `gix`'s re-export of `imara-diff`, with Git's slider
heuristics, so hunks land on the same lines the command line picks. A separate
`imara-diff` dependency was deliberately not added: two copies of a diff engine
eventually disagree, and the `@@` headers are compared against `git diff` in
`tests/diff.rs` precisely so that agreement stays checkable.

The intra-line refinement of DESIGN §4 pairs removed lines with added ones by
position and marks the words that differ — but **gives up when the two lines
share almost nothing**. Marking nine words in ten tells the reader less than
marking none, and turns the 24% intra-line surface into a solid block.

Two degradations rather than a freeze, both from SPEC §11: past 2 MiB a file is
reported as oversized instead of diffed, and a NUL byte in the first 8 000 makes
it binary — Git's own test, kept identical so omagit and the command line never
disagree about which files they refuse to show.

### 2.13 State lives in a store, view state lives in the view

SPEC §10 draws the line and M3 is where it first has to hold. `Store` owns the
persisted library and an `AsyncState<Summary>` per repository; the screen owns
the selection, the filter text, the focus and the drag. Nothing is duplicated
across the line, so there is no pair of values that can disagree.

Three consequences worth stating, because each is a bug that did not happen:

**Every read carries a generation, and an older one is dropped.** Clicking
through four repositories starts four reads that finish out of order; without
the check, the card shows whichever finished last. `AsyncState::finish` refuses
a result that a newer request has already superseded.

**`Loading` carries the value it is replacing.** A refresh redraws the old
figures rather than blanking the panel, which is the difference between "still
working" and "gone". Only a *first* load has nothing to draw, and that is a
distinct state (`is_blank`).

**A cancelled read is not a failure.** The user moved on; something else is
already loading, and an error state would report their own action back to them.

The filesystem watcher of SPEC §10 is not here yet: nothing holds a repository
open long enough to need invalidating. It arrives at M4 (see risk 8).

### 2.14 The repository list is set aside, never overwritten

`Settings` falls back to defaults when its file is malformed, which is right for
preferences — losing a theme choice costs nothing. The library is different: it
is a list the user arranged by hand, and defaulting to empty would mean the next
write silently replaces it. So a `repositories.toml` that will not parse is
**renamed to `repositories.toml.damaged`** and the app starts empty. No dialog,
no new UI state, and nothing lost.

The library holds only what the user put there — paths, names, notes, order. The
branch, the ahead/behind and the status counts are read from the repository
every time, because a cached branch name is wrong the moment someone checks out
another one in a terminal.

### 2.15 The watcher reports *what* changed, and is told to stop

SPEC §10 asks for a filesystem watcher that debounces at 150 ms, ignores
`.git/index.lock`, and invalidates in a targeted way. `omagit_git::watch` does
all three, and the third is the one that matters: a `git push` in a terminal
writes under `.git/refs/` and nothing else, so it must not cost a working-copy
scan — and a `cargo build` writing into `target/` must cost nothing at all.
Exclusion is asked of the repository rather than pattern-matched, so it is the
same answer `git status` gives, nested `.gitignore` files included.

Two things it got wrong first, both worth keeping written down:

**Shutdown cannot be inferred from the event channel closing.** Dropping the
`notify` watcher does not reliably drop the handler that holds the sender, so
the receive never returns and the join waits forever — a hang, not a slow
shutdown. Shutdown is now an explicit flag the loop reads on each wakeup.

**A watcher test has to start from silence.** `git commit` leaves `gc --auto`
running behind it, which touches `.git` a second later; the first tests were
asserting about the previous command. They settle the repository before acting.

The limitation to know: the watch is recursive, so the platform still spends a
descriptor per directory — on Linux, one inotify watch each, against
`fs.inotify.max_user_watches`. Walking the tree and watching only what Git cares
about would fix it. `Watcher::start` *reports* that failure rather than
swallowing it, and the screen says live updates are off, because a client that
has quietly stopped noticing changes is indistinguishable from a broken one.

### 2.16 Four things layer on one diff line, in one order

DESIGN §4 specifies the diff viewer unusually precisely, and the composition is
the design: the line background at 12%, the intra-line refinement at 24% over
it, the syntax colour under both, and the sign and gutter carrying the meaning
when none of them do. The last is not decoration — DESIGN §1's greyscale check
has to pass, so `+`, `−` and the two number columns are load-bearing.

**Unified and side-by-side are one renderer, not two.** The hunks are flattened
into rows and the mode only changes how the rows are built, which is what keeps
a fix to the refinement or the gutter from landing in one view and not the
other.

**Tabs are expanded here, with their offsets.** Leaving them to the text system
loses the gutter alignment that a diff is read by; expanding them without moving
the spans would colour the wrong characters on every indented line. So
`expand_tabs` returns the text *and* a map, and every span is translated
through it.

The rows are virtualised and the syntax query runs over the visible range only
(SPEC §12), so scrolling a 40 000-line diff costs what scrolling a short one
does.

### 2.17 A diff carries its two sides

`DiffContent::Text` keeps the whole of both files, not just the hunks. Two
things need it and neither can be done from hunks alone: a syntax parser handed
the twelve lines of a hunk produces nonsense, because a fragment is not a
program; and unfolding the context between two hunks needs the lines that were
left out. It is bounded by `DiffOptions::max_bytes` and only ever held for the
file on screen.

### 2.18 Syntax highlighting has five colours and its own grammars

DESIGN §4 allows exactly five: keywords `info`, types `warning`, functions
`accent`, strings `success`, comments `text_dim`. There is no syntax palette and
nothing may introduce one — a diff that invents six colours stops obeying the
theme, and on Matte Black it stops being readable. So the mapping from a
grammar's capture names is coarse on purpose: `function.method`,
`function.macro` and `function` are all *functions*.

The grammars are omagit's own, and SPEC §4 said they would not have to be: its
note reads "GPUI l'embarque déjà". `gpui-kit` does offer tree-sitter — behind
`gpui-component`, which this workspace does not enable, because doing so pulls
in a second component library competing with the vendored one. The SPEC is
amended in place; the practical consequence is that each language is a C
compile, so there is one grammar per language actually rendered — Rust, TOML,
JSON, Markdown — and adding more is a deliberate act rather than a default.

A file with no grammar is drawn plain, and that is a supported outcome rather
than a gap: DESIGN §1's first rule is that hierarchy never rests on hue.

### 2.19 The vendored design system is not linted

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

## 4. What is built

**M0 — skeleton.** The workspace, the pinned toolchain, `scripts/check.sh` as
the gate, `gpui-omarchy` 0.1.1 vendored with its divergences recorded and a sync
script, the `Platform` trait with Linux and macOS implementations, a decorated
window honouring the platform's edge reserves, rotating file logs, the
render-thread guard, and CI on both platforms.

**M1 — theme.** `DESIGN-TOKENS.md` implemented in full:

- The token vocabulary under canonical names, the sRGB `mix` derivation of §4.1
  and §5, the tint scale, density and typography.
- OKLCH derivation of unnamed status colours (§4.2) and the mandatory contrast
  correction (§4.3), applied to read colours as much as derived ones.
- The lane generator (§6), with gamut mapping that gives up chroma, not hue.
- The catalogue: six dark themes and two light, all passing the six tests.
- Omarchy Quattro reading and live tracking, legacy layouts deliberately absent.
- macOS system-appearance tracking, including the automatic switch.
- The six tests of §10, plus the fallback, ordering and lane-distance tests they
  imply.

**M2 — the Git core, reads.** `omagit-git`, with no UI dependency and a debug
binary that proves it:

- Opening and discovery, telling a repository that moved from a directory that
  never was one; `HEAD` as branch, detached or unborn; and the operation the
  repository is in the middle of (merge, rebase, cherry-pick, revert, bisect,
  `am`).
- The working copy in one entry per path, both sides of the index at once:
  modified, added, deleted, renamed, type-changed, untracked, ignored,
  conflicted, submodule — with rename detection on both halves and `--ignored`
  behaving as `git status` does.
- Branches, remote branches, remotes and tags, with ahead/behind against the
  upstream and an explicit "gone" when the upstream no longer exists.
- A resumable, paged history walk (§2.9), matched against `git log --date-order`
  and `--first-parent`.
- Diffs of a commit, between two commits, of the index and of the working tree,
  with hunks, context, intra-line refinement, and the binary / oversized /
  submodule / mode-only cases (§2.11).
- `cli.rs`: the SPEC §8 subprocess rules in full. M2 calls it once — the
  start-up check that a usable `git` ≥ 2.35 exists, which now goes through the
  same runner as everything else so the first `git` omagit ever runs already has
  a deadline and a scrubbed environment. The rest of the contract is verified
  rather than assumed (`tests/git_cli.rs`), including the one that is easy to
  implement wrongly: cancelling reaches the *process group*, so a shell `git`
  spawned dies with it.
- `Cancel`: one token that `gix`'s worker threads, the history walk and a `git`
  subprocess all poll, so a read stops when the user moves on.
- 103 tests — one of them `#[ignore]`d because it writes 50 000 files — of
  which the integration ones build real repositories with the real `git` and
  compare against what it says — including the edge cases SPEC §13
  names by name: no commits, detached `HEAD`, bare, merge and rebase in
  progress, fifty roots, a one-megabyte line, non-UTF-8 and accented names, and
  a rename that changes only case.

**M3 — the Repositories screen.** The first screen, navigable end to end from
the keyboard:

- A sidebar of repositories in collapsible, drag-reorderable groups, filtered by
  name or path, with a row that says what each repository is doing — the branch,
  the divergence, the working-copy state, or the operation that is half-finished
  — and a square status pip whose *shape* distinguishes a detached `HEAD`.
- A card reading out location, last opened, last commit, the committer identity
  and whether it is inherited, the branch and its upstream, the status by kind,
  the stashes, the remotes with a copy button, and a 90-day activity sparkline.
- Adding a repository from the folder picker or by dropping a folder into the
  window; removing one from the list; relocating one that has moved; a
  description edited in place, with no dialog and no edit button.
- The six tab stops of DESIGN §5 in a fixed order, `1`/`2`/`3` zone jumps, `j`/`k`
  and the arrows inside the list, `/` to filter, and `Esc` one level up — all as
  named actions, so M9's reassignment and the macOS menu bar are a settings
  screen rather than a rewrite (`docs/KEYMAP.md`).
- `omagit_git::Summary`, which is that whole read-out in one background read
  rather than six per row.

Deliberately **not** built: cloning (M7, with the network and the progress
overlay board 06 shows) and the command palette (M9). The topbar draws *Cloner…* and *Rechercher* in board 01's
disabled state rather than hiding them: board 02 fixes the topbar's content, and
an action that will exist reads better as not-yet than as absent.

**M4 — the Working Copy, in read.** The second screen, and the diff viewer
board 03 calls "la pièce maîtresse":

- A file list split into what is staged and what is not, with `git status`'s own
  letters, conflicts marked, and untracked files as ordinary rows.
- The diff viewer: unified **and** side-by-side from one row model, virtualised,
  with the four layers of §2.16 — 12% line, 24% words, syntax, sign and gutter —
  and the binary, oversized, submodule and mode-only cases drawn as themselves.
- Syntax highlighting in the five roles DESIGN §4 allows, over the visible range
  only, for Rust, TOML, JSON and Markdown (§2.18).
- A filesystem watcher that says *what* changed, so the screen refreshes itself
  when the repository moves underneath and costs nothing when a build does
  (§2.15).
- `RepoStore`: the `Entity<RepoStore>` of SPEC §10 with all its parts — status,
  summary and a per-file diff, each an `AsyncState` with a generation, each read
  on the background executor.
- Routing between the two screens, so "Ouvrir" now opens something.

Deliberately **not** built, because M5 is the write half: staging by file, hunk
or line, the commit-message editor, discard, and the checkboxes board 03 draws
on every row. The space above the file list is left for them rather than filled
with controls that would do nothing — an inert checkbox is a promise the screen
cannot keep. The sidebar's branch tree is M7's and the stash list is M8's; both
say which milestone they are waiting for instead of being drawn empty.

Also deliberately not built: the `GitBackend` trait, which has no second
implementation until M5 (§2.6); the commit-graph lanes, which are M6's and need
the walk that now exists; the filesystem watcher, which has nothing to
invalidate until a screen reads a repository (M4); and the preferences UI (M9).

## 5. Risks

Re-read at every milestone (SPEC §15).

| # | Risk | State at M4 |
|---|---|---|
| 1 | **The UI thread blocks.** The most likely failure mode. | **First real exercise, and it holds.** Every public entry point in `omagit-git` opens with `assert_off_render_thread`; the store runs every read through `cx.background_spawn` and lands it with `this.update`. A status, a reference read and a ninety-day walk per repository, on every launch, and the guard has not fired. It stays only as good as the next entry point somebody adds: a review item, permanently. |
| 2 | **The `gix` / CLI split lands wrong.** | **Answered.** `gix` covers every M2 read, with the measurements in `docs/notes/gitoxide-capabilities.md`; nothing fell back, and the trait is deferred to M5 rather than built empty (§2.6). The rule is unchanged for the milestones that follow: when `gix` does not cover a case, move it to the CLI and write down why — never work around it. |
| 3 | **`gpui-omarchy` is incomplete.** | Confirmed, and now measured against real use: M4 wrote the diff viewer from scratch, as expected, but `virtual_list` underneath it works and is what makes a 40 000-line diff cost what a short one does. Still to confirm at 100 000 rows (M6). The theme vocabulary mismatch stands, and M1's replacement of it stands with it. |
| 4 | **The commit graph.** The hardest algorithm here. | Not started; its input exists and is pinned down: the walk of §2.10 is total, reproducible and matched against `git log`. Lanes stay in `omagit-git/graph.rs` at M6, computing topology only, never colour, and never coupled to rendering. |
| 5 | **Data loss.** | Still no mutating Git operation. M3 added the first thing omagit *writes*, though — the repository list — and it is handled as data rather than as a cache: a malformed file is set aside, not overwritten (§2.13), and "Retirer de la liste" removes an entry while touching nothing on disk. `cli::Invocation` remains loggable exactly as it will run, before it runs. |
| 6 | **macOS distribution cost.** | Unchanged and recurring: Apple developer account, signing, notarisation, a macOS CI runner. Budget it now, not at M10. |
| 7 | **Nothing compiles the other platform.** | **Accepted, deliberately, after M4.** The CI workflow is deleted: on a private repository with one developer it verified work nobody else had written, on every push, with a macOS runner billed ten times the Linux rate — the first pull request's run alone cost about 113 minutes. What it genuinely covered is now uncovered, and that is the price: `scripts/check.sh` compiles only the host's half of `omagit-app/src/platform/`, and the macOS half of `status` — APFS case folding, decomposed names — is exercised nowhere. Both blind spots have already produced real bugs (§5, ninth and tenth entries), so this is a known cost, not an oversight: the project is developed on both machines, and each will find the other's breakage the next time it is used. The workflow's last state is commit `08193f5`, to restore unchanged if a second developer joins. |

### Known defects, open

**The three-column Working Copy does not fit below ~1100px.** The sidebar and
the file column are fixed widths and the diff panel has a floor, so under that
the diff header's right-hand content falls outside the window and is clipped.
DESIGN §4's answer is that below 1100px of usable width the detail panel becomes
a tab; that collapse is not built. What *was* a bug — the header's controls
drawn on top of the file path — is fixed: the path could not shrink and had no
clip, so its text spilled over them. It now keeps a floor and the stats clip
first (`tests/working_copy.rs`).

A seventh, found while building M0 and worth watching: `block 0.1.6`, deep under
`gpui-pre-apple`, emits a future-incompatibility warning. Not actionable from
here; re-check at each `gpui-kit` bump.

An eighth, from M1, now **closed**: the Omarchy watcher had only ever run on
FSEvents. Its integration test — a real symlink replacement, delivered exactly
once — passes on inotify as well, verified on the first green Linux CI run
(2026-09-08). The same question is still owed for the *repository* watcher,
which watches different paths for different events; it moves to **M4**, because
M2 deliberately built no watcher — there is nothing to invalidate until a screen
holds a repository open (SPEC §2: no code without a caller).

A ninth, from that same run: **local `scripts/check.sh` only compiles this
host's half of `omagit-app/src/platform/`.** The first CI run failed on an
unused import in `platform/linux.rs` that macOS never compiles. The script and
the README now say so; the two-platform matrix is the only thing that answers
for the other half, and a green local run is not a green milestone.

A thirteenth, from M4: **a recursive watch spends a descriptor per directory.**
On a repository with a `node_modules` or a deep `target`, that is thousands of
inotify watches against `fs.inotify.max_user_watches`, and the fix — walking the
tree and watching only what Git cares about — is real work that has not been
done. What *has* been done is making the failure visible: the watcher reports it
and the statusbar says live updates are off, rather than the screen quietly
going stale (§2.15).

An eleventh, from M3, and it earned its place by paying for itself immediately:
**the mouse has no interaction tests.** The keyboard now does —
`crates/omagit-app/tests/keyboard.rs` drives real keystrokes through the real
widget tree on GPUI's test platform — and writing it found two bugs that would
otherwise have shipped in the milestone whose headline is "navigable from the
keyboard":

1. **The screen never took focus.** It rendered correctly and answered no key at
   all: GPUI dispatches actions along the focus path, and a window that has
   focused nothing has no path.
2. **Bare letter and digit keys fired while the user was typing.** `/` then `j`
   put a `j` in the filter *and* moved the selection. The single-character
   bindings are now scoped `Repositories && !Input`.

Neither is visible in a screenshot, and neither would have been caught by
reading the code. M4 extended the same treatment to the Working Copy
(`tests/working_copy.rs`, against a real repository), and the Working Copy
screen was written with the focus bug already in mind — which is what a recorded
risk is for. Dragging a row between groups is still verified only by hand, and so
are the folder picker and the diff's mode switch.

A twelfth, from M3, and it is the ninth and tenth risks collecting: **M1's
Omarchy support never worked on Omarchy.** The reader required a key
(`color8`) that no Omarchy theme has, so every palette was rejected and the
source SPEC §6.1 makes the default on Linux fell back to an embedded theme —
silently, on every launch, for two milestones. It was written on macOS, where
there is no Omarchy at all, and tested against fixtures transcribed from the
prose that was wrong (§2.9).

The lesson is not "test on Linux", which the risks already said. It is narrower
and worth writing down: **a fixture written from a document tests the document.**
Where omagit reads a format somebody else owns, the fixture has to be a file
that somebody else wrote. `tests/fixtures/omarchy/` is the first of those.

A tenth, from M2, and the same lesson one layer down: **the macOS half of the
read path is only ever exercised by CI.** Two behaviours differ there and
nowhere else — APFS folds case, and the filesystem hands back decomposed names —
and both are load-bearing for `status`. Two tests carry them
(`a_rename_that_changes_only_case_is_a_rename`,
`an_accented_name_matches_its_index_entry`), and a third is compiled out on
macOS entirely because APFS rejects names that are not valid UTF-8. A green
Linux run says nothing about any of them.

**It fired on the first run that mattered**, and not on any of those three: two
tests compared a work tree against `canonicalize(fixture)`, which holds on Linux
and cannot on macOS, where `/var` is a symlink to `/private/var` and a temporary
directory therefore has two true names. The product was never exposed — the
three places that canonicalise each do it once and use the result throughout —
but the assertion was wrong on both platforms and only *observably* wrong on
one. The fix is on the M2 branch, and the general form of it is worth keeping:
**assert the property you need, not the incidental one.** Two paths naming the
same directory is what the app requires; string equality was never it.

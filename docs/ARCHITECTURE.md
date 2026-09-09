# ARCHITECTURE.md

Maintained as the project goes (SPEC §7). It records the decisions that are
expensive to reverse and the risks that have to be re-read at each milestone.

**Status: M6, interrupted — the UI is moving from GPUI to Tauri (2026-09-09).**

The Git core, the theme system and the settings are unaffected and stay where
they are. `omagit-ui` and the screens will be rewritten. What is written below
about them describes what exists today, not what will exist; the decision and
its reasoning are in §2.20, and SPEC §4's amendment carries the same in the
brief's own terms.

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

### 2.20 The UI leaves GPUI for Tauri

Decided between M6 and M7, on one argument: **GPUI is not a product for third
parties.** It exists to serve Zed. Its API moves when Zed needs it to — the
`gpui-pre-*` naming says so out loud — and every Zed refactor would be our
migration, with nothing owed to us. Tauri exists so that other people build on
it; its breaking changes are announced and documented. Over a project of months
that is the difference between a dependency and a bet.

Three things did **not** decide it, and saying so matters because two of them
were mine:

* **Performance across the IPC boundary.** Measured on a real 739-commit
  repository: a full page of history with its graph is 0.55 MB and crosses in
  17ms; the 60 rows a virtualised list actually shows, 1.5ms; the largest diff
  in the repository, 4ms. Against SPEC §12's budgets that is under 10%. I had
  argued the opposite before measuring, and I was wrong.
* **The work already done.** Sunk cost, and it was right to be told so.
* **Ecosystem size, or the curved graph lanes.** Real gains, not decisive ones.

**Tauri rather than Electron**, despite Tauri's three rendering engines against
Electron's one: a Tauri backend *is* Rust, so `omagit-git` — 8,300 lines with no
UI dependency and 3,959 lines of tests — stays as it is. Electron would mean
rewriting it or adding a process boundary.

**What it costs, unvarnished.** About 10,000 lines of interface, and the loss of
GPUI's interaction test harness, which drove the real widget tree and caught
four real bugs in one session. Nothing on the web side is as direct.

**What is still unknown.** Tauri uses the system webview, so WebKitGTK on Linux
— the design target, and the weakest of the three. That is what M6b's spike
exists to answer, on Linux, before anything is ported.

**Why the list of what survives is short.** SPEC §3 rules 5 and 6 — the Git core
knows nothing about the UI, no `cfg(target_os)` in the UI layer — were held from
M0. A core that had known about the UI would have made this change impossible to
consider, which is the argument for those rules stated as a cost avoided rather
than as a principle.

### 2.21 The interaction harness the port lost is Vitest, and it is three suites

§2.20 listed the loss of GPUI's `TestAppContext` as the real cost of the move,
and SPEC §13's amendment says to replace it early, "parce que c'est par ce trou
que sont passés tous les bugs d'interface de ce projet". The replacement is
three suites, and the split is deliberate, because one of them cannot catch what
another can:

* **`web/src/**/*.test.ts` (Vitest + jsdom).** Drives `state.ts` and the
  components against `backend.fake.ts` — a repository that *changes*: staging a
  hunk leaves the file on both sides of the index, staging the whole file moves
  it to one side, discarding an untracked file removes it. A fake whose answers
  never moved would agree with any bug at all. This is what covers the state
  machine, which is where every interface bug in this project has actually been.
* **`crates/omagit-app/tests/edits.rs`.** The write logic against real
  repositories built by the real `git`. This is why `edits.rs` exists as a
  module at all: the part worth testing was inside a `#[tauri::command]`, where
  reaching it meant building an application. What it pins is which side of the
  index a patch is built from — inverted, that produces a patch `git apply`
  rejects, or one it accepts against text that happens to match.
* **`edits::tests`.** The wire shapes, parsed from the literal JSON `ipc.ts`
  sends. Neither of the other two covers it: the front end's fake never
  serialises, and the integration tests build a `Target` in Rust.

**Every one of them was checked by putting the bug back.** Six in the front end
(byte offsets read as UTF-16, a two-state checkbox, `direction: rtl` reversing
the two halves of a path, the pane jumping to the staged tab after one hunk,
picked lines surviving a write, the confirmation running before it is answered)
and two in the write logic (the side inversion, and assuming nothing is
untracked). A test that passes with its bug reintroduced was already shipped
once in this project — for the diff header overlap in M5, where `debug_bounds`
reported layout and the bug was in paint.

**What none of them covers** is the browser: layout, overflow, whether a control
is reachable. jsdom has no layout engine, so `.file-path` rendering its two
halves in the wrong order is caught by DOM order here and would not be caught by
a screenshot test that does not exist. Three of the bugs above were found by
looking at the window, and that step has no substitute yet.

### 2.22 Prettier gates the front end, at the width rustfmt uses

The Rust half has had `cargo fmt --check` as the gate's first step since M0; the
web half had nothing, and the style drifted between files written on different
days. `prettier --check .` now runs in `npm run check`, which is what
`scripts/check.sh` calls — the same rule, applied to the other language.

`printWidth: 100` rather than Prettier's default 80, because that is rustfmt's
`max_width` and because the front end was already written at it. Adopting the
default would have reflowed every file in `web/` for no reason anyone could name
afterwards, which is the kind of diff that hides a real change inside it.

### 2.23 The write queue is gone; the lock and `busy` replaced it

`omagit-app/src/writes.rs` held a per-repository queue with a failure banner,
written for GPUI. Nothing referenced it after the port. The rule it enforced —
SPEC §10's "two writing commands never run at once on one repository" — is now
held in two places that are each closer to what they guard: a `Mutex` on the
open repository handle, taken for the duration of every write command, and
`state.busy` in the front end, which is also what disables the controls. Keeping
a third implementation of one rule, wired to nothing, is the debt §2.4 refuses
elsewhere.

### 2.24 The history walk reads ahead of what it hands out

Found by looking at the History screen on a repository built by a script: a
commit sat *below its own parent*, and the graph drew a line upwards into a node
already passed.

The walk ordered its frontier by commit time, newest first, with the hash
breaking ties. That is `git log`'s **default** order, and Git's default has the
same behaviour — but a graph needs `--date-order`, whose actual guarantee is
stronger and is the whole point: **show no parent before all of its children**.
Time alone does not give it. Two commits made in the same second tie, and a
parent can win the tie against its own grandchild. Same-second commits are not
exotic: a scripted import, a rebase, `git commit` twice in one second.

The fix is two halves, and a test proves each is load-bearing by putting it back:

* **Blocking.** Every commit counts how many of its children have been read and
  not yet handed out; it is held out of the eligible heap until that reaches
  zero.
* **Reading ahead.** Blocking alone is not enough — it only counts *discovered*
  children, and a child can be discovered after its parent has already gone out.
  So reading runs `LOOKAHEAD` (1024) rows ahead of handing out, which is what
  makes the count complete.

**This is exact in practice, not in principle**, and the difference is worth
naming. Git is exact: `--date-order` sorts the whole range before printing a
line. That trade is not available here — SPEC §11 wants history paged and
resumed, §12 wants the first screenful of a 100 000-commit repository at once. A
child and its parent are neighbours in time order, so a thousand rows of slack
is orders of magnitude more than any real history needs; a history that defeated
it would need a parent and child a thousand commits apart in commit-date order.

**Measured**, on a generated 100 000-commit history where *every commit shares
one timestamp* — the worst case for both halves:

| | |
|---|---|
| 1 000 first commits (SPEC §12 budget: < 250 ms) | **8.4 ms** |
| The whole 100 000 | 276 ms |
| Order over the first 2 000 | identical to `git log --date-order` |

The tests assert the *property*, not Git's sequence: when several commits become
eligible at once — which is what a tie is — any order among them is a valid
`--date-order`, and Git's own choice comes from the order it happened to load
them in. `walks_head_in_the_same_order_as_git` compares sequences, on a fixture
with distinct timestamps where there is only one answer.

### 2.25 A filtered history is a search result, so it has no graph

SPEC §11's filters — author, message, path, date range — narrow what is
*shown*, never what is *walked*. The topology is the repository's and does not
change because someone typed a name into a box, so a filtered walk visits the
same commits in the same order and skips the ones that do not match. That is
also why a page can come back short while the walk still has more, and why
`is_done` rather than a short page is what says the end has been reached.

**And the gutter goes away while a filter is on.** The lane algorithm places a
commit relative to the commits around it; under a filter those are not its
parents and children, they are the next things that matched. A line drawn
between two of them would claim a relationship whose only content is the search.
`git log --author=…` prints a list rather than a graph for the same reason, and
the screen says so out loud — a gutter that vanished without an explanation
reads as a bug.

Three decisions inside the filters that are not Git's, each because a filter box
is not a command line:

* **Substrings, not regular expressions.** Git's `--author` is a regex over the
  whole ident line. Someone typing `marek` means "commits by Marek", so the
  match is a case-insensitive substring, over the name *and* the address,
  because people search by both.
* **The date range matches the author date**, which is the date the row shows.
  Git's `--since` uses the committer date and is right to — it asks when the
  commit entered this history — but a range that excluded a row displaying a
  date inside it would be indefensible.
* **An empty box is not a filter.** It arrives as an empty string, and an empty
  substring matches every commit there is; a cleared box would go on filtering
  nothing out while the screen said it was filtering.

The path filter is TREESAME, Git's own rule: a commit is kept when the object at
that path differs from the same path in *every* parent. Resolving one path is a
walk down the tree's spine — a handful of object reads — rather than a full tree
diff, which is what makes it affordable once per commit. A merge that matches
one of its parents is dropped: it only joined two lines that had already changed
the file, and showing it would answer "who touched this" with a commit that did
not. `a_path_filter_keeps_what_git_log_keeps` compares against `git log -- path`
directly.

**What this does not do** is Git's history simplification — rewriting parents so
the kept commits form a connected graph. That is what would let a path-filtered
view keep its gutter, and it is a separate piece of work with its own
pathological cases.

### 2.26 A ↔ B is two clicks, not a mode

SPEC §11's comparison between two commits. `from` and `to` are exactly that and
not "older" and "newer": comparing a commit with one of its own descendants is
the ordinary case, and comparing the tips of two divergent branches is the
interesting one, where neither is older.

**Shift-click rather than a mode.** Comparing is something done to two rows you
can see; a mode would have to be entered, remembered and left again for an
action that takes two clicks. Marking where a comparison starts is therefore
separate from running one — the second commit is chosen by *scrolling*, and a
comparison that ran on every row the pointer landed on would read a diff per
row. `HistoryList.test.ts` pins that a plain click compares nothing, and both
halves were checked by putting the bug back.

The comparison is its own field rather than a mode on the commit detail, because
it answers a different question: a commit detail asks what this one changed, a
comparison asks what is between these two, and the second has no author, no
message and no parent to show. It also disappears when the walk that produced
its two ends is replaced — a filter typed into a box leaves the pane pointing at
rows nobody can see otherwise.

### 2.27 The Repositories screen draws before it knows anything Git knows

Board 06's list says what each repository *is* — the branch, what is modified, a
half-finished merge — and every one of those is a status walk. Reading them
before drawing would mean an empty window for as long as the slowest repository
in the list takes, which is the one on a network disk. So the library arrives
first, from the settings file alone, and the rows fill in.

That is a property a test has to hold on to deliberately: with a fake that
answers instantly, awaiting everything and awaiting nothing look identical. The
fake holds one repository's summary open by path, and the test asserts that
start-up finished with that row still loading — checked by putting the `await`
back, where it times out.

**A repository that is not where it was recorded keeps its row**, struck
through, saying "introuvable sur le disque" (DESIGN §4). It is never removed on
the app's own initiative: an unmounted disk comes back, and a list that tidied
itself would lose an entry the user arranged. Start-up skips it when choosing
what to open — greeting someone with an error they did not ask for is not a
convenience — and falls through to this screen when nothing can be opened.

Whether a row is missing comes from a `stat`, not from failing to open the
repository: the second costs a `gix` open per row for a question the filesystem
answers directly.

Removing an entry asks first, and the question says the repository stays on the
disk. That is not the usual reason for a confirmation — nothing is lost that
Git cannot restore — but "Retirer" beside a repository name reads as though it
might delete it, and the dialog is where that is answered.

**What is not built**, and is board 06's: drag to reorder, groups created and
renamed from the window, cloning from a URL, "Révéler dans le gestionnaire",
and the per-repository description edited in place. The data model carries all
of them — `Library::move_entry`, `add_group`, `Entry::description` — and each is
an interaction surface of its own.

### 2.28 The interface was built from the mock-ups' text, not from the mock-ups

Told that the interface did not resemble the boards "ni sur le fond ni sur la
forme", and it did not. The cause is worth writing down because it was a method
error, not a series of small mistakes: **every screen so far was built from a
flattened text extraction of `docs/design/*.dc.html`** — a script that stripped
the tags and printed the words. That yields the content of a board and none of
its form. Every dimension, weight, colour rôle, border and state was invented to
fit the words.

The boards are HTML with inline styles. Reading them tells you that a button is
26 high with 12 of padding and comes in three kinds and five states; that a
status badge is a letter *and* a colour so it survives greyscale; that a section
header is 11px uppercase with 0.08em of letter-spacing; that a repository row is
two lines and 36 high with a status square. None of that survives the flattening,
and all of it is what "la forme" means.

**On the substance**, one thing was plainly wrong: the window opened straight
into a repository, so the Repositories screen — the first of the three the
product is made of (SPEC §1), and what §12 measures cold start against — was
something you could only reach by going backwards. Board 06 is drawn "pas de
dépôt ouvert". It opens there now, and `showScreen` refuses a repository screen
when none is open, so the broken shell that state produced cannot be reached.

**And a stylesheet that grew by appending had started lying.** `button.primary`
was defined twice, the later one — left over from the commit box — winning and
turning the primary button into an outline. Worse, a bad slice had duplicated
230 lines, so the *old* repository card was overriding the new one: the screen
was drawing code I had already replaced. `stylelint`'s `no-duplicate-selectors`
now runs in `npm run check`, which is the gate. A dead CSS rule is invisible in
review and invisible in tests, and it is exactly how an interface drifts from
its design without anyone changing anything.

**The topbar's shape follows the screen, not the open repository.** Board 06
labels its frame "topbar réduite à 40px · pas de dépôt ouvert", so which topbar
to draw is a question about the screen. Keying it on `app.open` instead — on the
reasoning that the block says which repository the window is *in* — left the
repository's name and its Fetch / Pull / Push sitting on the Dépôts screen, and
going back there looked as though it had not worked. Reported, and it was right
to be: the window is not in a repository when it is showing the list of them.
The repository stays loaded, because you may be going back to switch and return.

That is the third bug of one kind in this project — chrome belonging to the
wrong thing. The GPUI build let History take the window over; a screen once drew
the way back to itself; this one let a screen keep a header it had left.
`Topbar.test.ts` mounts `App.vue` rather than the topbar, because the bug is
about the two moving together: a topbar test that set the screen by hand would
agree with any routing at all.

**What is still not board-accurate**, and is named rather than left to be
discovered: board 05's month separators and its All Branches / Remotes / Tags
segmented control; board 03's branch tree, which is M7's; board 06's drag to
reorder and its group editing, which are §2.27's; and the two-line topbar's
back / forward history navigation.

### 2.29 Two lines per commit, against board 05, and columns you can drag

**Two lines.** Board 05 draws one, and this departs from it deliberately, asked
for with the board in front of us. On a real history — 500 commits, long
branch names, a signed-off author — the single line has six things competing
for it: the graph, an avatar, the author, the refs, the date, the hash and the
message. The message is what the list is read for, and it is the one that
loses; at a 520px column it was reaching the reader as eight characters and an
ellipsis. Two lines give the author and the date the first, and the message the
whole of the second.

The avatar stays **square**. The reference the request came with is Tower's,
whose avatars are circles; DESIGN §1 is "square everything, no radii anywhere",
and one rounded thing would be the only rounded thing in the app.

The row's height is `2 × line_height + row_padding` rather than a number:
it is two of something that already has a token, plus the padding a row already
has, and a literal would be a third value to keep in step with the density by
hand.

**Draggable columns.** Every fixed column now sits inside a `.pane` wrapper that
carries the width and the splitter, so a pane knows nothing about being
resizable — the splitter has to know which two things it sits between, and that
is only true at the shell's level.

Two things about it are less obvious than they look, and each has a test that
fails when it is undone:

* **The travel is divided by `--scale`.** The pointer's coordinates are in
  scaled pixels because `#app` is zoomed; the width the stylesheet wants is
  unscaled. Without the division, dragging at 1.15 moves the edge fifteen per
  cent further than the pointer, which reads as the column running away.
* **The drag settles on the width it computed**, not on `props.width`. Settling
  on the prop means settling on whatever has made it back down through a render:
  it happens to work while the parent is reactive, and it is one refactor away
  from writing the width the drag started at. The first version did exactly
  that, and the test caught it.

The widths are remembered in `settings.toml` under `panes`, a map rather than
named fields: which columns exist is an interface concern, and a pane that
disappears leaves a key nobody reads rather than a migration. Written when the
drag ends and not while it moves — the file is rewritten on every call, and a
drag is a hundred of them.

### 2.30 The commit sits above its diff, and a filled row picks its own text

**The commit and the diff are stacked, not side by side.** Two narrow columns
were fighting for the same width and the diff — the wider of the two by nature,
because it holds code — got the worse half. Stacked, the diff has the column's
full width and the commit keeps only the height it needs. Inside the commit,
the message and the file list each scroll on their own, and the file list keeps
a floor of three rows: a long message would otherwise take the whole box and
leave no way to pick the file whose diff is showing underneath.

The one thing to get right there is that a percentage inside a percentage is
not a height. The message had `max-height: 40%` inside a box that was itself
`45%` of the column, which came to about a hundred pixels and cut its last line
in half. It shrinks by flex now, against a file list that has a floor.

**The filters fold behind a button**, and folding never clears them: a filter
still narrowing the list while the fields that explain it are hidden would be a
list that lies about what it is showing. So the row stays open as long as
something is filtered, which is a rule with a test rather than a habit.

**A filled row picks its own text colour.** `--on-accent` and `--on-danger` are
emitted by the theme as whichever of `bg` / `text` reads better against the
fill — DESIGN-TOKENS §4.3 applied to a case §4.3 did not name. A theme whose
accent is dark otherwise puts near-black on near-black.

That was not, in the end, what made the selected sidebar row unreadable, and
the difference is worth recording. **`.sidebar-row.selected` was written
twice**: once with the accent fill and `--bg` text, and once — later, inside a
shared "selected is a surface change" rule — with `--surface-raised`. Equal
specificity, so the later background won while the earlier text colour stayed:
`--bg` on `--surface-raised`, dark on dark. `stylelint`'s
`no-duplicate-selectors` does not see it, because the two selector *lists*
differ. What does see it is keeping every "selected" in one block, which is
where they now are, and the contrast token means the remaining pairing cannot
go wrong per theme.

### 2.31 A row is a button for the keyboard, not a control that looks like one

The hovered version of §2.30's bug, reported straight after it.
`button:hover:not(:disabled)` is `(0,2,1)`; `.sidebar-row.selected` is `(0,2,0)`.
So hovering a selected row took the generic background — `--surface-raised` —
while keeping the text colour chosen for the accent fill it no longer had.
Measured in a real engine: **1.4:1**, against 7.3:1 once fixed.

The same rule was quietly taking the background from `.library-row:hover` and
`.commit-row:hover` too, which had been writing `--surface-hover` into rules
that never applied.

The fix names the thing: a list row is a `<button>` so the keyboard can reach
it, not because it is a control that looks like one, so it carries `.row` and
the generic hover excludes it. Every row's hover and selected state is defined
in one block with the others.

**No gate we have can see this class of bug**, and that is worth stating rather
than discovering a third time. `stylelint`'s `no-duplicate-selectors` needs the
selector *lists* to match, and these differ. jsdom has no cascade, so a Vitest
component test reads the class list and not the colour that results from it.
The verification here was a probe page in Chrome — the real stylesheet, the real
tokens as `omagit-theme` emits them, `getComputedStyle` on a row with the hover
rules re-applied as a class — and it produced the reported 1.4:1 from the old
cascade and 7.3:1 from the new one. That is a technique, not a gate: putting a
headless browser in `scripts/check.sh` is infrastructure, and CLAUDE.md asks who
it protects before it is built. For now the protection is that every "selected"
and every row hover live in one block, where a second rule for the same state is
visible to a reader.

### 2.32 M7, first slice: local branches

`git switch`, not `git checkout`. The old command means two things — move
`HEAD`, and restore paths — and tells them apart by guessing, so
`git checkout -- feature` reads `feature` as a *pathspec* and reports that no
file by that name is known. That is not hypothetical: it is how the first
version of this failed, and `a_branch_whose_name_could_be_a_path_still_switches`
pins it with a repository that has both a `docs/` folder and a `docs` branch.
`switch` arrived in Git 2.23, comfortably under SPEC §8's floor of 2.35.

**What is deliberately not refused here.** Switching away from uncommitted work,
and deleting a branch whose commits are on no other. `git` refuses both, with a
message that names the file or the branch in the way — better than any this
layer could compose, and SPEC §3 rule 3 asks for Git's own words. The caller's
job is to offer the forcing flag afterwards, not to pre-empt the refusal.

**`merged` returns the whole set in one command.** The sidebar asks it of every
row it draws; a process each would make opening a repository with forty branches
forty processes. It is asked *before* a delete so the confirmation knows which
of its two questions it is asking — "remove a label" or "throw away four
commits". Those are different acts, and one wording for both would either
frighten people off the harmless one or wave them through the other.

**Grouping and sorting are the sidebar's**, which `omagit_git::refs` says in as
many words. Only the first `/` segment groups: `feature/ui/topbar` lands under
`feature/` showing `ui/topbar`, rather than nesting three deep for a tree nobody
arranged that way. And the rows are sorted, because the order `gix` enumerates
references in is a hash map's — a tree whose rows moved between two runs of the
same repository would be unusable.

A branch prefix keeps its case, against board 03, which uppercases it with the
section headers around it. A prefix is part of a name that Git treats
case-sensitively, and `FEATURE/` is the kind of tidiness that misleads.

**Not in this slice**, and named rather than left to be found: fetch, pull, push
with `--force-with-lease`, credential helpers and the cancellable progress
overlay — the network half of M7 — along with merge and rebase. `ops::rename`
exists and is tested; no command exposes it, because nothing in the window
renames a branch yet and a registered command nobody calls is the speculative
API SPEC §3 refuses.

### 2.33 M7, second slice: the network

**Nothing here ever waits for a person.** `GIT_TERMINAL_PROMPT=0` was already
set for every invocation (`cli.rs`, SPEC §8 rule 2), and it is what makes a
network operation cancellable rather than hung: there is no terminal behind this
window, so a `git` that decided to ask for a password would never be answered.
`SSH_ASKPASS` is deliberately not set either — an `ssh` with no agent fails
naming the key, which is actionable, where pointing it at a helper that cannot
draw a window would hang the same way. Authentication itself is `git`'s, through
`libsecret` and `osxkeychain`, which is the third reason SPEC §8 gives for the
subprocess and the one that matters most here.

**Progress required a second way of reading stderr.** `read_to_end` answers
once, at the end, and the end is exactly what the user is waiting to hear about.
`drain_watching` reads in chunks and cuts on **both** `\n` and `\r`, because
`git` overwrites its own progress line with a carriage return: a reader that
split on newlines alone would receive one enormous line at the end and report
nothing until then. There is a test for that specific shape, against a real push.

**One operation at a time**, which is a product decision as much as a technical
one: two fetches on one repository race for `.git/FETCH_HEAD`, and an overlay
that had to describe two things at once would describe neither. The slot is a
guard released on drop, so a panic cannot leave the app refusing every fetch
until it restarts.

**`--force-with-lease` and no plain `--force`.** `--force` overwrites whatever
is on the remote, including a colleague's commit pushed thirty seconds ago, and
cannot tell that case from the rebase you meant to publish.
`--force-with-lease` refuses exactly that case, and the refusal is the point —
`force_with_lease_refuses_exactly_the_case_force_would_destroy` builds the
divergence with a second clone and asserts both halves.

**No merge strategy is chosen for `pull`.** `pull.rebase`, `pull.ff` and a
branch's own `branch.<name>.rebase` are the user's settings, and passing
`--rebase` or `--no-rebase` of our own would quietly override a decision someone
made for the repository.

**The remote is a bare repository on disk** in every test. That is a real remote
to `git` — the same refspec handling, the same `--prune`, the same lease check —
and it needs no network, so the suite runs in a tunnel and never flakes on
someone else's outage. What it does *not* cover is authentication, which cannot
be tested without a server and a helper; the rule that protects it is the
environment, and `cli.rs` asserts that.

**The overlay is a bar at the foot of the window, not a modal.** A fetch does
not stop you reading the diff you were reading, and a modal held for two minutes
of network is the app hanging with extra steps. It sweeps rather than sitting at
zero while `git` counts with no total to count against — a bar frozen at 0%
reads as a stall — and it says "arrêt…" while a cancellation is in flight,
because `git` stops when it next looks and that is not instant.

**Still not built** in M7: merge and rebase, and clone. Board 06 draws `Cloner…`
disabled, which is where it stays.

### 2.34 M7, third slice: merge, rebase, and the way out

**`GIT_EDITOR=true` and `GIT_SEQUENCE_EDITOR=true`, for every command.** `git
merge` and `git pull` open an editor for the merge message; `git rebase` opens
one for its todo list. `git` skips both when stdin is not a terminal, and stdin
here is `/dev/null` — but that is a behaviour to rely on rather than a
guarantee, and the failure if it ever changed is a `vi` on a pipe nobody can
see, waiting forever. This is the second prompt nobody can answer, alongside
`GIT_TERMINAL_PROMPT=0`, and it is set in the same place so it cannot be
forgotten at a call site. It was already a latent hang in `pull`, which can
create a merge commit; shipped in §2.33 and found here.

`true` is the shell builtin: it exits 0 immediately and leaves the file
untouched, which is exactly "keep the message git prepared".
`a_merge_never_waits_for_an_editor` proves the cover holds even when the
repository configures `core.editor = false`, because `GIT_EDITOR` wins.

**`abort` ships with the thing it undoes.** Resolving a conflict is M8's, but an
application that can start a rebase and cannot stop one is a trap: it leaves the
user in a state they did not choose, with no exit but a terminal. The command
sent depends on what is running, read from the repository rather than passed in
— the front end's idea of what is running is a copy, and a stale copy would send
`git merge --abort` to a rebase, whose message would be about the wrong thing.
A bisect is refused by name, because it ends with `git bisect reset`.

**A failure now carries Git's words wherever Git put them.** `git merge`
explains a conflict on **stdout** — `CONFLICT (content): Merge conflict in
shared.txt` — and `CommandFailed` carried only `stderr`, so a conflicting merge
reached the user as an error with nothing in it. Found by a test that asserted
the message rather than the exit code. SPEC §3 rule 3 asks for Git's own words;
it does not say which pipe they arrive on.

Two smaller things found in the same place: the error read `git git merge failed`
because `command` already holds the whole line, and no merge strategy is chosen
for `merge` in the fast-forward direction — `merge.ff` is the user's setting,
and passing `--ff-only` would refuse merges the repository is set up to accept.

**Not built:** clone, which board 06 draws disabled, and `continue` for a
resolved conflict, which is M8's.

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

**M5 and M6 were built on GPUI and are not on screen.** Staging by file, hunk
and line, the commit box, discard, the operations journal, the history walk with
its lanes — all of it is in `omagit-git`, tested there, and untouched by §2.20's
change. What the port threw away is the 10,000 lines of interface that reached
it. The list below is therefore about the *front end*, not about the features.

**The Tauri front end, so far.** `omagit-app` is the backend and `web/` is the
window:

- The shell — topbar with the platform's edge reserves, sidebar, status bar —
  belongs to `App.vue` rather than to a screen. That is the fix for the GPUI
  bug where opening History left a window with no way out of it.
- `VirtualList.vue`: rows positioned by `transform` inside a spacer, `v-for`
  over slot *positions* keyed on the slot, a `ResizeObserver` for the viewport.
  Keying on the index instead changed every key on every scroll, which is the
  opposite of recycling; a pane laid out after mount measured zero.
- The diff, virtualised, with the hunk tint, the word-level refinement of
  DESIGN-TOKENS §5, folded context and the two sides of the index as tabs.
- **The Working Copy in write.** The tri-state checkbox (a file can be wholly
  in the index, wholly out, or half in — drawing the third as the second made
  partial staging look as though it had done nothing), per-hunk and per-line
  staging, discard behind the confirmation of SPEC §3 rule 7, and the commit box
  with `commit.template`, the 50/72 subject rule, amend / sign-off / no-verify,
  and the identity warning raised while the box is still empty.
- The operations journal panel: the exact command, marked destructive before it
  runs.
- The tests of §2.21.

- **History.** The walk paged and resumed, the graph drawn from
  `omagit_git::graph`'s lane indices as one `<svg>` per row, references drawn
  four ways, and a commit detail with its files. The walk is parked on the open
  repository handle rather than in a command, because it is stateful and because
  the lane assignment has to continue with it — a graph rebuilt per page puts a
  long-running branch in a different column every time the list scrolls.
- Screen switching, in the sidebar rather than in a screen. That is the
  structural fix for the GPUI bug: no screen can take the way out away, because
  no screen draws it.

- **The filters of SPEC §11** — author, message, path, date range — over the
  walk, with the graph off while one is on (§2.25).

- **A ↔ B**, on a shift-click (§2.26).

- **The Repositories screen** (§2.27): the grouped list with a summary per row,
  the card of board 06, adding through the platform's folder picker, and
  removing an entry without touching the disk.

The port is complete. What is left on these screens is named in §2.27 and
§2.25 — board 06's reordering, groups and cloning; and Git's history
simplification, which is what a path-filtered view would need to keep its
gutter. The Linux measurement §2.20 calls unknown is
still unknown — it needs one run of `scripts/dev.sh` there.

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

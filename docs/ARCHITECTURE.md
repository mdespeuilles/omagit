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

**What was unknown, and is not any more.** Tauri uses the system webview, so
WebKitGTK on Linux — the design target, and the weakest of the three. M6b's
spike existed to answer that before anything was ported, and the answer arrived
the other way round: the port went ahead on macOS, and the whole of it — M3
through M8, the virtualised diff, the graph's gutter, the conflict dialog — was
then run on Linux under WebKitGTK on 2026-09-09. It holds. Nothing about the
webview has had to be worked around, and no measurement has been given up on.

That is one report from one machine, not a benchmark: the numbers SPEC §12 asks
for are still owed, and `selectFile` logs the IPC time of every diff it reads
precisely so that they can be collected there rather than guessed at here. What
the run settles is the question the decision was taken *without* an answer to —
whether the weakest of the three engines could carry this interface at all.

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

**What is not built**, and is board 06's: "Révéler dans le gestionnaire", the
per-repository description edited in place, and dragging one *folder* above
another. The data model carries all of them — `Entry::description`,
`Library::move_group` — and each is an interaction surface of its own. Cloning
from a URL arrived with M7; the folders, and dragging a repository between
them, with §2.64.

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

### 2.35 M8, first slice: the shelf

**A stash list is a reflog, and everything awkward follows from that.**
`refs/stash` is one reference, not one per entry; `stash@{0}` is a *position* in
its log. So `stash::list` reads a log rather than enumerating references, reads
it forward and reverses — `gix`'s reverse iterator says in its own
documentation that it is I/O-expensive and meant for the last few entries, and a
shelf is short — and hands out an index that is `git`'s address rather than an
identity.

**Which is why every write is addressed by commit.** Dropping `stash@{1}`
renumbers everything below it, so an index that crossed to the webview and came
back is a copy of a numbering that may already have moved. `stash::find`
resolves a commit to the index it has *now*, and the command that follows runs
under the same write lock as the resolution. It is the rule `abort_operation`
already follows for the running operation, and the failure it prevents is worse
here: a `git stash drop` aimed one row off.

**A log line that will not parse fails the list rather than being skipped.**
Skipping renumbers every entry below it, silently, and the number is what a
destructive command is given. An error the reader can see beats a list that is
quietly wrong about which stash is which.

**The preview is not `Diff::commit`.** A stash made with `--include-untracked`
keeps those files in a *third* parent, whose tree is in neither side of the
ordinary first-parent comparison. A preview built from it would tell someone
their untracked files are not in the stash — and they would go and delete them.
`stash::diff` compares against the first parent and appends the third against
nothing, so those files read as the additions they are. The row says
`+ non suivis` for the same reason, before the entry is ever opened.

**Only `drop` asks.** Applying is recoverable — the entry stays — and what a
pop takes off the shelf is in the working tree by the time it does, with `git`
keeping the entry whenever the apply conflicts. `drop` is the one that leaves
what it held reachable only through the reflog. A confirmation on all three
would make the one that matters unreadable (SPEC §3 rule 7).

**The screen takes History's shape**: the list in the middle column, and what
one entry holds stacked above its diff, because a stash *is* a commit and the
right-hand side is the same two panes. `DiffView` already draws itself
read-only anywhere but the Working Copy, so nothing there invites a click that
would stage out of a stash.

**One diff pane is shared by three screens, and it did not belong to any of
them.** Found while building this and already latent since History: `settle()`
re-reads the working copy after every write, wherever the write was started
from, so applying a stash — or merging a branch from the sidebar while reading
History — replaced the diff on screen with a working-copy file the reader had
selected on another screen. The pane now belongs to the screen that is open:
`settle` leaves it alone unless the Working Copy is the one being looked at, and
each screen reclaims it when it opens. Two tests hold it, and removing either
half of the fix fails one of them.

**The shelf's row actions sit beside the row, not inside it.** A `<button>`
inside a `<button>` is invalid, and the Vue compiler says so; the branch tree
only escapes the warning because its actions are behind a `v-if`. So a stash
entry is an `<li>` carrying the row and its three buttons as siblings, and the
`<li>` is what hover and selected paint — with those two states defined in
`style.css`'s one block for them, where §2.31 put every other row's. The branch
tree's nesting is left as it is and recorded below rather than changed in a
slice about stashes.

### 2.36 M8, second slice: the two ways out of a conflict

**`ours` and `theirs` are only honest during a merge.** On a rebase the commits
being replayed are `theirs` — including the ones you wrote a minute ago — and
`ours` is the branch you are replaying *onto*. Two buttons labelled with the
pronouns would be asking someone to choose between two meanings they cannot
see, so `conflict::sides` names them: `MERGE_HEAD` for a merge,
`rebase-merge/onto` and `head-name` for a rebase, `CHERRY_PICK_HEAD` and
`REVERT_HEAD` for the replays — each resolved to the branch, tag or short hash a
reader would recognise. `replayed` travels with them so the interface can
explain the reversal rather than assume it is understood. Board 07 asked for
this in one line: *"Ours et Theirs sont nommés par leur branche et leur hash,
pas seulement par le mot."*

**Which side to keep is not always a version to check out.** Half the conflicts
Git can produce have no content on one side: a file we deleted and they changed
(`DU`) has no stage 2, and `git checkout --ours` answers *"does not have our
version"* — true, and useless. Keeping ours there means keeping the *deletion*,
which is `git rm --force`. The table lives in `ops::conflict::keeps_nothing` and
is read off Git's own status codes; `edits::resolve` looks the kind up in the
status rather than taking it from the front end, because a copy of the index
held on the other side of the wire restores a file the reader asked to see
deleted.

**A resolution is two commands, never one.** `git checkout --ours` leaves the
path unmerged in the index, so a reader would take "resolved" from the screen
while `git merge --continue` went on refusing. `git add` follows it in the same
function, where a call site cannot forget.

**The way forward, beside the way out.** `abort` shipped in M7 as the safety
valve; `resume` is the other half, and the two read *which* operation from the
repository for the same reason — the front end's copy can be stale, and a stale
copy sends `git merge --continue` to a rebase. `git` decides whether it may
continue: with a path still unmerged it refuses and names the file, so the
button being disabled while conflicts are counted is a courtesy rather than the
check. The index is the truth; this side holds a copy.

**A stash that conflicts on the way back has no operation at all.** No
`MERGE_HEAD`, nothing half-finished for `Repository::operation` to report — and
the unmerged stages are there all the same. So the sides fall back to the words
("la version en place", "celle qui arrive") and the resolution still works,
which is what the fallback in `StatusList` is for rather than a defensive
`?? ""`. A test pins the case, because it is where M8's two slices meet.

**Taking a side is not confirmed, and that will be revisited.** Both versions
are in commits, so what it overwrites is the file with its markers — nothing
that is not somewhere else. What it *would* overwrite is a resolution made by
hand in the file, and nothing on this side can yet tell whether one was: the
dialog that can is the next slice. Recorded here rather than left as an
oversight.

**Not built, and it is the rest of SPEC §11's conflict line:** the three
versions shown side by side, resolution per hunk with Ours / Theirs / Both, and
opening the file in the configured editor. Board 07 draws that dialog; it is
M8's third slice.

### 2.37 M8, third slice: board 07's conflict dialog

**The markers are the data.** A conflicted file is not a diff to be computed and
not a merge to be redone: `git` has already written both versions into the file,
between `<<<<<<<`, `=======` and `>>>>>>>`, and resolving is choosing which of
them to keep. So `conflict::read` parses what is on disk, and
`ops::conflict::resolve` writes it back with the chosen sides. Nothing here
re-derives a merge, which is the one thing a Git client must never get subtly
right.

**Parsed as bytes, displayed as text.** One scan produces byte ranges; the
dialog gets lossy strings and the rebuild copies the original ranges. That is
what makes a CRLF file come back CRLF, a file with no final newline keep none,
and text that is not UTF-8 survive a round trip it never became a `String` for.
A test writes a CRLF fixture and reads the bytes back.

**A malformed sequence is refused, not guessed at.** A region that never closes,
a `>>>>>>>` with nothing open, a second `<<<<<<<` inside one: any of them means
the file is no longer what `git` wrote — somebody has already been editing it —
and rebuilding from a misreading is how a resolution eats a line nobody chose.
The dialog shows the reason and the row keeps its two whole-file answers and the
editor.

**Zero conflicts is not an error.** A file settled by hand outside omagit has no
markers left, and the only thing missing is the `git add`. The dialog says so
and its primary button still works, with no answers to send.

**The answers are matched against the file as it is when they are written**, not
as the dialog read it. A count that no longer agrees means somebody resolved
something in between, and the refusal names both numbers. It is the same rule as
the stash's index (§2.35) and the running operation (§2.34): what crossed the
wire is a copy, and the repository is the original.

**`diff3` is parsed even though no button offers it.** With
`merge.conflictStyle = diff3` or `zdiff3` there is a third section — the common
ancestor — and a resolution that did not know about it would leave the base's
lines in the file. It is drawn dimmed, as context for the decision rather than
one of its answers.

**The editor is the configured one, unless it lives in a terminal.** SPEC §11
asks for "l'éditeur configuré", which is `git`'s: `GIT_EDITOR`, `core.editor`,
`VISUAL`, `EDITOR`, in that order. But most people's Git editor is `vim`,
because the only thing `git` ever opens one for is a commit message in a
terminal — and spawning `vim` from a window that has none starts a process
nobody can see, waiting on a pipe, holding the file. So `editor.rs` keeps a list
of terminal editors, hands those to the desktop's own opener (`xdg-open`,
`open`) instead, and the answer says which of the two happened and why. Being
wrong towards the opener costs a file opening in the wrong application; being
wrong the other way costs a button that does nothing.

It is also the one subprocess in this application that is *not* an
`cli::Invocation`: no deadline, no captured output, nothing waiting for it. An
editor is a program someone works in for minutes; those rules exist for `git`,
which always answers.

**The dialog answers its own keys** — `Esc` to close, `n` for the next conflict,
`⌘⏎`/`Ctrl+⏎` to apply — while the window still has no keymap. M9 makes bindings
reassignable and `docs/KEYMAP.md` is still about the GPUI build; a dialog on
screen answering three keys of its own is not what that file is for, and is
recorded here instead.

### 2.39 A conflicted file had no diff at all

The Working Copy drew **"ce fichier n'est plus dans le statut"** over a file
sitting in the list two panes to its left. Not a stale read: an unmerged path
has no stage-0 entry in the index — that is precisely what unmerged *means* —
and both `staged_file` and `unstaged_file` look the path up there and answer
`None` when it is missing. The two sides of the index cannot describe a file
that is on neither.

`diff::conflicted_file` compares **stage 2 — ours, the version we had before the
operation began — against what is on disk now**, so the reader sees exactly what
the merge did to their file: the markers, and the other side's lines, as
additions. `git diff` gives a combined diff against both stages here; that is a
third thing to read, and the two sides side by side are already the dialog's job
(§2.37). Half the kinds of conflict have no stage 2 at all — a file we deleted
and they changed — and then the comparison is against nothing, which reads as
the whole file arriving, because that is what it is.

Its staging controls went too, and for a sharper reason than tidiness: "Indexer
le bloc" builds a patch from a side of the index that does not exist, and
"Rejeter" is `git checkout -- <path>`, which refuses an unmerged path by name.
Both were buttons that could only ever fail. Hidden rather than disabled — a
control that is never usable on this kind of file teaches nothing by staying —
and line-picking is inert there for the same reason. A conflict is answered on
the row or in the dialog.

The pane's two tabs went with it: a file with no side of the index to be on had
"Non indexé" and "Indexé" over it, two controls that would have done the same
thing. It says `En conflit` instead.

Two smaller ones in the dialog, both from the same screenshot. `.dialog-head`
had no `gap` — the clone dialog puts one thing in that row and the conflict
dialog puts four, so they printed against each other ("Résoudre un
conflitsrc/merge.rs"). And the side bands showed the name twice — "Theirs —
feature/theme-runtime feature/theme-runtime" — because the label `git` writes
into the file is, on that side, the branch the operation already resolved. It is
shown only when it says something the name does not.

### 2.38 Three things the fixture found on screen in five minutes

All three were invisible to both suites, and all three are the same shape: a
rule that is right about one element and wrong about the row or the bar it sits
in. §2.31 said a probe page in a real engine is the technique for this class;
this is the second time it has paid, and the measurements below come from one.

**Invisible actions were still taking their width.** A row's actions are drawn
at zero opacity until it is pointed at — but a transparent flex child is a flex
child, and three of them ("Résoudre…" plus two branch names) were holding a
third of the file row. `src/merge.rs` drew as `sr… merge.rs` with four hundred
pixels of nothing beside it, and the sidebar's branches truncated the same way.
They now hang over the end of the row, absolutely positioned, painting the row's
own background (`--row-bg`, set beside every other row state in the one block
that owns them). The directory is capped at 40% of the row so that the *name* —
the thing you are pointing at the row for — stays out from under them.

**A 22-pixel status bar wrapped a sentence with no length limit.** `git`'s own
words about a failed merge run to two hundred characters; the bar has a fixed
height and no `overflow`, so the long ones wrapped inside it and every fixed
part overlapped the next. The message is now the only thing that gives way, on
one line, with the whole of it in the title — the rest of that bar is four words
already as short as they go.

Then the fix's own bug, caught by the same probe: matching the shrinkable
message on `.link.danger` also matched *Abandonner*, which promptly shrank below
its own text and printed over the next item. It is `.message` now, named for
what it is rather than for the colour it happens to share.

**Every error crossed to the window with its command in front of it.**
`Display` on a failed command reads `git merge feature/x failed: CONFLICT …` —
right for the journal and the log, where *which* command failed is the question,
and wrong in a status bar, where the echo repeats what the reader just pressed
and pushes the only new information off the end. `GitError::reason()` existed
since §2.35 and was being used in exactly one place, the clone dialog. It is now
what `say()` does, so it holds for every command; the exact line is still in the
journal, marked, before it runs.

### 2.41 "Appliquer" looked like a button that did nothing

Two reports from the same click, and they are the same report.

**Nothing said it had worked.** `git stash apply` prints a status — `On branch
main` — and the app was showing that as its note: a sentence about which branch
you are on, after an operation that put two files back somewhere else. On the
Stashes screen nothing else moves either, because the entry *staying* is what
apply means. So the outcome is now said in the app's own words — "stash@{0}
appliquée · elle reste sur l'étagère" — with `git`'s text on the second line and
the exact command in the journal. SPEC §3 rule 3 is about not re-wording Git's
*errors*; it does not ask us to hand over its status output as an answer.

**"Pourquoi il reste ?"** Because "Appliquer" and "Retirer" read as a pair where
one applies and the other does not, when in fact both apply and only one keeps
the entry. The labels were the bug: they are "Appliquer", "Appliquer et
retirer" and "Supprimer" now, which is longer and says which of three things is
about to happen. The titles spell out the rest, and the note repeats the answer
at the moment the question actually arises — just after the click.

### 2.42 The one question that is not "are you sure"

`git pull` on a diverged branch refuses when nothing says how to reconcile it —
since 2.27 — and prints twelve lines of hints whose every suggestion is a
`git config` command. From a terminal that is a nudge. From a window with no
preferences screen (M9) it is a dead end: the button can only fail, and the wall
of text says so in a way that reads as the app's fault.

§2.33's decision stands: **omagit does not pass a strategy of its own**, because
`pull.rebase`, `pull.ff` and a branch's own setting are decisions somebody made
for the repository and a flag would quietly override them. What was missing is
the case where there is *no* decision to override. There, the app asks — before
the pull, not after the refusal — and the answer travels with that one pull.
Nothing is written to the configuration: what to do *every* time is exactly the
decision this app has no business taking for somebody.

**Asked on facts, not on the message.** The failure could be matched — it says
"Need to specify how to reconcile divergent branches" — but that sentence is
translated on a machine whose `git` speaks the user's language, and matching
English against it would work everywhere it was written and nowhere else. So
the two conditions are read instead: the branch has diverged (ahead *and*
behind, which the summary already knows) and `reconcile_configured` finds
nothing in the three places `git` looks.

**And it gave the confirmation dialog a second verb.** Every question until now
was "are you sure", where the two ways out are do-it and don't. This one is a
choice between two things, neither of them the dangerous one, so neither wears
`danger`: "Fusionner" is primary, "Rebaser" is ordinary, and cancelling is
still cancelling. `Question.alternative` is optional and nothing else uses it —
a shape earns its second half when a second case turns up, and this is it.

### 2.43 Clicking a branch showed its history, once it did anything at all

A branch row answered a *double* click, which checked it out, and nothing else:
a single click — the ordinary gesture, the one everyone tries first — did
nothing. A row that looks like a control and answers no click is a broken
control, whatever the double click does.

What it does now is what Tower does and what SPEC §11 already asked for in one
word ("filtres par branche"): it shows that branch's history. The walk was
built for it — `HistoryQuery::from(tips)` has existed since M6 — and what was
missing was a way to say which branch on the wire, so `Query` gained a `branch`
and `refs::tip_of` resolves it. Local branches before remote-tracking ones, the
order `git` resolves a name in, so `main` means yours and not the one you have
not pulled.

Deliberately *not* `rev_parse`: this scopes a history to a branch, not to an
arbitrary revision, and accepting `HEAD~3` would be a second feature nobody
asked for with its own error cases. A name that resolves to nothing is an error
that names it, rather than a silent fall back to `HEAD` — which would show a
history that is not the one that was asked for and look right.

Checking out stays on the double click. Switching branches rewrites the working
tree, and that is not what a single click should do — which is the same reason
the scoped history is worth having at all: you want to *look* at a branch far
more often than you want to move onto it.

### 2.40 A failure gets a band, not a modal

Reported from the fixture: "les messages d'erreur en rouge en bas ne sont pas
très visibles — il ne serait pas mieux de les avoir dans une modale ?" The first
half is right and the second is the trap.

`git`'s refusals are the useful half of this application's error messages, and
they are paragraphs: *"your local changes to the following files would be
overwritten by merge: src/render.rs — please commit your changes or stash them
before you merge. Aborting."* In a 22-pixel status bar that truncates to the
half the reader already knew, and nothing else on screen says the operation did
not happen: the row stays, the shelf does not move, and a button that did
nothing is what it looks like.

But not a modal, for two reasons. A modal blocks, and what someone wants on
reading that sentence is to go and look at the file it names — a modal makes
them dismiss it first and carry the path in their head. And the modals of this
app are SPEC §3 rule 7's confirmations, the questions that must be answered
before something is lost; using the same shape to *report* teaches people to
dismiss them, and the one that matters goes with the rest. A shape should keep
meaning one thing.

So `Notice.vue`: a band at the foot of the window, above the status bar, in the
vocabulary the progress overlay already set. It wraps rather than truncates,
keeps `git`'s words in mono and unedited (rule 3), names the action in the app's
own words above them, points at the journal where the exact command line is, and
stays until it is dismissed or until the next write succeeds — an error that
evaporates is worse than a quiet one. The status bar keeps the successes, which
are four words and fit.

`writeError` became two fields rather than one sentence, because the band draws
them differently and a caller that had joined them would have to be unpicked.

### 2.44 M9, first slice: the bindings, and the table three things read

The topbar has been drawing `⌘F`, `⌘O`, `⇧⌘N` and `⌘K` since the port, beside
buttons that answered the mouse and nothing else. A hint that names a key which
does nothing is worse than no hint: it says the application has a keyboard, and
then does not.

**A table, not handlers on the components.** Three things in M9 need the same
list and none of them is the key handler: the command palette runs these actions
by name, the `?` sheet prints them, and the macOS menu bar fires them — SPEC §9
calls that bar mandatory, "sans elle, l'app paraît cassée". A binding declared
inside a component is reachable by its key and by nothing else. And SPEC §11
asks for the keymap to be *reassignable*, which over a table is a settings
screen and over handlers is a rewrite.

**The three rules that keep a shortcut from firing where it must not**, each of
them a bug this project has already shipped in some form:

* A binding with a modifier fires while you are typing — `⌘F` in a text box is
  still Fetch, as it is in every application on both platforms — and a bare key
  never does. That is what will make the movement letters of the next slice
  safe: M3's version of them put a `j` in the filter box *and* moved the
  selection (§5, eleventh defect).
* The other platform's modifier is never answered. `Ctrl+F` on macOS moves the
  caret forward a character.
* A key claimed by an action that cannot run *now* is still swallowed; a key
  claimed by nothing is left alone. Otherwise `⌘F` during a fetch falls through
  to the webview and opens a find bar over the application.

**The hints are read from the table.** They were written out beside each label,
so nothing tied them to the binding that answers — and nothing would have said
so when one moved.

`docs/KEYMAP.md` is rewritten to describe what ships, which closes the fifteenth
known defect: it had been pointing at `crates/omagit-app/src/actions.rs`, a file
the port deleted, and listing M3's movement keys, which this front end never
had.

### 2.45 M9, second slice: the palette, and what it can reach

Board 07 calls it "le point d'entrée principal de l'app", which is a claim about
*coverage* rather than about looks: what cannot be reached from `⌘K` has to be
found by knowing where it lives. So it searches four things — the actions of
§2.44, the repositories in the library, the branches of the open one, and the
files of its working copy — and each row says what `⏎` will do to it, because
`⏎` does four different things: `exécuter`, `ouvrir`, `basculer`, `ouvrir`.

**Groups in a fixed order, not one list sorted by score.** A palette whose rows
change *category* as you type makes the next keystroke unpredictable, and the
whole point of `⏎` on the first row is that it can be pressed without looking.
Within a group the score decides, and it is three rules: a letter that starts a
word counts double (`wc` finds "Working Copy" ahead of "switch", which is
shorter and would otherwise win), a letter next to the previous one counts
double again so a run beats a scatter, and a long label costs a little.

**The ranking is a pure function**, tested against itself rather than through
the overlay: "which of two rows comes first" and "which characters are marked"
are rules, and a test that had to type into a window to check them is one nobody
reads when a rule changes.

**Dimmed, not hidden**, for a row that cannot run now — an action that
disappears when it is unavailable is one nobody learns — and that includes the
branch you are already on, which says "branche courante" where the others say
how far they have diverged.

**Board 07's marking rule survives contact**: matched characters go accent and
600, and on the selected row — whose ground is already accent — they go to
weight alone. The same colour twice over itself says nothing.

### 2.46 M9, third slice: three zones, and a cursor that is not the browser's

DESIGN §5 and board 09 ask for one vocabulary across the screens: one zone is
one tab stop, `1` `2` `3` mean the sidebar, the centre column and the detail
panel everywhere, movement inside a zone is `j`/`k` or the arrows, and `Esc`
goes up one level.

**Where the keyboard is lives in the state, not in the DOM.** Every list here is
virtualised: the row the keyboard is on is routinely not rendered at all, so
`document.activeElement` cannot be the record of it — it would be `null` the
moment somebody scrolled past the cursor. So a zone is a number in the store, a
`Walkable` says how long the zone's list is and how to move in it, and the ring
is a class on the row that is selected *in the active zone*. DESIGN §1's
distinction falls out of that: selected is a surface change and survives the
keyboard leaving, focus is the ring and only one zone has it.

**Moving through branches does not walk a history.** The branch tree has its own
cursor rather than reusing the "which history is shown" selection, because
selecting *is* the walk there — `j` held down would start one per row, which is
the filter box's old mistake (§2.41's neighbour) in another place. `⏎` asks for
the walk.

**A bare key is only safe because of the table.** M3's version of these bindings
put a `j` in the filter box *and* moved the selection (§5, eleventh defect).
Here the rule that a bare key never fires while the caret is in a field is
written once, in `dispatch`, rather than remembered at each call site — which is
the whole argument for §2.44's table restated.

**`/` is a DOM act and stays out of the store.** Focus is the one piece of
interface state the browser owns; the store deliberately owns none of it. On
History the filter row is folded by default, so `/` unfolds it first: a key that
focused a box nobody can see would be a key that does nothing.

**The repository filter is no longer drawn disabled.** Board 06 has it and it
carried a "jalon M9" label since M3 — `/` needed somewhere to land, and the card
follows the list, because a panel about a repository the filter has hidden is a
panel about something nobody can see.

### 2.47 M9, fourth slice: the Preferences screen, which nothing draws

DESIGN §7 lists it as not yet designed, so it is built from the vocabulary the
window already has — pane heads, rows, the commit box's toggles, no invented
control — and the two rules it turns on are worth keeping whoever draws it
later.

**A source this machine cannot offer is drawn disabled, with the reason.**
"There is no Omarchy here" is an answer. Hidden, it becomes a thing nobody can
ask about — on the very screen somebody opens *because* the theme is not what
they expected. SPEC §6.1's four sources fall back silently by design; this is
where that silence gets a voice.

**It names the theme that is on screen, not the one that was asked for.**
Asking for Omarchy on a machine without it resolves to something else, and a
screen showing the request would be lying about what you are looking at. The
same distinction `Resolved::source` makes on the Rust side (§2.8), drawn.

**Every setter answers with the stylesheet its change renders to**, and the
window wears it in the same tick. The alternative — save, then ask for the theme
again — is two round trips where the second can fail on its own, and a
preference you cannot see the effect of is one nobody trusts. `measure()` goes
with it, always: the tokens carry the row heights, so a density change without
it leaves every virtualised list measuring against the old ones.

**The Git block is read-only on purpose.** It says which binary answers, what
"Ouvrir dans l'éditeur" will actually launch — including the case where that is
*not* `core.editor`, because a terminal editor goes to the desktop's opener
(§2.37) — and which credential helper this platform uses. Those are `git`'s
settings, not omagit's, and a screen that wrote them would be a second place
where they live.

Found on the probe page and fixed with it: the dimmed half of a *filled* row was
still `--text-dim`, a colour chosen against a surface, which on accent reads as
noise. It takes the fill's own text colour, faded — the §2.31 lesson, once more,
in the place where a screen full of two-line rows made it obvious.

### 2.48 M9, fifth slice: the sheet, and what printing a table finds

`?` opens the shortcut sheet. It is not a written list: it prints `ACTIONS` and
`MOVEMENTS` straight from `web/src/keymap.ts`, and a test asserts one line per
entry, so an action added without its line is not something anyone can forget to
do. This project has the receipt for the other way — `KEYMAP.md` spent three
milestones describing a GPUI build that had been deleted (§5, defect 12).

Printing the table is also how it gets audited, and it found two things in the
half hour it took to build:

**`Tab` was documented since M3 and bound to nothing.** DESIGN §5 and KEYMAP.md
both described it as the way between zones. Nothing answered it. It is bound now
— out where the caret is in no field only, because inside one it belongs to the
browser and taking it would trap somebody in a text box.

**The sheet's own binding matched nothing, on any layout.** It was written
`Shift+/`, which is how the key is engraved; a browser reports the character the
layout *produced*, which is `?`. So the feature whose reason for existing is to
stop the documentation drifting from the build was itself unreachable, and its
first test is what said so. `matches()` compares `event.key`, so the fix is the
binding string, not the comparison: `Shift+?` is right on QWERTY, on AZERTY
where the character sits over `,`, and on QWERTZ where it sits over `ß`.

Laid out as two blocks side by side, falling to one column under 320px per
block. Stacked, the sheet was taller than the window it explains, and a
reference you have to scroll to read is one you read once. The key column is
`max-content` rather than a measured width for the reason §2.31 keeps teaching:
the printed length is not ours to choose — `⇧⌘N` on macOS is `Maj Ctrl N` on
Linux, and a column measured for one clips the other.

Fixed on the way past, because the compiler says it out loud on every build: the
library row and two branch rows were `<button>` elements holding their own
action buttons. A button inside a button is invalid markup, and a parser undoes
it. They are `<div>`s now, which is the shape `.file-row` and `.commit-row`
already had — and the shape `.stash-entry` reached from the other side, with the
row as the `<li>` and the actions beside the button inside it.

### 2.49 M9, sixth slice: the menu bar macOS requires

SPEC §9 is blunt about it — « barre de menus native obligatoire. Sans elle,
l'app paraît cassée » — and it is not only about looking finished. On macOS the
Édition menu is what makes `⌘Z`, `⌘A`, `⌘C` and `⌘V` work *inside a WKWebView*.
Without it the commit box has no undo and no select-all, which is a text field
that does not behave like one on the platform this ships to.

**The items come from the front end.** `web/src/keymap.ts` is the table, and the
menu bar is its fourth reader after the key handler, the palette and the `?`
sheet — the front end sends it at start-up, `crates/omagit-app/src/menu.rs`
arranges it. A menu declared in Rust would be a second list of what the app can
do, next to the one that answers the keys, and §5's twelfth defect is what a
second list does. What Rust *does* own is what is not an omagit action: the
predefined items. Quitter, Masquer, Services, Annuler, Coller, Réduire, Plein
écran belong to the platform, not to the table.

**A menu item does not act; it names an action.** The click comes back as the
action's id, and the front end runs it through the same rules a key press goes
through. So there is one path to Fetch, whether it was reached by `⌘F`, by the
palette or from the Dépôt menu.

**Only a binding with `Primary` becomes an accelerator**, and that is the
subtlety worth the entry. A menu accelerator is answered by the window system
*before* the web view sees the key. Registering `⇧?` up there would mean nobody
could type a question mark into a commit message. `⌘F` is meant to fire while
you are typing (KEYMAP.md) so it belongs in the menu; the sheet's `?` does not,
and its item carries no accelerator — it still opens on a click, and the key
still works, through the front end, which is the layer that knows where the
caret is.

**The greying is coarse on purpose.** An item is disabled when the *screen*
cannot do it — no repository open — not when a fetch happens to be running. The
front end sends the bar again when a repository opens or closes, and compares
what it is about to send to what it sent last. The alternative is rebuilding the
menu bar on every state change, including while one of its menus is pulled down.

`Platform::native_menus` is where SPEC §9's table row now lives, and the command
refuses on a platform that answers no: Linux has everything in the window, and a
GTK menu bar built anyway by a caller who forgot would be a second place to find
the same commands. One consequence is visible for a moment at launch — Tauri
puts its own English default menu up until the front end has booted and sent
ours. The alternative is a skeleton menu in Rust, which is the second list this
whole design exists to avoid.

### 2.50 M9, seventh slice: the keymap, reassignable

SPEC §11 asks for it in three words — "keymap réassignable" — and the design was
settled two slices ago: a settings screen over the table, not a rewrite. What
this slice adds is the difference between the table and what the user chose.

**Overrides are stored, never the keymap.** `Settings::keymap` is a map from
action id to binding, holding only what was changed. Writing the whole keymap out
would freeze it: an action added in a later version would arrive bound to nothing
for anyone whose `settings.toml` predates it, and a binding this app no longer
has would be answered forever. The stored spelling is the front end's own —
`Shift+Primary+N` — because `Primary` is the point: a keymap written on macOS
and carried to a Linux machine must not arrive spelled `⌘`.

**Nothing in Rust reads the string.** What a binding may be — which keys are
free, which are taken, which belong to movement — is knowledge the table has, and
the table is in the front end. `set_binding` stores what it was told.

**Everything that answers or prints a binding goes through `binding(action)`**,
which is `app.keymap[id] ?? action.binding`. That is what makes one change reach
four readers at once: the key handler, the palette, the `?` sheet and the macOS
menu bar. The bar is watched on what it would *show* rather than on what is open,
so a reassignment moves its accelerator without anyone calling it.

**The screen listens on the window, not through a text box.** The way to say
which keys you want is to press them, and half the interesting bindings — `⌘,`,
`⌘.`, `⌘W` — are ones a text field would swallow or the window would act on. The
handler is registered in the capture phase for that reason, and removed when the
screen goes away: a listener left behind would answer keys for a row nobody can
see.

**What is refused is what could not answer.** `dispatch` consults movement before
the table, and it does so whenever no primary and no alt are held — Shift alone
counts as bare there. So `j` is refused, and so is `⇧G`: both would look assigned
in the settings screen and fire never. A binding another action already answers is
refused with that action's name. Everything else is allowed, `⇧?` included — a
bare key not firing while you type is the documented rule, not a defect.

Two defects the slice found by making them reachable:

* **`hint` could not print `Alt`.** Nothing in the table used it, so `⌥⌘F` came
  out as `⌘F` — a hint naming a key the app does not answer, which is the exact
  failure this table was built to end. It prints in the platform's order now,
  `⌥⇧⌘` on macOS.
* **A disabled `button.link` looked enabled.** `button:disabled` and
  `button.link` have the same specificity, and the link rule comes later in the
  file, so it won on source order — every disabled link in the window, "Effacer"
  on the History filters included, read as one you could press.

And one caught on the probe page, which is the §2.31 lesson met again: the row
being pressed used the accent *fill*, which took the key button's colour with it
and printed "Appuie…" in accent on accent. It lifts to the hover surface instead.
A filled row cannot hold a control that has a state of its own.

### 2.51 M9, eighth slice: which empty it is

SPEC §11 asks for "états vides et d'erreur partout", and every list already drew
something when it was empty. What none of them did was say **which** empty it
was, and that is where an empty state stops being useful and starts being wrong:

* **"Aucun commit" under a filter reads as an empty repository.** The filter row
  is above it, folded on some screens, and small on all of them. History now
  distinguishes three: nothing matches (with the way to lift it in the sentence),
  a repository whose first commit has not been made, and a genuinely empty walk.
* **A filtered library offered "Ajouter un dépôt local".** The repositories were
  still there, one word away; the offer answered a question nobody had asked.
* **A branch tree with a count of zero and no rows reads as a tree that failed to
  load.** It says the branch will be born with the first commit.
* **The journal said "Aucune commande"** to somebody who had gone looking for it,
  and left them unsure whether it records anything.

Behind them are two of SPEC §13's mandatory edge cases, which the Rust side has
handled since M2 and the window had no words for.

**`HEAD`'s shape crosses the wire as a state now, not as a label to be sniffed.**
`RepoSummary.head_kind` is `branch`, `detached` or `unborn`. The front end was
reading `head.startsWith("detached")` in two places, which is one branch name
away from being wrong — and `detached-head-fix` is a branch somebody writes while
fixing exactly this. There is a test with that name in it.

**A commit on a detached `HEAD` warns.** `repo.rs` has said since M2 that
`Head::Detached` "has to warn before a commit is made here (M5)", and M5 never
built it: the commit is made, no branch moves, and it is reachable by hash alone
until something points at it. Not blocked — it is a legitimate thing to do — and
not painted as an error either, which would teach people to fear a state they may
have chosen. It is a `--warning` line under the box. Amend in a repository with
no commit *is* blocked, with the reason.

### 2.52 M9, ninth slice: giving Tab back to the browser

Board 09 counts the tab stops of each screen — eleven on Working Copy, seven on
History, six on Repositories — and states the rule under them: *une zone = un
arrêt*. Tab does not cross a list's forty rows; it enters the zone on the row the
keyboard is already on.

What shipped in the third slice answered `Tab` from the key table, cycling the
three zones, and called `preventDefault` on every press to do it. The
consequence took a while to see and is worse than the thing it implemented:
**no button in the window was reachable from the keyboard at all.** Not Fetch,
not the commit box's toggles, not "Retirer" — the browser's own tab order was
being swallowed on every key press for a feature that moved a highlight.

So `Tab` is not in the table any more, and that is the decision rather than an
omission. The browser walks the stops; the markup declares them:

* **A list is one stop.** Its container carries `tabindex="0"` and a
  `data-zone`, and entering it tells the store which zone the keyboard is in —
  so `j`, `k` and `⏎` act on the list Tab just entered.
* **`1` `2` `3` move the native focus too**, in `App.vue`. They are a shortcut
  *through* the tab order, so the next `Tab` has to carry on from where they
  landed. It is the one piece of DOM the shell touches, and it is here because
  focus is the browser's to own — a principle this codebase already wrote down
  when `/` had to put the caret in a filter.
* **A row's own actions are stops only for the row the keyboard is on.** Not
  hidden from Tab entirely, which would make them mouse-only, and not all at
  once either — three per row over forty rows is a hundred and twenty presses to
  cross a file list. This is the shape board 09 draws: the list, then its current
  row's controls.

Where this build differs from the board's count, it differs deliberately. The
sidebar's three screen buttons keep their own stops: `j`/`k` in that zone walk
the *branches*, so nothing else would reach them. And the diff's hunk actions
stay in the order because board 09 assumes `⌥S`/`⌥D` reach them and those do not
exist yet — a control reachable by the mouse alone is worse than one stop too
many.

Untestable in jsdom, which implements no sequential focus navigation: the tests
assert the declaration — which elements are stops, which are not, that entering
one agrees with the state, that `1` `2` `3` move focus. The walking is the
browser's, which is the whole point.

### 2.53 The sixteenth defect, closed: a folder can be dropped again

M3 took a folder dropped on the window; the port to Tauri did not carry it over,
and the library was left with exactly one door — the platform's open panel,
which on macOS does not show `/var/folders/…` and therefore could not reach a
repository built under `$TMPDIR`. The fixture script moved to `~` because of it.

The webview's event, not the DOM's: a browser drop hands over file *contents*,
and what is wanted is the path on disk. Tauri answers with paths, which is why
this is not `@dragover`/`@drop` on the shell. Folders are added one at a time and
in order, so the first one that is not a repository says so and the rest still
arrive.

`dragDropEnabled` is now written out in both window configs rather than left to
its default. It is not decoration: the same switch disables HTML drag and drop
*inside* the page on macOS and Windows, and SPEC §11 asks for repository groups
reorderable by drag. Whoever builds that will have to reconcile the two, and the
line in the config is where they will find out.

### 2.54 The window that said `undefined @ undefined:undefined`

The first launch after M9's last slice opened on a white page with one line on
it: `undefined @ undefined:undefined`. Two defects behind it, and the second is
the one worth the entry.

**The module never loaded.** `drop.ts` added the first import of
`@tauri-apps/api/webview`, and Vite pre-bundles a dependency the first time it
sees an import of it — a module imported by a file written *after* the dev server
started is discovered late, and until the server has re-optimised its URL 404s.
In a browser that is a red line in a console nobody has open; in a web view it is
a window that shows nothing. Every Tauri module the front end imports is named in
`optimizeDeps.include` now, so a cold start prepares all four — and the list
doubles as the record of what the front end asks of Tauri.

**The guard that exists to explain a start-up failure could not explain this
one.** `index.html` registers an `error` listener before the module loads,
precisely so a failure to load *it* is caught. It knew two shapes — a script that
threw, and a script that ran and broke — and read `e.message`, `e.filename`,
`e.lineno` for the second. A resource that fails to load has none of the three:
it is a plain `Event` whose `target` is the tag that failed. So the one screen
whose whole job is to say what went wrong printed the word "undefined" three
times. It names the tag and its URL now, which is what the user had in front of
them and could not read.

Verified the way §2.31 verifies anything: a probe page with the old expression
prints `undefined @ undefined:undefined` against a missing module, and the new
one prints `SCRIPT n'a pas pu être chargé : …`.

### 2.55 Three points of design, and one of them moves a window control

Reported from the running app, and the third is the interesting one.

**The icon was a borrowed character.** The button back to the repositories drew
`◧`. A character's bearings belong to the font: it sat left of centre and half a
pixel high in Chrome, and would sit somewhere else again in another web view.
`Caption.vue` already states the rule for the window buttons — *drawn, not
borrowed*, 16px grid, 1.5px stroke — and this is the same rule applied to the
same bar. Measured on a probe page (§2.31) before and after: 9.08/9.09 left and
right, 4.58/5.42 top and bottom, against a shape whose geometry we now own.

**The shortcut hints came off the buttons.** Board 02 prints `⌘F` beside Fetch
and `⌘K` beside Rechercher, and M9's first slice made them true. They are in the
tooltip now: three places print every binding — the `?` sheet, the palette, the
macOS menu bar — and a fourth copy in the row where width is scarcest was noise.
Still read from the table, so a reassignment moves the tooltip too, and the test
that pinned the printed hint now pins the tooltip and asserts the label is bare.

**The traffic lights are centred, which cost a height.** They sat about eight
pixels above the centre of the topbar. Tauri places them from
`trafficLightPosition` — and only at window creation: there is a
`set_traffic_light_position` in the runtime trait and nothing public that reaches
it. So the position is fixed once, and a topbar whose height changes between
screens cannot be aligned with them on both. Board 06 asks for exactly that: 40px
with no repository open, 48px inside one.

The rule that resolves it: **a bar the window system draws into does not change
height.** On macOS the topbar is 48px on every screen; on Linux, where the app
owns the whole bar, board 06's reduction stands. `y: 24` is half of 48, which is
what tao's inset arithmetic works out to — it resizes the title-bar container to
`button height + y` and pins it to the top, so the button's top lands at `y`
minus its own offset inside a standard 28pt container.

### 2.56 The repository tabs

Board 02 draws no tab strip: the topbar names the repository you are in, and the
way to another was the Dépôts screen. That is a detour for the thing people do
most — two or three repositories open at once — so this is a deliberate addition
to the board, asked for and recorded in DESIGN §6.

**A row of its own, under the topbar.** The topbar's empty middle is the
window's drag region on macOS, and tabs filling it would leave nowhere to take
hold of the window. The strip scrolls sideways rather than shrinking its tabs:
twelve unreadable stubs are worse than eight readable ones and a scroll.

**The tab is added by `openRepository`, not by the caller.** Every way in — a
click on a row, a folder dropped, the palette, `⏎` on the keyboard — leaves a
tab, because none of them has to remember to.

**Closing the tab you are on goes to its left-hand neighbour**, which is the tab
you were on before it more often than not; closing the last one goes back to the
list, the only honest screen with no repository open. Closing one you are *not*
on leaves the window exactly where it is, screen included. Each of those is a
line in `tabs.test.ts`, because a strip of tabs that guesses wrong here is
infuriating in a way no other bug is.

**The backend lets go.** `close_repository` drops the handle from `AppState`'s
map. It is a `gix` repository and a lock, not a cache — nothing is lost and it
is rebuilt on the next open — and without it a session that visited twenty
repositories held twenty of them for the life of the window. Tabs are what make
visiting twenty ordinary. A write in flight keeps its own `Arc`, so closing
during one is safe.

The name travels with the path in the tab, rather than being looked up in the
library each time: "Retirer de la liste" while its tab is up would otherwise
leave a tab with nothing to show. (It closes the tab too — a way back to
something you have just said you were done with is not a kindness.)

Two things it deliberately does not do yet: the strip is not restored across
restarts — the window opens on Repositories, always (§2.28), and restoring five
tabs would mean five status walks before the first screen — and switching does
not remember which screen you were on in that repository. Both are additions
this shape leaves room for.

Found by the fake backend, which refuses a command it has not been taught:
`void api.closeRepository(path)` left an unhandled rejection, and `main.ts`
draws one of those over the whole window. A backend that could not let go of a
handle is not a reason to lose the window you were working in; it is a line in
the log.

### 2.57 The icons, in one place

`Caption.vue` has stated the rule since M6b — *drawn, not borrowed*, 16px grid,
1.5px stroke — and only the window buttons obeyed it. The sidebar drew `◱` for
the working copy, `⌸` for History (an APL symbol, which reads as a table), `⌥`
for Stashes (the macOS Option key, which means something else entirely), `⚙` for
the preferences, `▤` and `⊘` in the library, and `▾` for a disclosure triangle
the fallback font drew as a *dot*. Branches had no icon at all, and neither did
the groups a `/` makes.

`Glyph.vue` holds every shape the window draws, and one CSS rule strokes them.
One component rather than a path per call site, for the reason the bindings are
one table: a glyph copied into four files is a glyph that will differ in four
files. The topbar's own icon and the tab strip's cross, both written inline the
day they were added, are entries in the set now.

Right angles wherever a shape tolerates them, and no further: a clock is round,
and so are the three commits on the branch glyph — a square commit reads as a
stop. The gear is the one shape that lost: at 16px with a 1.5px stroke a gear is
a blob, so preferences are two sliders.

The test names the characters that were there — `◱ ⌸ ⌥ ⚙ ◧ ▾ ▸ ▤ ⊘` — and fails
if any of them comes back to either left-hand bar. That is a narrower rule than
"no pictographs anywhere", deliberately: `↑8 ↓2` on the Push button, `✓` in a
checkbox and `⚠` beside a conflict count are typography doing a typographic job,
not a picture standing in for an object.

### 2.58 The watcher nobody had ever called

Reported from use, and it is the same shape as §2.8's theme defect: the code was
written, tested, and never asked. `omagit-git/src/watch.rs` has done the hard
half since M2 — debounced at 150 ms, `.gitignore` respected so a build writing
into `target/` costs nothing, `.git/index.lock` ignored so we do not react to
Git reacting to us, and every change set labelled by what it invalidates — and
no line of the application had ever constructed a `Watcher`. The window
refreshed when clicked, which is wrong every time you touch a terminal.

**The watch lives on the repository handle.** `Open` holds it, so it starts when
a repository is opened and stops when the handle is dropped — which is what
closing a tab does (§2.56). No registry to keep in step, and no thread outliving
the screen that wanted it.

**What crosses is the invalidation, not the event.** `Changed { path, status,
refs }`: the front end has no business knowing `.git/index.lock` exists, only
that its status is out of date. A branch that moved re-reads the tree and does
not walk the status.

**Three rules on the window's side, all of them about not re-reading.** Only the
repository on screen — every open tab is watched, because the watch travels with
the handle, and a change elsewhere is not something this screen can show. Not
during our own write, because `settle()` re-reads at the end of one anyway and a
`git commit`'s burst would otherwise land in the middle of it. And status or
refs, never both by default.

**`AppState` holds an announcer, not an `AppHandle`.** The handle is generic
over the runtime and this state is not — and that genericity is what makes the
bridge testable: `tests/watching.rs` builds a real Tauri application on the mock
runtime, opens a real repository through `AppState`, writes a file, and waits
for the event. It is the one thing neither suite could reach, and it is exactly
the kind of wiring that compiles, type-checks and silently does nothing. Which
is what it did for seven milestones.

### 2.59 One question, one answer, on the Preferences screen

Four things reported from use, and three of them were the same thing.

**The sources did not look like anything you could press.** Five lines of plain
text with a hover: true, and invisible at rest. **And the catalogue below them
looked like a second, independent question** — "Suivre le système" and a named
theme could both appear chosen, which the model does not allow: they set the
same `ThemeSource`, and naming a theme is what turns following off. **And their
labels did not line up**, because the swatches carried a border and the rows did
not.

All three answered by making it what it is: **one radio group**. A mark on every
option — sources and themes alike — exactly one of them filled, and the border
gone from the swatches, which is what let their labels sit a pixel to the right
of the rows'.

The mark is drawn on the same 14px box as board 01 §05's checkbox, square,
because DESIGN §1 says everything is. What tells the two apart is what fills
them: a check means "this one is in", a solid block means "this one, and
therefore not the others" — with the group carrying the rest of the meaning,
since only ever one is filled.

**And "Automatique" said "ce que cette machine offre de mieux",** which names
nothing anyone can act on. It says what it resolves *through* now — Omarchy,
then the system, then the embedded theme — and the line above it already says
what that came out as.

### 2.60 Internationalisation: English by default, French beside it

Asked for whole: two languages at the start, English the default, the system's
language chosen at first launch, changeable in Preferences, and a third language
added by adding a file.

**English is the reference.** `web/src/i18n/languages/en.ts` declares the keys
and every other catalogue is typed against it, so a key missing from a
translation is a compile error and a key that no longer exists is one at its call
site. A catalogue that misses one anyway — a file somebody else wrote, half
finished — falls back to English rather than showing a raw key, which is what
makes "drop a file in" safe.

**Adding a language is adding a file.** `import.meta.glob` reads the directory;
nothing imports a catalogue by name. A list of imports would be a second place to
remember, and the requirement was explicit about this.

**`t()` is reactive.** It reads the language through Vue, so every string on
screen changes in the same tick the choice does — no reload, no re-read, and the
macOS menu bar is rebuilt because its signature includes the language.

**Plurals are `Intl.PluralRules`'.** One catalogue entry per category. What it
replaces was `n > 1 ? "s" : ""`, which knew exactly one language — and got French
wrong at zero, which is singular there and plural in English. A language with
four categories needs four entries and no code.

**The backend stopped wording things**, which is the half that took the thinking:

* what a command fails with is either *`git`'s own words* — shown verbatim,
  because they are the truth and translating them would be inventing — or one of
  ours, which crosses as `omagit:<key>|<arg>` and is worded by the window. The
  prefix is the whole of the distinction;
* the same marker carries the three conflict sides `omagit-git` cannot name — a
  cherry-picked commit, a reverted one, an applied patch. A key is a *name*, not
  a translation, which is why a crate with no locale may hold one: the same
  discipline as `Head::Detached` being a variant rather than the string
  "detached";
* "Open in the editor" answers with a fact — the program, what `core.editor`
  held, and why they differ — instead of a sentence;
* the macOS menu bar's own items (Quit, Paste, Full screen, and the menu titles)
  are sent from the catalogue with the actions, and an action's `label` is a key
  now, read by four things through `labelOf()`;
* and the library's default group name is a key *in the stored file*. A group
  name is data — SPEC §11 lets it be renamed — so it cannot be translated on the
  way out or a rename would be undone at every launch, and stored as a word it
  would be frozen in the language of the day. Stored as a key it is said in the
  language in force, and the moment somebody renames it the new name is a plain
  word, shown verbatim.

Deleted on the way: `omagit-app/src/time.rs`, a second date formatter with French
month names written out by hand, which nothing had called since the port to
Tauri. `format.ts` builds its formatters from the language in force instead.

The one place a French window still shows English is `git`'s own output, and that
is deliberate.

### 2.61 A release pipeline, which is not the gate that was deleted

CLAUDE.md says there is no CI, and SPEC §3 rule 8's amendment says why: on a
private repository with one developer, a gate on every push verifies the work of
somebody who already knows whether it compiles, at a real cost. That reasoning is
untouched, and the gate is still `scripts/check.sh`, run by hand.

**A release build is a different job.** It runs on a tag, from a clean checkout,
on a machine nobody has been editing, and what comes out is the thing other
people install. It protects somebody — whoever installs it — which is the test
CLAUDE.md sets for infrastructure: *name who it protects*.

Linux only for now, which is what was asked and what SPEC §11 puts in M10: macOS
needs an Apple developer account, signing and notarisation, and a pipeline that
cannot sign is a pipeline that ships an unopenable `.app`.

**One door.** `scripts/release.sh` is what the workflow runs, so a failure is
reproducible on a laptop instead of only in a log, and CI has no build knowledge
of its own.

**The one check a keyboard cannot do**: that the tag and the two manifests agree
about the version. A file named `0.2.0` whose About box says `0.1.0` is a release
nobody can reason about afterwards, and it is checked before anything is built,
so the failure costs seconds.

Two choices worth the ink. The runner is `ubuntu-22.04` and not the newest:
the glibc a binary is built against is the oldest one it will run on, and an
AppImage built on 24.04 refuses to start on a stable desktop. And the release is
created as a **draft**: the notes are written by a person, and a tag pushed by
mistake should not become an announcement.

### 2.62 The watcher was watching us read

Reported from use, and the report was precise enough to be the diagnosis: *at
every one of these log lines, the branch column reloads — the window jumps all
the time*. The log lines were the ones §2.58's watch produces on the way round:
one diff read, then another, exactly two seconds apart, on a repository nobody
had touched.

**`notify` asks inotify for `IN_OPEN`.** That is the whole of it. On Linux an
`open(2)` under the watched tree arrives as an event exactly like a write does,
and the process opening the most files under that tree is omagit. Reading a
status opens `.git/index` and walks the work tree; each open came back as a
change; the change invalidated the status; the status was read again. The window
had been re-reading itself, without pause, since the watch was first called.

**Two seconds, and that number is in the code.** The debounce is 150 ms, but a
stream of events with no gap in it never reaches a debounce: it is held to
`MAX_HOLD` and flushed there. So the loop ran at the ceiling — 2.000 s, visible
in `~/.config/omagit/omagit.log` as a metronome. The exclusion check amplified
it rather than damping it: asking whether a path is ignored opens `.gitignore`
and `.git/info/exclude`, and those opens are events, which are classified, which
ask again. Four seconds of `inotifywait` on the repository produced 694 KB of
`OPEN .git/info/exclude`.

**The fix is one question the classifier never asked: did anything *change*?**
`changes_something` keeps everything that is not an access, and of the accesses
keeps only a close-after-write, which is a write finishing rather than a read
happening. A backend that does not say — `EventKind::Any` — is kept, because a
watch that guesses wrong here should guess towards re-reading.

**Why no test caught it.** `tests/watch.rs` asserts on outcomes, which was the
right call, but every assertion made a change and waited for it; none of them
read the repository and waited for *silence*. One does now, and it has to wait
past `MAX_HOLD` to mean anything: the 900 ms silence the other tests use would
have passed with the bug in place, because the flush was still 1.1 s away. This
is also a Linux-only defect — FSEvents reports no such thing — which is the
blind spot risk 7 names, seen from the other side.

**And the flicker itself was a second bug, worth fixing separately.** `Async<T>`
has carried `loading { previous }` since the port, and nothing had ever used it:
every re-read replaced the pane's contents with a "Lecture des branches…" and
filled it back in. Even with the loop closed, a commit made in a terminal would
blink the sidebar. `shown()` is what panes read now — the answer, or the one it
is in the middle of replacing — so a re-read is invisible unless it changes
something, and a pane is blank only when it has never had an answer.

### 2.63 GPL-3.0, and why the licence was the decision

The distribution model — open source, free on Linux, paid on macOS — turns on one
line that was already in the repository and pointing the wrong way:
`Cargo.toml` said `license = "MIT"`, with no `LICENSE` file to go with it. MIT
lets anybody take this code, sign it with their own Apple account, and ship a
**closed** fork; a version published under it stays under it for ever. So the
licence had to be settled before the repository went public, not after.

**GPL-3.0-only.** It permits exactly what the model needs and forbids exactly
what it cannot survive: selling a build is legal and ordinary — Ardour has done
it for fifteen years — while a proprietary fork is not. What is sold on macOS is
the *build*: signed, notarised, self-updating, which is the one thing nobody can
make for themselves without an Apple developer account. Compiling it yourself
stays possible, and passing your copy on stays permitted. That is the deal, and
it is the reason the model is honest rather than a paywall with a source mirror
behind it.

**Checked, not assumed**: nothing in the dependency graph is GPL-incompatible.
Every crate offers MIT, Apache-2.0 or another permissive licence — and the
original OpenSSL terms, which are the classic incompatibility, appear nowhere.
`cargo deny` re-checks it at every gate; our own crates are skipped there
(`private.ignore`), because that list is about what we redistribute from others.

Two consequences worth writing down before somebody rediscovers them:

* **The Mac App Store is out.** Its sandbox forbids spawning an external binary,
  and SPEC §8 makes `git` the only thing that writes. Direct sale and
  notarisation, then, with a merchant of record carrying the VAT.
* **A licence check in the app is a convenience, not a lock.** The source is
  public, so any check can be removed in ten minutes. What it is for is making
  paying easy for people who want to; designing it as a defence would be
  designing for the people who will never pay anyway.

### 2.64 The folders, and the switch that would not let them be dragged

SPEC §11 asks for "groupes utilisateur repliables, réordonnables par drag &
drop, persistés", and the model has carried one per repository since M3:
`Library::move_entry`, `add_group`, a `collapsed` flag, all written, all
serialised, none of them ever called. §5 recorded it as **seen but not made** —
the list drew the groups the first repository created and there was no way to
make a second. This is that, built.

**The folders are read from the folders, not from the rows.** The list used to
group its rows by their `group` index and take each header's name from the first
row that named it, which works exactly until a folder holds nothing — and a
folder holds nothing the moment it is made. The button would have appeared to
do nothing. So `repositories` now answers with a `LibraryView`: the folders, and
the rows, from **one** read. Two commands could be taken a moment apart, across
a rename or a creation, and a row names its folder by *index* — off by one and
a repository is drawn under a folder it is not in. A test on each side holds the
shape: `tests/library.rs` for the command, `folders.test.ts` for the window.

**Pointer events, because the window cannot have HTML5 drag and drop.** §2.53
ends with a note: `dragDropEnabled` is what makes a folder dropped from the
Finder arrive as a *path* instead of as file contents, and the same switch turns
off drag and drop inside the page on macOS and Windows — "whoever builds that
will have to reconcile the two, and the line in the config is where they will
find out". The reconciliation is `filing.ts`: a press, a 4px threshold before it
counts as a drag at all — every row here is also a thing you select — a pointer
capture, and the insertion line chosen by arithmetic rather than by the DOM.

That arithmetic is a file of its own and not a lump inside the component,
because it is the part that can be wrong in a way no screenshot shows: a line
drawn one slot off files the repository where the user did not point. jsdom
measures nothing — every rect it returns is zero — so a test driven through the
DOM would assert on identical boxes. `filing.ts` takes numbers and returns the
slot, and `filing.test.ts` asks it about the seam between two folders, about a
folder with no rows to draw a line between, and about the space above and below
the whole list.

**Deleting a folder keeps what was in it.** `remove_group` was written in M3 as
"remove a group and everything filed under it", with a comment telling the
caller to confirm first. Building the caller changed the answer: a repository
can be in exactly one folder, so a folder deletion is the only gesture that can
lose several entries at once — and taking *one* entry out deliberately already
asks a question. The entries now move to the neighbouring folder, and the
confirmation's whole job is to say so, because "Delete" on a folder holding
eight rows reads as though it takes them. `Library::load` moves a damaged file
aside rather than replace it for the same reason: this list is arranged by hand.

**And the last folder stays.** A repository has to be somewhere. The button is
drawn disabled with the sentence in its title rather than hidden — a control
that disappears leaves the question of whether it was ever there — and the way
to be rid of the last folder is to rename it.

**Two ways in, because one of them is a pointer.** Board 06 draws the drag and
nothing else, and a drag is unreachable from a keyboard. The card gained a
folder `select`: reachable with Tab, legible without hovering anything, and the
obvious shape for "one of these" — which is the lesson §2.59 had just learned
from five rows that only looked like buttons. `New folder` is in `ACTIONS` too,
with no default binding, so it reaches the palette and the File menu and can be
given a key from Preferences.

**Two glyphs, and both contradicted a comment I had written.** `folder-plus`
was drawn with the plus outside the folder, on the theory that inside it would
meet the tab's fold and read as one scribble. Four constructions probed at 96,
32, 16 and 12px say the opposite: outside, the folder has to be cut short to
make room and stops being a folder, which is the half that carries the meaning.
The same probe caught the rename pencil at the 10px a row's actions use — a
diagonal at 10px is a slash — and the header's actions are set at 12.

**Two things reported from use, once it was in front of somebody.** Dragging a
repository highlighted every label the pointer crossed, and the rows were not
indented under their folder.

The selection is a WebKit matter and could not be reproduced here: a real drag
across the real markup in Chrome, driven through the browser rather than by
dispatching events, selects nothing — `user-select: none` on the body is
honoured. WKWebView is what the window is on macOS, and WebKit's own selection
code has always read `-webkit-user-select`, which the rule did not carry. Three
defences now, each correct on its own: the prefixed property beside the standard
one, `preventDefault` on the row's pointerdown — measured in that same real
browser: the drag still produced exactly one `click`, because `click` is not one
of the compatibility mouse events a prevented `pointerdown` suppresses — and any
selection cleared at the moment a drag begins.

The indentation was simply missing, and DESIGN §6 had already ruled on it while
M3 was being built: "indentation is structure, and §1 puts structure on value,
borders and position — so it applies everywhere or nowhere. Everywhere." The
26px is the header's own 10px of padding plus its 10px chevron plus the 6px gap,
so a repository's icon lands in the same column as its folder's and the chevron
keeps a gutter to the left. That is what a tree looks like, and it is why the
number is written as its parts rather than as 26.

And one plain defect, found by the probe and not by the tests: the rename box
was an `<input>` with no `type`. Every base rule in `style.css` is written
`input[type="text"]`, which does not match an input that declares none, so the
box came out with the browser's own white ground in the middle of a dark
window. The codebase's other twenty-one inputs all name their type; this one
now does too.

### 2.65 Eight words the window still said in French, and the guard that could not see them

You reported one of these at the time: "le terme journal reste en français en
bas et quand on l'ouvre". The status bar's was fixed. The error band's was not,
and nor were seven others — `Raccourcis`, `Commandes` and `Raccourcis clavier`
on the `?` sheet, `Annuler` in every confirmation, `Palette de commandes`,
`Largeur de la colonne` on each splitter, and a whole paragraph of French on the
Preferences screen whose catalogue key **already existed and was called by
nobody**. The same is true of `sheet.title`, `sheet.commands` and
`branches.row`: translated, typed, unreachable.

Two guards were supposed to make that impossible and neither could see it. The
first looks for accented letters; not one of those eight has one. The second
looks for two French function words inside a double-quoted string;
`Raccourcis` is one word, and `Largeur de la colonne ${pane}` was in backticks,
which it did not read at all.

The lesson is that **the vocabulary was the wrong thing to check**. What is
wrong with `Commandes` is not that it is French — it is that it is *literal*: a
word a template prints from itself is a word no language can reach, and it would
have been just as wrong in English. So the third guard checks the shape. Every
text node and every `title`, `aria-label` or `placeholder` written as a literal
rather than bound is a failure, against a list of eight exceptions: `omagit`, a
Git verb the catalogue's own preamble keeps untranslated, `Esc`, and an example
remote URL. Anything with no letters in it passes — a glyph is not a word.

The two older guards keep their jobs, since a French string reached through `t`
would still pass this one. Both were widened as well: backticks are read now,
and `de` joined the function words — without it `Largeur de la colonne` scored
one and stayed invisible.

**A fourth place, found while building §2.66.** `write(label, …)` puts its first
argument straight into the status bar, and eleven of those labels were French
sentences assembled with a template literal — `Fusionner ${branch}`,
`Basculer sur ${name}`, `Corriger le commit`. Every one of them had been on
screen since M5 or M7. None of the three guards could see them: no accents, one
French function word apiece, and not a literal in a template. The fourth checks
the same shape as the third — every busy label goes through `t` or `count` — and
an English literal there would be exactly as wrong.

### 2.66 The commit-message agent, and the key that is not stored

Asked for as "the user types an Anthropic, OpenAI, Mistral or Gemini key into
Preferences and that unlocks generated commit messages". Two questions were
about to go back — where a secret is kept, and how much context is worth its
privacy cost — when the better answer arrived as a screenshot of Tower:
**detect an agent already on the machine.**

It is better for reasons that are structural rather than a matter of taste.
**There is no secret**, so no keychain, no `credentials.toml`, no plaintext
token, and `settings.toml` goes on being a file the user edits by hand.
**There is no HTTP client**, so no TLS stack, no `cargo deny` churn, no wire
format per provider to keep up with. And it is the shape the app already has:
SPEC §8 says everything omagit writes it writes by running `git`, a command you
could have typed, visible in the journal. This is that again with a different
program — which is why the runner is `omagit-git`'s, generalised, and not a
second one.

**Rules 2 to 5 of SPEC §8 are not about Git.** A controlled environment, a
process group that can be signalled, a deadline and `stderr` passed through
unedited are true of anything this app starts. Rule 1 — machine formats — is
Git's alone. So `cli::Invocation` stopped holding a `&Git` and started holding a
program and a journal, and `cli::program()` is the door. The part that must not
be written twice is the signalling: `git` delegates to `git-remote-https` and
`ssh`, an agent delegates to a Node wrapper, and a kill that reaches only the
leader leaves both running.

**The patch is assembled, not shelled out for.** `git diff --staged` would have
been one line and would have broken the rule `ops/mod.rs` states — "gix does
every read; these do every write". `patch::build`, which staging by hunk already
depends on and which is tested where a mistake destroys work, serialises each
staged file instead. It is also strictly better: a binary file is *named* rather
than skipped in silence, because "the icon changed" is part of what the commit
is even when its bytes are not worth sending.

**Three things were learned by running the binaries rather than reading about
them**, and each changed the code:

* The prompt cannot be an argument. `claude`'s `--disallowed-tools` is variadic
  and swallowed every word of it — the agent was handed a prompt as a list of
  tool names and asked nothing at all. Everything goes on stdin.
* Being on the `PATH` is not being installed. `codex` was on it and answered
  every invocation with a Node stack trace: its wrapper could not find its own
  vendored binary. So detection is "it answered `--version`", and an agent that
  did not is drawn disabled with the reason — the rule §2.59 settled for theme
  sources, for the same reason.
* A tool with no terminal can simply not return. `gemini` took a prompt on
  stdin and produced nothing, indefinitely. The deadline is what turns that into
  a message instead of a button that never comes back.

**And one property nobody designed.** The agent is started *in the repository*,
so it reads the project's own `CLAUDE.md` before answering. Asked about a
three-line diff here, it came back in French, in this repository's voice, and
noted that nothing called the new function yet. An API key with a bare diff
could not have done either — which retired the second of the two questions
without it ever being asked.

The tools are denied all the same (`Bash`, `Edit`, `Read`, the rest). It is
being asked for a sentence; everything it needs is on its standard input, and an
agent with write access to the repository you are about to commit is a different
proposition from a sentence generator — one nobody agreed to by clicking
"Draft".

**Three things reported the moment it was used.** "Draft" did not say what the
button does, the icon could not be made out, and pressing it looked like nothing
happening.

The label is "Generate" now. The icon was a pen over two lines of text, chosen
against a sparkle on the argument that a sparkle means "new" and that borrowing
it was following a fashion rather than drawing a shape. The argument was about
the wrong thing: the glyph is 13px and a pen at 13px is a faint diagonal slash.
Four candidates probed at 96, 32, 16 and 13, in the button they actually sit in,
and the two stars are the only one legible at the last size. A convention that
is recognised beats a metaphor that is not.

And the third was the real one. The busy state *was* reported — `app.busy` put
"Drafting the message…" in the status bar — at the far bottom of the window,
while the eye is on the commit box, for the twenty-five seconds an agent takes.
The answer belongs on the control that was pressed: the button says "Writing…"
and carries a 2px bar along its own bottom edge, sweeping rather than filling
because an agent has nothing to report in between — the same answer, and the
same keyframes, as a `git fetch` counting objects with no total to count
against. It is disabled while it works but **not dimmed**: busy is not
unavailable, and a control greyed the way an off one is greyed says the wrong
thing about itself.

Two smaller decisions. The button is **not drawn at all** until an agent is
configured, because an affordance for a feature that is off is the dead code
SPEC §2 forbids wearing a button; `agent_chosen` exists so start-up can know
that for the price of a string, while `agents` — which starts a process per
candidate — is asked only when Preferences is looked at. And drafting does not
go through `write()`: that one calls `settle()`, which clears the lines picked
in the diff, and a draft is not a write. It has no business undoing a selection
somebody made by hand.

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
- **Commit messages drafted by an agent** (§2.66): a coding agent already
  installed on the machine — `claude`, `codex`, `gemini`, or a command of the
  user's own — asked in the repository, with no key stored anywhere and no HTTP
  client in the tree.
- **Folders** (§2.64): made, named, folded and persisted; a repository filed by
  dragging it — pointer events, since `dragDropEnabled` forbids the browser's
  own — or from a select on the card, which is the route a keyboard can take.
  Removing a folder keeps what was in it, and the last one stays.

The port is complete. What is left on those screens is named in §2.27 and
§2.25 — board 06's reordering and groups, and Git's history simplification,
which is what a path-filtered view would need to keep its gutter.

**M7 — branches and the network** (§2.32 to §2.34; the clone slice has no
section of its own — its reasoning is in commit `2e26d71`, and that gap is the
last of M7's). The sidebar's branch tree,
grouped on the first `/`, with tags and remote-tracking branches under it and a
"Merged" mark read from `git` rather than guessed. Switching (`git switch`, not
`checkout`, which guesses between a branch and a path), creating, deleting with
the two different questions that deserves. Fetch, pull, and push with
`--force-with-lease` and never a bare `--force`, each behind a cancellable
progress overlay fed by `git`'s own stderr, one operation at a time per window.
Merge and rebase, and `abort` as the way out of one that stopped. Cloning, with
a reachability probe run *before* the clone rather than four minutes into it.
Authentication is `git`'s own, through the platform's credential helper, and
nothing here ever waits for a person: `GIT_TERMINAL_PROMPT=0` and
`GIT_EDITOR=true` are set for every invocation, so a command that decided to ask
fails instead of hanging on a terminal that is not there.

**M8 — stashes and conflicts** (§2.35 to §2.37). The shelf: a screen of its own,
reading `refs/stash`'s reflog, with the preview of what one holds — including
the files an `--include-untracked` stash keeps in a third parent, which a
commit diff never looks at. Push, apply, pop and drop, each addressed by commit
because the index in `stash@{0}` moves the moment one is dropped.

Conflicts: the two sides named by their branch rather than by the pronouns,
which are only honest during a merge; whole-file resolution that knows a
"keep ours" over a file we deleted means `git rm` and not `git checkout`; board
07's dialog for the rest, answering conflict by conflict with Ours / Theirs /
Both over the markers `git` wrote, rebuilt byte for byte so line endings survive;
the file opened in the configured editor, or in the desktop's opener when that
editor lives in a terminal. And both ways out of a stopped operation, side by
side in the status bar: `continue` once the conflicts are settled, `abort` at
any time.

**M9 — finition** (§2.44 to §2.53). SPEC §11's transverse list, delivered whole:
a command palette that searches actions, repositories, branches and files and
runs what it finds; a keymap that is reassignable and stores only the difference
from the table; a Preferences screen for the theme, the density and the scale,
which nothing in DESIGN draws and which is therefore built from the vocabulary
the window already has; the `?` sheet, printed from the table that answers the
keys rather than written by hand; empty and error states that say *which* empty
they are; and the native macOS menu bar, whose Édition menu is what gives the
commit box an undo at all.

One table underneath all of it. `web/src/keymap.ts` holds every action, and four
things read it — the key handler, the palette, the sheet and the menu bar — so a
binding changed in Preferences moves in all four. Printing it is also what
audited it: the sheet found `Tab` documented since M3 and bound to nothing, and
its own binding written `Shift+/`, which no layout produces.

Keyboard navigation to board 09's model: three zones with `1` `2` `3`, `j`/`k`
inside one, `Esc` up a level — and `Tab` handed back to the browser, which is
what walks the window's stops. Answering it ourselves had left no button in the
window reachable from the keyboard.

**Verified on screen, not only in tests** (§2.38, §2.39). `scripts/fixture.sh`
builds a repository with the shape all of this needs — a history with a merge in
it, five branches, a divergence from a local "remote", two stashes, a working
copy holding seven states at once, and a merge that conflicts in three files —
and driving it by hand on both platforms found five defects the suites could not
see: invisible row actions taking their width, a status bar that wrapped a
sentence inside 22 pixels, every error crossing with its command in front of it,
a conflicted file with no diff at all, and two buttons on that diff that could
only ever fail.

## 5. Risks

Re-read at every milestone (SPEC §15).

> **The table below is stamped M4 and has not been re-read since.** Left stamped
> rather than quietly refreshed: a risk table that says M4 and means it is more
> use than one that looks current and is not. Read at the end of M9, and the
> pass is this note rather than a rewrite of the rows, because what changed is
> the ground under them rather than their wording:
>
> * **Risks 1 and 3 no longer describe this build.** Both are about GPUI — a
>   render thread to block, a vendored toolkit to be incomplete. The interface
>   is a web view (§2.20); `assert_off_render_thread` still guards every entry
>   point in `omagit-git` and still has not fired, and it now guards against a
>   Tauri command thread instead.
> * **Risk 4 is answered.** The graph shipped with M6 and the lanes stayed in
>   `omagit-git/graph.rs`, topology only.
> * **Risk 5 has come true in the only way that counts and is handled**:
>   omagit writes now — index, commits, discards, merges, rebases, pushes — and
>   every destructive one is behind SPEC §3 rule 7's confirmation, with the exact
>   command in the journal before it runs. `--force-with-lease`, never a bare
>   `--force`.
> * **Risk 7 stands, and is the one to re-read at M10**: nothing compiles the
>   other platform, and M9 added a whole file that only macOS ever executes
>   (`menu.rs`).
> * **Risk 6 is M10's, unchanged**: an Apple developer account, signing,
>   notarisation.

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

**Nothing but a person can drive the interface.** Not a defect in the app — a
limit in how it is verified, and the one that has cost the most. The suites run
in jsdom, which has no cascade and no layout, so every question of *shape* goes
to a probe page (§2.31): a fragment of markup with the real tokens, rendered
headless, measured or looked at. That catches geometry inside one component and
nothing about the window as a whole — an icon off-centre, a heading with the
wrong air around it, a control that vanished behind another are all found by
somebody opening the app and saying so.

The tooling that would close most of it: a **development-only bridge** in
`ipc.ts` that routes to `backend.fake.ts` when `window.__TAURI_INTERNALS__` is
absent, under `import.meta.env.DEV`. The front end would then run in an ordinary
browser — clickable, screenshotable, drivable — against the same fake the tests
already use, which is what keeps it from drifting from the real backend.

Its limits, so nobody expects more of it than it gives: no real Git, so
behaviour still needs the fixture repository; and Chrome is not WKWebView, so
what it shows is not what macOS ships. It would not replace looking at the real
window. It would mean far fewer round trips to get there.

**The three-column Working Copy does not fit below ~1100px.** The sidebar and
the file column are fixed widths and the diff panel has a floor, so under that
the diff header's right-hand content falls outside the window and is clipped.
DESIGN §4's answer is that below 1100px of usable width the detail panel becomes
a tab; that collapse is not built. What *was* a bug — the header's controls
drawn on top of the file path — is fixed: the path could not shrink and had no
clip, so its text spilled over them. It now keeps a floor and the stats clip
first (`tests/working_copy.rs`).

**Library groups could be seen but not made** — now **closed** (§2.64). They
are made, named, folded, filled by drag or from the card, and removed without
taking what was in them. What remains unbuilt of SPEC §11's sentence is
*reordering the folders themselves* by drag: `Library::move_group` and the
`move_group` command exist and are tested, and nothing in the window calls
them. Repositories move between folders, which is the half that was asked for;
dragging one folder above another is a second gesture over the same geometry
and has no caller yet.

A sixteenth, from M8, **half closed by M9** (§2.53): **the only way to add a
repository was the platform's folder picker.** A folder dropped on the window is
added again, which is the half that mattered — the Finder's open panel hides
`/var/folders/…`, so a repository built under `$TMPDIR` could not be reached by
the one button that adds one. What is still open is *typing* a path: there is no
dialog to type it into, and inventing one for this alone was not worth it while
a drop and the picker both work. What follows is the defect as it stood.

A fifteenth, from M8, now **closed by M9's first slice** (§2.44): `KEYMAP.md`
described a build that no longer existed. It is rewritten against
`web/src/keymap.ts`, and the bindings it lists are answered. What follows is the
defect as it stood.

**`docs/KEYMAP.md` describes a build that no longer exists.** It says the bindings live in `crates/omagit-app/src/actions.rs`, which
the port to Tauri deleted along with the rest of the GPUI interface, and it
lists M3's Repositories bindings — `j`/`k`, `/`, `1`/`2`/`3`, `Esc` — none of
which the web front end has. What the window answers today is the browser's own
tab order over rows that are buttons (§2.31), plus the keys a dialog binds while
it is open (§2.37). The whole of it is M9's, and until then the file should not
be read as a description of what ships.

A fourteenth, from M8, now **closed by M9's fifth slice** (§2.48): the rows are
`<div>`s, which is the shape `.file-row` and `.commit-row` already had, and the
Vue compiler's warning is gone with them. The library row had the same defect
and is fixed with it. What follows is the defect as it stood.

**The branch tree nests `<button>` inside `<button>`.**
Its row is a button so the keyboard can reach it (§2.31) and its three actions
are buttons inside that one, which is invalid HTML — the Vue compiler warns
about exactly this shape, and only stays quiet here because the actions are
behind a `v-if` it cannot resolve at compile time. Nothing is visibly broken:
Vue builds the tree through DOM calls rather than the HTML parser, so the
nesting survives where a parsed document would have flattened it. What it costs
is the accessibility tree — a button inside a button has no defined meaning for
a screen reader. The shelf's list (§2.35) does it the other way round; the
branch tree should follow when something else takes it apart.

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

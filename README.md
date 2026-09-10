# omagit

A graphical Git client for **Omarchy / Linux** and **macOS**, written in Rust,
with a web front end in a Tauri window. Inspired by Tower, deliberately simpler.

(It was rendered with GPUI until M6; `docs/ARCHITECTURE.md` §2.20 records why it
is not any more, and what that cost.)

## Run it

```sh
scripts/dev.sh
```

That is the whole of it: the script starts the front end's dev server, waits for
it, and then runs the app.

**Do not run the binary on its own in a debug build.** Tauri loads the dev
server rather than the embedded front end there, so `cargo run` with nothing
serving on port 5173 opens a blank window and says nothing about why. A release
build (`cargo build --release`) embeds the front end and needs no server.

`OMAGIT_LOG=debug scripts/dev.sh` turns up the logging. Front-end errors go to
the same log as the backend's, so a start-up failure is one file to read rather
than a devtools console to open.

## A repository to try it on

```sh
scripts/fixture.sh              # --conflict leaves it stopped in the middle of one
```

Builds a throw-away repository in `~/omagit-fixture` — where a file picker can
actually reach it; macOS's `$TMPDIR` is a `/var/folders/…` path the Finder hides
— with everything a screen needs to have something to draw: a history with a merge in it, five branches — one merged,
one nobody has touched since the spring — two tags, a divergence from a "remote"
that is a bare repository on the disk beside it, two stashes, a working copy
holding six kinds of change at once, and a merge that conflicts in three files.

It makes **two** repositories, and the reason is `git`'s: a merge refuses to
start when anything at all is staged, so the one that shows a half-filled index
cannot be the one that conflicts on demand. `atelier` is the working copy;
`atelier-collegue` has a clean tree and the same branches.

Rebuilt from scratch on every run, and it only ever deletes a directory it wrote
itself.

## The one gate

```sh
scripts/check.sh
```

`fmt --check`, `clippy -D warnings`, `test`, a release build, the front end's
own check, and `cargo deny`. Run by hand, when you decide to — there is no CI
and no pull request. Both existed and were removed, for the reason CLAUDE.md
gives: on a private repository with one developer they verified work nobody else
had written, and billed a macOS runner ten times the Linux rate to do it.

It compiles only **this** host's half of `omagit-app/src/platform/`, and the
macOS-only behaviours in `status` — APFS case folding, decomposed names — are
exercised only when it runs on a Mac. A green run says the milestone is green on
this machine and nothing about the other one. The project lives on both, and
each finds what the other broke the next time it is used.

## Documentation

| File | Authority over |
|---|---|
| `docs/SPEC.md` | everything not covered below |
| `docs/DESIGN-TOKENS.md` | token names and derivation formulas |
| `docs/DESIGN.md` | visual usage and interface states (mock-ups in `docs/design/`) |
| `docs/ARCHITECTURE.md` | the decisions taken, and the open risks |
| `docs/KEYMAP.md` | every binding, and the rules behind them |
| `docs/notes/` | what each dependency was verified to actually do |

Where the first three disagree, `DESIGN-TOKENS.md` wins on tokens, `DESIGN.md`
on appearance, `SPEC.md` on the rest.

## Where omagit keeps things

Two files in the config directory — `~/Library/Application Support/omagit` on
macOS, `$XDG_CONFIG_HOME/omagit` on Linux:

| File | Holds |
|---|---|
| `settings.toml` | the theme source and the density |
| `repositories.toml` | the repositories you added, and the groups you filed them in |

They are separate because they change at completely different rates, and a bad
write to one must not take the other with it. A `repositories.toml` that will not
parse is renamed aside rather than replaced: it is a list you arranged by hand.

## Choosing a theme

Réglages → Thème, or `⌘,`. The same choices live in `settings.toml` in the config
directory — `~/Library/Application Support/omagit` on macOS,
`$XDG_CONFIG_HOME/omagit` on Linux — and editing it by hand is still supported:

```toml
density = "compact"        # or "comfortable"

[theme]
source = "user-override"   # an explicit theme; disables all tracking
name = "Gruvbox"
```

Other sources: `omarchy` (Linux, follows the live Quattro palette),
`system-appearance` (follows light/dark), `embedded` with `mode = "dark"` or
`"light"`, and `automatic` — the default, which picks the best available.

## Current state

**M9 — finition.** The window answers the keyboard end to end: three zones with
`1` `2` `3`, `j`/`k` inside one, `Esc` up a level, and `Tab` walking the stops
the markup declares. A command palette on `⌘K` searches actions, repositories,
branches and files. `?` prints every binding — from the table that answers them,
not from a list somebody keeps up to date. Réglages holds the theme, the density,
the scale and the keymap, which is reassignable: press the key you want. On
macOS there is a native menu bar, which is also where `⌘Z` and `⌘A` inside the
commit box come from. And the empty states say which empty they are — a filter
with nothing behind it is not an empty repository.

Before it, **M8 — stashes and conflicts.** A merge or a rebase that stops half-way is now
something you can get out of in either direction, from inside the window: the
two sides of a conflicted file named by their branch rather than by "ours" and
"theirs" — which mean the opposite of what they read like during a rebase —
whole-file resolution, a dialog that answers conflict by conflict over the
markers `git` wrote, the file handed to your editor if you would rather, and
`continue` beside `abort` in the status bar. Plus the shelf: stashes listed,
previewed — including the untracked files an `-u` stash carries, which a commit
diff does not show — applied, popped and dropped.

Before it, **M7 — branches and the network**: the branch tree, switching,
creating and deleting, merge and rebase, fetch, pull, push with
`--force-with-lease` and never a bare `--force`, cloning with a reachability
check before rather than four minutes in, and a progress overlay you can cancel.
Authentication is `git`'s own, through the platform's credential helper.

Before that, **M3 to M6, ported to the Tauri window**: the Repositories screen,
the Working Copy with staging by file, hunk and line, the commit box, History
with its graph and filters, and the diff viewer under all of them.

`docs/ARCHITECTURE.md` §4 lists exactly what is built, and §5's known defects
list exactly what is not — including the three-column layout below 1100px, and
repository groups, which can be seen but not made.

Next is **M10 — distribution**: a signed and notarised macOS bundle, a Linux
archive and PKGBUILD.

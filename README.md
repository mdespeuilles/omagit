# omagit

A graphical Git client for **Omarchy / Linux** and **macOS**, written in Rust
and rendered with GPUI. Inspired by Tower, deliberately simpler.

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

Builds a throw-away repository under `$TMPDIR` with everything a screen needs to
have something to draw: a history with a merge in it, five branches — one merged,
one nobody has touched since the spring — two tags, a divergence from a "remote"
that is a bare repository on the disk beside it, two stashes, a working copy
holding six kinds of change at once, and a merge that conflicts in three files.

It makes **two** repositories, and the reason is `git`'s: a merge refuses to
start when anything at all is staged, so the one that shows a half-filled index
cannot be the one that conflicts on demand. `atelier` is the working copy;
`atelier-collegue` has a clean tree and the same branches.

Rebuilt from scratch on every run, and it only ever deletes a directory it wrote
itself.

## Before opening a PR

```sh
scripts/check.sh
```

`fmt --check`, `clippy -D warnings`, `test`, and a release build — the same gate
CI runs on Linux and macOS. A milestone is one PR, and it lands green on both
(SPEC §3 rule 8).

Running it locally only compiles **this** host's half of
`omagit-app/src/platform/`, so a break in the other one is invisible until CI
says so. CI also runs `cargo deny` and checks that `vendor/` has not drifted
from upstream.

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

There is no preferences screen yet (M9). Until then, `settings.toml` in the
config directory — `~/Library/Application Support/omagit` on macOS,
`$XDG_CONFIG_HOME/omagit` on Linux:

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

**M4 — the Working Copy, in read.** Open a repository and read it: the files
that changed, split into what is staged and what is not, and the diff of any of
them — unified or side by side, with the changed words picked out inside a line,
syntax colouring in the five roles the design allows, and a filesystem watcher
that refreshes the screen when the repository moves underneath. Nothing writes
yet; staging and committing are M5.

Before it, **M3's Repositories screen**: a sidebar of repositories in
collapsible, drag-reorderable groups, and a card reading out where each one is,
what its working copy looks like, who would sign a commit there, and ninety days
of activity. Navigable end to end from the keyboard (`docs/KEYMAP.md`).

Under both, **M2's Git core**: `omagit-git` opens repositories, reads the working
copy, the references and a paged history, and computes diffs down to the changed
words inside a line, with no UI dependency at all. `omagit-git-cli` drives every
bit of it from a terminal.

The `gix` / `git` split of SPEC §8 was measured rather than assumed before being
fixed: `gix` covers every read, the numbers and the six answers are in
`docs/notes/gitoxide-capabilities.md`, and the `GitBackend` trait waits for M5,
when writes give it a second implementation.

`docs/ARCHITECTURE.md` §4 lists exactly what is built and what is deliberately
not — including, on this screen, what is drawn but waiting for its milestone.

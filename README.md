# omagit

A graphical Git client for **Omarchy / Linux** and **macOS**, written in Rust
and rendered with GPUI. Inspired by Tower, deliberately simpler.

## Build and run

```sh
cargo run -p omagit-app          # or: cargo run --bin omagit
OMAGIT_LOG=debug cargo run       # logs to stderr and to the config directory
```

The Git core also runs without a window, which is how it is validated and
measured (SPEC §14, M2):

```sh
cargo run -p omagit-git-cli -- info                 # HEAD, work tree, git version
cargo run -p omagit-git-cli -- status --ignored
cargo run -p omagit-git-cli -- log -n 20 --all
cargo run -p omagit-git-cli -- show <full-hash>     # message, then the diff
cargo run -p omagit-git-cli -- diff --staged
cargo run -p omagit-git-cli --release -- -C ~/some/repo bench
```

Add `--timing` to any of them. `bench` prints the reads SPEC §12 sets budgets
for, against whatever repository it is pointed at.

Requires the toolchain pinned in `rust-toolchain.toml`; `rustup` picks it up on
its own.

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

**M3 — the Repositories screen.** The first screen: a sidebar of repositories in
collapsible, drag-reorderable groups, and a card reading out where each one is,
what its working copy looks like, who would sign a commit there, and ninety days
of activity. Navigable end to end from the keyboard — six tab stops, `1`/`2`/`3`
zone jumps, `j`/`k`, `/` to filter (`docs/KEYMAP.md`).

Under it, **M2's Git core**: `omagit-git` opens repositories, reads the working
copy, the references and a paged history, and computes diffs down to the changed
words inside a line, with no UI dependency at all. `omagit-git-cli` drives every
bit of it from a terminal.

The `gix` / `git` split of SPEC §8 was measured rather than assumed before being
fixed: `gix` covers every read, the numbers and the six answers are in
`docs/notes/gitoxide-capabilities.md`, and the `GitBackend` trait waits for M5,
when writes give it a second implementation.

`docs/ARCHITECTURE.md` §4 lists exactly what is built and what is deliberately
not — including, on this screen, what is drawn but waiting for its milestone.

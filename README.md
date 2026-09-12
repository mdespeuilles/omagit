<img src="crates/omagit-app/icons/128x128.png" width="88" align="right" alt="">

# omagit

[![Licence: GPL v3](https://img.shields.io/badge/licence-GPL--3.0-blue)](LICENSE)
[![Latest release](https://img.shields.io/github/v/release/mdespeuilles/omagit?include_prereleases&sort=semver)](https://github.com/mdespeuilles/omagit/releases)
![Platforms](https://img.shields.io/badge/platforms-Linux%20%C2%B7%20macOS-lightgrey)

A graphical Git client for **Linux** and **macOS**. Rust underneath, a web front
end in a Tauri window.

It reads with [`gitoxide`](https://github.com/GitoxideLabs/gitoxide) and writes
by running `git` itself — so every write is a command you could have typed, and
the journal shows you the exact line before it runs.

Free on Linux. The signed macOS build is sold; see [Licence](#licence).

## Features

**Working copy.** Staging by file, by hunk and by line. Discard at all three,
each behind a confirmation that says what is lost and where it can still be
found. Commit, amend, sign-off, `--no-verify`; `commit.template` respected, and
the committer identity checked before you start typing.

**History.** A virtualised list with a multi-lane graph, paged as you scroll.
Filters by author, message, path and date; a branch's own history on a click.
Commit detail with clickable parents, and A ↔ B comparison between any two.

**Diff.** Word-level refinement inside a changed line, folded context, and the
awkward cases handled rather than hung on: binary, oversized, mode-only,
submodule.

**Commit messages, drafted.** If a coding agent is installed — Claude Code,
Codex, the Gemini CLI, or a command of your own — omagit can ask it to write the
message for what is staged. **No API key is stored**: it asks the program you
already installed and logged in to. It runs the agent inside the repository, so
the message follows the conventions your project already documents, and the
exact command appears in the journal like every other.

**Repositories in folders.** Make one, name it, fold it, and drag repositories
into it — or pick the folder from the card, which is the route that does not
need a pointer. Removing a folder keeps what was filed in it.

**Tags.** Lightweight or annotated — the message is what decides, the way `git`
decides it — made on the current commit or on the one you are reading. Deleting
one says what it does *not* take: the commit stays, and a tag already pushed
stays on the remote. Published one at a time, never all at once.

**Branches, remotes, tags, stashes.** Grouped on `/`, ahead/behind kept fresh,
"merged" read from `git` rather than guessed. Fetch, pull (merge or rebase),
push — `--force-with-lease` only, never a bare `--force`. Cloning with a
reachability check _before_ the clone rather than four minutes into it.

**Conflicts.** Both sides named by their branch, not by "ours" and "theirs",
which mean the opposite of what they read like during a rebase. Whole-file
resolution, or conflict by conflict in a dialog over the markers `git` wrote —
rebuilt byte for byte, so line endings survive. `continue` and `abort` side by
side, so a repository stopped half-way is never a dead end.

**The keyboard.** Three zones with `1` `2` `3`, `j`/`k` inside one, `Esc` up a
level. A command palette on `⌘K` over actions, repositories, branches and files.
`?` prints every binding — from the table that answers them, so it cannot drift.
Every binding is reassignable.

**Themes.** Eight built in, or follow the system's light and dark, or follow
[Omarchy](https://omarchy.org)'s palette live. Two densities, and a scale.

**Two languages**, English and French, taken from the system and changeable in
Settings. Adding a third is adding one file.

## Installation

omagit needs **`git` 2.35 or newer** on the `PATH`. It says so plainly at
start-up if it cannot find one, and keeps working read-only.

### Linux

From the [releases page](https://github.com/mdespeuilles/omagit/releases):

```sh
# Debian, Ubuntu and derivatives
sudo apt install ./omagit_0.1.0_amd64.deb

# anything else
chmod +x omagit_0.1.0_amd64.AppImage && ./omagit_0.1.0_amd64.AppImage
```

Wayland is the primary target, X11 the fallback. The `.deb` pulls in what it
needs; the AppImage carries it.

### macOS

Not released yet — it arrives with M10, signed and notarised. It is the one
paid build: see [Licence](#licence) for why, and for how to build it yourself in
the meantime.

### From source

```sh
git clone https://github.com/mdespeuilles/omagit && cd omagit
./scripts/release.sh          # the front end, then the bundle
```

You need [Rust](https://rustup.rs) — the version in `rust-toolchain.toml`
installs itself — and Node 22. On Linux, the WebKitGTK headers as well:

```sh
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
                 libxdo-dev libssl-dev patchelf
```

The bundles land in `target/release/bundle/`.

## Usage

The window opens on **Repositories** — always, and on purpose: which repository
you work in is the first thing the product asks, not something it decides for
you. Add one with the button, or drop a folder on the window.

Each repository you open stays as a tab, so moving between two is one click.

Press `?` for every shortcut. The ones worth knowing first:

| Key             | What it does                                                 |
| --------------- | ------------------------------------------------------------ |
| `⌘K` / `Ctrl+K` | the command palette — actions, repositories, branches, files |
| `1` `2` `3`     | the sidebar, the centre column, the detail panel             |
| `j` `k`         | down and up inside a zone                                    |
| `⏎`             | what the row is for: open, switch, stage                     |
| `/`             | this screen's filter                                         |
| `⌘F` `⌘P` `⇧⌘P` | fetch, push, pull                                            |
| `⌘,`            | settings                                                     |

**Everything omagit writes, it writes by running `git`.** The journal — `⇧⌘J`,
or the button in the status bar — shows the exact command, before it runs and
after, with what it said.

## Configuration

Two files, in `$XDG_CONFIG_HOME/omagit` on Linux and
`~/Library/Application Support/omagit` on macOS:

| File                | Holds                                                        |
| ------------------- | ------------------------------------------------------------ |
| `settings.toml`     | theme, density, scale, language, reassigned shortcuts        |
| `repositories.toml` | the repositories you added, and the groups you filed them in |

They are separate because they change at completely different rates, and a bad
write to one must not take the other with it. A `repositories.toml` that will not
parse is renamed aside rather than replaced: it is a list you arranged by hand.

Everything in them is reachable from Settings. Editing by hand stays supported:

```toml
density = "compact"        # or "comfortable"
ui_scale = 1.15
language = "fr"            # omit it to follow the system

[theme]
source = "user-override"   # an explicit theme; disables all tracking
name = "Gruvbox"
```

Other theme sources: `omarchy` (Linux, follows the live Quattro palette),
`system-appearance` (follows light and dark), `embedded` with `mode = "dark"` or
`"light"`, and `automatic` — the default, which picks the best available.

## Development

```sh
./scripts/dev.sh      # the front end's dev server, then the app
./scripts/check.sh    # the gate: fmt, clippy, tests, release build, cargo-deny
./scripts/fixture.sh  # a throw-away repository with something to look at
```

**Do not run the debug binary on its own.** Tauri loads the dev server rather
than the embedded front end there, so `cargo run` with nothing on port 5173
opens a blank window and says nothing about why. `OMAGIT_LOG=debug` turns the
logging up; the front end's errors go to the same file as the backend's.

`scripts/fixture.sh` builds two repositories in `~/omagit-fixture` with
everything a screen needs to have something to draw — a history with a merge in
it, five branches, two tags, a divergence from a "remote" that is a bare
repository beside it, two stashes, a working copy holding six kinds of change at
once, and, with `--conflict`, a merge stopped in the middle of three files.

`scripts/check.sh` is the only gate, and it is run by hand: there is no CI for
it, for the reason `CLAUDE.md` gives. It compiles only **this** host's half of
`omagit-app/src/platform/`, so a green run says the tree is green on this
machine and nothing about the other one.

### Documentation

| File                    | Authority over                                                 |
| ----------------------- | -------------------------------------------------------------- |
| `docs/SPEC.md`          | everything not covered below                                   |
| `docs/DESIGN-TOKENS.md` | token names and derivation formulas                            |
| `docs/DESIGN.md`        | visual usage and interface states (mock-ups in `docs/design/`) |
| `docs/ARCHITECTURE.md`  | every decision taken and why, plus the open risks              |
| `docs/KEYMAP.md`        | every binding, and the rules behind them                       |
| `docs/notes/`           | what each dependency was verified to actually do               |

Where the first three disagree: `DESIGN-TOKENS.md` wins on tokens, `DESIGN.md`
on appearance, `SPEC.md` on the rest.

`docs/ARCHITECTURE.md` §4 lists exactly what is built and §5 the known defects —
including the three-column layout below 1100px, and repository groups, which can
be seen but not made.

### Releasing

```sh
./scripts/check.sh
$EDITOR crates/omagit-app/tauri.conf.json Cargo.toml   # the version, in both
git commit -am "0.2.0"
git tag --annotate v0.2.0 --message "omagit 0.2.0"     # annotated, see below
git push --follow-tags
```

The tag starts `.github/workflows/release.yml`: it refuses immediately if the
tag and the manifests disagree, builds from a clean checkout, and attaches the
Linux packages to a **draft** release. The notes are written by a person, so
publishing the draft is a person's gesture too.

**`--annotate` is not a flourish.** `--follow-tags` pushes annotated tags and
*silently skips* lightweight ones, so `git tag v0.2.0` followed by that push
leaves the tag on your machine, no build runs, and nothing says why. It is also
the kind a release should be: it carries a tagger, a date and a message, and
`git describe` passes over the other kind.

## Contributing

Issues are welcome — a bug report with a screenshot is worth a great deal, and
much of this window was shaped by them.

Before a pull request, read `docs/ARCHITECTURE.md`: it records why things are the
way they are, and a change that contradicts a decision recorded there needs to
argue with it rather than around it. Then run `./scripts/check.sh` — it is the
only gate, and nothing else will catch you.

One person develops this, so a review may take a while.

Contributions are GPL-3.0 like the rest, and there is no contributor agreement to
sign. Be aware that the licence lets anyone — this project included — sell builds
of the result; that is what pays for the macOS signing certificate.

## Licence

**GPL-3.0-only.** The whole of it is in [`LICENSE`](LICENSE), and every crate
carries the identifier. Nothing in the dependency graph is incompatible with it:
every crate omagit links offers MIT, Apache-2.0 or another permissive licence,
checked when the licence was chosen and re-checked by `cargo deny` at every gate.

**Linux is free**, and stays free.

**The macOS build is sold**, and what is sold is the _build_ — signed, notarised,
updating itself — never the right to use the program. That is the part nobody can
make for themselves without an Apple developer account, and what it costs pays
for the account, the notarisation and the time.

The GPL is what makes that honest: you may build macOS yourself from this
repository, you may pass your copy on, and nobody may take this code into a
closed fork. It is the model [Ardour](https://ardour.org) has run on for fifteen
years.

---

Built with [Tauri](https://tauri.app), [gitoxide](https://github.com/GitoxideLabs/gitoxide)
and [Vue](https://vuejs.org). The GPUI build that came before it, and why it was
left, are in `docs/ARCHITECTURE.md` §2.20.

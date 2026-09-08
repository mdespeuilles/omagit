# omagit

A graphical Git client for **Omarchy / Linux** and **macOS**, written in Rust
and rendered with GPUI. Inspired by Tower, deliberately simpler.

## Build and run

```sh
cargo run -p omagit-app          # or: cargo run --bin omagit
OMAGIT_LOG=debug cargo run       # logs to stderr and to the config directory
```

Requires the toolchain pinned in `rust-toolchain.toml`; `rustup` picks it up on
its own.

## Before opening a PR

```sh
scripts/check.sh
```

`fmt --check`, `clippy -D warnings`, `test`, and a release build — the same gate
CI runs on Linux and macOS. A milestone is one PR, and it lands green on both
(SPEC §3 rule 8).

## Documentation

| File | Authority over |
|---|---|
| `docs/SPEC.md` | everything not covered below |
| `docs/DESIGN-TOKENS.md` | token names and derivation formulas |
| `docs/DESIGN.md` | visual usage and interface states (mock-ups in `docs/design/`) |
| `docs/ARCHITECTURE.md` | the decisions taken, and the open risks |
| `docs/notes/` | what each dependency was verified to actually do |

Where the first three disagree, `DESIGN-TOKENS.md` wins on tokens, `DESIGN.md`
on appearance, `SPEC.md` on the rest.

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

**M1 — theme.** `DESIGN-TOKENS.md` is implemented in full: the four sources,
OKLCH derivation with contrast correction, generated graph lanes, eight embedded
themes, live Omarchy tracking and macOS appearance tracking, held by the six
tests of §10. `docs/ARCHITECTURE.md` §4 lists exactly what is built and what is
deliberately not.

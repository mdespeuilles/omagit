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

## Current state

**M0 — skeleton.** The workspace builds, the window opens and applies a theme on
both platforms, CI is green on both. `docs/ARCHITECTURE.md` §4 lists exactly
what M0 ships and what it deliberately does not.

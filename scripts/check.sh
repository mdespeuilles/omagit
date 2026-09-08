#!/usr/bin/env bash
# The gate a milestone has to pass before it becomes a PR (SPEC §3 rule 8).
# CI runs exactly this, on Linux and on macOS.
#
# It only ever compiles *this* host's half of `omagit-app/src/platform/`, so a
# break in the other one is invisible here and shows up in CI. That is what the
# two-platform matrix is for; do not read a green run on one machine as a green
# milestone. CI additionally runs `cargo deny` and the vendor drift check.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "── fmt ────────────────────────────────────────────"
cargo fmt --all --check

echo "── clippy ─────────────────────────────────────────"
# `gpui-omarchy` is in the workspace but silences clippy from its own manifest
# (see vendor/README.md): its sources stay byte-identical to upstream so
# scripts/sync-vendor.sh can tell a real upstream change from one of ours.
cargo clippy --workspace --all-targets -- -D warnings

echo "── test ───────────────────────────────────────────"
cargo test --workspace

echo "── release build ──────────────────────────────────"
cargo build --workspace --release

echo "── deny ───────────────────────────────────────────"
# Optional locally, mandatory in CI. Its `[graph] targets` cover Linux as well
# as macOS, so a licence or advisory problem that only appears in the Linux
# dependency tree is caught here too — unlike the compile steps above.
if command -v cargo-deny >/dev/null 2>&1; then
  cargo deny check bans licenses sources advisories
else
  echo "cargo-deny not installed (cargo install cargo-deny) — CI will run it"
fi

echo "all green on $(uname -s) — the other platform is CI's answer to give"

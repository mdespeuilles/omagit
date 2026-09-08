#!/usr/bin/env bash
# The gate a milestone has to pass before it becomes a PR (SPEC §3 rule 8).
# CI runs exactly this, on Linux and on macOS.
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

echo "all green"

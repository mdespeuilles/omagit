#!/usr/bin/env bash
# The gate a milestone has to pass (SPEC §3 rule 8). Since the amendment to that
# rule this is the *only* gate: there is no CI, so nothing runs unless you run
# it here.
#
# It compiles only *this* host's half of `omagit-app/src/platform/`, and the
# macOS-only behaviours in `status` — APFS case folding, decomposed names — are
# exercised only when this runs on a Mac. A green run says the milestone is
# green on this machine and nothing about the other one; run it on both before
# calling a milestone done.
#
# `scripts/sync-vendor.sh` is the other check worth running before a milestone,
# separately: it needs the network, which this script deliberately does not.
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
# Its `[graph] targets` cover Linux as well
# as macOS, so a licence or advisory problem that only appears in the Linux
# dependency tree is caught here too — unlike the compile steps above.
if command -v cargo-deny >/dev/null 2>&1; then
  cargo deny check bans licenses sources advisories
else
  echo "cargo-deny not installed (cargo install cargo-deny) — licences unchecked"
fi

echo "all green on $(uname -s) — the other platform is only ever answered for by running this there"

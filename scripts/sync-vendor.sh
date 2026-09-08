#!/usr/bin/env bash
# Diff vendor/gpui-omarchy against the upstream release it was taken from, and
# report what moved (SPEC §5).
#
#   scripts/sync-vendor.sh            # diff against the pinned version
#   scripts/sync-vendor.sh 0.2.0      # diff against another release
set -euo pipefail
cd "$(dirname "$0")/.."

VENDOR=vendor/gpui-omarchy
PINNED=$(awk -F'"' '/^version = /{print $2; exit}' "$VENDOR/Cargo.toml")
WANTED=${1:-$PINNED}

# Informational only: a network hiccup must not fail the drift check, which is
# the part CI actually depends on. crates.io rejects requests without a UA.
latest=$(curl -sSf -A "omagit-sync-vendor" "https://crates.io/api/v1/crates/gpui-omarchy" 2>/dev/null \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["crate"]["max_stable_version"])' 2>/dev/null \
  || echo "unknown")

echo "vendored: $PINNED   comparing against: $WANTED   latest on crates.io: $latest"
case "$latest" in
  unknown) echo "note: could not reach crates.io; skipping the staleness check." ;;
  "$PINNED") ;;
  *) echo "note: upstream has moved to $latest — review before bumping." ;;
esac

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
curl -sSfL -A "omagit-sync-vendor" \
  "https://static.crates.io/crates/gpui-omarchy/gpui-omarchy-$WANTED.crate" \
  | tar xz -C "$work"
upstream="$work/gpui-omarchy-$WANTED"

# Files we knowingly do not carry, or replace. Everything else must match
# upstream byte for byte; anything that shows up here is a divergence to record
# in vendor/README.md.
diff -ru \
  --exclude='.cargo_vcs_info.json' \
  --exclude='Cargo.toml' \
  --exclude='Cargo.toml.orig' \
  --exclude='Cargo.toml.upstream' \
  --exclude='Cargo.lock' \
  --exclude='.github' \
  --exclude='examples' \
  --exclude='website' \
  "$upstream" "$VENDOR" && echo "no divergence outside the recorded ones"

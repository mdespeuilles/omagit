#!/usr/bin/env bash
# Build omagit and install it in /Applications, on this Mac only.
#
# The build goes through `scripts/release.sh`, with two differences from a
# release: only the `.app` is made, and the updater artefacts are turned off —
# they need the private signing key, which a local install has no use for. The
# result is not signed with an Apple certificate, so it opens on the machine
# that built it and nowhere else. Updating means running this again.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ "$(uname -s)" != "Darwin" ]; then
  echo "ce script ne sert que sur macOS" >&2
  exit 1
fi

dest="${OMAGIT_INSTALL_DIR:-/Applications}/omagit.app"
built="target/release/bundle/macos/omagit.app"

echo "── build ──"
./scripts/release.sh --bundles app --config '{"bundle":{"createUpdaterArtifacts":false}}'

# Quit a running copy first: replacing a bundle under a live process leaves it
# running the old binary until it next starts.
if pgrep -f "$dest/Contents/MacOS/omagit" >/dev/null; then
  echo "── fermeture de l'omagit ouvert ──"
  osascript -e 'tell application "omagit" to quit' || true
  for _ in $(seq 1 20); do
    pgrep -f "$dest/Contents/MacOS/omagit" >/dev/null || break
    sleep 0.25
  done
fi

echo "── installation dans $dest ──"
# Remove, then copy: `ditto` over an existing bundle merges, and files the new
# build no longer has would stay behind.
rm -rf "$dest"
ditto "$built" "$dest"
xattr -dr com.apple.quarantine "$dest" 2>/dev/null || true

echo "installé : $dest"
if [ "${1:-}" = "--open" ]; then
  # Launch Services can still be holding the copy that was just quit and
  # replaced, and answers -600 for a moment; a retry is all it needs.
  for _ in 1 2 3 4 5; do
    open "$dest" 2>/dev/null && exit 0
    sleep 1
  done
  echo "omagit installé, mais ne s'est pas ouvert — à lancer à la main" >&2
fi

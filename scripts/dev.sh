#!/usr/bin/env bash
# Run omagit in development.
#
# In a debug build Tauri loads the Vite dev server rather than the embedded
# front end, so the binary on its own opens a blank window. This starts both and
# stops both.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ ! -d web/node_modules ]; then
  echo "── installing the front end's dependencies ──"
  npm --prefix web install
fi

echo "── vite ──"
npm --prefix web run dev >/tmp/omagit-vite.log 2>&1 &
vite=$!
trap 'kill "$vite" 2>/dev/null || true' EXIT

# Wait for the port rather than sleeping: a cold `npm` start is slower than any
# guess, and a window that opens first is the blank one.
for _ in $(seq 1 60); do
  if curl -sf -m 1 http://localhost:5173/ >/dev/null 2>&1; then break; fi
  sleep 0.5
done
if ! curl -sf -m 1 http://localhost:5173/ >/dev/null 2>&1; then
  echo "vite n'a pas démarré — voir /tmp/omagit-vite.log" >&2
  exit 1
fi

echo "── omagit ──"
OMAGIT_LOG="${OMAGIT_LOG:-info}" cargo run -p omagit-app

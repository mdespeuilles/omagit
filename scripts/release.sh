#!/usr/bin/env bash
# Build the release bundles.
#
# One door, so the workflow and a person run the same thing: what CI publishes
# is what `scripts/release.sh` makes here, and a failure is reproducible on a
# laptop rather than only in a log.
#
# It does *not* run the gate. `scripts/check.sh` is the gate, run by hand before
# a tag is cut — see CLAUDE.md, where the reason a gate is not automated on this
# repository is written down. What CI adds is the one check nobody can do at a
# keyboard: that the tag and the manifest agree.
set -euo pipefail
cd "$(dirname "$0")/.."

# `ci` rather than `install`: the lock file is the version being released, and
# `install` is free to move it.
if [ -n "${CI:-}" ]; then
  npm --prefix web ci
elif [ ! -d web/node_modules ]; then
  npm --prefix web install
fi

# The bundler is run from the crate that holds `tauri.conf.json`; the front end
# is built by its `beforeBuildCommand`.
cd crates/omagit-app
exec ../../web/node_modules/.bin/tauri build "$@"

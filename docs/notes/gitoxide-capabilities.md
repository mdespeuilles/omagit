# gitoxide (`gix`) — capability check

| | |
|---|---|
| Version on crates.io | **0.87.1**, published 2026-08-24 |
| Status here | **not yet a dependency** — the Git core is M2 |
| Checked | 2026-09-08 |

M0 adds no Git dependency, so this note records only what has to be settled
*before* M2 fixes the `gix` / CLI split of SPEC §8. Answering it early would
mean guessing; SPEC §3 rule 1 asks for a test that proves the behaviour instead.

## What M2 must answer, with a test each

1. **`status`** — does the resolved `gix` report the full working-copy state
   (modified, added, deleted, renamed, untracked, ignored, conflicted) on a
   repository with 50 000 files, inside the 300 ms of SPEC §12?
2. **Rename detection in diffs** — supported, and at what cost?
3. **Blame** — out of MVP scope (SPEC §11), so no need to answer at M2.
4. **`.gitattributes`** — are filters and `text`/`eol` handled, or is a file
   with a smudge filter reported as modified when it is not?
5. **Submodules** — reported as an entry kind, or skipped?
6. **Non-UTF-8 paths and Unicode normalisation** — the macOS case of SPEC §9,
   which `omagit-git/paths.rs` centralises.

Any of these that `gix` does not cover cleanly goes to the `git` CLI rather
than being worked around, and the reason gets written down (SPEC §15 risk 2).

## Companion crates, also unresolved until M2

| Crate | Latest | Use |
|---|---|---|
| `imara-diff` | 0.2.0 (2025-06-14) | Myers diff with word-level refinement |
| `notify` | 8.2.0 stable, 9.0.0-rc.5 | FS watching. Pin the stable line; SPEC §10 requires checking that inotify and FSEvents report the same events, and they do not |

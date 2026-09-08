# gitoxide (`gix`) — capability check

| | |
|---|---|
| Version in the lockfile | **0.87.1**, pinned with `=` |
| Status here | **the whole read path** of `omagit-git` |
| Checked | 2026-09-08, at M2 |

SPEC §8 asks for the `gix` / `git` split to be *measured* before it is fixed,
and SPEC §3 rule 1 asks for a test rather than a reading of the changelog. The
answers below each come from a test in
`crates/omagit-git/tests/capabilities.rs`; when a `gix` bump makes one of them
fail, this note is what has to be revisited, not just the assertion.

## Features enabled, and why the rest are not

```toml
gix = { version = "=0.87.1", default-features = false, features = [
    "max-performance-safe", "basic", "status", "sha1", "auto-chain-error",
] }
```

`default` would add `extras`, which pulls in `worktree-mutation`, `merge`,
`blame`, `notes`, `worktree-archive` and `credentials` — every one of them
either out of MVP scope (SPEC §11) or the `git` binary's job (SPEC §8). None of
the network features are on: omagit never speaks the Git protocol from Rust.

`max-performance-safe` is the parallel profile *without* the C zlib. Keeping the
C dependency out is half the reason `gix` was chosen at all (SPEC §4), so
`max-performance` is deliberately not used.

## The six questions

### 1. `status` on 50 000 files, within the 300 ms of SPEC §12 — **yes, by a wide margin**

`Repository::status()` reports the full working-copy state in one pass:
modified, added, deleted, renamed, untracked, ignored, conflicted, type-changed,
and submodules. It runs the tree↔index comparison, the index↔worktree
comparison and the directory walk on three threads at once, and it polls an
`Arc<AtomicBool>` we own — so a status is cancellable from the first
millisecond, not only between files.

Measured by `capabilities::status_of_fifty_thousand_files` (`#[ignore]`d; run it
with `cargo test -p omagit-git --release -- --ignored --nocapture`):

| | cold | warm |
|---|---|---|
| 50 000 files, clean tree | **36 ms** | **25 ms** |

Ryzen 7 5700G, Linux 7.1.9, btrfs, `git` 2.55.0, release profile. Read it as an
order of magnitude, not a benchmark: the fixture is written immediately before
being read, so the page cache is warm and a first status after a reboot will be
slower. Even ten times slower it clears the budget, which is the useful
conclusion — and the number to compare against after a `gix` bump.

### 2. Rename detection in diffs — **yes; cost is bounded and paid once**

Both halves of the status track renames (`index_worktree_rewrites`,
`tree_index_track_renames`), as does the tree-to-tree diff. `git mv` and a move
made in the Finder are both reported as one entry, not a deletion plus an
addition. 100 files moved in a single commit are detected in well under a second
(`capabilities::rename_detection_scales_to_a_hundred_files_at_once`).

One thing to know, found while writing that test: a rename inside a moved
directory is reported **twice** — once for the file and once for the tree.
`omagit-git/diff.rs` drops the tree-level entry; `relation` is there for callers
that want to rebuild a directory-level summary, which a file list does not.

### 3. Blame — **not asked**

Out of MVP scope (SPEC §11), so the `blame` feature is off.

### 4. `.gitattributes` — **handled, and it matters more than expected**

Two separate mechanisms, both verified:

* **`text` / `eol`.** A repository that stores LF and checks out CRLF reads as
  clean (`eol_conversion_is_applied_when_reading_the_working_tree`). Without
  this, every line of every file would show as modified on a mixed-platform
  team.
* **Clean filters.** A file whose blob is the output of a `clean` filter reads
  as clean even though the bytes on disk differ
  (`clean_filters_are_applied_when_reading_the_working_tree`).

`gix` applies the conversions inside `status` on its own. The *diff* does not
get them for free: `omagit-git/diff.rs` runs the working-tree side through
`Repository::filter_pipeline` before comparing it with the index blob, and the
second half of that test is what holds it.

### 5. Submodules — **reported as their own kind, not skipped**

A gitlink arrives with `Mode::is_submodule()` set, and a checked-out submodule
whose `HEAD` has moved is reported as modified
(`a_submodule_is_reported_as_its_own_kind`). Submodules are out of MVP scope,
which makes this *more* important, not less: a file list that silently omits a
directory is worse than one that shows it with nothing to do.

The same test caught a real bug in the first version of the diff — a gitlink's
object id names a commit in *another* repository, so reading it as a blob does
not merely waste time, it fails. `DiffContent::Submodule` is decided before
anything is read.

### 6. Non-UTF-8 paths and Unicode normalisation — **`gix` normalises, we carry bytes**

`core.precomposeUnicode` is honoured by `gix`'s directory walk, so the names
reaching `omagit-git` are already in the form the index holds — which is why
`omagit-git/paths.rs` deliberately does **not** normalise: doing it twice is how
a name stops matching its entry. `an_accented_name_matches_its_index_entry`
holds it, and it is the test to watch on the macOS half of CI, where the
filesystem is the thing doing the decomposing.

Undecodable names are carried as bytes end to end
(`status::carries_a_name_that_is_not_utf8`, Linux only — APFS rejects file names
that are not valid UTF-8, so the fixture cannot exist on macOS).

## What this means for the split

**Every read in M2 is `gix`.** Nothing had to fall back to the `git` binary, so
the `GitBackend` trait of SPEC §8 is *not* built yet: it would have exactly one
implementation, and an abstraction with no second implementation is a debt
rather than a preparation (SPEC §2). It arrives at M5, when writes give it its
second one. `omagit-git/cli.rs` already implements the subprocess rules of
SPEC §8 in full and carries the start-up check that a usable `git` exists.

## Companion crates

| Crate | Resolved | Use |
|---|---|---|
| `gix-imara-diff` | 0.2.5, through `gix::diff::blob` | the line diff, with Git's slider heuristics (`diff_with_slider_heuristics`). **Not** added as a separate `imara-diff` dependency: `gix` re-exports the whole crate, and two copies of a diff engine would eventually disagree |
| `libc` | 0.2, Unix only | `kill(-pid)`. SPEC §8 requires a cancelled subprocess to be signalled on its process *group*, which `std` cannot express |
| `notify` | 8.2.0 stable | FS watching. Still unused: M2 builds no watcher — see `docs/ARCHITECTURE.md` §5, risk 8 |

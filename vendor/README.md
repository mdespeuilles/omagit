# vendor/

Third-party code carried in-tree on purpose.

## gpui-omarchy

| | |
|---|---|
| Upstream | <https://github.com/huacnlee/gpui-omarchy> |
| Version vendored | **0.1.1** (published 2026-09-08) |
| Upstream commit | `e99166938aabf8bcbdbb9b25f7bdd741f62f4c32` (from the crate's `.cargo_vcs_info.json`) |
| Licence | MIT — `vendor/gpui-omarchy/LICENSE`, kept verbatim |
| Referenced as | `gpui-omarchy = { path = "vendor/gpui-omarchy" }` |

### Why it is vendored

`gpui-omarchy` is at 0.1.1, maintained by one person, pins `gpui-kit = "=0.6.0"`
hard, and its own README says the framework is not complete. omagit is a
multi-month project; a seven-star upstream must not be able to block it.

The arrangement: generic fixes go upstream as PRs, product-specific divergences
stay here. Nothing is edited in `src/` without being listed below.

### Divergences from upstream 0.1.1

`src/`, `assets/`, `tests/` and `docs/` are **byte-identical to the published
crate**. `scripts/sync-vendor.sh` asserts exactly that, so anything that appears
in its output is either an upstream change to review or an unrecorded edit here.

Only the manifest and the non-shipped directories differ:

| Change | Why |
|---|---|
| `Cargo.toml` rewritten from `Cargo.toml.orig` (kept as `Cargo.toml.upstream`) | The published manifest is Cargo's normalised form; the workspace needs `publish = false` and `gpui-kit = { workspace = true }` |
| `[profile.*]` removed | Cargo ignores profiles in workspace members anyway. The one that matters — `[profile.release.package.gpui-pre-macros] debug-assertions = true` — was verified unnecessary at the resolved `gpui-pre-macros` 0.3.4; the workspace root carries a note, not the override |
| `[lints.clippy] all = "allow"` added | Upstream code is not held to omagit's lint level, and silencing it from the manifest keeps `src/` untouched |
| `examples/`, `.github/`, `Cargo.lock` dropped | Unused here; the examples pull extra assets |

### Keeping it in sync

```sh
scripts/sync-vendor.sh          # diff against the pinned version
scripts/sync-vendor.sh 0.2.0    # diff against a newer release before bumping
```

It also reports the latest version on crates.io, so a stale vendor is visible
without anyone having to remember to look.

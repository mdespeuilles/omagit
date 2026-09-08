# tests/fixtures/omarchy/

Real `colors.toml` files, copied verbatim from `/usr/share/omarchy/themes/` on
an Omarchy machine (2026-09-08, Omarchy on Arch).

They are here because M1 was built on macOS, where there is no Omarchy at all,
against fixtures written from the *prose* of SPEC §6.2 — and the prose was
wrong. It names a guaranteed key `color8`; no Omarchy theme has ever had one.
All 23 installed themes spell it `muted`, so every real palette was rejected and
the whole Omarchy source silently fell back to an embedded theme.

A fixture written from a document can only ever confirm the document. These are
copied from the thing itself, and `omagit-theme/tests/omarchy_palettes.rs` reads
them, so the format is pinned by reality.

Three, chosen for what they cover rather than for variety: a light theme, a dark
one, and the one whose `accent` and `border` nearly collapse (DESIGN §1).

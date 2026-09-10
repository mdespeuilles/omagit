# KEYMAP.md

Every binding omagit answers, and the rules behind them.

The table lives in `web/src/keymap.ts` — one place, because three things need
the same list: the key handler, the command palette that runs these actions by
name, and the `?` sheet that prints them. The macOS menu bar will be the fourth
(SPEC §9).

**`Primary`** is `⌘` on macOS and `Ctrl` elsewhere (SPEC §9). The interface
always shows one or the other, never both, and never spells them together:
`⌘O` is a glyph beside a letter, `Ctrl O` is a word beside one (DESIGN §2).

> **Rewritten at M9.** What stood here described the GPUI build: it pointed at
> `crates/omagit-app/src/actions.rs`, which the port deleted, and listed M3's
> movement keys, which the web front end never had. Both are recorded in
> `docs/ARCHITECTURE.md` §5 as the defect they were.

## What ships

| Key | Action | Where |
|---|---|---|
| `Primary`+`O` | Ajouter un dépôt local | anywhere |
| `Shift`+`Primary`+`N` | Cloner un dépôt | anywhere |
| `Shift`+`Primary`+`O` | Tous les dépôts | anywhere |
| `Primary`+`1` `2` `3` | Copie de travail · Historique · Remises | with a repository open |
| `Primary`+`F` | Fetch | with a repository open |
| `Primary`+`P` | Push | with a repository open |
| `Shift`+`Primary`+`P` | Pull | with a repository open |
| `Primary`+`.` | Arrêter l'opération réseau | while one is running |
| `Primary`+`R` | Relire le dépôt — ou la liste, sur l'écran Dépôts | anywhere |
| `Shift`+`Primary`+`S` | Remiser les modifications | with a repository open |
| `Shift`+`Primary`+`J` | Journal des opérations | anywhere |
| `Primary`+`⏎` | Commiter | in the commit box |

A dialog answers its own keys while it is up — `Esc` closes every one of them,
`⌘⏎` applies the conflict dialog's answers, `n` moves to its next conflict — and
the table above stays quiet behind it.

## The rules

**A shortcut fires while you are typing; a bare key never does.** `⌘F` in a text
box is still Fetch, which is how every application on both platforms behaves. A
bare letter belongs to the field that has the caret. This is what will make the
movement letters safe when they arrive.

**The other platform's modifier is not answered.** `Ctrl+F` on macOS moves the
caret forward a character; treating it as Fetch would break every text field on
that platform.

**A key claimed by an action that cannot run now is still swallowed.** `⌘F`
during a fetch does nothing and stops there, rather than falling through to the
webview and opening a find bar over the application. A key claimed by *nothing*
is left alone.

**An action names a screen, not a component.** The handler is on the shell, so a
binding does not stop working the moment you leave the screen that declared it —
which is the same reason the topbar and the sidebar belong to `App.vue` (§2.28).

## Not yet bound

Movement — `j` `k`, `1` `2` `3` bare, `/`, and the tab-stop order of DESIGN §5
and board 09 — which needs a notion of zones and focus the window does not have
yet. It is M9's next slice, and it is why every action already carries the
context it fires in.

The command palette (`Primary`+`K`), the `?` sheet and the reassignment screen
are the slices after that. Reassignment is a settings screen over this same
table rather than a rewrite, which is the whole reason the table exists.

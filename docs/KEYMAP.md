# KEYMAP.md

Every binding omagit answers, and the rules behind them.

The table lives in `web/src/keymap.ts` — one place, because four things need the
same list: the key handler, the command palette that runs these actions by name,
the `?` sheet that prints them, and the macOS menu bar that fires them (SPEC §9).
Each action names the menu it belongs under; the menu bar is arranged from that
in `crates/omagit-app/src/menu.rs`.

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
| `Primary`+`K` | Palette de commandes | anywhere |
| `Shift`+`?` | La feuille des raccourcis | anywhere |
| `Primary`+`,` | Réglages | anywhere |
| `Shift`+`Primary`+`J` | Journal des opérations | anywhere |
| `Primary`+`⏎` | Commiter | in the commit box |

## Moving about

Bare keys, so none of them fires while the caret is in a field.

| Key | What it does |
|---|---|
| `1` `2` `3` | The sidebar, the centre column, the detail panel — the same three on every screen |
| `Tab` / `Shift`+`Tab` | The next zone, wrapping; focus never leaves for the window decoration |
| `j` `↓` / `k` `↑` | Down and up inside the zone; stops at the ends rather than wrapping |
| `g` `g` / `G` | The first row / the last |
| `⏎` | What the row is for: open a repository, show a branch's history, stage a file |
| `/` | The filter this screen has — the repository list's, or History's, which unfolds first |
| `Esc` | Up one level: a filter, then back to the sidebar. Never out of the repository |

A dialog answers its own keys while it is up — `Esc` closes every one of them,
`⌘⏎` applies the conflict dialog's answers, `n` moves to its next conflict — and
the table above stays quiet behind it.

Inside the palette: `↑` `↓` (or `Ctrl`+`P` / `Ctrl`+`N`) walk the rows across
its groups and wrap, `⏎` runs the row, `Esc` closes. What `⏎` *does* depends on
the row, which is why every row prints it: `exécuter`, `ouvrir`, `basculer`.

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

## The sheet

`?` prints this file's two tables from the table that answers them — the
commands from `ACTIONS`, each with its binding spelled for the platform running,
and the movements from `MOVEMENTS`. A test asserts the sheet holds one line per
entry, so an action added without a line is not something anyone can forget.

It is written `Shift`+`?`, not `Shift`+`/`, however the key is engraved: a
browser reports the character the layout produced. Written the other way it
matched nothing on any layout, which is what the sheet's first test found.

## In the menu bar

macOS only (SPEC §9). Every action above is in one of App, Fichier, Affichage,
Dépôt or Aide; Édition and Fenêtre hold the platform's own items, which is where
`⌘Z`, `⌘A` and `⌘C` inside the web view come from.

**Only a binding with `Primary` is registered as a menu accelerator.** A menu
accelerator is answered before the web view sees the key, so a binding that must
reach a text field cannot be one: `⌘F` fetches while you type and belongs there,
`⇧?` must yield to a question mark being typed and does not. Its item is still
in Aide, and the key still works — through the front end, which knows where the
caret is.

## Changing one

Réglages → Clavier lists every command with the key it answers. Press the key
button, then press the combination you want; `Échap` cancels, and *Défaut* puts
back the table's own. What changes is stored as an override in `settings.toml` —
only the difference, so an action added in a later version arrives with its own
binding rather than none.

Two bindings are refused, both because they could not answer:

* **a movement key with no `Primary` and no `Alt`** — `j`, `⇧G`, `Tab`, `/`.
  Movement is consulted before this table, and Shift alone does not change that,
  so the command would look assigned and fire never;
* **one another command already answers**, named in the refusal.

Everything else is allowed. A bare key that does not fire while you are typing is
the rule above, not a defect — `⇧?` is one.

## Tab is the browser's

`Tab` is not in the table, and that is the decision rather than an omission.
The window's tab stops are declared in the markup and the browser walks them
(board 09, ARCHITECTURE §2.52): **a whole list is one stop**, entered on the row
the keyboard is already on, and a row's own actions are stops only for that row.
`1` `2` `3` move the native focus with them, so the next `Tab` carries on from
where they landed.

It was answered here for one slice, by cycling the three zones — and
`preventDefault`ing every press to do it, which left no button in the window
reachable from the keyboard at all.

## Not yet bound

Per-hunk shortcuts: board 09 draws `⌥S` to stage the hunk the diff is on and
`⌥D` to discard it. Until they exist the hunk's own buttons stay in the tab
order, because a control reachable by the mouse alone is worse than one stop too
many.

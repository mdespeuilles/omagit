# KEYMAP.md

Every binding omagit ships, and the rules they follow. The bindings themselves
live in one file — `crates/omagit-app/src/actions.rs` — so this document has a
single place to correspond to.

**`Primary`** is `⌘` on macOS and `Ctrl` elsewhere (SPEC §9). The interface
always shows one or the other, never both (DESIGN §2), and never spells them
together: `⌘O` is a glyph beside a letter, `Ctrl O` is a word beside one.

Nothing here is reassignable yet. M9 adds that, and the reason these are *named
actions* rather than inline key handlers is so that it is a settings screen and
not a rewrite: the command palette lists actions by name, the macOS menu bar
dispatches them, and a keymap file rebinds them.

## Repositories (M3)

Bound in the `Repositories` key context, so none of them fire while the caret is
in a text field that wants the key.

| Key | Action | Note |
|---|---|---|
| `Primary`+`O` | Add a local repository | Opens the folder picker |
| `Primary`+`Shift`+`N` | Clone… | Arrives at M7; says so until then |
| `Primary`+`K` | Command palette | Arrives at M9 |
| `Primary`+`R` | Re-read every repository | Cancels whatever is still loading |
| `Primary`+`G` | New group | |
| `Primary`+`Backspace` | Remove the selected repository from the list | Nothing on disk is touched |
| `/` | Focus the filter box | |
| `1` `2` `3` | Sidebar · centre · detail | See below |
| `Tab` / `Shift`+`Tab` | Next / previous stop | Wraps; never leaves for the window decoration |
| `↓` `j` / `↑` `k` | Next / previous repository | |
| `←` `h` / `→` `l` | Fold / unfold the group | |
| `⏎` | Activate the focused control — open the selected repository | |
| `Esc` | Up one level: clear the filter, then return to the sidebar | |

## The rules behind them

**One zone is one tab stop.** Repositories has six, in this order: *Ajouter un
dépôt local* → *Rechercher* → the filter box → the repository list → *User
Description* → *Ouvrir*. Two of them hold a pair of controls — `Tab` moves to
the second (*Cloner…*, *Retirer de la liste*) before leaving the stop — which is
what makes DESIGN §5's "1b" and "6b" reachable without turning six stops into
eight.

**After the last stop, `Tab` returns to the first.** Focus never escapes into
the window decoration, and the window buttons and macOS traffic lights are never
in the tab order.

**`1`, `2` and `3` mean the same three zones on every screen.** Repositories has
only two columns — a sidebar and a card — so `2` and `3` both reach the card.
That is deliberate: a key that does nothing on one screen is worse than a key
that lands somewhere sensible.

**`Esc` goes up one level**, never straight out: overlay → zone → the sidebar.
On this screen there is no overlay yet, so it clears the filter first and then
returns focus to the list.

**Selected is not focused** (DESIGN §1). A selected row keeps its `surface_raised`
fill when focus leaves the zone; the 1px `border_focus` ring is what goes.

## Not yet bound

The `?` shortcut sheet, the reassignment screen, and everything belonging to
screens that do not exist yet. They arrive with M9 and with their own
milestones — see `docs/ARCHITECTURE.md` §4 for what is built.

// Every binding omagit answers, in one table.
//
// The port to Tauri threw away GPUI's named actions and never replaced them:
// the topbar has been drawing `⌘F`, `⌘O`, `⇧⌘N` and `⌘K` since M6b beside
// buttons that answer the mouse and nothing else. A hint that names a key which
// does nothing is worse than no hint — it says the app has a keyboard and then
// does not.
//
// ## Why a table rather than handlers on the components
//
// Three things in M9 need the same list, and none of them is the key handler:
// the command palette lists these actions and runs them, the `?` sheet prints
// them, and the macOS menu bar (SPEC §9, "sans elle, l'app paraît cassée")
// fires them. A binding declared inside a component would be reachable by the
// key and by nothing else. And SPEC §11 asks for the keymap to be
// *reassignable*, which is a settings screen over this table rather than a
// rewrite.
//
// ## The bare keys
//
// Movement — `j` `k`, `1` `2` `3`, `/`, `Esc` — is here too, and it is bare:
// no modifier, one letter. That is only safe because of the rule below that a
// bare key never fires while the caret is in a field. M3's version of these
// bindings did not have it and put a `j` in the filter box *and* moved the
// selection (§5, eleventh defect); this one is a table, so the rule is written
// once rather than remembered at each call site.

import { app } from "./state";
import * as store from "./state";

/// Where a binding is allowed to fire.
///
/// `always` is the whole window; `repository` needs one open, which is what
/// keeps `⌘F` from fetching on the Repositories screen where there is nothing
/// to fetch from.
export type Where = "always" | "repository";

export type Action = {
  /// Stable, and never shown: the palette searches labels, and settings will
  /// store bindings against this.
  id: string;
  /// What the palette and the sheet print. French, like the rest of the window.
  label: string;
  /// `Primary` is ⌘ on macOS and Ctrl elsewhere (SPEC §9). Parsed rather than
  /// stored as flags so the sheet can print it and settings can hold it as one
  /// string.
  binding: string;
  where: Where;
  /// Whether it can run now. The palette draws the rest dimmed rather than
  /// hiding them: an action that disappears is one nobody learns.
  enabled: () => boolean;
  run: () => void;
};

const inRepository = (): boolean => !!app.open;
const idle = (): boolean => !app.busy && !app.running;

/// The bare keys of DESIGN §5. Kept apart from the table above because they are
/// movement rather than commands: the palette does not list them — pressing `j`
/// from a palette row would be absurd — and the `?` sheet prints them in their
/// own block.
///
/// They answer in order, and the first one that claims the event ends it.
/// How the movement keys are *printed* — the `?` sheet's half of the same
/// vocabulary.
///
/// A second table, and a test holds the two together: what `dispatch` answers
/// and what the sheet prints must not drift, and the alternative — one table
/// carrying both the raw key names and their prose — made `g` `g`, `/` and
/// `Tab` fit badly, since each of the three is handled by hand.
export const MOVEMENTS: { print: string; label: string }[] = [
  { print: "1 2 3", label: "La sidebar, la colonne centrale, le panneau de détail" },
  { print: "Tab · ⇧Tab", label: "Zone suivante ou précédente, en boucle" },
  { print: "j k · ↓ ↑", label: "Descendre et monter dans la zone" },
  { print: "g g · G", label: "La première ligne · la dernière" },
  { print: "⏎", label: "Ce à quoi sert la ligne : ouvrir, basculer, indexer" },
  { print: "/", label: "Le filtre de cet écran" },
  { print: "Esc", label: "Remonter d'un niveau : un filtre, puis la sidebar" },
];

const MOVES: { keys: string[]; run: () => boolean }[] = [
  { keys: ["1"], run: () => (store.goToZone(1), true) },
  { keys: ["2"], run: () => (store.goToZone(2), true) },
  { keys: ["3"], run: () => (store.goToZone(3), true) },
  { keys: ["j", "ArrowDown"], run: () => store.moveInZone(1) },
  { keys: ["k", "ArrowUp"], run: () => store.moveInZone(-1) },
  { keys: ["G"], run: () => store.jumpInZone("end") },
  { keys: ["Enter"], run: () => store.activateInZone() },
  { keys: ["Escape"], run: () => store.escapeLevel() },
];

export const ACTIONS: Action[] = [
  {
    id: "repository.add",
    label: "Ajouter un dépôt local",
    binding: "Primary+O",
    where: "always",
    enabled: () => !app.gitUnusable,
    run: () => void store.addRepository(),
  },
  {
    id: "repository.clone",
    label: "Cloner un dépôt",
    binding: "Shift+Primary+N",
    where: "always",
    enabled: () => !app.gitUnusable && idle(),
    run: () => store.openClone(),
  },
  {
    id: "repository.all",
    label: "Tous les dépôts",
    binding: "Shift+Primary+O",
    where: "always",
    enabled: () => true,
    run: () => store.showScreen("repositories"),
  },
  {
    id: "screen.workingCopy",
    label: "Aller à la copie de travail",
    binding: "Primary+1",
    where: "repository",
    enabled: inRepository,
    run: () => store.showScreen("working-copy"),
  },
  {
    id: "screen.history",
    label: "Aller à l'historique",
    binding: "Primary+2",
    where: "repository",
    enabled: inRepository,
    run: () => store.showScreen("history"),
  },
  {
    id: "screen.stashes",
    label: "Aller aux remises",
    binding: "Primary+3",
    where: "repository",
    enabled: inRepository,
    run: () => store.showScreen("stashes"),
  },
  {
    id: "network.fetch",
    label: "Fetch",
    binding: "Primary+F",
    where: "repository",
    enabled: () => inRepository() && idle() && !app.gitUnusable,
    run: () => store.fetchRemote(),
  },
  {
    id: "network.pull",
    label: "Pull",
    binding: "Shift+Primary+P",
    where: "repository",
    enabled: () => inRepository() && idle() && !!app.summary?.tracking,
    run: () => store.pullRemote(),
  },
  {
    id: "network.push",
    label: "Push",
    binding: "Primary+P",
    where: "repository",
    enabled: () => inRepository() && idle() && !app.gitUnusable,
    run: () => store.pushBranch(false),
  },
  {
    id: "network.stop",
    label: "Arrêter l'opération réseau",
    binding: "Primary+.",
    where: "always",
    enabled: () => !!app.running,
    run: () => store.stopNetwork(),
  },
  {
    id: "workingCopy.refresh",
    label: "Relire le dépôt",
    binding: "Primary+R",
    where: "repository",
    enabled: inRepository,
    run: () => void store.refresh(),
  },
  {
    id: "stash.push",
    label: "Remiser les modifications",
    binding: "Shift+Primary+S",
    where: "repository",
    enabled: () => inRepository() && idle(),
    run: () => {
      store.showScreen("stashes");
      store.openStashForm();
    },
  },
  {
    id: "palette.open",
    label: "Palette de commandes",
    binding: "Primary+K",
    where: "always",
    enabled: () => true,
    run: () => store.openPalette(),
  },
  {
    id: "help.shortcuts",
    label: "Raccourcis clavier",
    // `Shift+?`, not `Shift+/`, however it is engraved on the key: a browser
    // reports the character the layout produced, and holding Shift over `/`
    // produces `?`. Written the other way it matched nothing, on any layout.
    binding: "Shift+?",
    where: "always",
    enabled: () => true,
    run: () => store.toggleShortcuts(),
  },
  {
    id: "settings.open",
    label: "Réglages",
    binding: "Primary+,",
    where: "always",
    enabled: () => true,
    run: () => store.showScreen("settings"),
  },
  {
    id: "journal.toggle",
    label: "Journal des opérations",
    binding: "Shift+Primary+J",
    where: "always",
    enabled: () => true,
    run: () => store.toggleJournal(),
  },
];

/// One event, matched against one binding.
///
/// `Primary` is resolved here rather than stored twice: the platform says which
/// modifier it is, and the table stays the same on both.
export function matches(
  binding: string,
  event: KeyboardEvent,
  primary: "meta" | "control",
): boolean {
  const parts = binding.split("+");
  const key = parts[parts.length - 1]!.toLowerCase();
  const wants = {
    primary: parts.includes("Primary"),
    shift: parts.includes("Shift"),
    alt: parts.includes("Alt"),
  };
  const has = {
    primary: primary === "meta" ? event.metaKey : event.ctrlKey,
    shift: event.shiftKey,
    alt: event.altKey,
    // The *other* modifier must be up: `Ctrl+F` on macOS is "move forward one
    // character", and answering it as Fetch would break a text field.
    other: primary === "meta" ? event.ctrlKey : event.metaKey,
  };
  if (has.other) return false;
  return (
    event.key.toLowerCase() === key &&
    wants.primary === has.primary &&
    wants.shift === has.shift &&
    wants.alt === has.alt
  );
}

/// Whether the caret is somewhere that wants the key for itself.
function typing(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target.tagName === "INPUT" ||
    target.tagName === "TEXTAREA" ||
    target.tagName === "SELECT" ||
    target.isContentEditable
  );
}

/// Whether a dialog is up. Dialogs answer their own keys — `Esc`, `⌘⏎`, `n` —
/// and a global binding firing behind one would act on a screen nobody can see.
function overlaid(): boolean {
  return !!app.question || !!app.clone || !!app.resolving || !!app.palette || app.shortcuts;
}

/// Run whichever action the event names, and say whether one did.
///
/// A binding with a modifier fires even while typing — `⌘F` in a text box is
/// still Fetch, and every application on both platforms behaves that way. A
/// bare key never does, which is what will make the movement letters of the
/// next slice safe.
/// `g` twice is the top — vim's own doubling, and the reason it needs a memory
/// of the last key rather than a table entry.
let lastKey = "";
let lastAt = 0;

export function dispatch(event: KeyboardEvent): boolean {
  if (overlaid()) return false;
  const primary = app.platform?.modifier === "command" ? "meta" : "control";
  const bare = !event.metaKey && !event.ctrlKey && !event.altKey;
  if (bare && typing(event.target)) return false;

  if (bare) {
    // `Tab` moves between zones (DESIGN §5) — but only out here, where the
    // caret is in no field: inside one it is the browser's, and taking it would
    // trap somebody in a text box. It was written in KEYMAP.md and bound to
    // nothing until the `?` sheet went to print it, which is the argument for
    // the sheet in one line.
    if (event.key === "Tab") {
      store.nextZone(event.shiftKey ? -1 : 1);
      return true;
    }
    // `/` puts the caret in whichever filter this screen has. A DOM act, done
    // here rather than in the store: focus is the one piece of interface state
    // the browser owns, and the store deliberately owns none of it.
    if (event.key === "/" && focusFilter()) return true;
    if (event.key === "g") {
      const doubled = lastKey === "g" && event.timeStamp - lastAt < 600;
      lastKey = doubled ? "" : "g";
      lastAt = event.timeStamp;
      return doubled ? store.jumpInZone("start") : true;
    }
    lastKey = "";
    for (const move of MOVES) {
      if (!move.keys.includes(event.key)) continue;
      if (move.run()) return true;
      // Claimed by movement but with nothing to move: the key stops here all
      // the same, so `j` on an empty list does not scroll the webview.
      return event.key.length === 1;
    }
  }

  for (const action of ACTIONS) {
    if (!matches(action.binding, event, primary)) continue;
    if (action.where === "repository" && !app.open) return false;
    if (!action.enabled()) return true;
    action.run();
    return true;
  }
  return false;
}

/// Put the caret in this screen's filter, and say whether there was one.
///
/// The History filter row is folded away by default, so `/` unfolds it first:
/// a key that focused a box nobody can see would be a key that does nothing.
function focusFilter(): boolean {
  if (app.screen === "repositories") {
    return focus(".library-filter input");
  }
  if (app.screen === "history") {
    if (!app.showFilters) store.toggleFilters();
    // After the row has been drawn, not before it exists.
    queueMicrotask(() => focus(".filter-box"));
    return true;
  }
  return false;
}

function focus(selector: string): boolean {
  const box = document.querySelector<HTMLInputElement>(selector);
  if (!box) return false;
  box.focus();
  box.select();
  return true;
}

/// The binding as the interface prints it: `⌘F`, `⇧⌘N`, `Ctrl F`.
///
/// DESIGN §2: the app shows one modifier or the other, never both, and never
/// spells them together — `⌘O` is a glyph beside a letter, `Ctrl O` is a word
/// beside one.
export function hint(binding: string, modifier: string): string {
  const parts = binding.split("+");
  const key = parts[parts.length - 1]!;
  const glyph = modifier === "⌘";
  const shift = parts.includes("Shift") ? (glyph ? "⇧" : "Maj ") : "";
  const primary = parts.includes("Primary") ? (glyph ? modifier : `${modifier} `) : "";
  return `${shift}${primary}${key.toUpperCase()}`;
}

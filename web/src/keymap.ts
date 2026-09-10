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
// ## What is not here
//
// Movement — `j` `k`, `1` `2` `3`, `/`, the tab-stop order of DESIGN §5 — which
// needs a notion of zones and focus this app does not have yet. It is the next
// slice, and it is why the table already carries a `where`: those bindings are
// bare letters, and a bare letter is only safe when something says which zone
// is listening.

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
  return !!app.question || !!app.clone || !!app.resolving;
}

/// Run whichever action the event names, and say whether one did.
///
/// A binding with a modifier fires even while typing — `⌘F` in a text box is
/// still Fetch, and every application on both platforms behaves that way. A
/// bare key never does, which is what will make the movement letters of the
/// next slice safe.
export function dispatch(event: KeyboardEvent): boolean {
  if (overlaid()) return false;
  const primary = app.platform?.modifier === "command" ? "meta" : "control";
  const bare = !event.metaKey && !event.ctrlKey && !event.altKey;
  if (bare && typing(event.target)) return false;

  for (const action of ACTIONS) {
    if (!matches(action.binding, event, primary)) continue;
    if (action.where === "repository" && !app.open) return false;
    if (!action.enabled()) return true;
    action.run();
    return true;
  }
  return false;
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

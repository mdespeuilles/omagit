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
import { t, type Key } from "./i18n";
import * as store from "./state";

/// Where a binding is allowed to fire.
///
/// `always` is the whole window; `repository` needs one open, which is what
/// keeps `⌘F` from fetching on the Repositories screen where there is nothing
/// to fetch from.
export type Where = "always" | "repository";

/// Which native menu an action appears under (SPEC §9: App, Fichier, Édition,
/// Affichage, Dépôt, Fenêtre, Aide).
///
/// Declared here rather than in the Rust that builds the bar, for the reason
/// this table exists at all: the menu is the fourth thing to read it, after the
/// key handler, the palette and the `?` sheet. Édition, Fenêtre and half of App
/// hold no omagit action — they are the platform's own items, and the backend
/// adds them.
///
/// The label a menu shows is the label the palette shows. One label per action:
/// if it reads badly under a menu title, it reads badly in the palette too, and
/// the fix is the label rather than a second one.
export type Place = "app" | "file" | "view" | "repository" | "help";

export type Action = {
  /// Stable, and never shown: the palette searches labels, and settings will
  /// store bindings against this.
  id: string;
  /// What the palette, the sheet, the menu bar and the topbar's tooltips print
  /// — a catalogue key, so the four of them say it in the language in force.
  /// `labelOf` is what turns it into words.
  label: Key;
  /// `Primary` is ⌘ on macOS and Ctrl elsewhere (SPEC §9). Parsed rather than
  /// stored as flags so the sheet can print it and settings can hold it as one
  /// string.
  binding: string;
  where: Where;
  menu: Place;
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
export const MOVEMENTS: { print: string; label: Key }[] = [
  { print: "1 2 3", label: "move.zones" },
  { print: "Tab · ⇧Tab", label: "move.tab" },
  { print: "j k · ↓ ↑", label: "move.updown" },
  { print: "g g · G", label: "move.ends" },
  { print: "⏎", label: "move.enter" },
  { print: "/", label: "move.filter" },
  { print: "Esc", label: "move.escape" },
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
    label: "action.repository.add",
    binding: "Primary+O",
    where: "always",
    menu: "file",
    enabled: () => !app.gitUnusable,
    run: () => void store.addRepository(),
  },
  {
    id: "repository.clone",
    label: "action.repository.clone",
    binding: "Shift+Primary+N",
    where: "always",
    menu: "file",
    enabled: () => !app.gitUnusable && idle(),
    run: () => store.openClone(),
  },
  {
    id: "repository.newGroup",
    label: "action.repository.newGroup",
    // No default: the window has seven Primary bindings already and a folder
    // is made a handful of times in a lifetime. It is in the table anyway,
    // because the table is what the palette, the menu bar and the `?` sheet
    // read — and because a binding can be *given* to it from Preferences,
    // which is the whole point of SPEC §11's reassignable keymap.
    binding: "",
    where: "always",
    menu: "file",
    enabled: () => true,
    run: () => {
      // On the screen where it can be seen: a folder made while the window is
      // looking at a diff is a folder nobody watched appear.
      store.showScreen("repositories");
      void store.createGroup();
    },
  },
  {
    id: "repository.all",
    label: "action.repository.all",
    binding: "Shift+Primary+O",
    where: "always",
    menu: "file",
    enabled: () => true,
    run: () => store.showScreen("repositories"),
  },
  {
    id: "screen.workingCopy",
    label: "action.screen.workingCopy",
    binding: "Primary+1",
    where: "repository",
    menu: "view",
    enabled: inRepository,
    run: () => store.showScreen("working-copy"),
  },
  {
    id: "screen.history",
    label: "action.screen.history",
    binding: "Primary+2",
    where: "repository",
    menu: "view",
    enabled: inRepository,
    run: () => store.showScreen("history"),
  },
  {
    id: "screen.stashes",
    label: "action.screen.stashes",
    binding: "Primary+3",
    where: "repository",
    menu: "view",
    enabled: inRepository,
    run: () => store.showScreen("stashes"),
  },
  {
    id: "network.fetch",
    label: "action.network.fetch",
    binding: "Primary+F",
    where: "repository",
    menu: "repository",
    enabled: () => inRepository() && idle() && !app.gitUnusable,
    run: () => store.fetchRemote(),
  },
  {
    id: "network.pull",
    label: "action.network.pull",
    binding: "Shift+Primary+P",
    where: "repository",
    menu: "repository",
    enabled: () => inRepository() && idle() && !!app.summary?.tracking,
    run: () => store.pullRemote(),
  },
  {
    id: "network.push",
    label: "action.network.push",
    binding: "Primary+P",
    where: "repository",
    menu: "repository",
    enabled: () => inRepository() && idle() && !app.gitUnusable,
    run: () => store.pushBranch(false),
  },
  {
    id: "network.stop",
    label: "action.network.stop",
    binding: "Primary+.",
    where: "always",
    menu: "repository",
    enabled: () => !!app.running,
    run: () => store.stopNetwork(),
  },
  {
    id: "workingCopy.refresh",
    label: "action.workingCopy.refresh",
    binding: "Primary+R",
    where: "repository",
    menu: "repository",
    enabled: inRepository,
    run: () => void store.refresh(),
  },
  {
    id: "stash.push",
    label: "action.stash.push",
    binding: "Shift+Primary+S",
    where: "repository",
    menu: "repository",
    enabled: () => inRepository() && idle(),
    run: () => {
      store.showScreen("stashes");
      store.openStashForm();
    },
  },
  {
    id: "palette.open",
    label: "action.palette.open",
    binding: "Primary+K",
    where: "always",
    menu: "view",
    enabled: () => true,
    run: () => store.openPalette(),
  },
  {
    id: "help.shortcuts",
    label: "action.help.shortcuts",
    // `Shift+?`, not `Shift+/`, however it is engraved on the key: a browser
    // reports the character the layout produced, and holding Shift over `/`
    // produces `?`. Written the other way it matched nothing, on any layout.
    binding: "Shift+?",
    where: "always",
    menu: "help",
    enabled: () => true,
    run: () => store.toggleShortcuts(),
  },
  {
    id: "settings.open",
    label: "action.settings.open",
    binding: "Primary+,",
    where: "always",
    menu: "app",
    enabled: () => true,
    run: () => store.openSettings(),
  },
  {
    id: "journal.toggle",
    label: "action.journal.toggle",
    binding: "Shift+Primary+J",
    where: "always",
    menu: "view",
    enabled: () => true,
    run: () => store.toggleJournal(),
  },
];

// ── Reassignment (SPEC §11) ─────────────────────────────────────────────────
//
// The table above is the *default* keymap. What the user changed is held in the
// store, read from `settings.toml`, and everything that answers or prints a
// binding goes through `binding()` — so a reassignment reaches the key handler,
// the palette, the sheet and the macOS menu bar at once, because all four read
// the same table through the same accessor.

/// What an action is called, in the language in force.
export function labelOf(action: Action): string {
  return t(action.label);
}

/// The binding this action answers now: the user's, or the table's own.
export function binding(action: Action): string {
  return app.keymap[action.id] ?? action.binding;
}

/// Whether this action is on a binding the user chose.
export function reassigned(action: Action): boolean {
  return app.keymap[action.id] !== undefined && app.keymap[action.id] !== action.binding;
}

/// The keys movement owns, which no command may take.
///
/// Not a copy of `MOVES`: `Tab`, `/`, `g` and `Enter` are answered by hand in
/// `dispatch` before the table is consulted, so a list built from `MOVES` alone
/// would miss four of them.
const MOVEMENT_KEYS = [
  "1",
  "2",
  "3",
  "j",
  "k",
  "g",
  "G",
  "Tab",
  "Enter",
  "Escape",
  "/",
  "ArrowDown",
  "ArrowUp",
];

/// A key press as a binding, or `null` when it is not one yet.
///
/// Used by the reassignment screen: the way to say which keys you want is to
/// press them. A press of a modifier *alone* is not an answer — it is the first
/// half of one — so it returns `null` and the screen keeps listening.
export function capture(event: KeyboardEvent, primary: "meta" | "control"): string | null {
  const key = event.key;
  if (["Shift", "Control", "Alt", "Meta", "CapsLock", "Dead"].includes(key)) return null;
  const parts: string[] = [];
  if (event.shiftKey) parts.push("Shift");
  if (event.altKey) parts.push("Alt");
  if (primary === "meta" ? event.metaKey : event.ctrlKey) parts.push("Primary");
  // The character the layout produced, upper-cased so the table reads as it is
  // written — `matches` lower-cases both sides, so this is spelling, not
  // meaning.
  parts.push(key.length === 1 ? key.toUpperCase() : key);
  return parts.join("+");
}

/// Why this binding cannot be given to this action, or `null` when it can.
///
/// Refused rather than accepted-and-broken: each of these is a binding that
/// would look assigned in the settings screen and answer nothing at the keyboard.
export function refuse(id: string, chosen: string): string | null {
  const parts = chosen.split("+");
  const key = parts[parts.length - 1]!;
  // What `dispatch` calls a bare event: no primary, no alt. Shift alone counts,
  // which is why `⇧G` is refused below — movement answers `G` before the table
  // is ever consulted.
  const bare = !parts.includes("Primary") && !parts.includes("Alt");

  if (bare && MOVEMENT_KEYS.some((movement) => movement.toLowerCase() === key.toLowerCase())) {
    return t("bind.movement", { key });
  }
  const taken = ACTIONS.find((action) => action.id !== id && binding(action) === chosen);
  if (taken) return t("bind.taken", { action: labelOf(taken) });
  return null;
}

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
///
/// Every dialog belongs here, and two did not: the tag dialog, which arrived
/// without being added, and Preferences, which used to be a screen. A list that
/// has to be remembered at each new dialog is a list that will be forgotten
/// again — but the alternative, asking the DOM whether an overlay is mounted,
/// makes the keyboard depend on rendering. This stays a list, and the tests
/// name it.
function overlaid(): boolean {
  return (
    !!app.question ||
    !!app.clone ||
    !!app.resolving ||
    !!app.palette ||
    !!app.tagging ||
    app.shortcuts ||
    app.showSettings
  );
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
    // `Tab` is not here, and that is the decision rather than an omission: it
    // belongs to the browser, which walks the window's tab stops in DOM order.
    // Answering it meant `preventDefault` on every press, which is how the
    // window ended up with *no* button reachable from the keyboard at all
    // (board 09, §2.52). The stops are declared in the markup instead.
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
    if (!matches(binding(action), event, primary)) continue;
    if (action.where === "repository" && !app.open) return false;
    if (!action.enabled()) return true;
    action.run();
    return true;
  }
  return false;
}

/// Run the action with this id, if it can run at all right now.
///
/// The way in for everything that names an action rather than pressing its key:
/// the native menu bar (SPEC §9), and anything after it. The two rules it keeps
/// are `dispatch`'s own — a screen that is not open refuses, and an action that
/// cannot run now does nothing — but not the third: whether the key is
/// *swallowed* is a question only a key press asks, and a menu item that did
/// nothing has already told the truth by being greyed.
export function runAction(id: string): boolean {
  const action = ACTIONS.find((entry) => entry.id === id);
  if (!action) return false;
  if (action.where === "repository" && !app.open) return false;
  if (!action.enabled()) return false;
  action.run();
  return true;
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
  // In the platform's own order: ⌥⇧⌘ on macOS is how every menu there prints
  // it. `Alt` had no printed form at all until bindings became reassignable —
  // nothing in the table used it, so `⌥⌘F` came out as `⌘F`, a hint that named
  // a key the app did not answer.
  const alt = parts.includes("Alt") ? (glyph ? "⌥" : "Alt ") : "";
  const shift = parts.includes("Shift") ? (glyph ? "⇧" : "Maj ") : "";
  const primary = parts.includes("Primary") ? (glyph ? modifier : `${modifier} `) : "";
  return `${alt}${shift}${primary}${key.toUpperCase()}`;
}

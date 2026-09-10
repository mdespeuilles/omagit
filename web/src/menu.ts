// The native menu bar, on the platform that requires one (SPEC §9).
//
// macOS is firm about this — "sans elle, l'app paraît cassée" — and it is not
// only about looking finished: the Édition menu is where `⌘Z`, `⌘A` and `⌘C`
// inside a web view come from. Without it, the commit box has no undo.
//
// Nothing here decides *what* is in the menu. The table in `keymap.ts` does, and
// this file is the fourth thing to read it after the key handler, the palette
// and the `?` sheet. What it adds is the two directions of the wire: the entries
// out, the chosen id back.

import { watch } from "vue";
import { listen } from "@tauri-apps/api/event";
import { api } from "./ipc";
import { ACTIONS, binding, labelOf, runAction } from "./keymap";
import { t } from "./i18n";
import { app } from "./state";

/// One item, as the backend needs it.
export type Entry = {
  id: string;
  label: string;
  binding: string;
  menu: string;
  enabled: boolean;
};

/// The table, arranged for the bar.
///
/// `enabled` is coarse on purpose: an item is greyed when the *screen* cannot do
/// it — no repository open — and not when a fetch happens to be running. The
/// fine-grained refusal stays in the action's own `enabled()`, because the
/// alternative is rebuilding the menu bar on every state change, including while
/// one of its menus is pulled down.
export function entries(): Entry[] {
  return ACTIONS.map((action) => ({
    id: action.id,
    label: labelOf(action),
    binding: binding(action),
    menu: action.menu,
    enabled: action.where === "always" || !!app.open,
  }));
}

/// The words for everything in the bar that is not an action of ours: the menu
/// titles, and the platform's own items. `menu.rs` looks these keys up and
/// falls back to English, so a catalogue that misses one still builds a bar.
const LABELS = [
  "menu.about",
  "menu.services",
  "menu.hide",
  "menu.hideOthers",
  "menu.showAll",
  "menu.quit",
  "menu.file",
  "menu.closeWindow",
  "menu.edit",
  "menu.undo",
  "menu.redo",
  "menu.cut",
  "menu.copy",
  "menu.paste",
  "menu.selectAll",
  "menu.view",
  "menu.fullscreen",
  "menu.repository",
  "menu.window",
  "menu.minimize",
  "menu.zoom",
  "menu.help",
] as const;

export function labels(): Record<string, string> {
  return Object.fromEntries(LABELS.map((key) => [key, t(key)]));
}

/// Whether this platform wants a menu bar at all.
///
/// Linux has none by design (SPEC §9): everything is in the window there, and a
/// menu bar would be a second place to find the same commands.
export function wanted(): boolean {
  return app.platform?.name === "macos";
}

/// Build the bar, keep it in step, and run what is chosen from it.
///
/// Called once, after `boot` — the platform has to be known before the question
/// "does this machine want a menu?" can be asked.
export async function installMenu(): Promise<void> {
  if (!wanted()) return;
  await send();
  // Watched rather than called from wherever the change happens, so a second
  // way to open a repository — or a third place a binding can be changed from —
  // cannot forget the menu. What is watched is what the bar would *show*:
  // opening a repository ungreys half of it, and a reassignment moves an
  // accelerator.
  watch(signature, () => void send());
  await listen<string>("menu", (event) => {
    // Ids the table does not know are the platform's own — Quitter, Coller,
    // Plein écran — and the window system has already handled them.
    runAction(event.payload);
  });
}

/// Everything the bar draws, in one string.
///
/// Rebuilding a menu bar is cheap but not free, and doing it on every state
/// change would mean doing it while one of its menus is pulled down.
function signature(): string {
  // The language is in it: the bar is words, and words change with it.
  return [
    ...entries().map((entry) => `${entry.id}:${entry.binding}:${entry.enabled}`),
    t("menu.file"),
  ].join(" ");
}

let sent = "";

async function send(): Promise<void> {
  const now = signature();
  if (now === sent) return;
  sent = now;
  await api.setMenu(entries(), labels());
}

/// For tests: the bar has no memory of a previous window.
export function forgetMenu(): void {
  sent = "";
}

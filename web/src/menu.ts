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
import { ACTIONS, runAction } from "./keymap";
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
    label: action.label,
    binding: action.binding,
    menu: action.menu,
    enabled: action.where === "always" || !!app.open,
  }));
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
  // Opening or closing a repository is the one change that moves what the bar
  // offers. Watched rather than called from `openRepository`, so a second way
  // to open one cannot forget the menu.
  watch(
    () => !!app.open,
    () => void send(),
  );
  await listen<string>("menu", (event) => {
    // Ids the table does not know are the platform's own — Quitter, Coller,
    // Plein écran — and the window system has already handled them.
    runAction(event.payload);
  });
}

/// The bar as it should be now, sent only if that differs from what it is.
///
/// Rebuilding a menu bar is cheap but not free, and doing it on every state
/// change would mean doing it while one of its menus is pulled down.
let sent = "";

async function send(): Promise<void> {
  const now = entries();
  const signature = now.map((entry) => `${entry.id}:${entry.enabled}`).join(" ");
  if (signature === sent) return;
  sent = signature;
  await api.setMenu(now);
}

/// For tests: the bar has no memory of a previous window.
export function forgetMenu(): void {
  sent = "";
}

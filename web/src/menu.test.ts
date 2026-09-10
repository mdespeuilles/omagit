// The native menu bar (SPEC §9), and the property that keeps it honest: it is
// the keymap table, arranged — not a second list of what the app can do.
//
// Everything here runs against the fake backend, so what is asserted is the
// *entries sent*, not what AppKit drew. The Rust side has its own test for the
// half this cannot see: which binding may become an accelerator.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { Repository } from "./backend.fake";

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("./backend.fake").Repository>,
  /// The handlers `listen("menu")` registered, so a test can pick an item.
  listeners: [] as ((event: { payload: string }) => void)[],
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: (event: { payload: string }) => void) => {
    if (name === "menu") backend.listeners.push(handler);
    return Promise.resolve(() => {});
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: () => Promise.resolve("/repo") }));

beforeEach(() => {
  backend.listeners = [];
});

async function started(os: "linux" | "macos", withRepository = false) {
  backend.current = new Repository([
    { path: "a.txt", staged: null, unstaged: "modified", hunks: 1 },
  ]);
  backend.current.os = os;
  vi.resetModules();
  const state = await import("./state");
  await state.boot();
  if (withRepository) await state.openRepository("/repo");
  const menu = await import("./menu");
  await menu.installMenu();
  return { state, menu };
}

/// The entries of the last `set_menu`, or none if it was never called.
function lastSent(): Record<string, unknown>[] | null {
  const calls = backend.current.calls.filter((call) => call.command === "set_menu");
  const last = calls[calls.length - 1];
  return last ? (last.args["entries"] as Record<string, unknown>[]) : null;
}

const pick = (id: string) => backend.listeners.forEach((handler) => handler({ payload: id }));

describe("the menu bar", () => {
  it("is not built on a platform that has none", async () => {
    // SPEC §9: Linux has no menu bar. Everything is in the window there, and a
    // second place to find the same commands is a place that goes stale.
    await started("linux");
    expect(lastSent()).toBeNull();
  });

  it("sends the keymap table, every action with a menu", async () => {
    const { menu } = await started("macos");
    const keymap = await import("./keymap");
    const sent = lastSent()!;

    expect(sent).toHaveLength(keymap.ACTIONS.length);
    for (const action of keymap.ACTIONS) {
      const entry = sent.find((row) => row["id"] === action.id)!;
      expect(entry, `${action.id} is in the bar`).toBeTruthy();
      expect(entry["label"]).toBe(action.label);
      expect(entry["binding"]).toBe(action.binding);
      expect(entry["menu"]).toBe(action.menu);
    }
    // And the arrangement the backend expects: every place it knows how to draw.
    const places = new Set(menu.entries().map((entry) => entry.menu));
    for (const place of places) {
      expect(["app", "file", "view", "repository", "help"]).toContain(place);
    }
  });

  it("greys what needs a repository until one is open", async () => {
    await started("macos");
    const before = lastSent()!;
    const fetch = (rows: Record<string, unknown>[]) =>
      rows.find((row) => row["id"] === "network.fetch")!;
    expect(fetch(before)["enabled"]).toBe(false);
    expect(before.find((row) => row["id"] === "repository.add")!["enabled"]).toBe(true);
  });

  it("sends the bar again when a repository opens, and only then", async () => {
    const { state } = await started("macos");
    const sends = () => backend.current.calls.filter((call) => call.command === "set_menu").length;
    const built = sends();

    await state.openRepository("/repo");
    // The watcher runs on Vue's own turn.
    await new Promise((resume) => setTimeout(resume, 0));
    expect(sends()).toBe(built + 1);
    expect(lastSent()!.find((row) => row["id"] === "network.fetch")!["enabled"]).toBe(true);

    // Reading the same repository again changes nothing the bar shows.
    await state.refresh();
    await new Promise((resume) => setTimeout(resume, 0));
    expect(sends()).toBe(built + 1);
  });

  it("runs the action a chosen item names, through the same rules", async () => {
    const { state } = await started("macos", true);
    pick("screen.history");
    expect(state.app.screen).toBe("history");
  });

  it("ignores an item the table does not know", async () => {
    // Quitter, Coller, Plein écran: the window system has already handled them.
    const { state } = await started("macos", true);
    const before = state.app.screen;
    pick("__tauri_paste__");
    expect(state.app.screen).toBe(before);
  });

  it("refuses an action whose screen is not open", async () => {
    const { state } = await started("macos");
    pick("screen.history");
    expect(state.app.screen).toBe("repositories");
  });
});

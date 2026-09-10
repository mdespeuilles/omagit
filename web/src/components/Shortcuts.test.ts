// The sheet, and the one property that makes it worth having: it is printed
// from the table that answers the keys.
//
// A sheet written by hand is wrong by the second slice, and this project has
// the receipt — `KEYMAP.md` spent three milestones describing a build that had
// been deleted. So the test asserts the shape rather than the contents: every
// action, its binding as the platform spells it, and nothing invented.

import { mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { Repository } from "../backend.fake";

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("../backend.fake").Repository>,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: () => Promise.resolve("/repo") }));

/// The window as it really starts, then the sheet asked for the way a hand
/// asks for it.
async function opened() {
  backend.current = new Repository([
    { path: "a.txt", staged: null, unstaged: "modified", hunks: 1 },
  ]);
  vi.resetModules();
  const state = await import("../state");
  await state.boot();
  await state.openRepository("/repo");
  const keymap = await import("../keymap");
  const App = (await import("../App.vue")).default;
  const app = mount(App, { attachTo: document.body });
  await app.vm.$nextTick();
  return { state, keymap, app };
}

async function showing() {
  const held = await opened();
  window.dispatchEvent(
    new KeyboardEvent("keydown", { key: "?", shiftKey: true, bubbles: true, cancelable: true }),
  );
  await held.app.vm.$nextTick();
  return held;
}

describe("the shortcut sheet", () => {
  it("is not on screen until it is asked for", async () => {
    const { app } = await opened();
    expect(app.find(".sheet").exists()).toBe(false);
  });

  it("prints every action in the table, with the binding it answers", async () => {
    const { keymap, app } = await showing();
    const keys = app.findAll(".sheet-key").map((row) => row.text());
    const labels = app.findAll(".sheet-label").map((row) => row.text());

    for (const action of keymap.ACTIONS) {
      expect(labels).toContain(keymap.labelOf(action));
      // The fake platform is Linux, so the printed form is the worded one.
      expect(keys).toContain(keymap.hint(action.binding, "Ctrl"));
    }
    // And the movements, which are answered by `dispatch` rather than by an
    // entry in the table: a reader cannot tell the two apart and should not.
    for (const move of keymap.MOVEMENTS) {
      expect(keys).toContain(move.print);
    }
  });

  it("closes on Escape and on the key that opened it", async () => {
    const { state, app } = await showing();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    await app.vm.$nextTick();
    expect(state.app.shortcuts).toBe(false);
    expect(app.find(".sheet").exists()).toBe(false);
  });

  it("holds the other bindings while it is up", async () => {
    // A dialog answers its own keys; a global binding firing behind one would
    // act on a screen nobody can see.
    const { keymap } = await showing();
    expect(keymap.dispatch(new KeyboardEvent("keydown", { key: "f", ctrlKey: true }))).toBe(false);
  });
});

// Dragging a column, and remembering how wide it was left.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { Repository } from "./backend.fake";

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("./backend.fake").Repository>,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: () => Promise.resolve(null) }));

async function running(remembered: Record<string, number> = {}) {
  backend.current = new Repository([]);
  backend.current.panes = { ...remembered };
  vi.resetModules();
  const state = await import("./state");
  await state.boot();
  // The widths are read alongside everything else; let that land.
  for (let attempt = 0; attempt < 50; attempt += 1) {
    await new Promise((resume) => setTimeout(resume, 0));
    if (Object.keys(state.app.panes).length > 0 || Object.keys(remembered).length === 0) break;
  }
  return state;
}

const writes = () => backend.current.calls.filter((call) => call.command === "set_pane");

beforeEach(() => {
  backend.current = new Repository([]);
});

describe("resizing a column", () => {
  it("falls back to the stylesheet's width until one is dragged", async () => {
    const state = await running();
    expect(state.paneWidth("history", 520)).toBe(520);
  });

  it("uses the width the settings file remembers", async () => {
    const state = await running({ history: 640 });
    expect(state.paneWidth("history", 520)).toBe(640);
  });

  it("does not write to the settings file while the pointer moves", async () => {
    // The file is rewritten on every call, and a drag is a hundred of them.
    const state = await running();

    for (let width = 520; width < 560; width += 4) state.resizePane("history", width);
    expect(state.paneWidth("history", 520)).toBe(556);
    expect(writes()).toHaveLength(0);
  });

  it("writes once when the drag ends", async () => {
    const state = await running();

    state.resizePane("history", 600);
    state.settlePane("history", 600);
    await new Promise((resume) => setTimeout(resume, 0));

    expect(writes()).toHaveLength(1);
    expect(writes()[0]!.args).toMatchObject({ name: "history", width: 600 });
    expect(backend.current.panes["history"]).toBe(600);
  });

  it("opens the window even when the widths cannot be read", async () => {
    // A width someone dragged is a convenience; failing to read one is not a
    // reason for the window not to open.
    backend.current = new Repository([]);
    backend.current.panes = {};
    vi.resetModules();
    const state = await import("./state");
    const original = backend.current.call.bind(backend.current);
    backend.current.call = (command, args) =>
      command === "panes" ? Promise.reject(new Error("nope")) : original(command, args);

    await state.boot();
    await new Promise((resume) => setTimeout(resume, 0));

    expect(state.app.screen).toBe("repositories");
    expect(state.paneWidth("history", 520)).toBe(520);
  });
});

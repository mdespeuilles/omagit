// Following the system palette (SPEC §6.1, source 2).
//
// The window read the theme once at start-up and then ignored the system for
// the rest of the session — switching the Omarchy theme left every open omagit
// window on the old palette. The backend now pushes a new stylesheet when the
// palette really changes; this is the half that puts it on the element.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { Repository } from "./backend.fake";

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("./backend.fake").Repository>,
  /// The handlers `listen` registered, so a test can push an event.
  themed: [] as ((event: { payload: string }) => void)[],
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: (event: { payload: string }) => void) => {
    if (name === "theme") backend.themed.push(handler);
    return Promise.resolve(() => {});
  },
}));

beforeEach(() => {
  backend.current = new Repository([]);
  backend.themed = [];
});

describe("the system palette", () => {
  it("repaints the window without a restart", async () => {
    vi.resetModules();
    const state = await import("./state");
    await state.boot();
    await state.watchTheme();

    const applied = document.documentElement.getAttribute("style");
    expect(applied).toContain("--bg");

    backend.themed[0]!({ payload: "--bg: #1a1b26; --text: #c0caf5;" });

    expect(document.documentElement.getAttribute("style")).toBe("--bg: #1a1b26; --text: #c0caf5;");
  });
});

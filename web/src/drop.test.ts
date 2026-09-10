// A folder dropped on the window (§5, sixteenth defect).
//
// M3 accepted one; the port to Tauri did not carry it over, and the library was
// left with a single door — the platform's open panel, which on macOS cannot
// even show a `/var/folders/…` path. The fixture script moved house because of
// it.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { Repository } from "./backend.fake";

type Payload =
  | { type: "over"; position: { x: number; y: number } }
  | { type: "drop"; paths: string[]; position: { x: number; y: number } }
  | { type: "leave" };

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("./backend.fake").Repository>,
  /// The handler `onDragDropEvent` registered, so a test can drop something.
  handler: null as null | ((event: { payload: Payload }) => void),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: () => Promise.resolve(null) }));
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: (handler: (event: { payload: Payload }) => void) => {
      backend.handler = handler;
      return Promise.resolve(() => {});
    },
  }),
}));

async function watching() {
  backend.current = new Repository([]);
  backend.current.library = [];
  vi.resetModules();
  const state = await import("./state");
  await state.boot();
  const drop = await import("./drop");
  await drop.watchDrops();
  return state;
}

const drop = (paths: string[]) =>
  backend.handler?.({ payload: { type: "drop", paths, position: { x: 0, y: 0 } } });

async function settled(state: typeof import("./state")): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    await new Promise((resume) => setTimeout(resume, 0));
    if (!state.app.busy) return;
  }
  throw new Error("le dépôt n'est jamais arrivé");
}

beforeEach(() => {
  backend.handler = null;
});

describe("a folder dropped on the window", () => {
  it("is added to the library, by path", async () => {
    const state = await watching();
    drop(["/repo"]);
    await settled(state);

    const added = backend.current.calls.filter((call) => call.command === "add_repository");
    expect(added).toHaveLength(1);
    expect(added[0]!.args["path"]).toBe("/repo");
    expect(state.app.repositories.some((row) => row.path === "/repo")).toBe(true);
    // Where they land is where the window goes: a row added on another screen
    // would look like a drop that did nothing.
    expect(state.app.screen).toBe("repositories");
  });

  it("adds every folder of a drop, and reports the one that is not a repository", async () => {
    const state = await watching();
    backend.current.notARepository = "/not-a-repo";
    drop(["/repo", "/not-a-repo"]);
    await settled(state);

    expect(state.app.repositories.some((row) => row.path === "/repo")).toBe(true);
    expect(state.app.addError).toContain("/not-a-repo");
  });

  it("says the window is a target while something is over it", async () => {
    const state = await watching();
    backend.handler?.({ payload: { type: "over", position: { x: 0, y: 0 } } });
    expect(state.app.dragging).toBe(true);

    backend.handler?.({ payload: { type: "leave" } });
    expect(state.app.dragging).toBe(false);
  });

  it("lets go of the target once the drop lands", async () => {
    const state = await watching();
    backend.handler?.({ payload: { type: "over", position: { x: 0, y: 0 } } });
    drop(["/repo"]);
    await settled(state);
    expect(state.app.dragging).toBe(false);
  });
});

// Noticing that the repository changed underneath us (SPEC §10).
//
// The window used to refresh only when clicked, which is wrong every time you
// touch a terminal — a commit from the shell, a branch switched, a formatter
// run over the tree. `omagit-git`'s watcher had been written since M2,
// debounced and targeted, and nothing had ever called it.
//
// The debouncing and the classification are tested against a real filesystem in
// `crates/omagit-git/tests/watch.rs`. What is tested here is the other half:
// which of those notifications this window acts on, and which it ignores.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { Repository } from "./backend.fake";
import type { Changed, StatusRow } from "./ipc";

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("./backend.fake").Repository>,
  /// The handlers `listen("changed")` registered, so a test can report one.
  listeners: [] as ((event: { payload: Changed }) => void)[],
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: (event: { payload: Changed }) => void) => {
    if (name === "changed") backend.listeners.push(handler);
    return Promise.resolve(() => {});
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: () => Promise.resolve(null) }));

async function watching() {
  backend.current = new Repository([
    { path: "a.txt", staged: null, unstaged: "modified", hunks: 1 },
  ]);
  vi.resetModules();
  const state = await import("./state");
  await state.boot();
  await state.openRepository("/repo");
  await settled(state);
  await state.watchRepository();
  return state;
}

async function settled(state: typeof import("./state")): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    await new Promise((resume) => setTimeout(resume, 0));
    if (!state.app.busy && state.app.status.status !== "loading") return;
  }
  throw new Error("l'écran ne s'est jamais stabilisé");
}

const changed = (over: Partial<Changed> = {}) =>
  backend.listeners.forEach((handler) =>
    handler({ payload: { path: "/repo", status: true, refs: false, ...over } }),
  );

const files = (state: typeof import("./state")) =>
  state.app.status.status === "ready" ? state.app.status.value.map((row) => row.path) : [];

const reads = (command: string) =>
  backend.current.calls.filter((call) => call.command === command).length;

beforeEach(() => {
  backend.listeners = [];
});

describe("a repository that changes underneath us", () => {
  it("re-reads the status without anyone clicking", async () => {
    const state = await watching();
    expect(files(state)).toEqual(["a.txt"]);

    // Something wrote a file — a terminal, an editor, a build.
    backend.current.files.push({
      path: "b.txt",
      staged: null,
      unstaged: "modified",
      hunks: 1,
    });
    changed();
    await settled(state);

    expect(files(state)).toEqual(["a.txt", "b.txt"]);
  });

  it("ignores a repository that is not the one on screen", async () => {
    // Every open tab is watched, because the watch travels with the handle.
    const state = await watching();
    const before = reads("status");

    changed({ path: "/elsewhere" });
    await settled(state);

    expect(reads("status")).toBe(before);
  });

  it("stays out of the way of our own write", async () => {
    // A write of ours produces a burst of events, and `settle()` already
    // re-reads at the end of one. Reacting in the middle would read an index
    // that is being rewritten.
    const state = await watching();
    backend.current.holdWrite = new Promise(() => {});
    const row: StatusRow = {
      path: "a.txt",
      staged: null,
      unstaged: "modified",
      conflict: null,
      code: " M",
    };
    state.stageFile(row, false);
    await new Promise((resume) => setTimeout(resume, 0));
    expect(state.app.busy).not.toBeNull();

    const before = reads("status");
    changed();
    await new Promise((resume) => setTimeout(resume, 0));
    expect(reads("status")).toBe(before);
  });

  it("does not walk the status for a branch that moved", async () => {
    // The backend says which invalidation it is, and a ref moving does not
    // cost a status walk.
    const state = await watching();
    const status = reads("status");
    const refs = reads("refs");

    changed({ status: false, refs: true });
    await settled(state);

    expect(reads("status")).toBe(status);
    expect(reads("refs")).toBe(refs + 1);
  });
});

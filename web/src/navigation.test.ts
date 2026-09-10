// DESIGN §5's vocabulary: three zones, movement inside one, and Esc going up a
// level rather than out.
//
// Tested against the state rather than against focus. The lists are
// virtualised, so the row the keyboard is on is routinely not rendered —
// `document.activeElement` cannot be the record of where the keyboard is when
// the element under it comes and goes with the scroll.

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

const files = [
  { path: "a.txt", staged: null, unstaged: "modified", hunks: 1 },
  { path: "b.txt", staged: null, unstaged: "modified", hunks: 1 },
  { path: "c.txt", staged: null, unstaged: "modified", hunks: 1 },
];

async function opened() {
  backend.current = new Repository(files.map((file) => ({ ...file })));
  backend.current.branches = [
    {
      name: "main",
      commit: { full: "m".repeat(40), short: "mmmmmmm" },
      head: true,
      tracking: null,
      merged: true,
      age: 0,
    },
    {
      name: "feature/one",
      commit: { full: "f".repeat(40), short: "fffffff" },
      head: false,
      tracking: null,
      merged: false,
      age: 0,
    },
  ];
  vi.resetModules();
  const state = await import("./state");
  const keymap = await import("./keymap");
  await state.boot();
  await state.openRepository("/repo");
  await settled(state);
  return { state, keymap };
}

async function settled(state: typeof import("./state")): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    await new Promise((resume) => setTimeout(resume, 0));
    if (!state.app.busy && state.app.refs.status !== "loading") return;
  }
  throw new Error("l'écran ne s'est jamais stabilisé");
}

const press = (key: string, held: Partial<KeyboardEvent> = {}) =>
  new KeyboardEvent("keydown", { key, cancelable: true, ...held });

beforeEach(() => {
  backend.current = new Repository([]);
});

describe("the three zones", () => {
  it("goes where 1 2 3 say, on every screen", async () => {
    const { state, keymap } = await opened();
    expect(keymap.dispatch(press("2"))).toBe(true);
    expect(state.app.zone).toBe(2);
    keymap.dispatch(press("3"));
    expect(state.app.zone).toBe(3);
    keymap.dispatch(press("1"));
    expect(state.app.zone).toBe(1);
  });

  it("wraps rather than letting focus out of the window", async () => {
    const { state } = await opened();
    state.goToZone(3);
    state.nextZone(1);
    expect(state.app.zone).toBe(1);
    state.nextZone(-1);
    expect(state.app.zone).toBe(3);
  });
});

describe("moving inside a zone", () => {
  it("walks the file list with j and k, and stops at the ends", async () => {
    // Stops rather than wraps: a list that jumped from the last row back to the
    // first would lose the place a reader was holding.
    const { state, keymap } = await opened();
    state.goToZone(2);
    expect(state.app.selected?.path).toBe("a.txt");

    keymap.dispatch(press("j"));
    await settled(state);
    expect(state.app.selected?.path).toBe("b.txt");

    keymap.dispatch(press("k"));
    keymap.dispatch(press("k"));
    await settled(state);
    expect(state.app.selected?.path).toBe("a.txt");
  });

  it("jumps to the ends with G and gg", async () => {
    const { state, keymap } = await opened();
    state.goToZone(2);

    keymap.dispatch(press("G"));
    await settled(state);
    expect(state.app.selected?.path).toBe("c.txt");

    // `g` alone is a prefix and does nothing; the second one is the jump.
    keymap.dispatch(press("g"));
    expect(state.app.selected?.path).toBe("c.txt");
    keymap.dispatch(press("g"));
    await settled(state);
    expect(state.app.selected?.path).toBe("a.txt");
  });

  it("moves through branches without walking a history per keystroke", async () => {
    // A walk of a hundred thousand commits per `j` is the filter box's old
    // mistake in another place. `⏎` is what asks for one.
    const { state, keymap } = await opened();
    state.goToZone(1);
    const walks = () => backend.current.calls.filter((call) => call.command === "history").length;
    const before = walks();

    keymap.dispatch(press("j"));
    expect(state.app.branchCursor).toBe("feature/one");
    expect(walks()).toBe(before);

    keymap.dispatch(press("Enter"));
    await settled(state);
    expect(state.app.screen).toBe("history");
    expect(state.app.query.branch).toBe("feature/one");
  });

  it("says nothing happened where a zone has no list", async () => {
    // The diff panel has its own line picking; `j` there is not a movement this
    // owns, and a key claimed and ignored is worse than one left alone.
    const { state } = await opened();
    state.goToZone(3);
    expect(state.moveInZone(1)).toBe(false);
  });
});

describe("Esc goes up one level", () => {
  it("clears a filter before it moves the keyboard", async () => {
    const { state } = await opened();
    state.showScreen("history");
    await settled(state);
    await state.setQuery({ text: "licence" });
    state.goToZone(2);

    expect(state.escapeLevel()).toBe(true);
    expect(state.isFilteringHistory()).toBe(false);
    expect(state.app.zone).toBe(2);

    expect(state.escapeLevel()).toBe(true);
    expect(state.app.zone).toBe(1);

    // And it stops there: "up one level" is not "out of the repository".
    expect(state.escapeLevel()).toBe(false);
    expect(state.app.open).toBe("/repo");
  });
});

describe("the repository filter", () => {
  it("narrows the list by name, path or description", async () => {
    const { state } = await opened();
    backend.current.library = [
      { ...backend.current.library[0]!, path: "/work/atelier", name: "atelier" },
      { ...backend.current.library[0]!, path: "/work/autre", name: "autre", index: 1 },
    ];
    await state.readLibrary();

    state.setLibraryFilter("atel");
    expect(state.visibleRepositories().map((row) => row.name)).toEqual(["atelier"]);

    // The card follows the list: one showing a repository the filter has hidden
    // is a panel about something nobody can see.
    expect(state.app.card).toBe("/work/atelier");
  });

  it("is what Esc empties first, on that screen", async () => {
    const { state } = await opened();
    state.showScreen("repositories");
    state.setLibraryFilter("rien");

    expect(state.escapeLevel()).toBe(true);
    expect(state.app.libraryFilter).toBe("");
  });
});

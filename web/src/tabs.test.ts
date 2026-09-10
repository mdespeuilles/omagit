// The repositories the window has open, one click apart.
//
// A deliberate addition to board 02, which draws no tab strip: the way to
// another repository was the Dépôts screen, and that is a detour for the thing
// people do most — two or three repositories open at once.
//
// What is pinned here is what a strip of tabs has to get right and usually does
// not: where the window lands when the tab you are on is the one you close.

import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Repository } from "./backend.fake";
import type { LibraryRow } from "./ipc";

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("./backend.fake").Repository>,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: () => Promise.resolve(null) }));

function entry(path: string): LibraryRow {
  return {
    group: 0,
    index: 0,
    group_name: "Récents",
    path,
    name: path.split("/").pop() ?? path,
    description: "",
    last_opened: null,
    missing: false,
  };
}

async function opened(...paths: string[]) {
  backend.current = new Repository([
    { path: "a.txt", staged: null, unstaged: "modified", hunks: 1 },
  ]);
  backend.current.library = [entry("/repo"), entry("/other"), entry("/third")];
  vi.resetModules();
  const state = await import("./state");
  await state.boot();
  for (const path of paths) {
    await state.openRepository(path);
    await settled(state);
  }
  return state;
}

async function settled(state: typeof import("./state")): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    await new Promise((resume) => setTimeout(resume, 0));
    if (!state.app.busy && state.app.status.status !== "loading") return;
  }
  throw new Error("l'écran ne s'est jamais stabilisé");
}

beforeEach(() => {
  backend.current = new Repository([]);
});

describe("the repository tabs", () => {
  it("keeps a tab per repository opened, in the order they were opened", async () => {
    const state = await opened("/repo", "/other");
    expect(state.app.tabs.map((tab) => tab.path)).toEqual(["/repo", "/other"]);
    expect(state.app.open).toBe("/other");
  });

  it("opens no second tab for a repository already open", async () => {
    const state = await opened("/repo", "/other", "/repo");
    expect(state.app.tabs).toHaveLength(2);
    expect(state.app.open).toBe("/repo");
  });

  it("falls back to the neighbour when the tab on screen is closed", async () => {
    const state = await opened("/repo", "/other", "/third");
    state.closeTab("/third");
    await settled(state);

    expect(state.app.tabs.map((tab) => tab.path)).toEqual(["/repo", "/other"]);
    // The one you were on before it, which is what a strip of tabs means by
    // "the neighbour" — never the far end, never the list.
    expect(state.app.open).toBe("/other");
    expect(state.app.screen).toBe("working-copy");
  });

  it("goes back to the list when the last tab closes", async () => {
    const state = await opened("/repo");
    state.closeTab("/repo");
    await settled(state);

    expect(state.app.tabs).toHaveLength(0);
    expect(state.app.open).toBeNull();
    // The only honest screen with no repository open.
    expect(state.app.screen).toBe("repositories");
  });

  it("leaves the window where it is when another tab is closed", async () => {
    const state = await opened("/repo", "/other");
    state.showScreen("history");
    state.closeTab("/repo");
    await settled(state);

    expect(state.app.open).toBe("/other");
    expect(state.app.screen).toBe("history");
  });

  it("tells the backend to let go of what it closed", async () => {
    // The handle is a `gix` repository and a lock, not a cache: a session that
    // visited twenty repositories held twenty of them for the life of the
    // window, and tabs make visiting twenty ordinary.
    const state = await opened("/repo", "/other");
    state.closeTab("/repo");
    await settled(state);

    const closed = backend.current.calls.filter((call) => call.command === "close_repository");
    expect(closed).toHaveLength(1);
    expect(closed[0]!.args["path"]).toBe("/repo");
  });

  it("draws one tab per open repository, and marks the one on screen", async () => {
    const state = await opened("/repo", "/other");
    const Tabs = (await import("./components/Tabs.vue")).default;
    const strip = mount(Tabs);

    const items = strip.findAll(".tabbar-item");
    expect(items).toHaveLength(2);
    expect(items[1]!.classes()).toContain("on");
    expect(items[0]!.classes()).not.toContain("on");

    await items[0]!.trigger("click");
    await settled(state);
    expect(state.app.open).toBe("/repo");
  });

  it("closes from the cross without switching to the tab first", async () => {
    const state = await opened("/repo", "/other");
    const Tabs = (await import("./components/Tabs.vue")).default;
    const strip = mount(Tabs);

    await strip.findAll(".tabbar-item")[0]!.find(".tabbar-close").trigger("click");
    await settled(state);

    expect(state.app.tabs.map((tab) => tab.path)).toEqual(["/other"]);
    // The click on the cross must not have opened the tab it closed.
    expect(state.app.open).toBe("/other");
  });

  it("is not on screen before a repository is opened", async () => {
    await opened();
    const Tabs = (await import("./components/Tabs.vue")).default;
    expect(mount(Tabs).find(".tabbar").exists()).toBe(false);
  });
});

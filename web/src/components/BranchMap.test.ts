// The branch map (DESIGN v2): open branches drawn against the trunk, newest
// first, and the selected one's own commits beside them.

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
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: () => Promise.resolve(null) }));

const NOW = Math.floor(Date.now() / 1000);
const DAY = 86_400;

async function drawn(map: Repository["map"], conflicts: string[] = []) {
  backend.current = new Repository([]);
  backend.current.map = map;
  backend.current.conflictsOnMerge = conflicts;
  vi.resetModules();
  const state = await import("../state");
  await state.boot();
  await state.openRepository("/repo");
  const BranchMap = (await import("./BranchMap.vue")).default;
  const view = mount(BranchMap, { attachTo: document.body });
  for (let i = 0; i < 20; i += 1) await new Promise((resume) => setTimeout(resume, 0));
  return { view };
}

const branch = (name: string, ahead: number, tip: number, head = false) => ({
  name,
  head,
  tip_time: NOW - tip * DAY,
  base_time: NOW - (tip + 5) * DAY,
  ahead,
  behind: 1,
  commits: [],
});

describe("the branch map", () => {
  it("draws the open branches, newest first, and the trunk last", async () => {
    const { view } = await drawn({
      trunk: "main",
      trunk_time: NOW,
      tags: [],
      trunk_commits: [],
      branches: [branch("old", 2, 20), branch("merged", 0, 3), branch("new", 1, 1, true)],
    });
    const names = view.findAll(".bmap-row .bmap-name").map((row) => row.text());
    expect(names).toEqual(["new", "old", "main"]);
    // The one HEAD is on is the one selected first.
    expect(view.find(".bmap-side").text()).toContain("new");
  });

  it("shows merged branches too when asked for all of them", async () => {
    const { view } = await drawn({
      trunk: "main",
      trunk_time: NOW,
      tags: [],
      trunk_commits: [],
      branches: [branch("old", 2, 20), branch("merged", 0, 3)],
    });
    await view.findAll(".segmented")[0]!.findAll("button")[1]!.trigger("click");
    const names = view.findAll(".bmap-row .bmap-name").map((row) => row.text());
    expect(names).toEqual(["merged", "old", "main"]);
  });

  it("says so when there is no trunk to draw against", async () => {
    const { view } = await drawn(null);
    expect(view.find(".bmap-card").exists()).toBe(false);
    expect(view.text()).toMatch(/main, master/);
  });

  it("says whether the selected branch merges into the trunk, and on which files it would not", async () => {
    backend.current = new Repository([]);
    const { view } = await drawn(
      {
        trunk: "main",
        trunk_time: NOW,
        tags: [],
        trunk_commits: [],
        branches: [branch("new", 1, 1, true)],
      },
      ["src/a.rs"],
    );
    expect(view.find(".bmap-check").text()).toContain("src/a.rs");
  });

  it("folds a burst of commits into a pill, and opens it when clicked", async () => {
    const burst = Array.from({ length: 5 }, (_, at) => ({
      time: NOW - DAY - at * 60,
      merge: false,
    }));
    const { view } = await drawn({
      trunk: "main",
      trunk_time: NOW,
      tags: [],
      trunk_commits: [],
      branches: [{ ...branch("burst", 5, 1, true), commits: burst }],
    });
    const run = view.find(".bmap-run:not(.legend)");
    expect(run.text()).toContain("5");
    expect(view.findAll(".bmap-row .bmap-dot")).toHaveLength(0);

    await run.trigger("click");
    expect(view.findAll(".bmap-row .bmap-dot")).toHaveLength(5);
    expect(view.find(".bmap-row .bmap-run").exists()).toBe(false);
  });
});

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

async function drawn(map: Repository["map"]) {
  backend.current = new Repository([]);
  backend.current.map = map;
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
});

describe("the branch map", () => {
  it("draws the open branches, newest first, and the trunk last", async () => {
    const { view } = await drawn({
      trunk: "main",
      trunk_time: NOW,
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
});

// The branch pill's switcher (DESIGN v2): the other branches one keystroke
// away, and a name nobody has yet offered as a new branch.

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

async function switcher() {
  backend.current = new Repository([]);
  backend.current.branches.push({
    name: "feat/search",
    commit: { full: "f".repeat(40), short: "fffffff" },
    head: false,
    tracking: { upstream: "origin/feat/search", ahead: 2, behind: 0, gone: false },
    merged: false,
    age: 3600,
  });
  vi.resetModules();
  const state = await import("../state");
  await state.boot();
  await state.openRepository("/repo");
  for (let i = 0; i < 50 && state.app.refs.status !== "ready"; i += 1)
    await new Promise((resume) => setTimeout(resume, 0));
  const Switcher = (await import("./BranchSwitcher.vue")).default;
  const view = mount(Switcher, { attachTo: document.body });
  backend.current.calls = [];
  return { state, view };
}

const settled = () => new Promise((resume) => setTimeout(resume, 0));
const called = (command: string) =>
  backend.current.calls.filter((call) => call.command === command);

describe("the branch switcher", () => {
  it("lists the current branch first, with where each one stands", async () => {
    const { view } = await switcher();
    const rows = view.findAll(".switcher-row").map((row) => row.text());
    expect(rows[0]).toContain("main");
    expect(rows[1]).toContain("feat/search");
    expect(rows[1]).toContain("↑2");
  });

  it("switches to the branch picked with the keyboard", async () => {
    const { view } = await switcher();
    const input = view.find("input");
    await input.setValue("search");
    await input.trigger("keydown", { key: "Enter" });
    for (let i = 0; i < 10; i += 1) await settled();
    expect(called("checkout").map((call) => call.args["name"])).toEqual(["feat/search"]);
    expect(view.emitted("close")).toHaveLength(1);
  });

  it("offers a name no branch has as a new branch, made here and switched to", async () => {
    const { view } = await switcher();
    const input = view.find("input");
    await input.setValue("fix/typo");
    expect(view.text()).toContain("fix/typo");
    await input.trigger("keydown", { key: "Enter" });
    for (let i = 0; i < 10; i += 1) await settled();
    expect(called("create_branch")).toHaveLength(1);
    expect(called("create_branch")[0]!.args).toMatchObject({ name: "fix/typo" });
  });

  it("does nothing for the branch already checked out but close", async () => {
    const { view } = await switcher();
    await view.find("input").trigger("keydown", { key: "Enter" });
    expect(called("checkout")).toHaveLength(0);
    expect(view.emitted("close")).toHaveLength(1);
  });
});

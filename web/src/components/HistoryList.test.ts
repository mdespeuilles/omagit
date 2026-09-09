// What a click on a history row does — and, as much, what it does not.

import { mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { Repository, type Made } from "../backend.fake";

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("../backend.fake").Repository>,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));

function chain(count: number): Made[] {
  return Array.from({ length: count }, (_, at) => ({
    id: String(count - at).padStart(40, "0"),
    summary: `commit ${count - at}`,
    parents: at === count - 1 ? [] : [String(count - at - 1).padStart(40, "0")],
  }));
}

async function drawn() {
  backend.current = new Repository([
    { path: "a.txt", staged: null, unstaged: "modified", hunks: 1 },
  ]);
  backend.current.log = chain(9);
  vi.resetModules();
  const state = await import("../state");
  await state.boot();
  state.showScreen("history");
  await settle(state);

  const HistoryList = (await import("./HistoryList.vue")).default;
  return { state, list: mount(HistoryList) };
}

async function settle(state: typeof import("../state")): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    await new Promise((resume) => setTimeout(resume, 0));
    if (state.app.history.status !== "loading" && state.app.commit.status !== "loading") return;
  }
  throw new Error("l'historique ne s'est jamais stabilisé");
}

const comparisons = () => backend.current.calls.filter((call) => call.command === "compare").length;

describe("clicking a history row", () => {
  it("opens the commit and compares nothing", async () => {
    // A comparison run on every row the pointer lands on would read a diff per
    // row, and the second commit is chosen by scrolling.
    const { state, list } = await drawn();
    const rows = list.findAll(".commit-row");

    await rows[0]!.trigger("click");
    await rows[2]!.trigger("click");
    await settle(state);

    expect(comparisons()).toBe(0);
    expect(state.app.compare.status).toBe("idle");
    expect(state.app.commit.status === "ready" && state.app.commit.value.summary).toBe("commit 7");
  });

  it("compares on a shift-click, from the row clicked before it", async () => {
    const { state, list } = await drawn();
    const rows = list.findAll(".commit-row");

    await rows[0]!.trigger("click");
    await rows[2]!.trigger("click", { shiftKey: true });
    await settle(state);

    expect(comparisons()).toBe(1);
    const call = backend.current.calls.find((call) => call.command === "compare");
    expect(call?.args).toMatchObject({
      from: chain(9)[0]!.id,
      to: chain(9)[2]!.id,
    });
  });

  it("treats a shift-click on the row already open as an ordinary one", async () => {
    const { state, list } = await drawn();
    const rows = list.findAll(".commit-row");

    await rows[1]!.trigger("click");
    await rows[1]!.trigger("click", { shiftKey: true });
    await settle(state);

    expect(comparisons()).toBe(0);
    expect(state.app.compare.status).toBe("idle");
  });
});

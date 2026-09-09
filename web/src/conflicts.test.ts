// Stopped on a conflict: what the row offers, and the two ways out.
//
// The thing worth testing here is naming. `ours` and `theirs` mean the opposite
// of what they read like during a rebase, and a screen that showed the two
// words alone would be asking someone to choose between two meanings they
// cannot see.

import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Repository, type Fixture } from "./backend.fake";

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("./backend.fake").Repository>,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: () => Promise.resolve(null) }));

const conflicted: Fixture = {
  path: "shared.txt",
  staged: null,
  unstaged: "modified",
  hunks: 1,
  conflict: "both modified",
};

/// A repository stopped half-way through `operation`, with one conflicted file.
async function stopped(
  operation = "merge",
  sides = { ours: "main", theirs: "feature", replayed: false },
) {
  backend.current = new Repository([{ ...conflicted }]);
  backend.current.operation = operation;
  backend.current.sides = sides;
  vi.resetModules();
  const state = await import("./state");
  await state.boot();
  await state.openRepository("/repo");
  await settled(state);
  return state;
}

async function settled(state: typeof import("./state")): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    await new Promise((resume) => setTimeout(resume, 0));
    if (!state.app.busy && state.app.status.status !== "loading") return;
  }
  throw new Error("l'écran ne s'est jamais stabilisé");
}

const sent = (command: string) =>
  backend.current.calls.filter((call) => call.command === command).map((call) => call.args);

beforeEach(() => {
  backend.current = new Repository([]);
});

describe("a repository stopped on a conflict", () => {
  it("names the two sides by their branches, not by the pronouns", async () => {
    const state = await stopped();
    const StatusList = (await import("./components/StatusList.vue")).default;

    const list = mount(StatusList);
    const actions = list.findAll(".row-action").map((button) => button.text());
    expect(actions).toEqual(["main", "feature"]);
    expect(state.app.sides).toEqual({ ours: "main", theirs: "feature", replayed: false });
  });

  it("says which side is which when the arriving one is being replayed", async () => {
    // A rebase: `ours` is the branch being replayed *onto*, and `theirs` is
    // your own work. The title is where that gets explained.
    await stopped("rebase", { ours: "main", theirs: "feature", replayed: true });
    const StatusList = (await import("./components/StatusList.vue")).default;

    const list = mount(StatusList);
    const titles = list.findAll(".row-action").map((button) => button.attributes("title") ?? "");
    expect(titles[0]).toContain("déjà en place");
    expect(titles[0]).toContain("rejou");
    expect(titles[1]).toContain("rejoué");
  });

  it("asks for the sides only while something is running", async () => {
    backend.current = new Repository([
      { path: "a.txt", staged: null, unstaged: "modified", hunks: 1 },
    ]);
    vi.resetModules();
    const state = await import("./state");
    await state.boot();
    await state.openRepository("/repo");
    await settled(state);

    expect(sent("conflict_sides")).toHaveLength(0);
    expect(state.app.sides).toBeNull();
  });

  it("resolves a file by keeping one side, and the row stops being a conflict", async () => {
    const state = await stopped();
    const row = state.app.status.status === "ready" ? state.app.status.value[0]! : null;

    state.resolveConflict(row!, "theirs");
    await settled(state);

    expect(sent("resolve_conflict")).toEqual([
      { path: "/repo", file: "shared.txt", side: "theirs" },
    ]);
    const now = state.app.status.status === "ready" ? state.app.status.value[0]! : null;
    expect(now!.conflict).toBeNull();
    expect(now!.staged).toBe("modified");
  });

  it("calls the checkbox what it does on a conflicted row", async () => {
    // `git add` on an unmerged path is what marks it resolved. Calling that
    // "indexer" sends the reader looking for a button that does not exist.
    await stopped();
    const StatusList = (await import("./components/StatusList.vue")).default;

    const list = mount(StatusList);
    expect(list.find(".check").attributes("title")).toBe("Marquer ce fichier résolu");
  });

  it("offers no discard on a conflicted row", async () => {
    // `git checkout -- <path>` refuses an unmerged path, so the button would
    // have been one that always fails.
    await stopped();
    const StatusList = (await import("./components/StatusList.vue")).default;

    const list = mount(StatusList);
    expect(list.findAll(".row-action.danger")).toHaveLength(0);
  });

  it("holds the way forward shut until the conflicts are settled", async () => {
    const state = await stopped();
    const StatusBar = (await import("./components/StatusBar.vue")).default;

    const bar = mount(StatusBar);
    const carry = bar.findAll("button").find((button) => button.text() === "Poursuivre")!;
    expect(carry.attributes("disabled")).toBeDefined();
    expect(carry.attributes("title")).toContain("conflit");

    const row = state.app.status.status === "ready" ? state.app.status.value[0]! : null;
    state.resolveConflict(row!, "ours");
    await settled(state);
    await bar.vm.$nextTick();

    expect(
      bar
        .findAll("button")
        .find((button) => button.text() === "Poursuivre")!
        .attributes("disabled"),
    ).toBeUndefined();
  });

  it("carries on, and keeps what git said about it", async () => {
    const state = await stopped();
    const row = state.app.status.status === "ready" ? state.app.status.value[0]! : null;
    state.resolveConflict(row!, "ours");
    await settled(state);

    state.continueOperation();
    await settled(state);

    expect(sent("continue_operation")).toEqual([{ path: "/repo" }]);
    expect(state.app.notes).toContain("Merge made by");
    expect(state.app.summary?.operation).toBeNull();
    // And the sides go with the operation they belonged to.
    expect(state.app.sides).toBeNull();
  });

  it("shows git's refusal when the way forward is taken too early", async () => {
    const state = await stopped();

    state.continueOperation();
    await settled(state);

    expect(state.app.writeError).toContain("resolve your current index");
    expect(state.app.summary?.operation).toBe("merge");
  });

  it("keeps the way out beside the way forward", async () => {
    // A repository left half-way with only one of the two on screen is one
    // somebody finishes in a terminal.
    await stopped();
    const StatusBar = (await import("./components/StatusBar.vue")).default;

    const bar = mount(StatusBar);
    const labels = bar.findAll("button").map((button) => button.text());
    expect(labels).toContain("Poursuivre");
    expect(labels).toContain("Abandonner");
  });
});

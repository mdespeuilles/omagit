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
    // The dialog first, then the two whole-file answers named by branch.
    expect(actions).toEqual(["Résoudre…", "main", "feature"]);
    expect(state.app.sides).toEqual({ ours: "main", theirs: "feature", replayed: false });
  });

  it("says which side is which when the arriving one is being replayed", async () => {
    // A rebase: `ours` is the branch being replayed *onto*, and `theirs` is
    // your own work. The title is where that gets explained.
    await stopped("rebase", { ours: "main", theirs: "feature", replayed: true });
    const StatusList = (await import("./components/StatusList.vue")).default;

    const list = mount(StatusList);
    const titles = list.findAll(".row-action").map((button) => button.attributes("title") ?? "");
    expect(titles[1]).toContain("déjà en place");
    expect(titles[1]).toContain("rejou");
    expect(titles[2]).toContain("rejoué");
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

  it("offers nothing to stage in the diff of a conflicted file", async () => {
    // There is no side of the index to build a patch from, and
    // `git checkout -- <path>` refuses an unmerged path, so both buttons would
    // have been ones that always fail. The row and the dialog are where a
    // conflict is answered.
    const state = await stopped();
    const DiffView = (await import("./components/DiffView.vue")).default;

    const diff = mount(DiffView);
    expect(state.app.diff.status).toBe("ready");
    expect(diff.findAll(".hunk-actions")).toHaveLength(0);
    // And the pane says which one file it is looking at, rather than offering
    // two tabs that would do the same thing.
    expect(diff.find(".tab").text()).toBe("En conflit");
  });

  it("holds the way forward shut until the conflicts are settled", async () => {
    const state = await stopped();
    const StatusBar = (await import("./components/StatusBar.vue")).default;

    const bar = mount(StatusBar);
    const carry = bar.findAll("button").find((button) => button.text() === "Continue")!;
    expect(carry.attributes("disabled")).toBeDefined();
    expect(carry.attributes("title")).toContain("conflict");

    const row = state.app.status.status === "ready" ? state.app.status.value[0]! : null;
    state.resolveConflict(row!, "ours");
    await settled(state);
    await bar.vm.$nextTick();

    expect(
      bar
        .findAll("button")
        .find((button) => button.text() === "Continue")!
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

    expect(state.app.writeError?.said).toContain("resolve your current index");
    expect(state.app.summary?.operation).toBe("merge");
  });

  it("keeps the way out beside the way forward", async () => {
    // A repository left half-way with only one of the two on screen is one
    // somebody finishes in a terminal.
    await stopped();
    const StatusBar = (await import("./components/StatusBar.vue")).default;

    const bar = mount(StatusBar);
    const labels = bar.findAll("button").map((button) => button.text());
    expect(labels).toContain("Continue");
    expect(labels).toContain("Abort");
  });
});

describe("board 07's conflict dialog", () => {
  /// The dialog, open on the file, with `regions` conflicts in it.
  async function opened(regions = 2) {
    backend.current = new Repository([{ ...conflicted, regions }]);
    backend.current.operation = "merge";
    vi.resetModules();
    const state = await import("./state");
    await state.boot();
    await state.openRepository("/repo");
    await settled(state);
    await state.openConflict("shared.txt");
    const ConflictDialog = (await import("./components/ConflictDialog.vue")).default;
    return { state, dialog: mount(ConflictDialog) };
  }

  it("reads the file and counts its conflicts", async () => {
    const { state, dialog } = await opened(2);

    expect(sent("conflict_file")).toEqual([{ path: "/repo", file: "shared.txt" }]);
    expect(state.app.resolving?.choices).toEqual([null, null]);
    expect(dialog.text()).toContain("conflit 1 / 2");
  });

  it("names the sides by their branch rather than by the pronoun", async () => {
    const { dialog } = await opened(1);

    const bands = dialog.findAll(".conflict-side-name").map((band) => band.text());
    expect(bands[0]).toContain("main");
    expect(bands[1]).toContain("feature");
  });

  it("holds the primary shut until every conflict has an answer", async () => {
    const { state, dialog } = await opened(2);
    const primary = () => dialog.findAll("button").find((b) => b.text().includes("indexer"))!;

    expect(primary().attributes("disabled")).toBeDefined();

    state.chooseSide(0, "ours");
    await dialog.vm.$nextTick();
    expect(primary().attributes("disabled")).toBeDefined();

    state.chooseSide(1, "both");
    await dialog.vm.$nextTick();
    expect(primary().attributes("disabled")).toBeUndefined();
  });

  it("moves to the next conflict nobody has answered", async () => {
    // Four conflicts is four decisions; a dialog that stayed on the one just
    // settled would make the reader find the next one every time.
    const { state } = await opened(3);

    state.chooseSide(0, "ours");
    expect(state.app.resolving?.at).toBe(1);

    // Answering out of order still leaves the cursor on the one nobody has
    // answered, not on the one just settled.
    state.chooseSide(2, "theirs");
    expect(state.app.resolving?.at).toBe(1);

    state.chooseSide(1, "both");
    expect(state.app.resolving?.at).toBe(1);
  });

  it("sends the answers in file order and stages the file", async () => {
    const { state } = await opened(2);

    state.chooseSide(0, "theirs");
    state.chooseSide(1, "ours");
    state.applyResolution();
    await settled(state);

    expect(sent("resolve_hunks")).toEqual([
      { path: "/repo", file: "shared.txt", choices: ["theirs", "ours"] },
    ]);
    expect(state.app.resolving).toBeNull();
    const row = state.app.status.status === "ready" ? state.app.status.value[0]! : null;
    expect(row!.conflict).toBeNull();
    expect(row!.staged).toBe("modified");
  });

  it("stands aside once the file is open in an editor", async () => {
    // What the dialog is drawing is about to stop being true.
    const { state } = await opened(1);

    state.openInEditor();
    await settled(state);

    expect(sent("open_in_editor")).toEqual([{ path: "/repo", file: "shared.txt" }]);
    expect(state.app.resolving).toBeNull();
    expect(state.app.notes).toContain("ouvert dans code");
  });

  it("says when the markers cannot be read rather than drawing nothing", async () => {
    backend.current = new Repository([{ ...conflicted }]);
    backend.current.operation = "merge";
    backend.current.unreadableConflict = "the markers do not pair up: `<<<<<<<` at line 1";
    vi.resetModules();
    const state = await import("./state");
    await state.boot();
    await state.openRepository("/repo");
    await settled(state);

    await state.openConflict("shared.txt");
    const ConflictDialog = (await import("./components/ConflictDialog.vue")).default;
    const dialog = mount(ConflictDialog);

    expect(state.app.resolving?.body.status).toBe("failed");
    expect(dialog.text()).toContain("do not pair up");
  });

  it("lets a file settled elsewhere be staged with no answers at all", async () => {
    const { state, dialog } = await opened(0);

    expect(dialog.text()).toContain("plus de marqueurs");
    state.applyResolution();
    await settled(state);

    expect(sent("resolve_hunks")).toEqual([{ path: "/repo", file: "shared.txt", choices: [] }]);
  });
});

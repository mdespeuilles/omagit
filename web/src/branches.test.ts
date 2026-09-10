// The branch tree, and what its rows do.

import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Repository } from "./backend.fake";
import type { BranchRow } from "./ipc";

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("./backend.fake").Repository>,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: () => Promise.resolve(null) }));

function branch(name: string, over: Partial<BranchRow> = {}): BranchRow {
  return {
    name,
    commit: { full: name.padEnd(40, "0"), short: name.slice(0, 7) },
    head: false,
    tracking: null,
    merged: true,
    age: 0,
    ...over,
  };
}

async function open(branches: BranchRow[]) {
  backend.current = new Repository([
    { path: "a.txt", staged: null, unstaged: "modified", hunks: 1 },
  ]);
  backend.current.branches = branches;
  vi.resetModules();
  const state = await import("./state");
  await state.boot();
  await state.openRepository("/repo");
  await settled(state);
  const BranchTree = (await import("./components/BranchTree.vue")).default;
  return { state, tree: mount(BranchTree) };
}

async function settled(state: typeof import("./state")): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    await new Promise((resume) => setTimeout(resume, 0));
    if (!state.app.busy && state.app.refs.status !== "loading") return;
  }
  throw new Error("l'arbre ne s'est jamais stabilisé");
}

const names = (tree: { findAll: (s: string) => { text: () => string }[] }) =>
  tree.findAll(".branch-name").map((n) => n.text());

beforeEach(() => {
  backend.current = new Repository([]);
});

describe("the branch tree", () => {
  it("groups on the first slash and leaves the rest alone", async () => {
    // Only the first segment groups: `feature/ui/topbar` lands under
    // `feature/` with its remaining path shown, rather than nesting three deep
    // for a tree nobody arranged that way.
    const { tree } = await open([
      branch("main", { head: true }),
      branch("feature/ui/topbar"),
      branch("feature/theme"),
      branch("fix/lanes"),
    ]);

    // Sorted, because the order `gix` enumerates references in is a hash map's
    // and a tree whose rows moved between two runs would be unusable.
    expect(names(tree)).toEqual(["main", "theme", "ui/topbar", "lanes"]);
    const groups = tree.findAll(".group-head.as-button").map((g) => g.text());
    expect(groups.join(" ")).toContain("feature/");
    expect(groups.join(" ")).toContain("fix/");
  });

  it("shows a branch's history when its row is clicked", async () => {
    // A single click did nothing at all — the checkout was on the double click
    // — so the row looked like a control that was broken. SPEC §11 asks for a
    // filter by branch, and this is where anyone would look for it.
    const { state, tree } = await open([
      branch("main", { head: true }),
      branch("feature/theme-runtime"),
    ]);

    await tree.findAll(".branch-row")[1]!.trigger("click");
    await settled(state);

    expect(state.app.screen).toBe("history");
    expect(state.app.query.branch).toBe("feature/theme-runtime");
    const walk = backend.current.calls.filter((call) => call.command === "history").at(-1);
    expect(walk?.args["query"]).toMatchObject({ branch: "feature/theme-runtime", all: false });

    // And the row says it is the one being shown.
    expect(tree.findAll(".branch-row")[1]!.classes()).toContain("selected");

    // Back to HEAD's history, which is what the chip in the header does.
    state.showHeadHistory();
    await settled(state);
    expect(state.app.query.branch).toBe("");
  });

  it("keeps checking out on the double click, where it was", async () => {
    // Switching branches rewrites the working tree; a single click must not.
    const { state, tree } = await open([branch("main", { head: true }), branch("feature/x")]);

    await tree.findAll(".branch-row")[1]!.trigger("dblclick");
    await settled(state);

    expect(backend.current.calls.some((call) => call.command === "checkout")).toBe(true);
  });

  it("marks as merged the branch that is merged, and only it", async () => {
    // The badge answers "deleting this loses nothing", so it has to sit on the
    // row it is about: `merged` travels with the row from `git branch
    // --merged`, and a badge one row off would wave someone through a delete
    // that throws commits away.
    const { tree } = await open([
      branch("main", { head: true, merged: true }),
      branch("feature/graph-lanes", { merged: false }),
      branch("feature/theme-runtime", { merged: true }),
      branch("old/spike", { merged: false }),
    ]);

    const marked = tree
      .findAll(".branch-row")
      .filter((row) => row.find(".ref.merged").exists())
      .map((row) => row.find(".branch-name").text());

    // Not `main`: it is HEAD, and "merged into itself" says nothing.
    expect(marked).toEqual(["theme-runtime"]);
  });

  it("folds a group away and back", async () => {
    const { state, tree } = await open([branch("main", { head: true }), branch("feature/theme")]);
    expect(names(tree)).toContain("theme");

    state.toggleBranchGroup("feature/");
    await tree.vm.$nextTick();
    expect(names(tree)).not.toContain("theme");

    state.toggleBranchGroup("feature/");
    await tree.vm.$nextTick();
    expect(names(tree)).toContain("theme");
  });

  it("offers nothing on the branch you are on", async () => {
    // `git` refuses all three — you cannot delete, merge or rebase onto the
    // branch you are standing on — and a button that always fails is worse
    // than no button.
    const { tree } = await open([branch("main", { head: true }), branch("other")]);
    const rows = tree.findAll(".branch-row");
    expect(rows[0]!.findAll(".row-action")).toHaveLength(0);
    expect(rows[1]!.findAll(".row-action").map((b) => b.text())).toEqual([
      "Fusionner",
      "Rebaser",
      "Suppr.",
    ]);
  });
});

describe("deleting a branch", () => {
  it("asks the question the branch's state deserves", async () => {
    // Removing a label and throwing away commits are different acts; one
    // wording for both would either frighten people off the harmless one or
    // wave them through the other.
    const { state } = await open([
      branch("main", { head: true }),
      branch("merged-one", { merged: true }),
      branch("unmerged-one", { merged: false }),
    ]);

    state.deleteBranch({ name: "merged-one", merged: true });
    expect(state.app.question?.detail).toContain("Seule l'étiquette");
    state.answer(false);

    state.deleteBranch({ name: "unmerged-one", merged: false });
    expect(state.app.question?.detail).toContain("reflog");
    state.answer(false);
  });

  it("forces only when the branch is not merged", async () => {
    const { state } = await open([
      branch("main", { head: true }),
      branch("merged-one", { merged: true }),
      branch("unmerged-one", { merged: false }),
    ]);

    state.deleteBranch({ name: "merged-one", merged: true });
    state.answer(true);
    await settled(state);

    state.deleteBranch({ name: "unmerged-one", merged: false });
    state.answer(true);
    await settled(state);

    const calls = backend.current.calls.filter((c) => c.command === "delete_branch");
    expect(calls.map((c) => c.args["force"])).toEqual([false, true]);
    expect(backend.current.branches.map((b) => b.name)).toEqual(["main"]);
  });

  it("does not delete while the question stands", async () => {
    const { state } = await open([branch("main", { head: true }), branch("other")]);
    const before = backend.current.calls.length;

    state.deleteBranch({ name: "other", merged: true });
    expect(backend.current.calls).toHaveLength(before);

    state.answer(false);
    await settled(state);
    expect(backend.current.calls.filter((c) => c.command === "delete_branch")).toHaveLength(0);
  });
});

describe("switching and creating", () => {
  it("re-reads the tree after a checkout, because HEAD moved", async () => {
    const { state } = await open([branch("main", { head: true }), branch("other")]);
    const before = backend.current.calls.filter((c) => c.command === "refs").length;

    state.checkoutBranch("other");
    await settled(state);

    expect(backend.current.head).toBe("other");
    expect(backend.current.calls.filter((c) => c.command === "refs").length).toBeGreaterThan(
      before,
    );
  });

  it("creates and switches in one act", async () => {
    const { state } = await open([branch("main", { head: true })]);

    state.createBranch("  feature/new  ", "", true);
    await settled(state);

    const call = backend.current.calls.find((c) => c.command === "create_branch");
    // Trimmed: a name with a space around it is a name someone typed, not a
    // name they meant.
    expect(call?.args).toMatchObject({ name: "feature/new", switch: true });
    expect(backend.current.head).toBe("feature/new");
  });

  it("refuses an empty name without asking the backend", async () => {
    const { state } = await open([branch("main", { head: true })]);

    state.createBranch("   ", "", true);
    await settled(state);

    expect(backend.current.calls.filter((c) => c.command === "create_branch")).toHaveLength(0);
  });
});

describe("ordering", () => {
  it("sorts branches and groups by name, whatever order the refs arrive in", async () => {
    const { tree } = await open([
      branch("zebra"),
      branch("main", { head: true }),
      branch("fix/z"),
      branch("fix/a"),
      branch("alpha"),
    ]);

    expect(names(tree)).toEqual(["alpha", "main", "zebra", "a", "z"]);
  });
});

describe("integrating", () => {
  it("asks before a merge, and says what a conflict would leave behind", async () => {
    const { state } = await open([branch("main", { head: true }), branch("other")]);
    const before = backend.current.calls.length;

    state.mergeBranch("other");
    expect(state.app.question?.title).toContain("other");
    expect(state.app.question?.detail).toContain("conflit");
    expect(backend.current.calls).toHaveLength(before);

    state.answer(true);
    await settled(state);
    expect(backend.current.calls.find((c) => c.command === "merge")?.args).toMatchObject({
      branch: "other",
    });
  });

  it("says a rebase rewrites commits, because that is the part that surprises", async () => {
    const { state } = await open([branch("main", { head: true }), branch("other")]);

    state.rebaseOnto("other");
    expect(state.app.question?.detail).toContain("reflog");
    state.answer(false);

    expect(backend.current.calls.filter((c) => c.command === "rebase")).toHaveLength(0);
  });

  it("leaves a way out when the merge stops half-way", async () => {
    // A repository left half-finished with no visible exit is the state this
    // application must never put someone in.
    const { state } = await open([branch("main", { head: true }), branch("other")]);
    backend.current.failIntegrate = "CONFLICT (content): Merge conflict in shared.txt";

    state.mergeBranch("other");
    state.answer(true);
    await settled(state);

    expect(state.app.writeError?.said).toContain("CONFLICT");
    expect(state.app.summary?.operation).toBe("merge");

    state.abortOperation();
    expect(state.app.question?.title).toContain("merge");
    state.answer(true);
    await settled(state);

    expect(state.app.summary?.operation).toBeNull();
  });

  it("aborts nothing when nothing is running", async () => {
    const { state } = await open([branch("main", { head: true })]);
    state.abortOperation();
    expect(state.app.question).toBeNull();
  });

  it("folds every section that draws a chevron, Branches included", async () => {
    // It was the one head that did not fold. Tags, Remotes and every `feat/`
    // prefix did; "Branches" — the longest section, and the one worth folding —
    // drew the same triangle and answered nothing.
    const { tree } = await open([
      branch("main", { head: true }),
      branch("feat/one"),
      branch("feat/two"),
    ]);
    expect(tree.findAll(".branch-row").length).toBe(3);

    const head = tree.findAll(".group-head").find((one) => one.text().includes("Branches"))!;
    await head.trigger("click");

    // Everything under it: the loose branches and the prefix groups both.
    expect(tree.findAll(".branch-row")).toHaveLength(0);
    expect(tree.text()).not.toContain("feat/");
    // And the head itself stays, with its chevron the other way round.
    expect(tree.text()).toContain("Branches");

    await head.trigger("click");
    expect(tree.findAll(".branch-row").length).toBe(3);
  });
});

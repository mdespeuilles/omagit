// The empty and error states SPEC §11 asks for "partout", and SPEC §13's edge
// cases behind them: a repository with no commit, a detached checkout.
//
// What is tested is not that a list draws *something* when it is empty — every
// one of them already did — but that it says **which** empty it is. "Aucun
// commit" under a filter reads as an empty repository; "Aucune branche" with no
// explanation reads as a tree that failed to load. Each of these is a sentence
// that used to be wrong or missing.

import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Repository } from "./backend.fake";

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("./backend.fake").Repository>,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: () => Promise.resolve("/repo") }));

async function opened(dress: (fake: InstanceType<typeof Repository>) => void = () => {}) {
  backend.current = new Repository([
    { path: "a.txt", staged: null, unstaged: "modified", hunks: 1 },
  ]);
  dress(backend.current);
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
    if (!state.app.busy) return;
  }
  throw new Error("l'écran ne s'est jamais stabilisé");
}

async function screenOf(name: string) {
  const component = (await import(`./components/${name}.vue`)).default;
  return mount(component);
}

beforeEach(() => {
  backend.current = new Repository([]);
});

describe("a history with nothing in it", () => {
  it("says the filter is what is hiding the commits, and offers to lift it", async () => {
    const state = await opened();
    state.showScreen("history");
    await settled(state);
    await state.setQuery({ text: "rien-de-tel" });
    await settled(state);

    const list = await screenOf("HistoryList");
    expect(list.text()).toContain("Aucun commit ne correspond");
    // And the way out is in the sentence, not only in the filter row above it.
    const clear = list.findAll("button").find((button) => button.text().includes("Effacer"))!;
    await clear.trigger("click");
    await settled(state);
    expect(state.isFilteringHistory()).toBe(false);
  });

  it("says a repository with no commit is waiting for one", async () => {
    // SPEC §13: `git init` and nothing since. "Aucun commit" is true and
    // useless — the question it leaves is whether the app failed to read.
    const state = await opened((fake) => {
      fake.headKind = "unborn";
      fake.log = [];
    });
    state.showScreen("history");
    await settled(state);

    const list = await screenOf("HistoryList");
    expect(list.text()).toContain("pas encore de commit");
  });
});

describe("a branch tree with no branch", () => {
  it("says why, rather than showing a count of zero", async () => {
    const state = await opened((fake) => {
      fake.headKind = "unborn";
      fake.branches = [];
    });
    await settled(state);

    const tree = await screenOf("BranchTree");
    expect(tree.text()).toContain("Aucune branche");
    expect(tree.findAll(".branch-row")).toHaveLength(0);
  });
});

describe("the commit box in a repository that is not on a branch", () => {
  it("warns that the commit will belong to nothing, without blocking it", async () => {
    // Promised at M5 in `repo.rs` — `Head::Detached` says the checkout "has to
    // warn before a commit is made here" — and never built.
    const state = await opened((fake) => {
      fake.headKind = "detached";
      fake.head = "detached at 9f3c1a2";
    });
    state.showScreen("working-copy");
    await settled(state);

    const box = await screenOf("CommitBox");
    expect(box.find(".commit-caution").text()).toContain("n'appartiendra à aucune branche");
  });

  it("refuses to amend a commit that does not exist yet", async () => {
    const state = await opened((fake) => {
      fake.headKind = "unborn";
    });
    state.showScreen("working-copy");
    state.setAmend(true);
    await settled(state);

    const box = await screenOf("CommitBox");
    expect(box.find(".commit-blocked").text()).toContain("Aucun commit à corriger");
  });
});

describe("the repository list", () => {
  it("does not offer to add a repository when a filter is what emptied it", async () => {
    const state = await opened();
    state.showScreen("repositories");
    state.setLibraryFilter("zzz");
    await settled(state);

    const list = await screenOf("RepositoryList");
    expect(list.text()).toContain("Aucun dépôt ne correspond");
    expect(list.text()).not.toContain("Aucun dépôt pour l'instant");
  });

  it("does not read a branch name as a detached HEAD", async () => {
    // The state used to be sniffed out of the printed label with
    // `head.startsWith("detached")`, so this branch — the one somebody writes
    // while fixing exactly this — drew the row as "not on a branch".
    const state = await opened((fake) => {
      fake.head = "detached-head-fix";
    });
    state.showScreen("repositories");
    await settled(state);

    const list = await screenOf("RepositoryList");
    expect(list.text()).toContain("detached-head-fix");
    expect(list.find(".library-dot.detached").exists()).toBe(false);
  });
});

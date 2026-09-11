// Making, deleting and publishing tags, from the window (SPEC §11).
//
// `omagit-git`'s own suite holds what `git` does — that a message is what makes
// a tag annotated, that one tag is pushed and not all of them. What is held
// here is the half of the feature that is a decision rather than a command:
// which question is asked before which act, and what a refusal turns into.

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
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: () => Promise.resolve(null) }));

async function opened(dress: (fake: InstanceType<typeof Repository>) => void = () => {}) {
  backend.current = new Repository([]);
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

/// The re-walk is started and not awaited — a walk of a hundred thousand
/// commits must not hold the window — so what a test waits on is the answer,
/// not the busy flag.
async function until(what: () => boolean): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    if (what()) return;
    await new Promise((resume) => setTimeout(resume, 0));
  }
  throw new Error("la condition n'est jamais devenue vraie");
}

beforeEach(() => {
  backend.current = new Repository([]);
});

describe("making one", () => {
  it("puts it in the tree, and says nothing about a kind nobody chose", async () => {
    const state = await opened();
    state.openTag("", "the current commit");
    state.setTagField("name", "  v1.0.0  ");
    await state.createTag();

    expect(state.app.tagging).toBeNull();
    const refs = state.app.refs;
    const tags = refs.status === "ready" ? refs.value.tags : [];
    expect(tags.map((one) => one.name)).toEqual(["v1.0.0"]);
    expect(tags[0]?.annotated, "no message, so a bare ref").toBe(false);
  });

  it("is annotated exactly when there is a message, and nothing else decides", async () => {
    // `git`'s own rule — `-m` implies `-a` — said with the presence of the
    // message rather than a second switch, so the two cannot disagree.
    const state = await opened();
    state.openTag("", "the current commit");
    state.setTagField("name", "v2.0.0");
    state.setTagField("message", "Ce que cette version apporte.");
    await state.createTag();

    const refs = state.app.refs;
    const tags = refs.status === "ready" ? refs.value.tags : [];
    expect(tags[0]?.annotated).toBe(true);
  });

  it("keeps the dialog open on a name already taken, and offers to move it", async () => {
    // The refusal is `git`'s, with the tag it names (SPEC §3 rule 3), and it is
    // what makes the offer possible: "move it" cannot be answered before
    // somebody is told there is something to move.
    const state = await opened();
    state.openTag("", "the current commit");
    state.setTagField("name", "v1");
    await state.createTag();

    state.openTag("", "the current commit");
    state.setTagField("name", "v1");
    await state.createTag();

    expect(state.app.tagging, "still open").not.toBeNull();
    expect(state.app.tagging?.refused).toContain("already exists");
    expect(state.app.tagging?.force, "offered, not taken").toBe(false);
    expect(state.app.writeError, "not an error band: it is a question").toBeNull();

    state.setTagForce(true);
    await state.createTag();
    expect(state.app.tagging).toBeNull();
  });

  it("forgets a refusal as soon as the name changes", async () => {
    const state = await opened();
    state.openTag("", "the current commit");
    state.setTagField("name", "v1");
    await state.createTag();
    state.openTag("", "the current commit");
    state.setTagField("name", "v1");
    await state.createTag();
    expect(state.app.tagging?.refused).not.toBeNull();

    state.setTagField("name", "v1.1");
    expect(state.app.tagging?.refused).toBeNull();
    expect(state.app.tagging?.force, "and the offer goes with it").toBe(false);
  });
});

describe("deleting one", () => {
  it("asks first, and the question says what is not lost", async () => {
    const state = await opened();
    state.openTag("", "the current commit");
    state.setTagField("name", "v1");
    await state.createTag();

    state.deleteTag("v1");
    // A tag leaves nothing behind — no reflog entry, unlike a branch — so the
    // question has to say which parts of that are true and which are not.
    expect(state.app.question?.detail).toContain("commit it named is untouched");
    expect(state.app.question?.detail).toContain("stays on the remote");

    state.answer(true);
    await settled(state);
    const refs = state.app.refs;
    expect(refs.status === "ready" ? refs.value.tags : []).toHaveLength(0);
  });
});

describe("publishing one", () => {
  it("pushes the one that was named and no other", async () => {
    const state = await opened();
    for (const name of ["v1", "scratch"]) {
      state.openTag("", "the current commit");
      state.setTagField("name", name);
      await state.createTag();
    }

    state.publishTag("v1", "origin", false);
    await settled(state);

    expect(backend.current.published).toEqual(["v1"]);
    expect(state.app.question, "publishing takes nothing away").toBeNull();
  });

  it("asks before taking one off the remote", async () => {
    // The asymmetry is the point: one of the two can remove something other
    // people are already using.
    const state = await opened();
    state.openTag("", "the current commit");
    state.setTagField("name", "v1");
    await state.createTag();
    state.publishTag("v1", "origin", false);
    await settled(state);

    state.publishTag("v1", "origin", true);
    expect(state.app.question?.detail).toContain("Your own copy stays");
    state.answer(true);
    await settled(state);

    expect(backend.current.published).toEqual([]);
    const refs = state.app.refs;
    expect(refs.status === "ready" ? refs.value.tags : []).toHaveLength(1);
  });
});

describe("the history underneath", () => {
  it("carries the new tag on its commit row, with nothing refreshed by hand", async () => {
    // Reported from use, on T06 of the test plan: "j'ai dû refresh pour voir
    // l'étiquette sur le commit". The rows carry their own ref badges and are
    // walked once — `settle()` re-read the status, the summary, the journal,
    // the refs and the shelf, and never the history.
    const state = await opened((fake) => {
      fake.log = [
        { id: "a".repeat(40), summary: "le dernier", parents: [] },
        { id: "b".repeat(40), summary: "celui d'avant", parents: ["a".repeat(40)] },
      ];
    });
    state.showScreen("history");
    await settled(state);
    const before = state.app.history;
    expect(before.status === "ready" ? before.value[0]?.labels : null).toEqual([]);

    state.openTag("", "the current commit");
    state.setTagField("name", "v1.0.0");
    await state.createTag();
    await settled(state);

    await until(() => {
      const held = state.app.history;
      return held.status === "ready" && (held.value[0]?.labels.length ?? 0) > 0;
    });
    const after = state.app.history;
    expect(after.status === "ready" ? after.value[0]?.labels : null).toEqual([
      { kind: "tag", name: "v1.0.0" },
    ]);
  });

  it("leaves the commit being read open across the re-walk", async () => {
    // `loadHistory` clears the detail pane, because a *query* change replaces
    // the walk that produced it. Here the query has not changed, and closing
    // what somebody is reading would be a worse answer than a stale badge.
    const state = await opened((fake) => {
      fake.log = [{ id: "a".repeat(40), summary: "le dernier", parents: [] }];
    });
    state.showScreen("history");
    await settled(state);
    await state.selectCommit("a".repeat(40));
    await settled(state);
    expect(state.app.commit.status).toBe("ready");

    state.openTag("", "the current commit");
    state.setTagField("name", "v1.0.0");
    await state.createTag();
    await settled(state);

    await until(() => state.app.commit.status === "ready");
    expect(state.app.commit.status, "still open").toBe("ready");
    const held = state.app.commit;
    expect(held.status === "ready" ? held.value.id.full : "").toBe("a".repeat(40));
  });

  it("does not re-walk when nothing about the refs moved", async () => {
    // The cost the shelf above is careful about: a walk per checkbox. Staging a
    // file moves no ref, so it must not pay for one.
    const state = await opened((fake) => {
      fake.files = [{ path: "a.txt", staged: null, unstaged: "modified", hunks: 1 }];
      fake.log = [{ id: "a".repeat(40), summary: "le dernier", parents: [] }];
    });
    state.showScreen("history");
    await settled(state);
    const walks = backend.current.historyWalks;

    state.showScreen("working-copy");
    await settled(state);
    const row = state.app.status.status === "ready" ? state.app.status.value[0]! : null;
    state.stageFile(row!, false);
    await settled(state);
    // And a beat more, in case one was started late.
    await new Promise((resume) => setTimeout(resume, 0));

    expect(backend.current.historyWalks).toBe(walks);
  });
});

describe("the sidebar", () => {
  it("draws the section even with nothing in it, so there is a way to make one", async () => {
    // It was drawn only when there were tags already, which meant a repository
    // with none had no way to get its first.
    const state = await opened();
    await settled(state);

    const BranchTree = (await import("./components/BranchTree.vue")).default;
    const drawn = mount(BranchTree);
    expect(drawn.find(".tag-head").exists()).toBe(true);
    expect(drawn.findAll(".tag-row")).toHaveLength(0);
  });

  it("says which tags are annotated, since nothing else shows it", async () => {
    const state = await opened();
    state.openTag("", "the current commit");
    state.setTagField("name", "v1");
    state.setTagField("message", "une version");
    await state.createTag();
    state.openTag("", "the current commit");
    state.setTagField("name", "scratch");
    await state.createTag();
    await settled(state);

    const BranchTree = (await import("./components/BranchTree.vue")).default;
    const rows = mount(BranchTree).findAll(".tag-row");
    expect(rows).toHaveLength(2);
    expect(rows[0]!.text()).toContain("annotated");
    expect(rows[1]!.text()).not.toContain("annotated");
  });
});

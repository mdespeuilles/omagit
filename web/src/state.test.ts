// What the screen does with the repository's answers.
//
// Every case here is a bug this project has shipped or nearly shipped: a pane
// that kept drawing a diff the index no longer had, a selection that survived
// the file it named, a stale answer that overwrote a fresh one, a destructive
// action that ran before it was confirmed.

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

const file = (path: string, over: Partial<Fixture> = {}): Fixture => ({
  path,
  staged: null,
  unstaged: "modified",
  hunks: 2,
  ...over,
});

/// A fresh module graph per test: `state.ts` is a singleton on purpose, and a
/// test that shared it with the previous one would be testing the order they
/// happen to run in.
async function open(files: Fixture[]): Promise<typeof import("./state")> {
  backend.current = new Repository(files);
  vi.resetModules();
  const state = await import("./state");
  await state.boot();
  // Start-up stops at the Repositories screen (SPEC §12); these tests are about
  // what a repository's screens do once one is open.
  await state.openRepository("/repo");
  return state;
}

/// The state settles asynchronously after a write — that is the whole shape of
/// it — so a test has to wait for the re-read rather than for a tick.
async function settled(state: typeof import("./state")): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    await Promise.resolve();
    await new Promise((resume) => setTimeout(resume, 0));
    if (!state.app.busy && state.app.diff.status !== "loading") return;
  }
  throw new Error("l'écriture ne s'est jamais terminée");
}

beforeEach(() => {
  backend.current = new Repository([]);
});

describe("opening", () => {
  it("shows the first file rather than an empty pane", async () => {
    const state = await open([file("a.txt"), file("b.txt")]);
    expect(state.app.selected).toEqual({ path: "a.txt", staged: false });
    expect(state.app.diff.status).toBe("ready");
  });

  it("asks who the commit would be from before anything is typed", async () => {
    const state = await open([file("a.txt")]);
    expect(state.app.committer).toBe("Test <test@omagit.test>");
    // The order matters: the warning is useless once a paragraph is written.
    const commands = backend.current.calls.map((call) => call.command);
    expect(commands).toContain("committer");
  });

  it("starts the message from commit.template when the repository has one", async () => {
    backend.current = new Repository([file("a.txt")]);
    backend.current.template = "# le gabarit\n";
    vi.resetModules();
    const state = await import("./state");
    await state.boot();
    await state.openRepository("/repo");
    expect(state.app.message).toBe("# le gabarit\n");
  });
});

describe("staging", () => {
  it("moves the pane to the staged side when the whole file goes in", async () => {
    const state = await open([file("a.txt")]);
    state.stageFile(
      {
        path: "a.txt",
        staged: null,
        unstaged: "modified",
        conflict: null,
        code: " M",
      },
      false,
    );
    await settled(state);

    expect(state.app.selected).toEqual({ path: "a.txt", staged: true });
  });

  it("stays on the unstaged side when only a hunk goes in", async () => {
    // The point of staging one hunk is to look at what is left. A pane that
    // jumped to the staged tab would show the opposite of what was asked.
    const state = await open([file("a.txt")]);
    state.stageHunk(0, false);
    await settled(state);

    expect(state.app.selected).toEqual({ path: "a.txt", staged: false });
    expect(backend.current.find("a.txt")).toMatchObject({
      staged: "modified",
      unstaged: "modified",
    });
  });

  it("sends the hunk's own index, not the row's", async () => {
    const state = await open([file("a.txt")]);
    state.stageHunk(1, false);
    await settled(state);

    const write = backend.current.calls.find((call) => call.command === "stage");
    expect(write?.args["target"]).toEqual({ kind: "hunks", hunks: [1] });
  });

  it("drops the selection when the file leaves the working copy", async () => {
    const state = await open([file("a.txt")]);
    state.discardFile({
      path: "a.txt",
      staged: null,
      unstaged: "modified",
      conflict: null,
      code: " M",
    });
    state.answer(true);
    await settled(state);

    expect(state.app.selected).toBeNull();
    expect(state.app.diff.status).toBe("idle");
  });

  it("says a failure in the band, whole, and not in the status bar", async () => {
    // `git`'s refusals are the useful half of this app's errors and they run to
    // two hundred characters. In a 22-pixel bar what survived was the half the
    // reader already knew.
    const state = await open([file("a.txt")]);
    backend.current.failNextWrite =
      "error: Your local changes to the following files would be overwritten by merge: src/render.rs Please commit your changes or stash them before you merge. Aborting";
    state.stageHunk(0, false);
    await settled(state);

    const Notice = (await import("./components/Notice.vue")).default;
    const band = mount(Notice);
    expect(band.find(".notice-what").text()).toContain("Indexer");
    expect(band.find(".notice-said").text()).toContain("Aborting");

    // And the bar keeps only what fits it: the successes.
    const StatusBar = (await import("./components/StatusBar.vue")).default;
    expect(mount(StatusBar).text()).not.toContain("Aborting");

    // It stays until it is dismissed — an error that evaporates is worse than
    // a quiet one.
    state.dismissWriteError();
    expect(state.app.writeError).toBeNull();
  });

  it("asks the question the row deserves before rejecting it", async () => {
    // The same button does three different things. A confirmation that warns
    // about losing work when the act *gives a file back* is a confirmation
    // people learn to click through.
    const state = await open([file("a.txt")]);
    const row = (unstaged: string) => ({
      path: "a.txt",
      staged: null,
      unstaged,
      conflict: null,
      code: " M",
    });

    state.discardFile(row("modified"));
    expect(state.app.question?.verb).toBe("Rejeter");
    expect(state.app.question?.detail).toContain("perdues");
    state.answer(false);

    state.discardFile(row("untracked"));
    expect(state.app.question?.detail).toContain("supprime du disque");
    state.answer(false);

    state.discardFile(row("deleted"));
    expect(state.app.question?.title).toContain("Restaurer");
    expect(state.app.question?.verb).toBe("Restaurer");
    expect(state.app.question?.detail).toContain("rien n'est perdu");
    state.answer(false);
    await settled(state);
  });

  it("keeps the failure until something succeeds", async () => {
    const state = await open([file("a.txt")]);
    backend.current.failNextWrite = "error: patch does not apply";
    state.stageHunk(0, false);
    await settled(state);

    expect(state.app.writeError?.said).toContain("patch does not apply");
    // A write that failed is visible nowhere else: the repository did not move.
    expect(backend.current.find("a.txt")?.staged).toBeNull();

    state.stageHunk(0, false);
    await settled(state);
    expect(state.app.writeError).toBeNull();
  });

  it("indexes everything with one command rather than one per file", async () => {
    const state = await open([file("a.txt"), file("b.txt"), file("c.txt")]);
    state.stageEverything(false);
    await settled(state);

    expect(backend.current.calls.filter((call) => call.command === "stage")).toHaveLength(0);
    expect(backend.current.calls.filter((call) => call.command === "stage_all")).toHaveLength(1);
  });
});

describe("picking lines", () => {
  it("sends the coordinates the diff rows carry", async () => {
    const state = await open([file("a.txt")]);
    state.pickLine(0, 1, false);
    state.pickLine(1, 2, false);
    state.stagePicked(false);
    await settled(state);

    const write = backend.current.calls.find((call) => call.command === "stage");
    expect(write?.args["target"]).toEqual({
      kind: "lines",
      lines: [
        [0, 1],
        [1, 2],
      ],
    });
  });

  it("extends a range only inside one hunk", async () => {
    // A selection across two hunks would have to mean something about the
    // context between them, and it does not.
    const state = await open([file("a.txt")]);
    state.pickLine(0, 0, false);
    state.pickLine(0, 3, true);
    expect(state.app.picked.size).toBe(4);

    state.clearPicked();
    state.pickLine(0, 0, false);
    state.pickLine(1, 3, true);
    expect(state.app.picked.size).toBe(2);
  });

  it("forgets what was picked once a write has moved the lines", async () => {
    const state = await open([file("a.txt")]);
    state.pickLine(0, 1, false);
    expect(state.app.picked.size).toBe(1);

    state.stageHunk(0, false);
    await settled(state);
    // The coordinates index the diff that has just stopped being true.
    expect(state.app.picked.size).toBe(0);
  });

  it("forgets what was picked when the pane changes file", async () => {
    const state = await open([file("a.txt"), file("b.txt")]);
    state.pickLine(0, 1, false);
    await state.selectFile("b.txt", false);
    expect(state.app.picked.size).toBe(0);
  });
});

describe("the destructive ones", () => {
  it("do not write until the question is answered", async () => {
    const state = await open([file("a.txt")]);
    const before = backend.current.calls.length;

    state.discardHunk(0);
    expect(state.app.question).not.toBeNull();
    expect(backend.current.calls).toHaveLength(before);

    state.answer(false);
    expect(state.app.question).toBeNull();
    await settled(state);
    expect(backend.current.calls.filter((call) => call.command === "discard")).toHaveLength(0);
  });

  it("say what an untracked file's discard really does", async () => {
    const state = await open([file("a.txt", { unstaged: "untracked" })]);
    state.discardFile({
      path: "a.txt",
      staged: null,
      unstaged: "untracked",
      conflict: null,
      code: "??",
    });
    // Not "your changes will be lost": there is no earlier version to go back
    // to, so the file is deleted outright.
    expect(state.app.question?.detail).toContain("supprime");
  });

  it("treat an amend as one, because it replaces a commit", async () => {
    const state = await open([file("a.txt", { staged: "modified", unstaged: null })]);
    await state.setAmend(true);
    state.setMessage("un message");
    state.commit();
    expect(state.app.question?.title).toContain("Remplacer");
    state.answer(false);

    expect(backend.current.commits).toHaveLength(0);
  });
});

describe("the commit box", () => {
  it("refuses a commit with nothing staged, and says which of the two is wrong", async () => {
    const state = await open([file("a.txt")]);
    state.setMessage("un message");
    expect(state.canCommit()).toBe(false);

    state.stageFile(
      {
        path: "a.txt",
        staged: null,
        unstaged: "modified",
        conflict: null,
        code: " M",
      },
      false,
    );
    await settled(state);
    expect(state.canCommit()).toBe(true);
  });

  it("refuses a commit with no identity", async () => {
    backend.current = new Repository([file("a.txt", { staged: "modified", unstaged: null })]);
    backend.current.committer = null;
    vi.resetModules();
    const state = await import("./state");
    await state.boot();
    await state.openRepository("/repo");

    state.setMessage("un message");
    expect(state.canCommit()).toBe(false);
  });

  it("brings in the message an amend is about to replace", async () => {
    const state = await open([file("a.txt", { staged: "modified", unstaged: null })]);
    await state.setAmend(true);
    expect(state.app.message).toBe("le message précédent");
  });

  it("does not overwrite a message already written", async () => {
    const state = await open([file("a.txt", { staged: "modified", unstaged: null })]);
    state.setMessage("ce que je viens d'écrire");
    await state.setAmend(true);
    expect(state.app.message).toBe("ce que je viens d'écrire");
  });

  it("empties the box once the commit is made", async () => {
    const state = await open([file("a.txt", { staged: "modified", unstaged: null })]);
    state.setMessage("un message");
    state.commit();
    await settled(state);

    expect(backend.current.commits).toEqual([{ message: "un message", amend: false }]);
    expect(state.app.message).toBe("");
    expect(state.app.status.status === "ready" && state.app.status.value).toHaveLength(0);
  });
});

describe("counting", () => {
  it("counts the two sides separately, because a file can be on both", async () => {
    // The bug: the file list said "1 indexé · 4 non" by subtracting, while the
    // status bar said "5 non indexé" by counting. A half-staged file is in both
    // sets, so subtraction was the wrong arithmetic and the two disagreed.
    const state = await open([
      file("a.txt", { staged: "modified", unstaged: "modified" }),
      file("b.txt"),
    ]);
    expect(state.stagedCount()).toBe(1);
    expect(state.unstagedCount()).toBe(2);
  });
});

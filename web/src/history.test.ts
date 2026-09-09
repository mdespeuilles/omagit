// The History screen's state machine.
//
// The paging is the part worth testing: the backend parks a walk and hands out
// a page at a time, so every bug here is a bug about *when* the front end asks
// and what it does with an answer that arrives late — rows appended twice, a
// page requested four times by one scroll, a page belonging to a query nobody
// is looking at any more.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { Repository, type Fixture, type Made } from "./backend.fake";

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("./backend.fake").Repository>,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));

const modified: Fixture = {
  path: "a.txt",
  staged: null,
  unstaged: "modified",
  hunks: 1,
};

/// `count` commits in a chain, newest first, with ids that sort the same way.
function chain(count: number): Made[] {
  return Array.from({ length: count }, (_, at) => ({
    id: String(count - at).padStart(40, "0"),
    summary: `commit ${count - at}`,
    parents: at === count - 1 ? [] : [String(count - at - 1).padStart(40, "0")],
  }));
}

async function open(commits: Made[]) {
  backend.current = new Repository([modified]);
  backend.current.log = commits;
  vi.resetModules();
  const state = await import("./state");
  await state.boot();
  return state;
}

/// The state settles asynchronously; a test waits for it rather than for a tick.
async function settled(state: typeof import("./state")): Promise<void> {
  await until(state, (app) => !app.historyLoading);
}

/// Waits only for what is on screen, not for a request still in flight.
///
/// The two are different when a test deliberately holds a page open: the list
/// has been redrawn and there is something to assert, while `historyLoading` is
/// still true and will stay true until the test releases it.
async function drawn(state: typeof import("./state")): Promise<void> {
  await until(state, () => true);
}

async function until(
  state: typeof import("./state"),
  extra: (app: typeof state.app) => boolean,
): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    await new Promise((resume) => setTimeout(resume, 0));
    if (
      state.app.history.status !== "loading" &&
      state.app.commit.status !== "loading" &&
      extra(state.app)
    ) {
      return;
    }
  }
  throw new Error("l'historique ne s'est jamais stabilisé");
}

const rows = (state: typeof import("./state")) =>
  state.app.history.status === "ready" ? state.app.history.value : [];

beforeEach(() => {
  backend.current = new Repository([]);
});

describe("opening History", () => {
  it("does not read the history until someone looks at it", async () => {
    // A walk of a hundred thousand commits is not what someone who wanted to
    // stage a file asked for.
    const state = await open(chain(9));
    expect(state.app.history.status).toBe("idle");
    expect(backend.current.calls.filter((call) => call.command === "history")).toHaveLength(0);
  });

  it("reads it on the first switch and not on the second", async () => {
    const state = await open(chain(9));

    state.showScreen("history");
    await settled(state);
    expect(rows(state)).toHaveLength(3);

    state.showScreen("working-copy");
    state.showScreen("history");
    await settled(state);
    expect(backend.current.calls.filter((call) => call.command === "history")).toHaveLength(1);
  });

  it("opens on the newest commit rather than on an empty pane", async () => {
    const state = await open(chain(9));
    state.showScreen("history");
    await settled(state);

    expect(state.app.commit.status).toBe("ready");
    expect(state.app.commit.status === "ready" && state.app.commit.value.summary).toBe("commit 9");
    // And on a file of it, so the diff pane is not blank either.
    expect(state.app.commitFile).toBe("a.txt");
  });

  it("says an empty history is empty rather than failing", async () => {
    const state = await open([]);
    state.showScreen("history");
    await settled(state);

    expect(rows(state)).toHaveLength(0);
    expect(state.app.historyDone).toBe(true);
    expect(state.app.history.status).toBe("ready");
  });
});

describe("paging", () => {
  it("appends the next page instead of replacing what is there", async () => {
    const state = await open(chain(9));
    state.showScreen("history");
    await settled(state);

    await state.moreHistory();
    await settled(state);

    expect(rows(state)).toHaveLength(6);
    const ids = rows(state).map((row) => row.id.full);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids).toEqual(backend.current.handedOut);
  });

  it("stops asking once the walk has reached the roots", async () => {
    const state = await open(chain(5));
    state.showScreen("history");
    await settled(state);

    await state.moreHistory();
    await settled(state);
    expect(state.app.historyDone).toBe(true);

    const before = backend.current.calls.length;
    await state.moreHistory();
    await settled(state);
    expect(backend.current.calls).toHaveLength(before);
    expect(rows(state)).toHaveLength(5);
  });

  it("asks once for a page even when the list asks four times", async () => {
    // The virtualised list emits `nearEnd` on every scroll event inside the
    // last overscan, which is several per wheel notch. Without the guard the
    // same page arrives four times and every row is drawn four times.
    const state = await open(chain(30));
    state.showScreen("history");
    await settled(state);

    void state.moreHistory();
    void state.moreHistory();
    void state.moreHistory();
    void state.moreHistory();
    await settled(state);

    expect(backend.current.calls.filter((call) => call.command === "history_more")).toHaveLength(1);
    expect(rows(state)).toHaveLength(6);
  });

  it("never continues a walk it has not started", async () => {
    // `history_more` on the backend refuses when nothing is parked, and the
    // front end must not be the thing that provokes that error.
    const state = await open(chain(9));
    await state.moreHistory();
    await settled(state);

    expect(backend.current.calls.filter((call) => call.command === "history_more")).toHaveLength(0);
  });
});

describe("changing the query", () => {
  it("restarts the walk rather than adding to it", async () => {
    const state = await open(chain(9));
    state.showScreen("history");
    await settled(state);
    await state.moreHistory();
    await settled(state);
    expect(rows(state)).toHaveLength(6);

    await state.setQuery({ all: true });
    await settled(state);

    expect(state.app.query.all).toBe(true);
    expect(rows(state)).toHaveLength(3);
    const restart = backend.current.calls.filter((call) => call.command === "history");
    expect(restart).toHaveLength(2);
    expect(restart[1]!.args["query"]).toEqual({
      all: true,
      firstParent: false,
      author: "",
      text: "",
      path: "",
      since: 0,
      until: 0,
    });
  });

  it("drops a page that belonged to the query before it changed", async () => {
    const state = await open(chain(30));
    state.showScreen("history");
    await settled(state);

    // The page is held open, so it lands *after* the restarted walk has already
    // filled the list. That is the order that matters: if the restart finished
    // last it would replace whatever the stale page appended, and the bug would
    // leave no trace.
    let release = (): void => {};
    backend.current.holdHistoryMore = new Promise((resume) => {
      release = () => resume();
    });
    const inFlight = state.moreHistory();

    await state.setQuery({ firstParent: true });
    await drawn(state);
    expect(rows(state)).toHaveLength(3);

    release();
    await inFlight;
    await settled(state);

    // Still three: the page that arrived describes a history nobody is looking
    // at any more.
    expect(rows(state)).toHaveLength(3);
  });
});

describe("the commit detail", () => {
  it("follows the selection", async () => {
    const state = await open(chain(9));
    state.showScreen("history");
    await settled(state);

    const second = rows(state)[1]!.id.full;
    await state.selectCommit(second);
    await settled(state);

    expect(state.app.commit.status === "ready" && state.app.commit.value.id.full).toBe(second);
    expect(state.app.diff.status).toBe("ready");
  });

  it("leaves the Working Copy's own state alone", async () => {
    // Both screens write to `state.diff`. What must not happen is History
    // quietly changing which file the Working Copy thinks is selected.
    const state = await open(chain(9));
    expect(state.app.selected).toEqual({ path: "a.txt", staged: false });

    state.showScreen("history");
    await settled(state);
    expect(state.app.selected).toEqual({ path: "a.txt", staged: false });
  });

  it("forgets the history when another repository is opened", async () => {
    const state = await open(chain(9));
    state.showScreen("history");
    await settled(state);
    expect(rows(state)).toHaveLength(3);

    await state.openRepository("/repo");
    expect(state.app.history.status).toBe("idle");
    expect(state.app.commit.status).toBe("idle");
  });
});

describe("filtering", () => {
  it("sends what was typed and restarts the walk", async () => {
    const state = await open(chain(9));
    state.showScreen("history");
    await settled(state);

    await state.setQuery({ author: "marek" });
    await settled(state);

    const restart = backend.current.calls.filter((call) => call.command === "history");
    expect(restart).toHaveLength(2);
    expect(restart[1]!.args["query"]).toMatchObject({ author: "marek" });
    expect(state.isFilteringHistory()).toBe(true);
  });

  it("stops filtering when the box is cleared", async () => {
    // An empty box is not a filter. If it were, clearing one would leave the
    // screen filtering on the empty string — which matches everything, so
    // nothing would look wrong while the graph stayed hidden.
    const state = await open(chain(9));
    state.showScreen("history");
    await settled(state);

    await state.setQuery({ text: "commit" });
    await settled(state);
    expect(state.isFilteringHistory()).toBe(true);

    await state.setQuery({ text: "   " });
    await settled(state);
    expect(state.isFilteringHistory()).toBe(false);
  });

  it("clears the filters without touching which branches are walked", async () => {
    const state = await open(chain(9));
    state.showScreen("history");
    await settled(state);

    await state.setQuery({ all: true, firstParent: true, author: "marek", path: "src" });
    await settled(state);

    await state.clearFilters();
    await settled(state);

    expect(state.app.query.author).toBe("");
    expect(state.app.query.path).toBe("");
    // Which history to look at is a different question from what to look for.
    expect(state.app.query.all).toBe(true);
    expect(state.app.query.firstParent).toBe(true);
  });
});

describe("comparing two commits", () => {
  it("compares the marked commit with the second one, in that order", async () => {
    const state = await open(chain(9));
    state.showScreen("history");
    await settled(state);

    const [newest, , older] = rows(state).map((row) => row.id.full);
    state.markCompareFrom(newest!);
    await state.compareWith(older!);
    await drawn(state);

    const call = backend.current.calls.find((call) => call.command === "compare");
    expect(call?.args).toMatchObject({ from: newest, to: older });
    expect(state.app.compare.status).toBe("ready");
    // And it opens on a file, so the diff pane is not blank.
    expect(state.app.commitFile).toBe("a.txt");
  });

  it("refuses to compare a commit with itself", async () => {
    const state = await open(chain(9));
    state.showScreen("history");
    await settled(state);

    const only = rows(state)[0]!.id.full;
    state.markCompareFrom(only);
    await state.compareWith(only);
    await drawn(state);

    expect(backend.current.calls.filter((call) => call.command === "compare")).toHaveLength(0);
    expect(state.app.compare.status).toBe("idle");
  });

  it("goes back to the open commit when the comparison stops", async () => {
    const state = await open(chain(9));
    state.showScreen("history");
    await settled(state);
    const [newest, , older] = rows(state).map((row) => row.id.full);

    state.markCompareFrom(newest!);
    await state.compareWith(older!);
    await drawn(state);

    state.stopComparing();
    await settled(state);

    expect(state.app.compare.status).toBe("idle");
    expect(state.app.compareFrom).toBeNull();
    // The detail pane has something in it again, rather than being left blank.
    expect(state.app.commit.status).toBe("ready");
    expect(state.app.commitFile).toBe("a.txt");
  });

  it("drops the comparison when the walk it named is replaced", async () => {
    // Both ends are rows of a history that is about to be replaced; keeping
    // them would leave the pane pointing at commits nobody can see.
    const state = await open(chain(9));
    state.showScreen("history");
    await settled(state);
    const [newest, , older] = rows(state).map((row) => row.id.full);

    state.markCompareFrom(newest!);
    await state.compareWith(older!);
    await drawn(state);
    expect(state.app.compare.status).toBe("ready");

    await state.setQuery({ all: true });
    await settled(state);

    expect(state.app.compare.status).toBe("idle");
    expect(state.app.compareFrom).toBeNull();
  });
});

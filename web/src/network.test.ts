// The network half of M7: what the window does while `git` is talking to a
// remote, and what it does when that stops.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { Repository } from "./backend.fake";
import type { Progress } from "./ipc";

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("./backend.fake").Repository>,
  /// The handlers `listen("progress")` registered, so a test can push events.
  listeners: [] as ((event: { payload: Progress }) => void)[],
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: () => Promise.resolve(null) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (_name: string, handler: (event: { payload: Progress }) => void) => {
    backend.listeners.push(handler);
    return Promise.resolve(() => {});
  },
}));

async function running(tracking: { ahead: number; behind: number } | null = null) {
  backend.current = new Repository([
    { path: "a.txt", staged: null, unstaged: "modified", hunks: 1 },
  ]);
  backend.listeners = [];
  if (tracking) {
    backend.current.tracking = {
      upstream: "origin/main",
      ahead: tracking.ahead,
      behind: tracking.behind,
      gone: false,
    };
  }
  vi.resetModules();
  const state = await import("./state");
  await state.boot();
  await state.watchProgress();
  await state.openRepository("/repo");
  await settled(state);
  return state;
}

async function settled(state: typeof import("./state")): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    await new Promise((resume) => setTimeout(resume, 0));
    if (!state.app.running && !state.app.busy && state.app.refs.status !== "loading") return;
  }
  throw new Error("le réseau ne s'est jamais arrêté");
}

const report = (payload: Progress) => backend.listeners.forEach((h) => h({ payload }));

function gate() {
  let release = (): void => {};
  const held = new Promise<void>((resume) => {
    release = () => resume();
  });
  backend.current.holdNetwork = held;
  return () => {
    backend.current.holdNetwork = null;
    release();
  };
}

beforeEach(() => {
  backend.current = new Repository([]);
  backend.listeners = [];
});

describe("what the network says when it is done", () => {
  it("puts the answer where every other success is read", async () => {
    // It had a field of its own that nothing rendered: a fetch, a pull and a
    // push all finished in silence — on the operations that take the longest,
    // which is the worst place for it.
    const state = await running();
    backend.current.networkSays = "Everything up-to-date";

    state.fetchRemote();
    await settled(state);

    expect(state.app.notes).toContain("Fetch terminé");
    expect(state.app.notes).toContain("Everything up-to-date");
  });

  it("says so when git said nothing at all", async () => {
    const state = await running();
    backend.current.networkSays = "";

    state.fetchRemote();
    await settled(state);

    expect(state.app.notes).toContain("rien à faire");
  });
});

describe("a pull that has to choose", () => {
  it("asks how to reconcile when nothing says, and sends the answer once", async () => {
    // Since 2.27 `git pull` refuses on a diverged branch when neither
    // `pull.rebase`, `pull.ff` nor the branch's own setting answers — and every
    // hint it prints is a `git config` line this window cannot run.
    const state = await running({ ahead: 3, behind: 1 });
    backend.current.reconcileConfigured = false;

    state.pullRemote();
    await settled(state);

    expect(state.app.question?.title).toContain("Fusionner ou rebaser");
    expect(state.app.question?.verb).toBe("Fusionner");
    expect(state.app.question?.alternative).toBe("Rebaser");
    // Nothing has run yet.
    expect(backend.current.calls.filter((call) => call.command === "pull")).toHaveLength(0);

    state.answerAlternative();
    await settled(state);

    const pulls = backend.current.calls.filter((call) => call.command === "pull");
    expect(pulls).toHaveLength(1);
    expect(pulls[0]!.args).toMatchObject({ reconcile: "rebase" });
  });

  it("asks nothing when the repository already decided", async () => {
    const state = await running({ ahead: 3, behind: 1 });
    backend.current.reconcileConfigured = true;

    state.pullRemote();
    await settled(state);

    expect(state.app.question).toBeNull();
    const pulls = backend.current.calls.filter((call) => call.command === "pull");
    expect(pulls).toHaveLength(1);
    expect(pulls[0]!.args["reconcile"]).toBeUndefined();
  });

  it("asks nothing when the branch has not diverged", async () => {
    // Only a divergence needs reconciling: behind alone fast-forwards, and
    // ahead alone has nothing to bring in.
    const state = await running({ ahead: 0, behind: 2 });
    backend.current.reconcileConfigured = false;

    state.pullRemote();
    await settled(state);

    expect(state.app.question).toBeNull();
    expect(
      backend.current.calls.filter((call) => call.command === "pull_reconcile_configured"),
    ).toHaveLength(0);
  });
});

describe("running something on the network", () => {
  it("raises the overlay for its duration and lowers it after", async () => {
    const state = await running();
    const release = gate();

    state.fetchRemote();
    await new Promise((r) => setTimeout(r, 0));
    expect(state.app.running?.what).toBe("Fetch");

    release();
    await settled(state);
    expect(state.app.running).toBeNull();
  });

  it("shows what git is saying, as it says it", async () => {
    // The answer to the command only comes back at the end, which is exactly
    // what the user is waiting to hear about.
    const state = await running();
    const release = gate();
    state.fetchRemote();
    await new Promise((r) => setTimeout(r, 0));

    report({ what: "Fetch", phase: "Receiving objects", percent: 42 });
    expect(state.app.running).toMatchObject({ phase: "Receiving objects", percent: 42 });

    report({ what: "Fetch", phase: "Resolving deltas", percent: 100 });
    expect(state.app.running?.phase).toBe("Resolving deltas");

    release();
    await settled(state);
  });

  it("runs one at a time", async () => {
    // Two fetches on one repository race for `.git/FETCH_HEAD`, and an overlay
    // that had to describe two things at once would describe neither.
    const state = await running();
    const release = gate();

    state.fetchRemote();
    state.fetchRemote();
    state.pullRemote();
    await new Promise((r) => setTimeout(r, 0));

    expect(backend.current.calls.filter((c) => c.command === "fetch")).toHaveLength(1);
    expect(backend.current.calls.filter((c) => c.command === "pull")).toHaveLength(0);

    release();
    await settled(state);
  });

  it("keeps what git said", async () => {
    const state = await running();
    backend.current.networkSays = "From github.com:owner/repo\n * [new branch] main";
    state.fetchRemote();
    await settled(state);

    expect(state.app.notes).toContain("new branch");
  });

  it("lowers the overlay when the operation fails, and says why", async () => {
    const state = await running();
    backend.current.failNetwork = "could not resolve host: github.com";

    state.fetchRemote();
    await settled(state);

    expect(state.app.running).toBeNull();
    expect(state.app.writeError?.said).toContain("could not resolve host");
  });

  it("re-reads the branches afterwards, because the divergence moved", async () => {
    const state = await running();
    const before = backend.current.calls.filter((c) => c.command === "refs").length;

    state.fetchRemote();
    await settled(state);

    expect(backend.current.calls.filter((c) => c.command === "refs").length).toBeGreaterThan(
      before,
    );
  });
});

describe("stopping it", () => {
  it("says it is stopping rather than looking as though nothing happened", async () => {
    // `git` stops when it next checks, which is not instant.
    const state = await running();
    const release = gate();
    state.fetchRemote();
    await new Promise((r) => setTimeout(r, 0));

    state.stopNetwork();
    expect(state.app.stopping).toBe(true);
    expect(backend.current.cancelled).toBe(1);

    release();
    await settled(state);
    expect(state.app.stopping).toBe(false);
  });

  it("does nothing when nothing is running", async () => {
    const state = await running();
    state.stopNetwork();
    expect(backend.current.cancelled).toBe(0);
  });
});

describe("pushing", () => {
  it("goes straight out when it is a fast-forward, to the remote it tracks", async () => {
    const state = await running({ ahead: 2, behind: 0 });
    state.pushBranch(false);
    await settled(state);

    const call = backend.current.calls.find((c) => c.command === "push");
    expect(call?.args).toMatchObject({
      remote: "origin",
      branch: "main",
      force: false,
      // It already has an upstream, so there is none to set.
      setUpstream: false,
    });
  });

  it("pushes to the remote of the upstream, not always to origin", async () => {
    const state = await running(null);
    backend.current.tracking = {
      upstream: "upstream/main",
      ahead: 1,
      behind: 0,
      gone: false,
    };
    await state.openRepository("/repo");
    await settled(state);

    state.pushBranch(false);
    await settled(state);

    expect(backend.current.calls.find((c) => c.command === "push")?.args).toMatchObject({
      remote: "upstream",
    });
  });

  it("asks before it forces, and says what force-with-lease means", async () => {
    const state = await running({ ahead: 2, behind: 3 });
    const before = backend.current.calls.length;

    state.pushBranch(true);
    expect(state.app.question?.detail).toContain("force-with-lease");
    expect(backend.current.calls).toHaveLength(before);

    state.answer(true);
    await settled(state);
    expect(backend.current.calls.find((c) => c.command === "push")?.args).toMatchObject({
      force: true,
    });
  });

  it("sets the upstream for a branch that tracks nothing", async () => {
    // Pushing a new branch and then finding it tracks nothing is a second step
    // nobody wants.
    const state = await running(null);
    state.pushBranch(false);
    await settled(state);

    expect(backend.current.calls.find((c) => c.command === "push")?.args).toMatchObject({
      setUpstream: true,
    });
  });
});

// The background fetch (DESIGN v2): the overview's "2 to pull" is only as true
// as the last fetch, so the window keeps it fresh on its own — quietly, never
// in the way of a click, and without hammering a remote it cannot reach.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { Repository } from "./backend.fake";

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("./backend.fake").Repository>,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: () => Promise.resolve(null) }));

const NOW = 2_000_000_000;

async function opened(setup: (repo: Repository) => void = () => {}) {
  backend.current = new Repository([]);
  backend.current.remotes = [{ name: "origin", url: "git@example.com:me/repo.git" }];
  backend.current.fetchedAt = NOW - 60;
  setup(backend.current);
  vi.resetModules();
  const state = await import("./state");
  await state.boot();
  await state.openRepository("/repo");
  backend.current.calls = [];
  return state;
}

const fetches = () => backend.current.calls.filter((call) => call.command === "fetch");

beforeEach(() => vi.useRealTimers());

describe("the background fetch", () => {
  it("leaves a repository fetched a minute ago alone", async () => {
    const state = await opened();
    await state.fetchIfStale(NOW);
    expect(fetches()).toHaveLength(0);
  });

  it("fetches one whose last fetch is older than the period, then re-reads", async () => {
    const state = await opened();
    await state.fetchIfStale(NOW + state.FETCH_EVERY);
    expect(fetches()).toHaveLength(1);
    expect(backend.current.calls.some((call) => call.command === "summary")).toBe(true);
    // Quietly: nothing was drawn over the window, and nothing was said.
    expect(state.app.running).toBeNull();
    expect(state.app.notes).toBeNull();
  });

  it("never fetches a repository with no remote", async () => {
    const state = await opened((repo) => {
      repo.remotes = [];
      repo.fetchedAt = null;
    });
    await state.fetchIfStale(NOW);
    expect(fetches()).toHaveLength(0);
  });

  it("waits a whole period after a failure instead of retrying every check", async () => {
    const state = await opened((repo) => (repo.fetchedAt = null));
    backend.current.failNetwork = "Could not resolve host: example.com";
    await state.fetchIfStale(NOW);
    await state.fetchIfStale(NOW + 30);
    expect(fetches()).toHaveLength(1);
    expect(state.app.writeError).toBeNull();

    await state.fetchIfStale(NOW + state.FETCH_EVERY);
    expect(fetches()).toHaveLength(2);
  });

  it("makes a click on Push wait for it rather than be refused", async () => {
    const state = await opened((repo) => (repo.fetchedAt = null));
    let release = () => {};
    backend.current.holdNetwork = new Promise<void>((resume) => (release = resume));
    const background = state.fetchIfStale(NOW);
    state.pushBranch(false);
    await new Promise((resume) => setTimeout(resume, 0));
    expect(backend.current.calls.filter((call) => call.command === "push")).toHaveLength(0);

    backend.current.holdNetwork = null;
    release();
    await background;
    for (let i = 0; i < 20; i += 1) await new Promise((resume) => setTimeout(resume, 0));
    expect(backend.current.calls.filter((call) => call.command === "push")).toHaveLength(1);
  });
});

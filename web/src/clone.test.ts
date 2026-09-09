// Cloning: the one operation with no repository to start from.
//
// The dialog is where most of the behaviour is — the folder that fills itself
// in, the probe that does not fire on half a URL, the name that stops being
// overwritten once someone types in it — and none of that is reachable from the
// backend's own tests.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { Repository } from "./backend.fake";

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("./backend.fake").Repository>,
  /// What the folder picker hands back when `Parcourir…` is pressed.
  picked: null as string | null,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: () => Promise.resolve(backend.picked),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));

async function booted() {
  backend.current = new Repository([
    { path: "a.txt", staged: null, unstaged: "modified", hunks: 1 },
  ]);
  backend.picked = null;
  vi.resetModules();
  const state = await import("./state");
  await state.boot();
  return state;
}

/// Let every timer the dialog set fire, and every promise they started settle.
async function typed(ms = 800): Promise<void> {
  await vi.advanceTimersByTimeAsync(ms);
  for (let attempt = 0; attempt < 20; attempt += 1) await Promise.resolve();
}

async function idle(state: typeof import("./state")): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    await new Promise((resume) => setTimeout(resume, 0));
    if (!state.app.running) return;
  }
  throw new Error("le clonage ne s'est jamais arrêté");
}

describe("the clone dialog", () => {
  beforeEach(() => {
    vi.useRealTimers();
  });

  it("proposes a destination under the home the platform reported", async () => {
    const state = await booted();
    state.openClone();
    // `~/src`, board 06's own example. Not guessed from a path's shape: there
    // is no path yet to guess from.
    expect(state.app.clone?.parent).toBe("/home/dev/src");
  });

  it("fills the folder name in from the URL", async () => {
    vi.useFakeTimers();
    const state = await booted();
    state.openClone();
    state.setCloneUrl("git@github.com:owner/omarchy-themes.git");
    await typed();

    expect(state.app.clone?.name).toBe("omarchy-themes");
    vi.useRealTimers();
  });

  it("stops filling the name in once someone has typed one", async () => {
    // Overwriting what somebody typed is the worst thing a helpful default can
    // do — and a second clone of the same repository needs a different folder
    // while the URL stays the same.
    vi.useFakeTimers();
    const state = await booted();
    state.openClone();
    state.setCloneUrl("https://github.com/owner/repo.git");
    await typed();
    state.setCloneName("repo-2");
    state.setCloneUrl("https://github.com/owner/repo.git?x=1");
    await typed();

    expect(state.app.clone?.name).toBe("repo-2");
    vi.useRealTimers();
  });

  it("asks the remote whether it is there, once the typing has stopped", async () => {
    vi.useFakeTimers();
    const state = await booted();
    state.openClone();
    state.setCloneUrl("https://github.com/owner/repo.git");
    await typed();

    expect(state.app.clone?.probe).toBe("reachable");
    expect(backend.current.calls.filter((call) => call.command === "check_remote")).toHaveLength(1);
    vi.useRealTimers();
  });

  it("does not ask about half a URL", async () => {
    // A `git` process against a server for every keystroke, and a red line
    // under a field somebody is still typing into.
    vi.useFakeTimers();
    const state = await booted();
    state.openClone();
    for (const partial of ["h", "ht", "http", "https:", "https:/"]) {
      state.setCloneUrl(partial);
      await typed();
    }

    expect(backend.current.calls.filter((call) => call.command === "check_remote")).toHaveLength(0);
    expect(state.app.clone?.probe).toBe("idle");
    vi.useRealTimers();
  });

  it("says what the remote said when it refuses", async () => {
    vi.useFakeTimers();
    const state = await booted();
    backend.current.unreachable = "Permission denied (publickey).";
    state.openClone();
    state.setCloneUrl("git@github.com:owner/private.git");
    await typed();

    expect(state.app.clone?.probe).toEqual({ error: "Permission denied (publickey)." });
    vi.useRealTimers();
  });

  it("refuses to start until it has a URL, a folder and a name", async () => {
    const state = await booted();
    state.openClone();
    expect(state.cloneBlocker(state.app.clone!)).toContain("URL");

    state.setCloneUrl("https://example.com/repo.git");
    expect(state.cloneBlocker(state.app.clone!)).toContain("nom");

    state.setCloneName("repo");
    expect(state.cloneBlocker(state.app.clone!)).toBeNull();
  });

  it("refuses a folder name with a separator in it", async () => {
    // It would put the clone somewhere other than where the destination line
    // says, which is the one thing that line has to be right about.
    const state = await booted();
    state.openClone();
    state.setCloneUrl("https://example.com/repo.git");
    state.setCloneName("a/b");

    expect(state.cloneBlocker(state.app.clone!)).toContain("/");
  });

  it("takes the folder the picker hands back", async () => {
    const state = await booted();
    backend.picked = "/home/dev/work";
    state.openClone();
    await state.browseCloneParent();

    expect(state.app.clone?.parent).toBe("/home/dev/work");
  });

  it("keeps the folder it had when the picker is dismissed", async () => {
    const state = await booted();
    backend.picked = null;
    state.openClone();
    await state.browseCloneParent();

    expect(state.app.clone?.parent).toBe("/home/dev/src");
  });
});

describe("cloning", () => {
  beforeEach(() => {
    vi.useRealTimers();
  });

  it("clones what the form collected, and opens what landed", async () => {
    const state = await booted();
    state.openClone();
    state.setCloneUrl("https://example.com/omarchy-themes.git");
    state.setCloneName("omarchy-themes");
    state.setCloneOption("shallow", true);
    state.startClone();
    await idle(state);

    const asked = backend.current.calls.find((call) => call.command === "clone_repository");
    expect(asked?.args["request"]).toMatchObject({
      url: "https://example.com/omarchy-themes.git",
      parent: "/home/dev/src",
      name: "omarchy-themes",
      shallow: true,
      submodules: false,
    });
    // Straight into it: a clone that finished and left you looking at the same
    // list you started from has made you find it yourself.
    expect(state.app.open).toBe("/home/dev/src/omarchy-themes");
    expect(state.app.screen).toBe("working-copy");
    expect(state.app.repositories.some((row) => row.name === "omarchy-themes")).toBe(true);
  });

  it("closes the dialog for the whole clone, not once it is over", async () => {
    // It has nothing left to show — the overlay takes over — and leaving a form
    // nobody can use over a four-minute operation blocks a screen they can.
    // Asserted while the clone is *in flight*: a dialog closed at the end would
    // look identical from either side of it.
    const state = await booted();
    const held = gate();
    backend.current.holdNetwork = held.promise;
    state.openClone();
    state.setCloneUrl("https://example.com/repo.git");
    state.setCloneName("repo");
    state.startClone();
    await Promise.resolve();

    expect(state.app.running?.what).toBe("Clonage");
    expect(state.app.clone).toBeNull();
    held.release();
    await idle(state);
  });

  it("puts the overlay up for the whole clone and takes it down after", async () => {
    const state = await booted();
    const held = gate();
    backend.current.holdNetwork = held.promise;
    state.openClone();
    state.setCloneUrl("https://example.com/repo.git");
    state.setCloneName("repo");
    state.startClone();
    await Promise.resolve();

    expect(state.app.running?.what).toBe("Clonage");
    held.release();
    await idle(state);
    expect(state.app.running).toBeNull();
  });

  it("says what failed, on the screen the clone was asked from", async () => {
    // `addError` was being set and never rendered; a clone that failed silently
    // looks exactly like a button that does nothing.
    const state = await booted();
    backend.current.failNetwork = "repository not found";
    state.openClone();
    state.setCloneUrl("https://example.com/gone.git");
    state.setCloneName("gone");
    state.startClone();
    await idle(state);

    expect(state.app.addError).toContain("repository not found");
    expect(state.app.open).toBeNull();
    expect(state.app.screen).toBe("repositories");
  });

  it("does not start a second one while the first is running", async () => {
    const state = await booted();
    const held = gate();
    backend.current.holdNetwork = held.promise;
    state.openClone();
    state.setCloneUrl("https://example.com/repo.git");
    state.setCloneName("repo");
    state.startClone();
    await Promise.resolve();

    state.openClone();
    state.setCloneUrl("https://example.com/other.git");
    state.setCloneName("other");
    state.startClone();
    await Promise.resolve();

    held.release();
    await idle(state);
    expect(
      backend.current.calls.filter((call) => call.command === "clone_repository"),
    ).toHaveLength(1);
  });
});

function gate() {
  let release = (): void => {};
  const promise = new Promise<void>((resume) => {
    release = resume;
  });
  return { promise, release };
}

// The band that offers a newer build (SPEC §11, M10).
//
// Whether the download really lands, and whether its signature verifies, is
// `tauri-plugin-updater`'s business and is not tested here. What is tested is
// the half this project owns: that nothing is replaced without a click, that
// the band says which version it is about in every state it has, and that "no
// thanks" is an answer about a version rather than about updating.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { Repository } from "./backend.fake";
import type { Downloaded, UpdateOffer } from "./ipc";

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("./backend.fake").Repository>,
  /// The handlers `listen("update-progress")` registered, so a test can report
  /// a download moving.
  listeners: [] as ((event: { payload: Downloaded }) => void)[],
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: (event: { payload: Downloaded }) => void) => {
    if (name === "update-progress") backend.listeners.push(handler);
    return Promise.resolve(() => {});
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: () => Promise.resolve(null) }));

const OFFER: UpdateOffer = { version: "0.2.0" };

async function window(offer: UpdateOffer | null) {
  backend.current = new Repository([]);
  backend.current.offer = offer;
  vi.resetModules();
  const state = await import("./state");
  await state.boot();
  await state.watchUpdate();
  await state.checkForUpdate();
  return state;
}

/// The band, mounted against whatever `state` currently holds. Imported inside
/// rather than at the top of the file: `vi.resetModules()` runs between tests,
/// and a component captured before it would be bound to a dead store.
async function band() {
  const Update = (await import("./components/Update.vue")).default;
  return mount(Update);
}

const settle = () => new Promise((resume) => setTimeout(resume, 0));

const downloaded = (percent: number | null) =>
  backend.listeners.forEach((handler) => handler({ payload: { percent } }));

const ran = (command: string) =>
  backend.current.calls.filter((call) => call.command === command).length;

beforeEach(() => {
  backend.listeners = [];
});

describe("a newer build", () => {
  it("says nothing at all when there is nothing to offer", async () => {
    const state = await window(null);
    expect(state.app.update).toBeNull();
    expect((await band()).find("aside").exists()).toBe(false);
  });

  it("names the version rather than announcing that an update exists", async () => {
    await window(OFFER);
    expect((await band()).text()).toContain("0.2.0");
  });

  // The whole point of the shape that was chosen: a check at start-up is not
  // an install at start-up.
  it("downloads nothing until somebody asks", async () => {
    await window(OFFER);
    expect(ran("update_offer")).toBe(1);
    expect(ran("update_install")).toBe(0);
    expect(ran("update_restart")).toBe(0);
  });

  it("follows the download, and runs indeterminate until the size is known", async () => {
    const state = await window(OFFER);
    const installing = state.installUpdate();
    expect(state.app.update?.phase).toBe("downloading");
    expect(state.app.update?.percent).toBeNull();

    downloaded(40);
    expect(state.app.update?.percent).toBe(40);
    await installing;
  });

  // Two clicks, not one: an application that replaced itself and vanished
  // mid-sentence is indistinguishable from one that crashed.
  it("waits for a second click before restarting", async () => {
    const state = await window(OFFER);
    await state.installUpdate();
    expect(state.app.update?.phase).toBe("ready");
    expect(backend.current.restarted).toBe(false);

    await state.restartForUpdate();
    expect(backend.current.restarted).toBe(true);
  });

  it("keeps the version in the sentence when the install fails, and what it said", async () => {
    const state = await window(OFFER);
    backend.current.updateFails = "le serveur a fermé la connexion";
    await state.installUpdate();

    expect(state.app.update?.phase).toBe("failed");
    expect(state.app.update?.said).toContain("le serveur a fermé la connexion");
    const text = (await band()).text();
    expect(text).toContain("0.2.0");
    expect(text).toContain("le serveur a fermé la connexion");
  });

  // "No thanks" is an answer about 0.2.0, not about updating — the backend is
  // told which version, so the next one comes back.
  it("skips a version by name", async () => {
    const state = await window(OFFER);
    await state.dismissUpdate();

    expect(state.app.update).toBeNull();
    expect(backend.current.skipped).toEqual(["0.2.0"]);
  });

  // Dismissing a build that is already on disk would leave the new one beside
  // the old one with no way back to the button that starts it.
  it("cannot be waved away once the build is on disk", async () => {
    const state = await window(OFFER);
    await state.installUpdate();
    await state.dismissUpdate();

    expect(state.app.update?.phase).toBe("ready");
    expect(backend.current.skipped).toEqual([]);
  });

  // Nor while it is being fetched, and for the same reason: the download does
  // not stop because the band went away, so dismissing it would hide a build
  // that is about to be on disk.
  it("cannot be waved away while it is being fetched", async () => {
    const state = await window(OFFER);
    let finish = () => {};
    backend.current.holdUpdate = new Promise((resume) => (finish = () => resume()));

    const installing = state.installUpdate();
    await settle();
    await state.dismissUpdate();

    expect(state.app.update?.phase).toBe("downloading");
    expect(backend.current.skipped).toEqual([]);

    finish();
    await installing;
    expect(state.app.update?.phase).toBe("ready");
  });
});

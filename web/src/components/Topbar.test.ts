// What the topbar says, and when.
//
// Board 06 labels its frame "topbar réduite à 40px · pas de dépôt ouvert", so
// the topbar's shape is a property of the screen. Keying it on the *open*
// repository instead left the repository's name and its Fetch/Pull/Push on the
// Dépôts screen, which made going back there look as though it had not worked.
//
// It mounts `App.vue` rather than `Topbar.vue`, because the bug is about the
// two moving together: a topbar test that set the screen by hand would agree
// with any routing at all.

import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Repository } from "../backend.fake";

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("../backend.fake").Repository>,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: () => Promise.resolve(null) }));

async function running() {
  backend.current = new Repository([
    { path: "a.txt", staged: null, unstaged: "modified", hunks: 1 },
  ]);
  vi.resetModules();
  const state = await import("../state");
  await state.boot();
  const App = (await import("../App.vue")).default;
  const app = mount(App, { attachTo: document.body });
  await settle(state, app);
  return { state, app };
}

async function settle(
  state: typeof import("../state"),
  app: { vm: { $nextTick: () => Promise<void> } },
): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    await new Promise((resume) => setTimeout(resume, 0));
    if (state.app.status.status !== "loading" && state.app.summary !== null) break;
  }
  await app.vm.$nextTick();
}

const topbar = (app: { find: (s: string) => { text: () => string } }) => app.find(".topbar").text();

beforeEach(() => {
  backend.current = new Repository([]);
});

describe("the topbar", () => {
  it("names the app and its actions while no repository is open", async () => {
    const { app } = await running();

    expect(topbar(app)).toContain("omagit");
    expect(topbar(app)).toContain("Dépôts");
    expect(topbar(app)).toContain("Ajouter un dépôt local");
    expect(topbar(app)).not.toContain("Fetch");
    expect(app.find(".topbar").classes()).toContain("compact");
  });

  it("names the repository and its actions once one is open", async () => {
    const { state, app } = await running();
    await state.openRepository("/repo");
    await settle(state, app);

    expect(topbar(app)).toContain("repo");
    expect(topbar(app)).toContain("Fetch");
    expect(topbar(app)).not.toContain("Ajouter un dépôt local");
    // Board 02's 48, because the repository block is two lines.
    expect(app.find(".topbar").classes()).not.toContain("compact");
  });

  it("goes back to the app's own topbar on Dépôts, with a repository still open", async () => {
    // The reported bug: the header did not come back. The repository stays
    // loaded on purpose — you may be going back to switch and return — but the
    // window is not *in* it any more, and the topbar has to say so.
    const { state, app } = await running();
    await state.openRepository("/repo");
    await settle(state, app);

    state.showScreen("repositories");
    await app.vm.$nextTick();

    expect(state.app.open).toBe("/repo");
    expect(topbar(app)).toContain("Dépôts");
    expect(topbar(app)).toContain("Ajouter un dépôt local");
    expect(topbar(app)).not.toContain("Fetch");
    expect(app.find(".topbar").classes()).toContain("compact");
  });
});

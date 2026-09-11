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

async function running(os: "linux" | "macos" = "linux") {
  backend.current = new Repository([
    { path: "a.txt", staged: null, unstaged: "modified", hunks: 1 },
  ]);
  backend.current.os = os;
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
    expect(topbar(app)).toContain("Repositories");
    expect(topbar(app)).toContain("Add a local repository");
    expect(topbar(app)).not.toContain("Fetch");
    expect(app.find(".topbar").classes()).toContain("compact");
  });

  it("names the repository and its actions once one is open", async () => {
    const { state, app } = await running();
    await state.openRepository("/repo");
    await settle(state, app);

    expect(topbar(app)).toContain("repo");
    expect(topbar(app)).toContain("Fetch");
    expect(topbar(app)).not.toContain("Add a local repository");
    // Board 02's 48, because the repository block is two lines.
    expect(app.find(".topbar").classes()).not.toContain("compact");
  });

  it("draws the window buttons the backend asked for, and no others", async () => {
    // Board 02: on Linux the window has no system title bar, so the app draws
    // its own — but *which* ones is the backend's answer, not the front end's
    // guess. The fake speaks for a tiled Hyprland session: close alone, because
    // the compositor owns the size.
    const { app } = await running();

    const buttons = app.findAll(".caption button");
    expect(buttons).toHaveLength(1);
    const close = buttons[0]!;
    expect(close.attributes("aria-label")).toBe("Close");
    // Never in the tab order: Tab belongs to the app's own controls.
    expect(close.attributes("tabindex")).toBe("-1");
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
    expect(topbar(app)).toContain("Repositories");
    expect(topbar(app)).toContain("Add a local repository");
    expect(topbar(app)).not.toContain("Fetch");
    expect(app.find(".topbar").classes()).toContain("compact");
  });
});

describe("the way to Preferences", () => {
  it("is in the topbar, on the screen where there is no sidebar", async () => {
    // Preferences belong to the application, not to a repository — the routing
    // has said so since the screen was built. The only ways in were the
    // sidebar, which is not drawn until a repository is open, and a shortcut
    // you had to know already.
    const { state, app } = await running();
    expect(state.app.open).toBeNull();

    const settings = app
      .find(".topbar")
      .findAll("button")
      .find((one) => one.attributes("aria-label") === "Settings")!;
    await settings.trigger("click");
    await app.vm.$nextTick();

    expect(state.app.screen).toBe("settings");
    // And the bar says where you are rather than "Repositories", which is what
    // it said while showing another screen.
    expect(topbar(app)).toContain("Settings");
  });
});

describe("the topbar under a window system that draws into it", () => {
  it("keeps one height on macOS, where the traffic lights are placed once", async () => {
    // Board 06 reduces the bar to 40px with no repository open. On macOS the
    // system draws its window controls *into* that bar and Tauri places them
    // once, at window creation — a bar that changed height between screens
    // would have them centred on one and off-centre on the other, for ever.
    const { app } = await running("macos");
    expect(app.find(".topbar").classes()).not.toContain("compact");
    // Still the app's own bar, with the app's own content on it.
    expect(topbar(app)).toContain("Repositories");
  });

  it("still reduces it where the app owns the whole bar", async () => {
    const { app } = await running("linux");
    expect(app.find(".topbar").classes()).toContain("compact");
  });
});

// The Preferences screen: what it says, and what it changes the moment it is
// said.
//
// DESIGN §7 lists this screen as not yet designed, so what is tested here is
// the behaviour rather than the shape: a source this machine cannot offer is
// drawn and disabled, the theme on screen is named, and every change lands in
// the window in the same tick it is saved.

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

async function opened() {
  backend.current = new Repository([]);
  vi.resetModules();
  const state = await import("./state");
  await state.boot();
  state.showScreen("settings");
  await settled(state);
  const Settings = (await import("./components/Settings.vue")).default;
  return { state, screen: mount(Settings) };
}

async function settled(state: typeof import("./state")): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    await new Promise((resume) => setTimeout(resume, 0));
    if (!state.app.busy && state.app.preferences.status !== "loading") return;
  }
  throw new Error("les réglages ne se sont jamais stabilisés");
}

beforeEach(() => {
  backend.current = new Repository([]);
});

describe("the preferences screen", () => {
  it("opens without a repository, because that is when you may need it", async () => {
    // The theme is unreadable, or `git` is missing: both are reasons to come
    // here before opening anything.
    const { state } = await opened();
    expect(state.app.open).toBeNull();
    expect(state.app.screen).toBe("settings");
    expect(state.app.preferences.status).toBe("ready");
  });

  it("names the theme that is on screen, not the one that was asked for", async () => {
    // SPEC §6.1's sources fall back: asking for Omarchy on a machine without it
    // resolves to something else, and a screen showing the request would lie.
    const { screen } = await opened();
    expect(screen.text()).toContain("Tokyo Night");
  });

  it("draws a source this machine cannot offer, disabled, with the reason", async () => {
    // Hidden, it becomes a thing nobody can ask about — on the very screen
    // somebody opens *because* the theme is not what they expected.
    const { screen } = await opened();
    const omarchy = screen.findAll(".settings-row").find((row) => row.text().includes("Omarchy"))!;
    expect(omarchy.attributes("disabled")).toBeDefined();
    expect(omarchy.text()).toContain("aucun Omarchy");
  });

  it("applies a change to the window in the same tick it saves it", async () => {
    const { state } = await opened();
    state.chooseDensity("compact");
    await settled(state);

    const sent = backend.current.calls.find((call) => call.command === "set_density");
    expect(sent?.args).toEqual({ density: "compact" });
    // The stylesheet the backend rendered is on the element, not asked for
    // again afterwards — two round trips where the second can fail on its own.
    expect(document.documentElement.getAttribute("style")).toContain("--row-height");
    expect(state.app.preferences.status === "ready" && state.app.preferences.value.density).toBe(
      "compact",
    );
  });

  it("holds the scale to the range a window can be used at", async () => {
    const { state, screen } = await opened();
    state.chooseScale(2);
    await settled(state);
    await screen.vm.$nextTick();

    const plus = screen.findAll("button").find((button) => button.text() === "+")!;
    expect(plus.attributes("disabled")).toBeDefined();
  });

  it("chooses a theme by name, which turns every follower off", async () => {
    const { state } = await opened();
    state.chooseTheme("user-override", "Rosé Pine Dawn");
    await settled(state);

    const sent = backend.current.calls.find((call) => call.command === "set_theme");
    expect(sent?.args).toEqual({ source: "user-override", name: "Rosé Pine Dawn" });
  });
});

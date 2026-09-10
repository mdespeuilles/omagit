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

async function opened(keymap: Record<string, string> = {}) {
  backend.current = new Repository([]);
  backend.current.keymap = keymap;
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
    // By its label, not by the word: "Automatique" names Omarchy too, in the
    // sentence that says what it falls back through.
    const omarchy = screen
      .findAll(".settings-row")
      .find((row) => row.text().startsWith("Suivre Omarchy"))!;
    expect(omarchy.attributes("disabled")).toBeDefined();
    expect(omarchy.text()).toContain("aucun Omarchy");
  });

  it("marks exactly one theme choice, whichever half it is in", async () => {
    // The sources and the catalogue set the same setting, and drawing them as
    // two lists let "Suivre le système" and a named theme both look chosen. One
    // radio group: one mark, wherever it lands.
    const { state, screen } = await opened();
    const marked = () => screen.findAll('[role="radio"][aria-checked="true"]').map((o) => o.text());

    expect(marked()).toHaveLength(1);

    // A theme the catalogue actually holds: the fake carries two, and asking
    // for one it does not have marks nothing — which is the truth, not a bug.
    state.chooseTheme("user-override", "Rosé Pine Dawn");
    await settled(state);
    await screen.vm.$nextTick();
    expect(marked()).toHaveLength(1);
    expect(marked()[0]).toContain("Rosé Pine Dawn");

    state.chooseTheme("embedded-dark");
    await settled(state);
    await screen.vm.$nextTick();
    expect(marked()).toHaveLength(1);
    expect(marked()[0]).toContain("Embarqué");
  });

  it("says what Automatique resolves through", async () => {
    // "ce que cette machine offre de mieux" said nothing anyone could act on.
    const { screen } = await opened();
    const automatic = screen
      .findAll(".settings-row")
      .find((row) => row.text().startsWith("Automatique"))!;
    expect(automatic.text()).toContain("Omarchy");
    expect(automatic.text()).toContain("embarqué");
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

// ── The reassignable keymap (SPEC §11) ──────────────────────────────────────
//
// The screen listens on the window rather than through a text box: a key press
// is not something a box could report without inventing a spelling for it, and
// half the interesting bindings are ones it would swallow.

/// The key button of one action's row.
function keyOf(screen: ReturnType<typeof mount>, label: string) {
  const row = screen.findAll(".keymap-row").find((entry) => entry.text().includes(label))!;
  return { row, key: row.find(".keymap-key") };
}

const held = (init: KeyboardEventInit) =>
  new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });

describe("reassigning a binding", () => {
  it("takes the key that is pressed, and says so everywhere at once", async () => {
    const { state, screen } = await opened();
    const { key } = keyOf(screen, "Fetch");
    await key.trigger("click");
    expect(key.text()).toBe("Appuie…");

    window.dispatchEvent(held({ key: "f", ctrlKey: true, altKey: true }));
    await settled(state);
    await screen.vm.$nextTick();

    const sent = backend.current.calls.find((call) => call.command === "set_binding");
    expect(sent?.args).toEqual({ id: "network.fetch", binding: "Alt+Primary+F" });
    // The store, which is what the sheet, the palette and the topbar read.
    expect(state.app.keymap["network.fetch"]).toBe("Alt+Primary+F");
    expect(keyOf(screen, "Fetch").key.text()).toBe("Alt Ctrl F");
  });

  it("refuses a binding somebody else answers, and writes nothing", async () => {
    const { state, screen } = await opened();
    await keyOf(screen, "Fetch").key.trigger("click");

    window.dispatchEvent(held({ key: "p", ctrlKey: true }));
    await screen.vm.$nextTick();

    expect(keyOf(screen, "Fetch").row.text()).toContain("Push");
    expect(backend.current.calls.some((call) => call.command === "set_binding")).toBe(false);
    // Still listening: a refusal is not an answer, so the row keeps the key.
    expect(keyOf(screen, "Fetch").key.text()).toBe("Appuie…");
    expect(state.app.keymap["network.fetch"]).toBeUndefined();
  });

  it("leaves the movement keys alone", async () => {
    // `j` is answered before the table is consulted, so a command bound to it
    // would look assigned and never fire.
    const { screen } = await opened();
    await keyOf(screen, "Fetch").key.trigger("click");

    window.dispatchEvent(held({ key: "j" }));
    await screen.vm.$nextTick();

    expect(keyOf(screen, "Fetch").row.text()).toContain("déplacer");
    expect(backend.current.calls.some((call) => call.command === "set_binding")).toBe(false);
  });

  it("cancels on Escape without changing anything", async () => {
    const { screen } = await opened();
    await keyOf(screen, "Fetch").key.trigger("click");

    window.dispatchEvent(held({ key: "Escape" }));
    await screen.vm.$nextTick();

    expect(keyOf(screen, "Fetch").key.text()).toBe("Ctrl F");
    expect(backend.current.calls.some((call) => call.command === "set_binding")).toBe(false);
  });

  it("puts a binding back, and stops calling it reassigned", async () => {
    const { state, screen } = await opened({ "network.fetch": "Alt+Primary+F" });
    // Both of them: what it answers now, and what it used to.
    expect(keyOf(screen, "Fetch").key.text()).toBe("Alt Ctrl F");
    expect(keyOf(screen, "Fetch").row.find(".keymap-was").text()).toBe("Ctrl F");

    const back = keyOf(screen, "Fetch").row.findAll("button").at(-1)!;
    await back.trigger("click");
    await settled(state);
    await screen.vm.$nextTick();

    const sent = backend.current.calls.find((call) => call.command === "set_binding");
    expect(sent?.args).toEqual({ id: "network.fetch", binding: null });
    expect(state.app.keymap["network.fetch"]).toBeUndefined();
    expect(keyOf(screen, "Fetch").key.text()).toBe("Ctrl F");
  });
});

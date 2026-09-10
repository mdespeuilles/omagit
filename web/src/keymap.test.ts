// The bindings, and the three rules that keep them from firing where they must
// not.
//
// Every one of these is a bug the app shipped: hints beside buttons that
// answered the mouse alone, and — the reason movement keys are not here yet —
// a bare letter that fires while somebody is typing.

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
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: () => Promise.resolve("/repo") }));

async function opened(repository = true) {
  backend.current = new Repository([
    { path: "a.txt", staged: null, unstaged: "modified", hunks: 1 },
  ]);
  vi.resetModules();
  const state = await import("./state");
  await state.boot();
  if (repository) await state.openRepository("/repo");
  await settled(state);
  const keymap = await import("./keymap");
  return { state, keymap };
}

async function settled(state: typeof import("./state")): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    await new Promise((resume) => setTimeout(resume, 0));
    if (!state.app.busy) return;
  }
  throw new Error("l'écran ne s'est jamais stabilisé");
}

/// A key press as the window receives it. The fake platform is Linux, so
/// `Primary` is Ctrl.
function press(key: string, held: Partial<KeyboardEvent> = {}): KeyboardEvent {
  return new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...held });
}

beforeEach(() => {
  backend.current = new Repository([]);
});

describe("the bindings", () => {
  it("runs the action the key names", async () => {
    const { state, keymap } = await opened();

    expect(keymap.dispatch(press("f", { ctrlKey: true }))).toBe(true);
    await settled(state);

    expect(backend.current.calls.some((call) => call.command === "fetch")).toBe(true);
  });

  it("leaves a key nothing claims alone", async () => {
    const { keymap } = await opened();
    expect(keymap.dispatch(press("q", { ctrlKey: true }))).toBe(false);
    expect(keymap.dispatch(press("f"))).toBe(false);
  });

  it("does not answer the other platform's modifier", async () => {
    // `Ctrl+F` on macOS moves the caret forward, and `⌘F` on Linux is nothing.
    // Answering either would break a text field on the platform it belongs to.
    const { keymap } = await opened();
    expect(keymap.dispatch(press("f", { metaKey: true }))).toBe(false);
  });

  it("stays quiet while a dialog is up", async () => {
    // A dialog answers its own keys, and a binding firing behind one would act
    // on a screen nobody can see.
    const { state, keymap } = await opened();
    state.openClone();

    expect(keymap.dispatch(press("f", { ctrlKey: true }))).toBe(false);
    expect(backend.current.calls.some((call) => call.command === "fetch")).toBe(false);
  });

  it("fires a shortcut while typing, but never a bare key", async () => {
    const { keymap } = await opened();
    const box = document.createElement("input");
    document.body.append(box);

    const typed = new KeyboardEvent("keydown", { key: "f", bubbles: true, cancelable: true });
    Object.defineProperty(typed, "target", { value: box });
    expect(keymap.dispatch(typed)).toBe(false);

    const held = new KeyboardEvent("keydown", { key: "f", ctrlKey: true, cancelable: true });
    Object.defineProperty(held, "target", { value: box });
    expect(keymap.dispatch(held)).toBe(true);
    box.remove();
  });

  it("refuses an action whose screen is not open, and says nothing happened", async () => {
    // With no repository there is nothing to fetch from. The key is claimed by
    // no one rather than claimed and ignored, so the browser keeps it.
    const { keymap } = await opened(false);
    expect(keymap.dispatch(press("f", { ctrlKey: true }))).toBe(false);
  });

  it("swallows a key whose action is claimed but not available now", async () => {
    // `⌘F` during a fetch is claimed by Fetch and refused by it: the key must
    // not fall through to the browser, or it would open a find bar over the app.
    const { state, keymap } = await opened();
    backend.current.holdNetwork = new Promise(() => {});
    state.fetchRemote();
    await new Promise((resume) => setTimeout(resume, 0));

    expect(keymap.dispatch(press("f", { ctrlKey: true }))).toBe(true);
    expect(backend.current.calls.filter((call) => call.command === "fetch")).toHaveLength(1);
  });

  it("moves between zones on Tab, but leaves it to a text field", async () => {
    // Documented in KEYMAP.md since M3 and bound to nothing until the `?` sheet
    // went to print it. Inside a field it is the browser's: taking it there
    // would trap somebody in a text box.
    const { state, keymap } = await opened();
    expect(keymap.dispatch(press("Tab"))).toBe(true);
    expect(state.app.zone).toBe(2);
    expect(keymap.dispatch(press("Tab", { shiftKey: true }))).toBe(true);
    expect(state.app.zone).toBe(1);

    const box = document.createElement("input");
    document.body.append(box);
    const inField = new KeyboardEvent("keydown", { key: "Tab", cancelable: true });
    Object.defineProperty(inField, "target", { value: box });
    expect(keymap.dispatch(inField)).toBe(false);
    box.remove();
  });

  it("prints every key it answers, so the sheet cannot drift from the table", async () => {
    // A sheet maintained by hand is wrong by the second slice — this project
    // has the receipt: KEYMAP.md spent three milestones describing a build that
    // had been deleted.
    const { keymap } = await opened();
    const printed = keymap.MOVEMENTS.map((move) => move.print).join(" ");

    for (const key of ["1", "j", "k", "g", "G", "⏎", "/", "Esc", "Tab"]) {
      expect(printed).toContain(key);
    }
  });

  it("opens the sheet on the character the layout actually produces", async () => {
    // The binding was written `Shift+/` and answered nothing: a browser reports
    // the character Shift produced, which is `?`. The sheet meant to stop the
    // documentation drifting was itself unreachable.
    const { state, keymap } = await opened();
    expect(keymap.dispatch(press("?", { shiftKey: true }))).toBe(true);
    expect(state.app.shortcuts).toBe(true);
  });

  it("leaves `?` to a text field, so it can still be typed", async () => {
    const { state, keymap } = await opened();
    const box = document.createElement("textarea");
    document.body.append(box);
    const typed = press("?", { shiftKey: true });
    Object.defineProperty(typed, "target", { value: box });

    expect(keymap.dispatch(typed)).toBe(false);
    expect(state.app.shortcuts).toBe(false);
    box.remove();
  });

  it("prints the binding the way the platform spells it", async () => {
    const { keymap } = await opened();
    expect(keymap.hint("Primary+F", "⌘")).toBe("⌘F");
    expect(keymap.hint("Shift+Primary+N", "⌘")).toBe("⇧⌘N");
    // DESIGN §2: a word beside a letter where there is no glyph.
    expect(keymap.hint("Primary+O", "Ctrl")).toBe("Ctrl O");
    expect(keymap.hint("Shift+Primary+S", "Ctrl")).toBe("Maj Ctrl S");
  });

  it("shows in the topbar what it answers, from the same table", async () => {
    const { keymap } = await opened();
    const Topbar = (await import("./components/Topbar.vue")).default;

    const bar = mount(Topbar);
    const fetch = keymap.ACTIONS.find((action) => action.id === "network.fetch")!;
    expect(bar.text()).toContain(keymap.hint(fetch.binding, "Ctrl"));
  });
});

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

  it("does not answer Tab at all, so the browser can walk the stops", async () => {
    // It answered it for one slice, by cycling the three zones — and
    // `preventDefault`ing every press to do it, which left no button in the
    // window reachable from the keyboard (board 09, §2.52). The stops are
    // declared in the markup instead; the table's job is to stay out of the
    // way.
    const { state, keymap } = await opened();
    expect(keymap.dispatch(press("Tab"))).toBe(false);
    expect(keymap.dispatch(press("Tab", { shiftKey: true }))).toBe(false);
    expect(state.app.zone).toBe(1);
  });

  it("prints every key the window moves by, so the sheet cannot drift", async () => {
    // A sheet maintained by hand is wrong by the second slice — this project
    // has the receipt: KEYMAP.md spent three milestones describing a build that
    // had been deleted. `Tab` is in the list without being in `dispatch`: the
    // browser walks the stops, and a reader cannot tell which of the two
    // answered a key — nor should the sheet ask them to.
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

  it("answers the binding the user chose, not the table's own", async () => {
    // SPEC §11. The whole point of one table: a reassignment reaches the key
    // handler without anything else in the window being told.
    backend.current = new Repository([]);
    backend.current.keymap = { "network.fetch": "Shift+Primary+F" };
    vi.resetModules();
    const state = await import("./state");
    await state.boot();
    await state.openRepository("/repo");
    await settled(state);
    const keymap = await import("./keymap");

    expect(keymap.dispatch(press("f", { ctrlKey: true }))).toBe(false);
    expect(keymap.dispatch(press("f", { ctrlKey: true, shiftKey: true }))).toBe(true);
    await settled(state);
    expect(backend.current.calls.some((call) => call.command === "fetch")).toBe(true);
  });

  it("reads a key press as the binding it would be", async () => {
    const { keymap } = await opened();
    expect(keymap.capture(press("f", { metaKey: true }), "meta")).toBe("Primary+F");
    expect(keymap.capture(press("N", { metaKey: true, shiftKey: true }), "meta")).toBe(
      "Shift+Primary+N",
    );
    expect(keymap.capture(press(",", { ctrlKey: true }), "control")).toBe("Primary+,");
    // A modifier alone is the first half of an answer, not an answer.
    expect(keymap.capture(press("Shift", { shiftKey: true }), "meta")).toBeNull();
  });

  it("refuses a binding that would answer nothing", async () => {
    const { keymap } = await opened();
    // Movement runs before the table, so this one would never fire. Whichever
    // case it was captured in: `matches` compares case-insensitively.
    expect(keymap.refuse("network.fetch", "J")).toContain("move around");
    // Shift is not enough to escape movement — `dispatch` calls that bare too,
    // and `G` is the last row before the table is ever reached.
    expect(keymap.refuse("network.fetch", "Shift+G")).toContain("move around");
    // A modifier that makes the event no longer bare does escape it.
    expect(keymap.refuse("network.fetch", "Primary+J")).toBeNull();
    // And the binding that looks assigned because somebody else answers it.
    expect(keymap.refuse("network.fetch", "Primary+P")).toContain("Push");
    // Its own binding is not a conflict with itself.
    expect(keymap.refuse("network.fetch", "Primary+F")).toBeNull();
    expect(keymap.refuse("network.fetch", "Alt+Primary+F")).toBeNull();
  });

  it("prints a modifier it never had to print before", async () => {
    // Nothing in the table used `Alt`, so `hint` dropped it — and a hint that
    // names a key the app does not answer is the defect this table exists to
    // stop. Reassignment is what made it reachable.
    const { keymap } = await opened();
    expect(keymap.hint("Alt+Primary+F", "⌘")).toBe("⌥⌘F");
    expect(keymap.hint("Alt+Shift+Primary+F", "⌘")).toBe("⌥⇧⌘F");
    expect(keymap.hint("Alt+Primary+F", "Ctrl")).toBe("Alt Ctrl F");
  });

  it("prints the binding the way the platform spells it", async () => {
    const { keymap } = await opened();
    expect(keymap.hint("Primary+F", "⌘")).toBe("⌘F");
    expect(keymap.hint("Shift+Primary+N", "⌘")).toBe("⇧⌘N");
    // DESIGN §2: a word beside a letter where there is no glyph.
    expect(keymap.hint("Primary+O", "Ctrl")).toBe("Ctrl O");
    expect(keymap.hint("Shift+Primary+S", "Ctrl")).toBe("Maj Ctrl S");
  });

  it("tells the topbar what it answers, from the same table", async () => {
    // Printed beside the label until M9's last pass, which is what board 02
    // draws. It is in the tooltip now — three other places print every binding,
    // and a fourth copy in the row where width is scarcest was noise — but it
    // still comes from the table, so a reassignment moves it here too.
    const { keymap } = await opened();
    const Topbar = (await import("./components/Topbar.vue")).default;

    const bar = mount(Topbar);
    const fetch = keymap.ACTIONS.find((action) => action.id === "network.fetch")!;
    const button = bar.findAll("button").find((one) => one.text() === "Fetch")!;
    expect(button.attributes("title")).toContain(keymap.hint(fetch.binding, "Ctrl"));
    // And not beside the label, which is the change.
    expect(bar.text()).not.toContain(keymap.hint(fetch.binding, "Ctrl"));
  });
});

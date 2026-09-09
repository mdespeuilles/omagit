// The confirmation of SPEC §3 rule 7.
//
// Three properties make it one rather than a speed bump, and all three are easy
// to lose to a refactor: the default is Cancel, Escape cancels, and nothing
// runs until it is answered.
//
// It mounts `App.vue` rather than `Confirm.vue`. That is not thoroughness, it
// is the point: the bug this pins is that the dialog used to be mounted for the
// whole session with a `v-if` *inside* it, so `onMounted` ran before there was
// anything to focus and the cancel button never got focus. A test that mounted
// the dialog with the question already set passed with that bug in place — it
// reproduced the test's order, not the application's.

import { mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { Repository, type Fixture } from "../backend.fake";

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("../backend.fake").Repository>,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));

const untracked: Fixture = {
  path: "notes.md",
  staged: null,
  unstaged: "untracked",
  hunks: 1,
};

const row = {
  path: "notes.md",
  staged: null,
  unstaged: "untracked",
  conflict: null,
  code: "??",
};

/// The window as it really starts: shell first, question afterwards.
async function running() {
  backend.current = new Repository([untracked]);
  vi.resetModules();
  const state = await import("../state");
  await state.boot();
  const App = (await import("../App.vue")).default;
  const app = mount(App, { attachTo: document.body });
  await app.vm.$nextTick();
  return { state, app };
}

async function asking() {
  const { state, app } = await running();
  state.discardFile(row);
  await app.vm.$nextTick();
  return { state, app };
}

describe("the confirmation", () => {
  it("is not on screen until there is something to confirm", async () => {
    const { app } = await running();
    expect(app.find(".dialog").exists()).toBe(false);
  });

  it("opens with Cancel focused, so Enter is never the destructive answer", async () => {
    const { app } = await asking();
    const cancel = app.find(".dialog-actions").findAll("button")[0]!;
    expect(cancel.text()).toBe("Annuler");
    expect(document.activeElement).toBe(cancel.element);
  });

  it("names the file and says why the loss is final", async () => {
    const { app } = await asking();
    expect(app.find(".dialog-title").text()).toContain("notes.md");
    expect(app.find(".dialog-detail").text()).toContain("supprime");
  });

  it("cancels on Escape without writing", async () => {
    const { state, app } = await asking();
    const before = backend.current.calls.length;

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    await app.vm.$nextTick();

    expect(state.app.question).toBeNull();
    expect(app.find(".dialog").exists()).toBe(false);
    expect(backend.current.calls).toHaveLength(before);
  });

  it("writes only once the destructive button is pressed", async () => {
    const { app } = await asking();
    await app.find(".dialog-actions .danger").trigger("click");
    // The write is queued asynchronously; one turn of the loop is enough.
    await new Promise((resume) => setTimeout(resume, 0));

    const discard = backend.current.calls.filter((call) => call.command === "discard");
    expect(discard).toHaveLength(1);
    expect(discard[0]!.args["file"]).toBe("notes.md");
  });
});

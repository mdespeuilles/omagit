// What the diff draws, and what clicking in it names.

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

async function drawn(files: Fixture[]) {
  backend.current = new Repository(files);
  vi.resetModules();
  const state = await import("../state");
  await state.boot();
  const DiffView = (await import("./DiffView.vue")).default;
  return { state, diff: mount(DiffView) };
}

const modified = (over: Partial<Fixture> = {}): Fixture => ({
  path: "a.txt",
  staged: null,
  unstaged: "modified",
  hunks: 2,
  ...over,
});

describe("the diff", () => {
  it("highlights the changed words at the right place in a line that is not ASCII", async () => {
    // The refinements are Rust's **byte** offsets and a JavaScript string is
    // indexed in UTF-16 units. Slicing the string directly puts the highlight
    // two characters early on the first line with an accent in it — which is
    // most lines, in this application's own language.
    const text = "première ligne — modifiée";
    const word = "modifiée";
    const bytes = new TextEncoder().encode(text).length;
    const start = bytes - new TextEncoder().encode(word).length;
    // The two disagree, which is the point: 25 characters, 28 bytes.
    expect(bytes).not.toBe(text.length);

    const { diff } = await drawn([modified({ line: { text, refined: [[start, bytes]] } })]);

    const marked = diff.findAll(".diff-line .word").map((span) => span.text());
    expect(marked).toContain(word);
  });

  it("names the hunk a button belongs to, not the row it is drawn on", async () => {
    const { state, diff } = await drawn([modified({ hunks: 3 })]);

    // The third hunk's "stage" button, which is the one that has ever been
    // wrong: the row index and the hunk index are different numbers.
    const buttons = diff.findAll(".hunk-actions button");
    await buttons[4]!.trigger("click");

    const write = backend.current.calls.find((call) => call.command === "stage");
    expect(write?.args["target"]).toEqual({ kind: "hunks", hunks: [2] });
    expect(state.app.selected?.path).toBe("a.txt");
  });

  it("picks a changed line and leaves context alone", async () => {
    const { state, diff } = await drawn([modified()]);

    const lines = diff.findAll(".diff-line");
    await lines[0]!.trigger("click"); // context
    expect(state.app.picked.size).toBe(0);

    await lines[1]!.trigger("click"); // added
    expect(state.app.picked.size).toBe(1);
  });

  it("offers no discard on the staged side", async () => {
    // There is nothing in the working tree to reject: what is staged comes out
    // of the index, which is a different and non-destructive button.
    const { diff } = await drawn([modified({ staged: "modified", unstaged: null })]);

    const labels = diff.findAll(".hunk-actions button").map((button) => button.text());
    expect(labels).toContain("Désindexer le bloc");
    expect(labels).not.toContain("Rejeter");
  });

  it("offers nothing to stage when the diff is a commit's", async () => {
    // Both screens draw through this component, and History's diff is a
    // commit's: there is no index behind it, so the two sides, the hunk buttons
    // and the line picking all name something that does not exist.
    const { state, diff } = await drawn([modified()]);
    state.showScreen("history");
    await diff.vm.$nextTick();

    expect(diff.findAll(".tab")).toHaveLength(0);
    expect(diff.findAll(".hunk-actions")).toHaveLength(0);

    await diff.findAll(".diff-line")[1]!.trigger("click");
    expect(state.app.picked.size).toBe(0);
    expect(diff.find(".picked-bar").exists()).toBe(false);
  });

  it("disables the tab for a side with nothing on it", async () => {
    const { diff } = await drawn([modified({ staged: null, unstaged: "modified" })]);

    const tabs = diff.findAll(".tab");
    expect(tabs[0]!.classes()).toContain("on");
    expect(tabs[1]!.attributes("disabled")).toBeDefined();
  });
});

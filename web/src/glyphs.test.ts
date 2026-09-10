// The icons, and the rule they exist for: drawn, not borrowed.
//
// The sidebar drew `◱` for the working copy, `⌸` for History — an APL symbol —
// `⌥` for Stashes, which is the macOS Option key and means something else
// entirely, and `▾` for a disclosure triangle the fallback font rendered as a
// dot. A character's shape is the font's business, and the font is not ours.

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
  backend.current = new Repository([
    { path: "a.txt", staged: null, unstaged: "modified", hunks: 1 },
  ]);
  vi.resetModules();
  const state = await import("./state");
  await state.boot();
  await state.openRepository("/repo");
  for (let attempt = 0; attempt < 200; attempt += 1) {
    await new Promise((resume) => setTimeout(resume, 0));
    if (!state.app.busy && state.app.refs.status !== "loading") break;
  }
  return state;
}

/// The characters the two left-hand bars used to stand in for objects. None of
/// them is a shape this app chose.
const BORROWED = ["◱", "⌸", "⌥", "⚙", "◧", "▾", "▸", "▤", "⊘"];

beforeEach(() => {
  backend.current = new Repository([]);
});

describe("the drawn icons", () => {
  it("draws one for each row of the sidebar, and borrows none", async () => {
    await opened();
    const Sidebar = (await import("./components/Sidebar.vue")).default;
    const bar = mount(Sidebar);

    // Four screens, the section head's chevron, and the way back to the list.
    expect(bar.findAll("svg.glyph").length).toBeGreaterThanOrEqual(6);
    for (const character of BORROWED) {
      expect(bar.text()).not.toContain(character);
    }
  });

  it("marks a branch as a branch and a prefix as a folder", async () => {
    await opened();
    const Tree = (await import("./components/BranchTree.vue")).default;
    const tree = mount(Tree);

    const rows = tree.findAll(".branch-row");
    expect(rows.length).toBeGreaterThan(0);
    for (const row of rows) expect(row.find("svg.glyph").exists()).toBe(true);
    for (const character of BORROWED) {
      expect(tree.text()).not.toContain(character);
    }
  });

  it("gives every name in the set a shape", async () => {
    // The component is the one place they live, so an icon added without a
    // path — or a name misspelt at a call site — is a blank square on screen.
    const Glyph = (await import("./components/Glyph.vue")).default;
    const names = ["working-copy", "history", "stashes", "settings", "branch", "folder"] as const;
    for (const name of names) {
      const drawn = mount(Glyph, { props: { name } });
      expect(drawn.html(), name).toContain("<path");
      // jsdom lower-cases an attribute it does not know as SVG's own.
      expect(drawn.html()).toContain('viewBox="0 0 16 16"');
    }
  });
});

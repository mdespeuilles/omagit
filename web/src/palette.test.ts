// The palette: what it finds, in what order, and what ⏎ does to it.
//
// The ranking is tested against the pure function rather than through the
// overlay — the rules are "which of two rows comes first" and "which characters
// are marked", and neither needs a window to be true.

import { mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { Repository } from "./backend.fake";
import { flatten, fuzzy, search } from "./palette";
import type { Action } from "./keymap";

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("./backend.fake").Repository>,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: () => Promise.resolve(null) }));

const action = (id: string, label: string, enabled = true): Action => ({
  id,
  label,
  binding: "Primary+X",
  where: "always",
  menu: "view",
  enabled: () => enabled,
  run: () => {},
});

const sources = (over: Partial<Parameters<typeof search>[1]> = {}) => ({
  actions: [],
  repositories: [],
  branches: [],
  files: [],
  ...over,
});

describe("the fuzzy match", () => {
  it("finds the letters in order, wherever they are", () => {
    expect(fuzzy("thm", "Theme")).not.toBeNull();
    expect(fuzzy("thm", "Theme")!.marks).toEqual([0, 1, 3]);
    // Out of order is not a match: `mth` is not a worse "Theme", it is a
    // different word.
    expect(fuzzy("mth", "Theme")).toBeNull();
  });

  it("puts the letters that start words first", () => {
    // `wc` is Working Copy, not "switch". Without the word-start bonus the
    // scatter wins, because it is shorter.
    const rows = ["switch", "Working Copy"];
    const best = rows
      .map((label) => ({ label, hit: fuzzy("wc", label)! }))
      .sort((a, b) => b.hit.score - a.hit.score)[0]!;
    expect(best.label).toBe("Working Copy");
  });

  it("prefers a run over a scatter", () => {
    const run = fuzzy("push", "Push")!;
    const scatter = fuzzy("push", "Publier sur ce hôte")!;
    expect(run.score).toBeGreaterThan(scatter.score);
  });
});

describe("what the palette searches", () => {
  it("groups what it finds by what it is, in a fixed order", () => {
    const groups = search("", {
      actions: [action("a.one", "Fetch")],
      repositories: [
        {
          group: 0,
          index: 0,
          group_name: "Récents",
          path: "/repo",
          name: "repo",
          description: "",
          last_opened: null,
          missing: false,
        },
      ],
      branches: [
        {
          name: "feature/theme-runtime",
          commit: { full: "a".repeat(40), short: "aaaaaaa" },
          head: false,
          tracking: null,
          merged: false,
          age: 0,
        },
      ],
      files: [
        { path: "src/theme.rs", staged: null, unstaged: "modified", conflict: null, code: " M" },
      ],
    });

    expect(groups.map((group) => group.name)).toEqual([
      "Actions",
      "Dépôts",
      "Branches",
      "Fichiers",
    ]);
    // Each row says what ⏎ will do to it: four kinds, four sentences.
    expect(flatten(groups).map((row) => row.hint)).toEqual([
      "exécuter ⏎",
      "ouvrir ⏎",
      "basculer ⏎",
      "ouvrir ⏎",
    ]);
  });

  it("keeps an empty group out rather than drawing its header over nothing", () => {
    const groups = search("zzz", sources({ actions: [action("a.one", "Fetch")] }));
    expect(groups).toEqual([]);
  });

  it("dims what cannot run rather than hiding it", () => {
    // An action that disappears when it is unavailable is one nobody learns.
    const groups = search("", sources({ actions: [action("a.one", "Fetch", false)] }));
    expect(flatten(groups)[0]!.enabled).toBe(false);
  });

  it("marks the branch you are on as the one there is nothing to do to", () => {
    const groups = search(
      "",
      sources({
        branches: [
          {
            name: "main",
            commit: { full: "a".repeat(40), short: "aaaaaaa" },
            head: true,
            tracking: { upstream: "origin/main", ahead: 2, behind: 0, gone: false },
            merged: true,
            age: 0,
          },
        ],
      }),
    );
    const row = flatten(groups)[0]!;
    expect(row.enabled).toBe(false);
    expect(row.detail).toBe("branche courante");
  });
});

describe("the palette on screen", () => {
  async function opened() {
    backend.current = new Repository([
      { path: "src/theme.rs", staged: null, unstaged: "modified", hunks: 1 },
    ]);
    vi.resetModules();
    const state = await import("./state");
    await state.boot();
    await state.openRepository("/repo");
    for (let attempt = 0; attempt < 100; attempt += 1) {
      await new Promise((resume) => setTimeout(resume, 0));
      if (!state.app.busy) break;
    }
    state.openPalette();
    const Palette = (await import("./components/Palette.vue")).default;
    return { state, palette: mount(Palette) };
  }

  it("opens on ⌘K and closes on Esc", async () => {
    const { state } = await opened();
    expect(state.app.palette).not.toBeNull();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(state.app.palette).toBeNull();
  });

  it("walks the rows across the groups, and wraps", async () => {
    const { state, palette } = await opened();
    const rows = palette.findAll(".palette-row").length;
    expect(rows).toBeGreaterThan(1);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown" }));
    expect(state.app.palette?.at).toBe(1);

    // Up from the first reaches the last without a second key.
    state.movePalette(-1, rows);
    state.movePalette(-1, rows);
    expect(state.app.palette?.at).toBe(rows - 1);
  });

  it("goes back to the top when the query changes", async () => {
    // The row under the cursor was chosen against a list that no longer exists.
    const { state } = await opened();
    state.movePalette(2, 10);
    state.setPaletteQuery("th");
    expect(state.app.palette?.at).toBe(0);
  });

  it("opens the file it was asked for, on the screen that shows it", async () => {
    const { state } = await opened();
    state.showScreen("history");
    state.setPaletteQuery("theme.rs");
    const groups = search("theme.rs", {
      actions: [],
      repositories: [],
      branches: [],
      files: state.app.status.status === "ready" ? [...state.app.status.value] : [],
    });
    state.runPaletteRow(flatten(groups)[0]!, []);

    expect(state.app.palette).toBeNull();
    expect(state.app.screen).toBe("working-copy");
    expect(state.app.selected?.path).toBe("src/theme.rs");
  });
});

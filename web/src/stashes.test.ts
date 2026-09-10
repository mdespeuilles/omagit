// The shelf: what the list draws, and what its three buttons send.
//
// The bug this file exists to catch is the one the numbering makes easy. `git`
// addresses a stash by its position in a reflog, that position moves whenever
// one is dropped, and a front end holding an index would eventually apply the
// wrong entry. Several of these assert what crossed the wire rather than what
// the screen shows, because that is where the mistake would be.

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

const dirty = () => [{ path: "a.txt", staged: null, unstaged: "modified", hunks: 1 }];

/// A repository with `count` entries already on the shelf, opened on the
/// Stashes screen.
async function shelf(count: number) {
  backend.current = new Repository(dirty());
  vi.resetModules();
  const state = await import("./state");
  await state.boot();
  await state.openRepository("/repo");
  await settled(state);

  for (let made = 0; made < count; made += 1) {
    backend.current.files = dirty();
    state.openStashForm();
    state.setStashMessage(`remise ${made + 1}`);
    state.stashChanges();
    await settled(state);
  }
  backend.current.files = dirty();

  state.showScreen("stashes");
  await settled(state);
  const StashList = (await import("./components/StashList.vue")).default;
  return { state, list: mount(StashList) };
}

/// A shelf that already existed, with the working copy still holding a change.
///
/// The stash-then-look-at-it fixture cannot show what these tests are about:
/// stashing empties the working copy, so nothing is selected on the other
/// screen and the pane has nothing to lose.
async function seeded() {
  backend.current = new Repository(dirty());
  backend.current.stashes = [
    {
      row: {
        index: 0,
        id: { full: "a".repeat(40), short: "aaaaaaa" },
        branch: "main",
        message: "remisé avant",
        when: 1_767_225_600,
        untracked: false,
      },
      held: dirty(),
    },
  ];
  vi.resetModules();
  const state = await import("./state");
  await state.boot();
  await state.openRepository("/repo");
  await settled(state);
  return state;
}

async function settled(state: typeof import("./state")): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    await new Promise((resume) => setTimeout(resume, 0));
    if (!state.app.busy && state.app.stashes.status !== "loading") return;
  }
  throw new Error("la liste ne s'est jamais stabilisée");
}

const sent = (command: string) =>
  backend.current.calls.filter((call) => call.command === command).map((call) => call.args);

beforeEach(() => {
  backend.current = new Repository([]);
});

describe("the shelf", () => {
  it("is read when the screen is first looked at, and not before", async () => {
    // A reflog walk plus an object per entry is not what someone who came to
    // stage a file asked for. Same rule as History.
    backend.current = new Repository(dirty());
    vi.resetModules();
    const state = await import("./state");
    await state.boot();
    await state.openRepository("/repo");
    await settled(state);

    expect(sent("stashes")).toHaveLength(0);

    state.showScreen("stashes");
    await settled(state);
    expect(sent("stashes").length).toBeGreaterThan(0);
  });

  it("draws the newest first, with the address git gives it", async () => {
    const { list } = await shelf(2);

    const rows = list.findAll(".stash-row").map((row) => row.text());
    expect(rows[0]).toContain("stash@{0}");
    expect(rows[0]).toContain("remise 2");
    expect(rows[1]).toContain("stash@{1}");
    expect(rows[1]).toContain("remise 1");
  });

  it("stashes what the form was given, and empties the working copy", async () => {
    backend.current = new Repository(dirty());
    vi.resetModules();
    const state = await import("./state");
    await state.boot();
    await state.openRepository("/repo");
    await settled(state);

    state.showScreen("stashes");
    await settled(state);
    state.openStashForm();
    state.setStashMessage("  la moitié runtime  ");
    state.setStashUntracked(true);
    state.stashChanges();
    await settled(state);

    expect(sent("stash_push")).toEqual([
      { path: "/repo", message: "  la moitié runtime  ", untracked: true },
    ]);
    expect(state.app.stashes.status === "ready" && state.app.stashes.value[0]?.message).toBe(
      "la moitié runtime",
    );
    // The form closes on submit, and the status it emptied is re-read.
    expect(state.app.stashing).toBeNull();
    expect(state.app.status.status === "ready" && state.app.status.value).toEqual([]);
  });

  it("keeps git's own sentence when there was nothing to stash", async () => {
    // The answer worth reading is the one it gives when it did nothing.
    backend.current = new Repository([]);
    vi.resetModules();
    const state = await import("./state");
    await state.boot();
    await state.openRepository("/repo");
    await settled(state);

    state.showScreen("stashes");
    await settled(state);
    state.openStashForm();
    state.stashChanges();
    await settled(state);

    expect(state.app.notes).toContain("No local changes to save");
    expect(state.app.stashes.status === "ready" && state.app.stashes.value).toEqual([]);
  });

  it("addresses a write by commit, never by the index on screen", async () => {
    // The whole point. After a drop the entry that was `stash@{1}` becomes
    // `stash@{0}`, and a command carrying the number the row showed would come
    // back for a different stash.
    const { state, list } = await shelf(2);
    const older = state.app.stashes.status === "ready" ? state.app.stashes.value[1]! : null;

    await list.findAll(".stash-entry")[0]!.findAll(".row-action")[2]!.trigger("click");
    state.answer(true);
    await settled(state);

    const shown = state.app.stashes.status === "ready" ? state.app.stashes.value : [];
    expect(shown).toHaveLength(1);
    expect(shown[0]!.index).toBe(0);
    expect(shown[0]!.id.full).toBe(older!.id.full);

    state.restoreStash(shown[0]!, true);
    await settled(state);
    expect(sent("stash_restore")).toEqual([{ path: "/repo", id: older!.id.full, keep: true }]);
  });

  it("asks before dropping and not before applying", async () => {
    // A confirmation on everything is a confirmation on nothing. Applying is
    // recoverable — the entry stays; dropping leaves what it held reachable
    // only through the reflog.
    const { state } = await shelf(1);
    const row = state.app.stashes.status === "ready" ? state.app.stashes.value[0]! : null;

    state.restoreStash(row!, true);
    expect(state.app.question).toBeNull();
    await settled(state);

    state.dropStash(row!);
    expect(state.app.question?.verb).toBe("Supprimer");
    expect(state.app.question?.detail).toContain("remise 1");
    state.answer(false);
    await settled(state);
    expect(sent("stash_drop")).toHaveLength(0);
  });

  it("puts the files back on the working copy when one is popped", async () => {
    const { state } = await shelf(1);
    const row = state.app.stashes.status === "ready" ? state.app.stashes.value[0]! : null;

    state.restoreStash(row!, false);
    await settled(state);

    expect(sent("stash_restore")).toEqual([{ path: "/repo", id: row!.id.full, keep: false }]);
    expect(state.app.stashes.status === "ready" && state.app.stashes.value).toEqual([]);
    // Nothing is selected any more, because there is nothing to select — and
    // the preview does not go on drawing an entry that has left the shelf.
    expect(state.app.stash).toBeNull();
    expect(state.app.diff.status).toBe("idle");
  });

  it("opens an entry on its first file", async () => {
    const { state } = await shelf(1);

    expect(state.app.stashFiles.status === "ready" && state.app.stashFiles.value).toEqual([
      { path: "a.txt", change: "modified", added: 1, removed: 0, reason: null },
    ]);
    expect(state.app.stashFile).toBe("a.txt");
    expect(state.app.diff.status).toBe("ready");
    expect(sent("stash_file_diff")).toEqual([
      { path: "/repo", id: state.app.stash, file: "a.txt" },
    ]);
  });

  it("says so when the entry it was asked about is gone", async () => {
    // Two windows, or a `git stash drop` in a terminal: the list is a copy, and
    // the backend refuses by name rather than acting on whatever is at that
    // index now.
    const { state } = await shelf(1);
    const row = state.app.stashes.status === "ready" ? state.app.stashes.value[0]! : null;
    backend.current.stashes = [];

    state.restoreStash(row!, true);
    await settled(state);

    expect(state.app.writeError?.said).toContain("not found");
    expect(state.app.stashes.status === "ready" && state.app.stashes.value).toEqual([]);
  });

  it("marks the entries that carry files git never tracked", async () => {
    const { state, list } = await shelf(0);
    state.openStashForm();
    state.setStashUntracked(true);
    state.stashChanges();
    await settled(state);

    expect(list.find(".stash-note").text()).toContain("non suivis");
  });

  it("leaves the diff pane where it is while a write settles", async () => {
    // One diff pane is shared by three screens. A write re-reads everything it
    // could have changed, and the working copy is one of those things — but
    // re-reading the file selected on *another* screen would swap the stash's
    // diff for it under the reader's eyes.
    const state = await seeded();
    state.showScreen("stashes");
    await settled(state);
    expect(state.app.selected?.path).toBe("a.txt");
    const row = state.app.stashes.status === "ready" ? state.app.stashes.value[0]! : null;
    const before = sent("file_diff").length;

    state.restoreStash(row!, true);
    await settled(state);

    expect(sent("file_diff")).toHaveLength(before);
    expect(state.app.stashFile).toBe("a.txt");
    expect(state.app.diff.status).toBe("ready");
  });

  it("takes the diff pane back when a screen opens", async () => {
    const state = await seeded();
    expect(state.app.selected?.path).toBe("a.txt");

    state.showScreen("stashes");
    await settled(state);
    const afterOpening = sent("stash_file_diff").length;
    expect(afterOpening).toBeGreaterThan(0);

    const beforeFile = sent("file_diff").length;
    state.showScreen("working-copy");
    await settled(state);
    expect(sent("file_diff").length).toBeGreaterThan(beforeFile);

    state.showScreen("stashes");
    await settled(state);
    expect(sent("stash_file_diff").length).toBeGreaterThan(afterOpening);
  });

  it("counts files in the sidebar, not sides of the index", async () => {
    // A file staged and then edited again is on both sides at once. Adding the
    // two sides together made the sidebar say 2 over a list of one row, which
    // is two counters on one screen disagreeing about the same thing.
    backend.current = new Repository([
      { path: "half.txt", staged: "modified", unstaged: "modified", hunks: 2 },
      { path: "whole.txt", staged: null, unstaged: "modified", hunks: 1 },
    ]);
    vi.resetModules();
    const state = await import("./state");
    await state.boot();
    await state.openRepository("/repo");
    await settled(state);
    const Sidebar = (await import("./components/Sidebar.vue")).default;

    const sidebar = mount(Sidebar);
    const row = sidebar.findAll(".sidebar-row").find((entry) => entry.text().includes("Working"))!;

    expect(state.changedCount()).toBe(2);
    expect(row.text()).toContain("2");
    // And the two sides are still counted as sides where that is the question.
    expect(state.stagedCount()).toBe(1);
    expect(state.unstagedCount()).toBe(2);
  });

  it("counts the shelf in the sidebar", async () => {
    const { state } = await shelf(2);
    const Sidebar = (await import("./components/Sidebar.vue")).default;

    const sidebar = mount(Sidebar);
    expect(sidebar.text()).toContain("Stashes");
    expect(state.app.summary?.stashes).toBe(2);
    // And the row is a way in, not a label waiting for a milestone.
    expect(
      sidebar
        .findAll(".sidebar-row.deferred")
        .map((row) => row.text())
        .join(" "),
    ).not.toContain("Stashes");
  });
});

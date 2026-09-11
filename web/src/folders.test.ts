// Filing repositories into folders (SPEC §11).
//
// The model has carried a group per repository since M3 and the list has drawn
// them ever since; what did not exist was any way to make one, which is what
// ARCHITECTURE §5 recorded as "seen but not made". These are the sentences that
// were not true before.
//
// What is *not* here is the drag itself: where the insertion line goes is
// `filing.test.ts`, in numbers, because jsdom measures nothing — every rect it
// returns is zero, so a drag driven through the DOM would assert on a list of
// identical boxes.

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

function row(path: string, name: string, group = 0) {
  return {
    group,
    index: 0,
    group_name: "omagit:library.recents",
    path,
    name,
    description: "",
    last_opened: null,
    missing: false,
  };
}

async function opened(dress: (fake: InstanceType<typeof Repository>) => void = () => {}) {
  backend.current = new Repository([]);
  backend.current.library = [row("/src/alpha", "alpha"), row("/src/beta", "beta")];
  dress(backend.current);
  vi.resetModules();
  const state = await import("./state");
  await state.boot();
  await settled(state);
  return state;
}

async function settled(state: typeof import("./state")): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    await new Promise((resume) => setTimeout(resume, 0));
    if (!state.app.busy) return;
  }
  throw new Error("l'écran ne s'est jamais stabilisé");
}

async function list() {
  const RepositoryList = (await import("./components/RepositoryList.vue")).default;
  return mount(RepositoryList);
}

beforeEach(() => {
  backend.current = new Repository([]);
});

describe("making a folder", () => {
  it("draws it even though there is nothing in it, and opens its name", async () => {
    // The defect this replaces: the folders were derived from the rows, so a
    // folder with no rows did not exist. Making one looked like a button that
    // did nothing, and there was no way to put the first repository in.
    const state = await opened();
    await state.createGroup();

    expect(state.app.groups).toHaveLength(2);
    expect(state.app.renamingGroup).toBe(1);

    const drawn = await list();
    expect(drawn.findAll(".library-group")).toHaveLength(2);
    expect(drawn.text()).toContain("Nothing filed here yet");
  });

  it("names it, and refuses a name that is only spaces", async () => {
    const state = await opened();
    await state.createGroup();
    await state.setGroupName(1, "  Clients  ");
    expect(state.app.groups[1]?.name).toBe("Clients");

    // `Esc` and an emptied box do the same thing, which is what makes the box
    // safe to open on a folder that already has a name.
    await state.setGroupName(1, "   ");
    expect(state.app.groups[1]?.name).toBe("Clients");
  });
});

describe("filing a repository", () => {
  it("moves it into the folder the card names, and it stays there", async () => {
    const state = await opened();
    await state.createGroup();
    await state.setGroupName(1, "Clients");
    await state.fileRepository("/src/beta", 1);

    expect(state.app.repositories.find((entry) => entry.path === "/src/beta")?.group).toBe(1);
    expect(state.app.repositories.find((entry) => entry.path === "/src/alpha")?.group).toBe(0);

    // And on the next read, which is the only thing that says it was written.
    await state.readArrangement();
    expect(state.app.repositories.find((entry) => entry.path === "/src/beta")?.group).toBe(1);
  });

  it("draws it under its own folder and not the other", async () => {
    const state = await opened();
    await state.createGroup();
    await state.setGroupName(1, "Clients");
    await state.fileRepository("/src/beta", 1);

    const drawn = await list();
    expect(drawn.findAll(".library-group")[1]?.text()).toContain("Clients");
    expect(drawn.findAll(".library-row").map((one) => one.attributes("data-group"))).toEqual([
      "0",
      "1",
    ]);
  });
});

describe("folding a folder", () => {
  it("hides what is in it, and says so to a screen reader", async () => {
    const state = await opened();
    await state.toggleGroup(0);

    expect(state.app.groups[0]?.collapsed).toBe(true);
    const drawn = await list();
    expect(drawn.findAll(".library-row")).toHaveLength(0);
    expect(drawn.find(".group-fold").attributes("aria-expanded")).toBe("false");
  });

  it("remembers it, because it is an arrangement and not a passing state", async () => {
    const state = await opened();
    await state.toggleGroup(0);
    await state.readArrangement();
    expect(state.app.groups[0]?.collapsed).toBe(true);
  });
});

describe("removing a folder", () => {
  it("asks first, and keeps the repositories", async () => {
    const state = await opened();
    await state.createGroup();
    await state.setGroupName(1, "Clients");
    await state.fileRepository("/src/beta", 1);

    state.removeGroup(1);
    // "Delete" on a folder holding a repository reads as though it takes it
    // with it. The sentence's whole job is to say it does not.
    expect(state.app.question?.detail).toContain("Nothing leaves the list");
    state.answer(true);
    await settled(state);

    expect(state.app.groups).toHaveLength(1);
    expect(state.app.repositories.map((entry) => entry.path).sort()).toEqual([
      "/src/alpha",
      "/src/beta",
    ]);
  });

  it("does not ask about an empty one", async () => {
    const state = await opened();
    await state.createGroup();
    state.removeGroup(1);
    await settled(state);

    expect(state.app.question).toBeNull();
    expect(state.app.groups).toHaveLength(1);
  });

  it("refuses the last one: a repository has to be somewhere", async () => {
    const state = await opened();
    state.removeGroup(0);
    await settled(state);

    expect(state.app.question).toBeNull();
    expect(state.app.groups).toHaveLength(1);
    expect(state.app.repositories).toHaveLength(2);
  });
});

describe("the filter", () => {
  it("leaves out a folder with nothing matching, rather than a row of empties", async () => {
    const state = await opened();
    await state.createGroup();
    await state.setGroupName(1, "Clients");
    state.setLibraryFilter("alpha");
    await settled(state);

    const drawn = await list();
    expect(drawn.findAll(".library-group")).toHaveLength(1);
    expect(drawn.text()).not.toContain("Clients");
  });
});

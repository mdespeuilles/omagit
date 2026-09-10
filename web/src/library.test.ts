// The Repositories screen: what the library says, and what happens to it.
//
// The rule that shapes every case here is DESIGN §4's: a repository that is not
// where it was recorded **keeps its row**. Nothing about a missing folder is an
// error to be cleaned up — an unmounted disk comes back, and a list that tidied
// itself would lose an entry the user arranged.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { Repository } from "./backend.fake";
import type { LibraryRow } from "./ipc";

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("./backend.fake").Repository>,
  picked: null as string | null,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));

// The folder picker is the platform's; what a test controls is what it returns.
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: () => Promise.resolve(backend.picked),
}));

function entry(path: string, over: Partial<LibraryRow> = {}): LibraryRow {
  return {
    group: 0,
    index: 0,
    group_name: "Récents",
    path,
    name: path.split("/").pop() ?? path,
    description: "",
    last_opened: null,
    missing: false,
    ...over,
  };
}

async function open(library: LibraryRow[]) {
  backend.current = new Repository([
    { path: "a.txt", staged: null, unstaged: "modified", hunks: 1 },
  ]);
  backend.current.library = library;
  vi.resetModules();
  const state = await import("./state");
  await state.boot();
  await settled(state);
  return state;
}

async function settled(state: typeof import("./state")): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    await new Promise((resume) => setTimeout(resume, 0));
    const pending = Object.values(state.app.library).some(
      (summary) => summary.status === "loading",
    );
    if (!pending && state.app.status.status !== "loading") return;
  }
  throw new Error("la bibliothèque ne s'est jamais stabilisée");
}

beforeEach(() => {
  backend.current = new Repository([]);
  backend.picked = null;
});

describe("reading the library", () => {
  it("finishes starting up without waiting for a summary per row", async () => {
    // Reading one is a status walk. Waiting for all of them before the window
    // is usable would mean an empty screen for as long as the slowest
    // repository takes, which is the one on a network disk.
    backend.current = new Repository([]);
    backend.current.library = [entry("/one"), entry("/two")];
    let release = (): void => {};
    backend.current.holdSummary = {
      path: "/two",
      until: new Promise((resume) => {
        release = () => resume();
      }),
    };

    vi.resetModules();
    const state = await import("./state");
    await state.boot();

    // Start-up is over — the rows are drawn — while a summary is still in
    // flight.
    expect(state.app.repositories).toHaveLength(2);
    expect(state.app.screen).toBe("repositories");
    expect(state.app.library["/two"]?.status).toBe("loading");

    release();
    await settled(state);
    expect(state.app.library["/two"]?.status).toBe("ready");
  });

  it("keeps a missing repository's row and says why", async () => {
    const state = await open([entry("/gone", { missing: true }), entry("/here")]);

    expect(state.app.repositories.map((row) => row.path)).toEqual(["/gone", "/here"]);
    const gone = state.app.library["/gone"];
    expect(gone?.status).toBe("failed");
    expect(gone?.status === "failed" && gone.error).toContain("introuvable");
  });

  it("opens on the Repositories screen and opens nothing by itself", async () => {
    // SPEC §12 measures cold start "jusqu'à l'écran Repositories", and board 06
    // is drawn with no repository open. Choosing which one to work in is the
    // first thing the product asks; picking one on the user's behalf also means
    // the first thing they see is a status walk they did not ask for.
    const state = await open([entry("/one"), entry("/two")]);

    expect(state.app.screen).toBe("repositories");
    expect(state.app.open).toBeNull();
    expect(state.app.status.status).toBe("idle");
    expect(backend.current.calls.filter((call) => call.command === "status")).toHaveLength(0);
  });

  it("shows the Repositories screen when the library is empty", async () => {
    const state = await open([]);
    expect(state.app.screen).toBe("repositories");
    expect(state.app.card).toBeNull();
  });
});

describe("adding", () => {
  it("adds what the picker returned and selects it", async () => {
    const state = await open([entry("/one")]);
    backend.picked = "/two";

    await state.addRepository();
    await settled(state);

    expect(state.app.repositories.map((row) => row.path)).toEqual(["/one", "/two"]);
    expect(state.app.card).toBe("/two");
    expect(state.app.addError).toBeNull();
  });

  it("does nothing when the picker is dismissed", async () => {
    const state = await open([entry("/one")]);
    backend.picked = null;

    await state.addRepository();
    await settled(state);

    expect(state.app.repositories).toHaveLength(1);
    expect(backend.current.calls.filter((call) => call.command === "add_repository")).toHaveLength(
      0,
    );
  });

  it("refuses a folder that is not a repository, and says so", async () => {
    // Opening it is what answers the question, so a folder that is not one is
    // refused here rather than added and struck through afterwards.
    const state = await open([entry("/one")]);
    backend.current.notARepository = "/not-a-repo";
    backend.picked = "/not-a-repo";

    await state.addRepository();
    await settled(state);

    expect(state.app.repositories).toHaveLength(1);
    expect(state.app.addError).toContain("n'est pas un dépôt");
  });
});

describe("forgetting", () => {
  it("asks first, and says the repository stays on the disk", async () => {
    const state = await open([entry("/one"), entry("/two")]);
    const before = backend.current.calls.length;

    state.forgetRepository(entry("/two"));
    expect(state.app.question?.title).toContain("two");
    expect(state.app.question?.detail).toContain("stays on the disk");
    expect(backend.current.calls).toHaveLength(before);
  });

  it("removes only the entry, once the question is answered", async () => {
    const state = await open([entry("/one"), entry("/two")]);

    state.forgetRepository(entry("/two"));
    state.answer(true);
    await settled(state);

    expect(state.app.repositories.map((row) => row.path)).toEqual(["/one"]);
  });

  it("leaves the list alone when the question is declined", async () => {
    const state = await open([entry("/one"), entry("/two")]);

    state.forgetRepository(entry("/two"));
    state.answer(false);
    await settled(state);

    expect(state.app.repositories).toHaveLength(2);
  });
});

describe("opening", () => {
  it("records when it happened, for the Last Opened line", async () => {
    const state = await open([entry("/one")]);
    await state.openRepository("/one");
    await settled(state);

    expect(
      backend.current.calls.filter((call) => call.command === "touch_repository"),
    ).not.toHaveLength(0);
  });
});

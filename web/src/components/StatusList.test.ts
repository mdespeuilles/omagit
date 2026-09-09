// What the file list actually draws.
//
// Both cases here are bugs that were in the file and would have shipped: a
// checkbox that could not say "half", and a path whose two halves rendered in
// the wrong order because the CSS that elides the front of a path also reverses
// inline boxes.

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
  const StatusList = (await import("./StatusList.vue")).default;
  return { state, list: mount(StatusList) };
}

describe("the file list", () => {
  it("draws the three states of the index, not two", async () => {
    // A file can be wholly in the index, wholly out of it, or half in — which
    // is what staging one hunk leaves behind. Drawing "half in" as unchecked
    // made partial staging look as though it had done nothing.
    const { list } = await drawn([
      { path: "in.txt", staged: "modified", unstaged: null, hunks: 1 },
      { path: "half.txt", staged: "modified", unstaged: "modified", hunks: 2 },
      { path: "out.txt", staged: null, unstaged: "modified", hunks: 1 },
    ]);

    const boxes = list.findAll(".stage-box");
    expect(boxes).toHaveLength(3);
    expect(boxes[0]!.classes()).toContain("all");
    expect(boxes[0]!.text()).toBe("✓");
    expect(boxes[1]!.classes()).toContain("partial");
    expect(boxes[1]!.text()).toBe("–");
    expect(boxes[2]!.classes()).toContain("none");
    expect(boxes[2]!.text()).toBe("");
  });

  it("puts the directory before the filename", async () => {
    const { list } = await drawn([
      {
        path: "crates/omagit-app/src/commands.rs",
        staged: null,
        unstaged: "modified",
        hunks: 1,
      },
    ]);

    const path = list.find(".file-path");
    expect(path.find(".dir").text()).toBe("crates/omagit-app/src/");
    expect(path.find(".name").text()).toBe("commands.rs");
    // The order in the DOM is the order on screen only as long as nothing
    // reverses it, which is what `direction: rtl` on the container did.
    expect(path.text().replace(/\s+/g, "")).toBe("crates/omagit-app/src/commands.rs");
  });

  it("offers no discard on a file with nothing in the working tree", async () => {
    // There would be nothing to reject: the change is in the index, and taking
    // it out of the index is not destructive and is a different button.
    const { list } = await drawn([
      { path: "in.txt", staged: "modified", unstaged: null, hunks: 1 },
    ]);
    expect(list.find(".row-action").attributes("disabled")).toBeDefined();
  });

  it("asks before it discards, and does not write while the question stands", async () => {
    const { state, list } = await drawn([
      { path: "out.txt", staged: null, unstaged: "modified", hunks: 1 },
    ]);
    const before = backend.current.calls.length;

    await list.find(".row-action").trigger("click");

    expect(state.app.question?.title).toContain("out.txt");
    expect(backend.current.calls).toHaveLength(before);
  });

  it("stages the rest of a half-staged file rather than undoing it", async () => {
    const { list } = await drawn([
      { path: "half.txt", staged: "modified", unstaged: "modified", hunks: 2 },
    ]);

    await list.find(".stage-box").trigger("click");

    const write = backend.current.calls.find((call) => call.command === "stage");
    expect(write?.args["unstage"]).toBe(false);
  });
});

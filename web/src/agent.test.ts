// Drafting a commit message with an agent installed on the machine
// (SPEC §11, amended).
//
// What is tested here is the window's half: when the button exists, when it is
// off, and what a refusal does. The part that decides what *leaves* the machine
// is Rust's — `crates/omagit-app/tests/agent.rs` — because that is where the
// staged patch is assembled, and a patch that quietly omits a file produces a
// confident message about a change nobody made.

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

async function opened(dress: (fake: InstanceType<typeof Repository>) => void = () => {}) {
  backend.current = new Repository([
    { path: "a.txt", staged: "modified", unstaged: null, hunks: 1 },
  ]);
  dress(backend.current);
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
    if (!state.app.busy) return;
  }
  throw new Error("l'écran ne s'est jamais stabilisé");
}

async function box() {
  const CommitBox = (await import("./components/CommitBox.vue")).default;
  return mount(CommitBox);
}

/// By its class, not by its words: while an agent is thinking the button says
/// "Writing…", which is the state half of these tests are about.
function draftButton(drawn: Awaited<ReturnType<typeof box>>) {
  const found = drawn.find("button.generate");
  return found.exists() ? found : undefined;
}

beforeEach(() => {
  backend.current = new Repository([]);
});

describe("the button", () => {
  it("is not drawn at all while no agent is configured", async () => {
    // An affordance for a feature that is off is the dead code SPEC §2 forbids,
    // wearing a button — and this one, pressed, could only say "no agent".
    const state = await opened();
    expect(state.hasAgent()).toBe(false);
    expect(draftButton(await box())).toBeUndefined();
  });

  it("appears once one is chosen, without reopening the window", async () => {
    const state = await opened();
    await state.setAgent("claude");

    expect(state.hasAgent()).toBe(true);
    expect(draftButton(await box())).toBeDefined();
  });

  it("is off with nothing staged, and says which of the two reasons it is", async () => {
    const state = await opened((fake) => {
      fake.agent = "claude";
      fake.files = [];
    });
    await settled(state);

    const button = draftButton(await box())!;
    expect(button.attributes("disabled")).toBeDefined();
    expect(button.attributes("title")).toContain("Nothing is staged");
  });

  it("warns that a message already typed will be replaced", async () => {
    // Work, and a button that silently replaces it is the loss rule 7 is about
    // arriving through a control nobody thinks of as destructive. Not blocked —
    // asking again after an edit is exactly what somebody does — but said.
    const state = await opened((fake) => {
      fake.agent = "claude";
    });
    expect(draftButton(await box())!.attributes("title")).toContain("Ask claude");

    state.setMessage("Un début écrit à la main");
    expect(draftButton(await box())!.attributes("title")).toContain("replaces what is in the box");
  });
});

describe("drafting", () => {
  it("puts what the agent said in the box", async () => {
    const state = await opened((fake) => {
      fake.agent = "claude";
      fake.drafted = "Un sujet\n\nUn corps.";
    });
    await state.draftMessage();

    expect(state.app.message).toBe("Un sujet\n\nUn corps.");
    expect(state.app.busy).toBeNull();
  });

  it("leaves the lines picked in the diff alone", async () => {
    // It goes through its own wrapper and not `write`, which calls `settle` and
    // clears them. A draft is not a write: it has no business undoing a
    // selection somebody made by hand.
    const state = await opened((fake) => {
      fake.agent = "claude";
    });
    state.pickLine(0, 0, false);
    await state.draftMessage();

    expect(state.app.picked.size).toBe(1);
  });

  it("says on the button itself that it is working", async () => {
    // It was only in the status bar — true, and at the far bottom of the window
    // while the eye is on the commit box, which read as nothing happening at
    // all for the twenty-five seconds an agent takes.
    let release = (): void => {};
    const state = await opened((fake) => {
      fake.agent = "claude";
      fake.holdDraft = new Promise((resume) => {
        release = () => resume();
      });
    });

    const drafting = state.draftMessage();
    const working = draftButton(await box())!;
    expect(working.text()).toContain("Writing");
    expect(working.classes()).toContain("working");
    expect(working.attributes("disabled")).toBeDefined();

    release();
    await drafting;

    const done = draftButton(await box())!;
    expect(done.text()).toContain("Generate");
    expect(done.classes()).not.toContain("working");
  });

  it("says what went wrong rather than leaving an empty box", async () => {
    const state = await opened((fake) => {
      fake.agent = "claude";
      fake.failDraft = "claude: command not found";
    });
    await state.draftMessage();

    expect(state.app.message).toBe("");
    expect(state.app.writeError?.said).toContain("command not found");
    expect(state.app.busy).toBeNull();
  });
});

describe("the Preferences block", () => {
  it("offers an agent that is not installed, disabled, with the reason", async () => {
    // The same rule as the theme sources: "Codex is not here" is an answer, and
    // an option that vanishes is one nobody can ask about.
    const state = await opened();
    state.showScreen("settings");
    await settled(state);
    await state.readAgents();

    const Settings = (await import("./components/Settings.vue")).default;
    const drawn = mount(Settings);
    const options = drawn.findAll("option");
    const codex = options.find((one) => one.text().includes("Codex"))!;

    expect(codex.attributes("disabled")).toBeDefined();
    expect(codex.text()).toContain("not installed");
    expect(
      options.find((one) => one.text().includes("Claude Code"))!.attributes("disabled"),
    ).toBeUndefined();
  });

  it("keeps a command this version does not know, as a command", async () => {
    // What a settings file written by a later release looks like. Refusing it
    // would turn "omagit knows one more agent than you do" into a feature that
    // is off with no way to find out why.
    const state = await opened();
    await state.setAgent("aider");
    await state.readAgents();

    expect(state.app.agents?.command).toBe("aider");
    expect(state.hasAgent()).toBe(true);
  });
});

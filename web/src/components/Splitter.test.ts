// Dragging an edge.
//
// The part worth testing is the arithmetic: `#app` is zoomed by `--scale`, so
// the pointer's coordinates are in scaled pixels while the width the stylesheet
// wants is unscaled. Without dividing, dragging at 1.15 moves the edge fifteen
// per cent further than the pointer, which reads as the column running away.

import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Repository } from "../backend.fake";

const backend = vi.hoisted(() => ({
  current: null as unknown as InstanceType<typeof import("../backend.fake").Repository>,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) =>
    backend.current.call(command, args ?? {}),
}));

/// jsdom has no pointer capture.
class Capturing extends HTMLElement {}
void Capturing;

async function edge(props: Record<string, unknown>, scale = 1) {
  backend.current = new Repository([]);
  vi.resetModules();
  const state = await import("../state");
  document.documentElement.setAttribute("style", `--scale: ${scale};`);
  const Splitter = (await import("./Splitter.vue")).default;
  const splitter = mount(Splitter, { props: { pane: "history", size: 520, ...props } });
  splitter.element.setPointerCapture = () => {};
  return { state, splitter };
}

function drag(element: Element, from: number, to: number): void {
  element.dispatchEvent(
    new MouseEvent("pointerdown", { clientX: from, bubbles: true, cancelable: true }),
  );
  window.dispatchEvent(new MouseEvent("pointermove", { clientX: to, bubbles: true }));
  window.dispatchEvent(new MouseEvent("pointerup", { clientX: to, bubbles: true }));
}

beforeEach(() => {
  backend.current = new Repository([]);
});

describe("a splitter", () => {
  it("drags down as well as across", async () => {
    // The same arithmetic on the other axis, and the reason there is one
    // component: the scale, settling once at the end, and the keyboard are the
    // same in both directions.
    const { state, splitter } = await edge({ sizes: "height", size: 200, min: 48 });
    splitter.element.dispatchEvent(
      new MouseEvent("pointerdown", { clientY: 300, bubbles: true, cancelable: true }),
    );
    window.dispatchEvent(new MouseEvent("pointermove", { clientY: 380, bubbles: true }));
    window.dispatchEvent(new MouseEvent("pointerup", { clientY: 380, bubbles: true }));

    expect(state.paneWidth("history", 200)).toBe(280);
  });

  it("is one element, so the pointer reaches it", async () => {
    // A comment above the root in the template makes the component a fragment,
    // Vue keeps the comment as a node in development, and `mount(…).element`
    // then points at *it* rather than the `<div>`. Every test here went silent
    // when that happened — no handler ever fired.
    const { splitter } = await edge({});
    expect(splitter.element.nodeType).toBe(Node.ELEMENT_NODE);
    expect((splitter.element as HTMLElement).classList.contains("splitter")).toBe(true);
  });

  it("moves the edge with the pointer", async () => {
    const { state, splitter } = await edge({});
    drag(splitter.element, 100, 180);
    expect(state.paneWidth("history", 520)).toBe(600);
  });

  it("divides the travel by the scale the window is zoomed to", async () => {
    // 80 pointer pixels at 1.25 is 64 of the pixels the stylesheet counts in.
    const { state, splitter } = await edge({}, 1.25);
    drag(splitter.element, 100, 180);
    expect(state.paneWidth("history", 520)).toBe(584);
  });

  it("grows a leading edge as the pointer moves towards it", async () => {
    // `leading` rather than `left`: the same prop now serves an edge along the
    // bottom of a row, where "left" would mean nothing.
    const { state, splitter } = await edge({ side: "leading" });
    drag(splitter.element, 200, 120);
    expect(state.paneWidth("history", 520)).toBe(600);
  });

  it("holds the column to its floor", async () => {
    const { state, splitter } = await edge({ min: 360 });
    drag(splitter.element, 400, 0);
    expect(state.paneWidth("history", 520)).toBe(360);
  });

  it("writes the width once, when the drag ends", async () => {
    const { splitter } = await edge({});
    drag(splitter.element, 100, 180);
    await new Promise((resume) => setTimeout(resume, 0));

    const written = backend.current.calls.filter((call) => call.command === "set_pane");
    expect(written).toHaveLength(1);
    // The width the drag produced, not the prop the parent has not re-rendered
    // yet: settling on the prop works only while something else keeps it in
    // step, and is one refactor away from writing the width it started at.
    expect(written[0]!.args).toMatchObject({ width: 600 });
  });
});

import { describe, expect, it } from "vitest";
import { fold, type Dot } from "./runs";

const x = (time: number) => time; // times are already percentages here
const dots = (...times: number[]): Dot[] => times.map((time) => ({ time, merge: false }));
const options = { gap: 2, min: 3, fold: true, open: new Set<string>(), line: "main" };

describe("folding runs of commits", () => {
  it("folds a burst of three or more close commits into one run", () => {
    const pieces = fold(dots(10, 11, 12, 13, 50), x, 0, options);
    expect(pieces).toEqual([
      { kind: "run", key: "main:10", from: 10, to: 13, count: 4 },
      { kind: "dot", x: 50, merge: false, time: 50 },
    ]);
  });

  it("leaves two close commits as dots: a pill saying 2 hides nothing", () => {
    const pieces = fold(dots(10, 11), x, 0, options);
    expect(pieces.map((piece) => piece.kind)).toEqual(["dot", "dot"]);
  });

  it("draws every dot when folding is off, or when that run was opened", () => {
    expect(fold(dots(10, 11, 12), x, 0, { ...options, fold: false })).toHaveLength(3);
    expect(fold(dots(10, 11, 12), x, 0, { ...options, open: new Set(["main:10"]) })).toHaveLength(
      3,
    );
  });

  it("leaves off what is before the range, and keeps a merge a merge", () => {
    const pieces = fold(
      [
        { time: 5, merge: false },
        { time: 40, merge: true },
      ],
      x,
      20,
      options,
    );
    expect(pieces).toEqual([{ kind: "dot", x: 40, merge: true, time: 40 }]);
  });
});

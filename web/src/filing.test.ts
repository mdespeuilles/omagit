// The insertion line, in numbers.

import { describe, expect, it } from "vitest";
import { nearest, targets, type Measured } from "./filing";

/// Two folders: "Recent" with two rows, "Work" with one, and "Clients" with
/// none. Rows are 32px, headers 24px, laid out from the top.
const list: Measured[] = [
  {
    group: 0,
    head: 24,
    rows: [
      { top: 24, bottom: 56 },
      { top: 56, bottom: 88 },
    ],
  },
  { group: 1, head: 112, rows: [{ top: 112, bottom: 144 }] },
  { group: 2, head: 168, rows: [] },
];

describe("where a dragged repository can land", () => {
  it("offers a place between every two rows, and one at each end", () => {
    expect(targets(list).filter((place) => place.group === 0)).toEqual([
      { group: 0, index: 0, y: 24 },
      { group: 0, index: 1, y: 56 },
      { group: 0, index: 2, y: 88 },
    ]);
  });

  it("offers one under an empty folder's header", () => {
    // The folder somebody just made. Built from rows alone it would have none,
    // and nothing could ever be filed into it.
    expect(targets(list).filter((place) => place.group === 2)).toEqual([
      { group: 2, index: 0, y: 168 },
    ]);
  });

  it("takes the nearest line, including in a header's own band", () => {
    const places = targets(list);
    // Just inside the first row: above its middle.
    expect(nearest(places, 30)).toEqual({ group: 0, index: 0, y: 24 });
    // Just below it: the line between the two.
    expect(nearest(places, 50)).toEqual({ group: 0, index: 1, y: 56 });
    // In the "Work" header, which is no row at all — 88 and 112 are the two
    // candidates and 104 is nearer the second.
    expect(nearest(places, 104)).toEqual({ group: 1, index: 0, y: 112 });
    expect(nearest(places, 96)).toEqual({ group: 0, index: 2, y: 88 });
  });

  it("answers above and below the whole list", () => {
    const places = targets(list);
    expect(nearest(places, -400)).toEqual({ group: 0, index: 0, y: 24 });
    expect(nearest(places, 4000)).toEqual({ group: 2, index: 0, y: 168 });
  });

  it("has nothing to say about an empty list", () => {
    expect(targets([])).toEqual([]);
    expect(nearest([], 10)).toBeNull();
  });
});

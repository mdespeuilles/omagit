// Where an insertion line goes while a repository is being dragged between the
// folders of the Repositories list.
//
// Separate from the component, and pure, because this is the part that can be
// wrong in ways a screenshot does not show: a line drawn one slot off files the
// repository somewhere the user did not point at. jsdom measures nothing — every
// rect it returns is zero — so a test that went through the DOM would assert on
// zeroes. Here the input is numbers.
//
// Pointer events rather than HTML5 drag and drop, and not by preference: the
// window sets `dragDropEnabled` so that a folder dropped from the Finder
// arrives as a *path* rather than as file contents, and the same switch turns
// off drag and drop inside the page on macOS and Windows (ARCHITECTURE §2.53
// left the note that whoever built groups would have to reconcile the two).
// The reconciliation is this file.

/// One folder as it is drawn: where its header is, and where its rows are.
/// Only the vertical edges matter — the list is one column.
export type Measured = {
  group: number;
  /// The bottom edge of the header, which is where a line at the top of an
  /// empty folder goes.
  head: number;
  /// Top and bottom of each row, in order.
  rows: { top: number; bottom: number }[];
};

/// A place a repository can land: a folder, a position in it, and the height
/// the line is drawn at.
export type Target = { group: number; index: number; y: number };

/// Every place the dragged repository could go, top to bottom.
///
/// A folder with nothing in it gets one, under its header — that is the whole
/// point of a folder that was just made, and a list of targets built from rows
/// alone would leave it impossible to file anything into.
export function targets(measured: Measured[]): Target[] {
  const places: Target[] = [];
  for (const folder of measured) {
    if (folder.rows.length === 0) {
      places.push({ group: folder.group, index: 0, y: folder.head });
      continue;
    }
    folder.rows.forEach((row, index) => {
      places.push({ group: folder.group, index, y: row.top });
    });
    const last = folder.rows[folder.rows.length - 1]!;
    places.push({ group: folder.group, index: folder.rows.length, y: last.bottom });
  }
  return places;
}

/// The one the pointer is closest to.
///
/// Nearest rather than "the row under the pointer, above or below its middle":
/// the two agree everywhere except at the seam between two folders, where a
/// pointer in the header's own band belongs to neither row. Nearest answers
/// there too, and answers past the ends of the list without a special case.
export function nearest(places: Target[], y: number): Target | null {
  let best: Target | null = null;
  let distance = Number.POSITIVE_INFINITY;
  for (const place of places) {
    const how = Math.abs(place.y - y);
    // Strictly closer, so a tie goes to the one higher up — the order they are
    // built in, which is the order they are drawn.
    if (how < distance) {
      distance = how;
      best = place;
    }
  }
  return best;
}

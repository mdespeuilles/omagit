// The branch map's folded runs (DESIGN v2, board "Carte"): a burst of commits
// drawn as one "n commits" pill rather than a smear of overlapping dots.
//
// Pure, so the grouping is tested without a layout: positions come in as
// percentages of the track, and pieces come out to be drawn.

export type Dot = { time: number; merge: boolean };

export type Piece =
  | { kind: "dot"; x: number; merge: boolean; time: number }
  | { kind: "run"; key: string; from: number; to: number; count: number };

export type Folding = {
  /// Two dots closer than this, in percent of the track, are one burst.
  gap: number;
  /// The fewest commits a burst needs to be folded; fewer are drawn as dots.
  min: number;
  /// Off draws every dot, which is what the board's checkbox turns off.
  fold: boolean;
  /// The runs that were clicked open, by key.
  open: ReadonlySet<string>;
  /// What a key starts with, so two lines' runs never share one.
  line: string;
};

/// Dots placed on the track and grouped. `x` turns a time into a position;
/// dots outside the range — before the left edge — are left off.
export function fold(
  dots: readonly Dot[],
  x: (time: number) => number,
  start: number,
  options: Folding,
): Piece[] {
  const placed = dots
    .filter((dot) => dot.time >= start)
    .map((dot) => ({ ...dot, x: x(dot.time) }))
    .sort((a, b) => a.x - b.x || a.time - b.time);

  const groups: (typeof placed)[] = [];
  for (const dot of placed) {
    const last = groups[groups.length - 1];
    const previous = last?.[last.length - 1];
    if (last && previous && dot.x - previous.x < options.gap) last.push(dot);
    else groups.push([dot]);
  }

  const pieces: Piece[] = [];
  for (const group of groups) {
    // Keyed by its oldest commit's time, which does not move when the range
    // changes — a run opened at three months stays open at six.
    const key = `${options.line}:${group[0]!.time}`;
    if (options.fold && group.length >= options.min && !options.open.has(key)) {
      pieces.push({
        kind: "run",
        key,
        from: group[0]!.x,
        to: group[group.length - 1]!.x,
        count: group.length,
      });
    } else {
      for (const dot of group)
        pieces.push({ kind: "dot", x: dot.x, merge: dot.merge, time: dot.time });
    }
  }
  return pieces;
}

// The density's numbers, for the code that needs them as numbers.
//
// A virtualised list places its rows itself, so it needs the row height as an
// integer rather than as a `var()` — which is exactly why the three lists had
// hard-coded 18, 24 and 24, and why changing the density moved everything on
// screen except them.
//
// Read once at start-up, after the theme is applied. Density is a setting, not
// something that changes while the window is open (M9 will make it changeable,
// and will re-read).

let cache: Record<string, number> = {};

/// Read the metric tokens off the root element.
export function measure(): void {
  const style = getComputedStyle(document.documentElement);
  cache = {};
  for (const name of [
    "row-height",
    "row-padding",
    "line-height",
    "control-height",
    "header-height",
  ]) {
    cache[name] = Number.parseFloat(style.getPropertyValue(`--${name}`)) || 0;
  }
}

/// One row of a list: a file, a repository, a commit.
export function rowHeight(): number {
  return cache["row-height"] || 26;
}

/// One line of a diff. Board 03: 17px compact, 20px comfortable.
export function lineHeight(): number {
  return cache["line-height"] || 17;
}

/// A row holding two lines of text: a commit, in the History list.
///
/// Derived rather than named, because it is two of something that already has
/// a token plus the padding a row already has. A literal here would be a third
/// number to keep in step with the density by hand.
export function doubleRowHeight(): number {
  return 2 * lineHeight() + (cache["row-padding"] || 6);
}

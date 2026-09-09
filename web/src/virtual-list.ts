// A virtualised list, by hand.
//
// SPEC §12 asks for a hundred thousand commits and constant 120fps, which rules
// out a row per commit in the DOM. It also, deliberately, rules out reaching
// for a framework: the technique is DOM recycling, most frameworks get in the
// way of it, and the argument that moved this project off GPUI — a dependency
// should exist to serve you — applies to a front-end framework too. This is
// ninety lines; it earns its place until something needs more.
//
// Rows are absolutely positioned inside a spacer of the full height, so the
// browser scrolls natively and only the visible window exists as elements.

export type Renderer<T> = (element: HTMLElement, item: T, index: number) => void;

export type VirtualListOptions = {
  rowHeight: number;
  /** Rows drawn beyond each edge, so a fast scroll does not show gaps. */
  overscan?: number;
  /** Called when the visible window nears the end — where the next page is asked for. */
  onNearEnd?: () => void;
};

export class VirtualList<T> {
  readonly element: HTMLElement;
  private readonly viewport: HTMLElement;
  private readonly spacer: HTMLElement;
  private readonly pool: HTMLElement[] = [];
  private readonly rowHeight: number;
  private readonly overscan: number;
  private readonly onNearEnd: (() => void) | undefined;
  private readonly render: Renderer<T>;
  private items: readonly T[] = [];
  private first = -1;
  private count = 0;

  constructor(render: Renderer<T>, options: VirtualListOptions) {
    this.render = render;
    this.rowHeight = options.rowHeight;
    this.overscan = options.overscan ?? 8;
    this.onNearEnd = options.onNearEnd;

    this.spacer = document.createElement("div");
    this.spacer.className = "vlist-spacer";

    this.viewport = document.createElement("div");
    this.viewport.className = "vlist";
    this.viewport.appendChild(this.spacer);
    // Passive: this listener never calls `preventDefault`, and saying so lets
    // the compositor scroll without waiting for it.
    this.viewport.addEventListener("scroll", () => this.draw(), { passive: true });

    this.element = this.viewport;
  }

  setItems(items: readonly T[]): void {
    this.items = items;
    this.spacer.style.height = `${items.length * this.rowHeight}px`;
    // The window may now hold different items at the same indices.
    this.first = -1;
    this.draw();
  }

  /** Redraw the current window without changing what is in it. */
  refresh(): void {
    this.first = -1;
    this.draw();
  }

  scrollToIndex(index: number, position: "start" | "centre" = "centre"): void {
    const top =
      position === "start"
        ? index * this.rowHeight
        : index * this.rowHeight - this.viewport.clientHeight / 2 + this.rowHeight / 2;
    this.viewport.scrollTop = Math.max(0, top);
  }

  private draw(): void {
    const total = this.items.length;
    const height = this.viewport.clientHeight || 1;
    const visible = Math.ceil(height / this.rowHeight);
    const first = Math.max(0, Math.floor(this.viewport.scrollTop / this.rowHeight) - this.overscan);
    const count = Math.min(total - first, visible + this.overscan * 2);

    // Nothing moved: the cheapest frame is the one that does nothing.
    if (first === this.first && count === this.count) return;
    this.first = first;
    this.count = count;

    this.grow(count);
    for (let slot = 0; slot < this.pool.length; slot += 1) {
      const element = this.pool[slot]!;
      if (slot >= count) {
        element.hidden = true;
        continue;
      }
      const index = first + slot;
      const item = this.items[index];
      if (item === undefined) {
        element.hidden = true;
        continue;
      }
      element.hidden = false;
      element.style.transform = `translateY(${index * this.rowHeight}px)`;
      this.render(element, item, index);
    }

    if (this.onNearEnd && total > 0 && first + count >= total - this.overscan) {
      this.onNearEnd();
    }
  }

  /** Grow the pool to `needed`, never shrinking it: a scroll back up reuses it. */
  private grow(needed: number): void {
    while (this.pool.length < needed) {
      const element = document.createElement("div");
      element.className = "vlist-row";
      element.style.height = `${this.rowHeight}px`;
      this.spacer.appendChild(element);
      this.pool.push(element);
    }
  }
}

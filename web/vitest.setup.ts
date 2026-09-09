// jsdom has no layout, so it has no `ResizeObserver` either. The virtual list
// asks for one to learn its viewport's height — a stub that never fires is the
// right answer here: the list falls back to its minimum window, which is more
// rows than any test needs.
class NoLayout {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

globalThis.ResizeObserver ??= NoLayout as unknown as typeof ResizeObserver;

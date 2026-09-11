<script setup lang="ts">
// A draggable edge between two panes, across or down.
//
// It sits *on* the border rather than beside it: a separator that took room of
// its own would push everything along by a pixel and put a gap in the hairline
// grid the whole interface is drawn on. So it is absolutely positioned over the
// border, five pixels thick, and the pane keeps its 1px edge.
//
// The size it produces is unscaled CSS pixels, the same units the stylesheet
// and the tokens use. The pointer's coordinates are not — `#app` is zoomed —
// so a delta in client pixels is divided by the scale before it becomes a
// size. Without that, dragging at 1.15 moves the edge 15% further than the
// pointer, which reads as the pane running away.
//
// One component for both axes rather than two. Everything that is hard here —
// the scale, settling once at the end instead of a hundred times, the keyboard
// — is the same in both directions, and the only difference is which
// coordinate is read and which property is written.

import { computed, onBeforeUnmount, ref } from "vue";
import { resizePane, settlePane } from "../state";
import { t } from "../i18n";

const props = defineProps<{
  /// Which pane this edge belongs to, as the settings file names it.
  pane: string;
  /// Its current size, in unscaled pixels — a width across, a height down.
  size: number;
  /// Which dimension this edge sets. `width` is a column's right edge, dragged
  /// left and right; `height` is a row's bottom edge, dragged up and down.
  sizes?: "width" | "height";
  /// Which side of the pane the edge is on. A leading edge grows the pane as
  /// the pointer moves towards it.
  side?: "leading" | "trailing";
  min?: number;
  max?: number;
}>();

const down = computed(() => props.sizes === "height");

/// What `aria-orientation` names is the separator's own line, not the direction
/// it travels: an edge you drag left and right *is* a vertical line.
///
/// Written here and not as a comment above the root element, which is not a
/// detail: a comment in the template makes the component a fragment, Vue keeps
/// it as a node in development, and `mount(…).element` then points at the
/// comment instead of the `<div>`. Every one of this component's tests went
/// silent — the handler simply never fired.
const orientation = computed(() => (down.value ? "horizontal" : "vertical"));

const dragging = ref(false);
/// Where the pointer was when it went down, on the axis that matters.
let from = 0;
let startSize = 0;
/// The last size this drag produced.
///
/// Not `props.size`: the prop is the parent's copy, and settling on it means
/// settling on whatever has made it back down through a render. That happens to
/// work while the parent is reactive, and it is one refactor away from writing
/// the size the drag started at.
let current = 0;

function scale(): number {
  const value = getComputedStyle(document.documentElement).getPropertyValue("--scale");
  const parsed = Number.parseFloat(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : 1;
}

function along(event: PointerEvent): number {
  return down.value ? event.clientY : event.clientX;
}

function press(event: PointerEvent): void {
  dragging.value = true;
  from = along(event);
  startSize = props.size;
  current = props.size;
  (event.target as HTMLElement).setPointerCapture(event.pointerId);
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up, { once: true });
}

function move(event: PointerEvent): void {
  if (!dragging.value) return;
  const travelled = (along(event) - from) / scale();
  const wanted = startSize + (props.side === "leading" ? -travelled : travelled);
  current = clamp(wanted);
  resizePane(props.pane, current);
}

function up(): void {
  dragging.value = false;
  window.removeEventListener("pointermove", move);
  // Written once, at the end: the settings file is rewritten on every call and
  // a drag is a hundred of them.
  settlePane(props.pane, current);
}

function clamp(size: number): number {
  return Math.round(Math.min(props.max ?? 900, Math.max(props.min ?? 180, size)));
}

/// The keyboard's version of the same thing, because a drag is not reachable
/// without a pointer.
function nudge(by: number): void {
  current = clamp(props.size + by);
  resizePane(props.pane, current);
  settlePane(props.pane, current);
}

onBeforeUnmount(() => window.removeEventListener("pointermove", move));
</script>

<template>
  <div
    class="splitter"
    :class="{ dragging, down }"
    role="separator"
    tabindex="0"
    :aria-orientation="orientation"
    :aria-label="down ? t('panes.height', { pane }) : t('panes.width', { pane })"
    :aria-valuenow="Math.round(size)"
    @pointerdown.prevent="press"
    @dblclick="nudge(0)"
    @keydown.up.prevent="down && nudge(side === 'leading' ? 8 : -8)"
    @keydown.down.prevent="down && nudge(side === 'leading' ? -8 : 8)"
    @keydown.left.prevent="!down && nudge(side === 'leading' ? 8 : -8)"
    @keydown.right.prevent="!down && nudge(side === 'leading' ? -8 : 8)"
  />
</template>

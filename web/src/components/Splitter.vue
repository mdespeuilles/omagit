<script setup lang="ts">
// A draggable edge between two columns.
//
// It sits *on* the border rather than beside it: a separator that took width of
// its own would push everything along by a pixel and put a gap in the hairline
// grid the whole interface is drawn on. So it is absolutely positioned over the
// border, four pixels wide, and the column keeps its 1px edge.
//
// The width it produces is unscaled CSS pixels, the same units the stylesheet
// and the tokens use. The pointer's coordinates are not — `#app` is zoomed —
// so a delta in client pixels is divided by the scale before it becomes a
// width. Without that, dragging at 1.15 moves the edge 15% further than the
// pointer, which reads as the column running away.

import { onBeforeUnmount, ref } from "vue";
import { resizePane, settlePane } from "../state";
import { t } from "../i18n";

const props = defineProps<{
  /// Which column this edge belongs to, as the settings file names it.
  pane: string;
  /// Its current width, in unscaled pixels.
  width: number;
  /// Which side of the column the edge is on. A left-hand edge grows the
  /// column as the pointer moves left.
  side?: "left" | "right";
  min?: number;
  max?: number;
}>();

const dragging = ref(false);
let startX = 0;
let startWidth = 0;
/// The last width this drag produced.
///
/// Not `props.width`: the prop is the parent's copy, and settling on it means
/// settling on whatever has made it back down through a render. That happens to
/// work while the parent is reactive, and it is one refactor away from writing
/// the width the drag started at.
let current = 0;

function scale(): number {
  const value = getComputedStyle(document.documentElement).getPropertyValue("--scale");
  const parsed = Number.parseFloat(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : 1;
}

function down(event: PointerEvent): void {
  dragging.value = true;
  startX = event.clientX;
  startWidth = props.width;
  current = props.width;
  (event.target as HTMLElement).setPointerCapture(event.pointerId);
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up, { once: true });
}

function move(event: PointerEvent): void {
  if (!dragging.value) return;
  const travelled = (event.clientX - startX) / scale();
  const wanted = startWidth + (props.side === "left" ? -travelled : travelled);
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

function clamp(width: number): number {
  return Math.round(Math.min(props.max ?? 900, Math.max(props.min ?? 180, width)));
}

/// The keyboard's version of the same thing, because a drag is not reachable
/// without a pointer.
function nudge(by: number): void {
  current = clamp(props.width + by);
  resizePane(props.pane, current);
  settlePane(props.pane, current);
}

onBeforeUnmount(() => window.removeEventListener("pointermove", move));
</script>

<template>
  <div
    class="splitter"
    :class="{ dragging }"
    role="separator"
    tabindex="0"
    aria-orientation="vertical"
    :aria-label="t('panes.width', { pane })"
    :aria-valuenow="Math.round(width)"
    @pointerdown.prevent="down"
    @dblclick="nudge(0)"
    @keydown.left.prevent="nudge(side === 'left' ? 8 : -8)"
    @keydown.right.prevent="nudge(side === 'left' ? -8 : 8)"
  />
</template>

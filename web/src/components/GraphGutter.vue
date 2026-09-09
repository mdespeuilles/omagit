<script setup lang="ts">
// The commit graph, one row at a time.
//
// SPEC §15 calls the topology the hardest algorithm in the project and asks
// that it never be coupled to rendering. It is not: `omagit_git::graph` hands
// over lane indices and links, and this file turns those into an SVG. It knows
// nothing about history, parents or merges — only about lanes, and which of
// them arrive, pass through or leave.
//
// One `<svg>` per row rather than one for the whole list, because the list is
// virtualised: a single canvas would have to be redrawn on every scroll and
// would have to know about rows that do not exist as elements.

import { computed } from "vue";

/// Exactly what a gutter reads, and nothing else.
///
/// Not `HistoryRow`: this component has no business knowing a commit has a
/// summary or an author, and SPEC §15 asks that the topology never be coupled
/// to rendering — stating the coupling this narrowly is what that looks like in
/// a type. It is also what lets the deeply-readonly application state be passed
/// in without a cast.
type Topology = {
  lane: number;
  passing: readonly number[];
  incoming: readonly number[];
  outgoing: readonly number[];
  width: number;
  merge: boolean;
};

const props = defineProps<{ row: Topology; height: number }>();

/// Distance between two lanes, and the left margin before the first.
const LANE = 12;
const EDGE = 8;
/// Board 05's node: a filled disc, hollow for a merge.
const RADIUS = 3.5;

const x = (lane: number): number => EDGE + lane * LANE;
const mid = computed(() => props.height / 2);

/// The gutter is as wide as the row needs and no wider. `width` is how many
/// lanes are open across this row, which `omagit_git::graph` computes as it
/// goes so no second pass is needed to size the column.
const width = computed(() => EDGE * 2 + Math.max(props.row.width, 1) * LANE);

/// A lane's colour. The lane palette never reads the theme — only its lightness
/// and chroma do (DESIGN-TOKENS §6) — and wraps at eight.
const colour = (lane: number): string => `var(--lane-${lane % 8})`;

/// A line from `(from, top)` down to `(to, mid)`, bent rather than diagonal.
///
/// The vertical part runs the full height of its own lane before the bend, so a
/// line that merely passes through a busy row still reads as one column. A
/// straight diagonal across four lanes is what makes a dense graph unreadable.
function arriving(from: number): string {
  const start = x(from);
  const end = x(props.row.lane);
  if (start === end) return `M ${start} 0 L ${end} ${mid.value}`;
  const bend = mid.value / 2;
  return `M ${start} 0 L ${start} ${bend} C ${start} ${mid.value} ${end} ${bend} ${end} ${mid.value}`;
}

function leaving(to: number): string {
  const start = x(props.row.lane);
  const end = x(to);
  const bottom = props.height;
  if (start === end) return `M ${start} ${mid.value} L ${end} ${bottom}`;
  const bend = mid.value + (bottom - mid.value) / 2;
  return `M ${start} ${mid.value} C ${start} ${bend} ${end} ${mid.value + (bottom - mid.value) / 4} ${end} ${bend} L ${end} ${bottom}`;
}
</script>

<template>
  <svg
    class="gutter-graph"
    :width="width"
    :height="height"
    :viewBox="`0 0 ${width} ${height}`"
    aria-hidden="true"
  >
    <!-- Lanes that cross without touching the node: one straight line, full
         height, so a branch stays a column while other things happen. -->
    <line
      v-for="lane in row.passing"
      :key="`p${lane}`"
      :x1="x(lane)"
      :y1="0"
      :x2="x(lane)"
      :y2="height"
      :stroke="colour(lane)"
    />
    <path
      v-for="lane in row.incoming"
      :key="`i${lane}`"
      :d="arriving(lane)"
      :stroke="colour(lane)"
      fill="none"
    />
    <path
      v-for="lane in row.outgoing"
      :key="`o${lane}`"
      :d="leaving(lane)"
      :stroke="colour(lane)"
      fill="none"
    />
    <!-- Hollow for a merge: two parents is the one thing about a node that a
         reader has to see without following a line. -->
    <circle
      class="gutter-node"
      :class="{ merge: row.merge }"
      :cx="x(row.lane)"
      :cy="mid"
      :r="RADIUS"
      :stroke="colour(row.lane)"
      :fill="row.merge ? 'var(--bg)' : colour(row.lane)"
    />
  </svg>
</template>

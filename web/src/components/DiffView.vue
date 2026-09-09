<script setup lang="ts">
// The diff, virtualised. The piece the stack change was decided without a
// measurement of, so it is the one that had to be built first.

import { app } from "../state";
import VirtualList from "./VirtualList.vue";

/// Board 03's interline: 17px compact, 20px comfortable. Fixed rows are what
/// lets the list place them without measuring any of them.
const ROW_HEIGHT = 18;

// Takes the side rather than the row: `readonly()` makes the state deeply
// immutable, and a parameter typed as the mutable row would not accept one.
function sign(side: "context" | "added" | "removed"): string {
  return side === "added" ? "+" : side === "removed" ? "−" : " ";
}
</script>

<template>
  <section class="diff">
    <header class="pane-head">
      {{ app.diff.status === "ready" ? app.diff.value.header : "—" }}
    </header>

    <!-- The four states of SPEC §10, each drawn as itself: no spinner over the
         window, no blank pane, no error swallowed into an empty list. -->
    <p v-if="app.diff.status === 'idle'" class="pane-empty">Aucun fichier ouvert</p>
    <p v-else-if="app.diff.status === 'loading'" class="pane-empty">Lecture du diff…</p>
    <p v-else-if="app.diff.status === 'failed'" class="pane-error mono">{{ app.diff.error }}</p>
    <p v-else-if="app.diff.value.rows.length === 0" class="pane-empty">
      Rien à afficher pour ce fichier
    </p>

    <VirtualList
      v-else
      :items="app.diff.value.rows"
      :row-height="ROW_HEIGHT"
      v-slot="{ item }"
    >
      <div v-if="item.kind === 'header'" class="diff-header mono">{{ item.text }}</div>
      <div v-else-if="item.kind === 'fold'" class="diff-fold mono">
        ⌄ {{ item.lines }} lignes de contexte repliées
      </div>
      <div v-else class="diff-line mono" :class="item.side">
        <span class="gutter">{{ item.old ?? "" }}</span>
        <span class="gutter">{{ item.new ?? "" }}</span>
        <span class="sign">{{ sign(item.side) }}</span>
        <span class="text">{{ item.text }}</span>
      </div>
    </VirtualList>
  </section>
</template>

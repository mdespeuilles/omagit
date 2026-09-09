<script setup lang="ts">
// Board 03's footer: where HEAD is, how much is staged, and what is happening
// right now.
//
// The busy state lives here rather than over the window. A write takes long
// enough to notice and short enough that a modal spinner would flash — and
// covering the screen would hide the very list the user is deciding from.

import {
  app,
  dismissNotes,
  dismissWriteError,
  plural,
  stagedCount,
  toggleJournal,
  unstagedCount,
} from "../state";
</script>

<template>
  <footer class="statusbar">
    <span v-if="app.summary" class="mono">⑂ {{ app.summary.head }}</span>
    <span class="sep">│</span>
    <span>{{ plural(stagedCount(), "indexé") }} · {{ unstagedCount() }} non indexé</span>

    <!-- A half-finished merge or rebase, said instead of the branch: "on main"
         is misleading while one is stuck. -->
    <span v-if="app.summary?.operation" class="conflict">│ {{ app.summary.operation }}</span>
    <span v-if="app.summary && app.summary.counts.conflicted > 0" class="conflict">
      │ {{ plural(app.summary.counts.conflicted, "conflit") }}
    </span>

    <span class="pane-head-spacer" />

    <span v-if="app.busy" class="busy mono">{{ app.busy }}…</span>

    <button v-if="app.notes" class="link ok" :title="app.notes" @click="dismissNotes()">
      {{ app.notes.split("\n")[0] }} ✓
    </button>
    <button v-if="app.writeError" class="link danger" @click="dismissWriteError()">
      {{ app.writeError }} ✗
    </button>

    <button class="link" @click="toggleJournal()">Journal</button>
  </footer>
</template>

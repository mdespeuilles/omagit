<script setup lang="ts">
// Board 03's footer: where HEAD is, how much is staged, and what is happening
// right now.
//
// The busy state lives here rather than over the window. A write takes long
// enough to notice and short enough that a modal spinner would flash — and
// covering the screen would hide the very list the user is deciding from.

import {
  abortOperation,
  app,
  conflictCount,
  continueOperation,
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
         is misleading while one is stuck. Both ways out sit next to it —
         forward once the conflicts are settled, back at any time — because a
         repository left half-way with no visible exit, or with only the exit
         that throws the work away, is one somebody finishes in a terminal. -->
    <template v-if="app.summary?.operation">
      <span class="conflict">│ {{ app.summary.operation }} en cours</span>
      <button
        class="link"
        :disabled="!!app.busy || conflictCount() > 0"
        :title="
          conflictCount() > 0
            ? `Il reste ${conflictCount()} conflit(s) à résoudre`
            : `Terminer ${app.summary.operation}`
        "
        @click="continueOperation()"
      >
        Poursuivre
      </button>
      <button class="link danger" :disabled="!!app.busy" @click="abortOperation()">
        Abandonner
      </button>
    </template>
    <span v-if="app.summary && app.summary.counts.conflicted > 0" class="conflict">
      │ {{ plural(app.summary.counts.conflicted, "conflit") }}
    </span>

    <span class="pane-head-spacer" />

    <span v-if="app.busy" class="busy mono">{{ app.busy }}…</span>

    <!-- Both carry a sentence of `git`'s, which has no length limit: the text
         is what shrinks, on one line, with the whole of it in the title. A bar
         22 pixels tall that wrapped one of these overlapped everything else in
         it. -->
    <button v-if="app.notes" class="link ok message" :title="app.notes" @click="dismissNotes()">
      <span class="said">{{ app.notes.split("\n")[0] }} ✓</span>
    </button>
    <button
      v-if="app.writeError"
      class="link danger message"
      :title="app.writeError"
      @click="dismissWriteError()"
    >
      <span class="said">{{ app.writeError }} ✗</span>
    </button>

    <button class="link" @click="toggleJournal()">Journal</button>
  </footer>
</template>

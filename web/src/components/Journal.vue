<script setup lang="ts">
// The operations journal of SPEC §11: the exact command, not a summary of it.
//
// The point is that a user can read what omagit ran and run it again in a
// terminal. A line that said "staged 3 files" would be a claim; a line that
// says `git apply --cached -` is a fact they can check.

import { app, toggleJournal } from "../state";
</script>

<template>
  <aside class="journal">
    <header class="pane-head">
      <span>Journal</span>
      <span class="pane-head-spacer" />
      <button class="link" @click="toggleJournal()">Fermer</button>
    </header>
    <!-- Says what the panel is for, not only that it is empty: this one is
         opened by somebody looking for it, and "Aucune commande" leaves them
         wondering whether it records anything at all. -->
    <p v-if="app.journal.length === 0" class="pane-empty">
      Aucune commande pour l'instant — chaque écriture Git s'inscrit ici avec la ligne exacte qui a
      été lancée.
    </p>
    <ol v-else class="journal-list">
      <li v-for="(row, at) in app.journal" :key="at" class="journal-row" :class="row.outcome">
        <span class="journal-mark">{{
          row.outcome === "ok" ? "✓" : row.outcome === "failed" ? "✗" : "…"
        }}</span>
        <span class="journal-command mono" :class="{ destructive: row.destructive }">{{
          row.command
        }}</span>
        <pre v-if="row.stderr" class="journal-stderr mono">{{ row.stderr }}</pre>
      </li>
    </ol>
  </aside>
</template>

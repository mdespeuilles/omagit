<script setup lang="ts">
// What one stash holds, above its diff.
//
// The same shape as the commit detail, because a stash *is* a commit — with one
// thing said out loud that a commit never has to say: which side of the index
// its files came from stops mattering here, and whether it carries files that
// were never tracked starts to.

import { computed } from "vue";
import { app, selectStashFile } from "../state";
import { when } from "../format";

const entry = computed(() =>
  app.stashes.status === "ready"
    ? (app.stashes.value.find((row) => row.id.full === app.stash) ?? null)
    : null,
);
const files = computed(() => (app.stashFiles.status === "ready" ? app.stashFiles.value : []));

function sign(row: { added: number; removed: number; reason: string | null }): string {
  return row.reason ?? `+${row.added} −${row.removed}`;
}
</script>

<template>
  <section class="detail">
    <header class="pane-head">
      <span>Remise</span>
      <span v-if="entry" class="pane-head-title mono">{{ entry.id.short }}</span>
    </header>

    <p v-if="!entry" class="pane-empty">Aucune remise sélectionnée</p>

    <template v-else>
      <div class="detail-head">
        <p class="detail-summary">{{ entry.message }}</p>
        <p class="detail-who">
          <span class="dim">remisé depuis</span>
          <span class="mono">{{ entry.branch ?? "HEAD détaché" }}</span>
          <span class="dim">{{ when(entry.when) }}</span>
        </p>
        <!-- Said here and not only as a mark on the row: these files are in no
             commit and no index anywhere else, so a preview that showed them
             without saying where they come from would be the same trap the
             other way round. -->
        <p v-if="entry.untracked" class="detail-who">
          <span class="dim">contient des fichiers non suivis, emportés avec la remise</span>
        </p>
      </div>

      <header class="pane-head">
        <span>Fichiers</span>
        <span class="pane-head-count">{{ files.length }}</span>
      </header>

      <p v-if="app.stashFiles.status === 'loading'" class="pane-empty">Lecture de la remise…</p>
      <p v-else-if="app.stashFiles.status === 'failed'" class="pane-error mono">
        {{ app.stashFiles.error }}
      </p>
      <p v-else-if="files.length === 0" class="pane-empty">Cette remise ne change aucun fichier</p>
      <ol v-else class="detail-files">
        <li
          v-for="file in files"
          :key="file.path"
          class="file-row"
          :class="{ selected: app.stashFile === file.path }"
          @click="selectStashFile(file.path)"
        >
          <span class="file-code mono" :class="file.change">{{ file.change.charAt(0) }}</span>
          <span class="file-path mono">
            <span class="dir">{{ file.path.slice(0, file.path.lastIndexOf("/") + 1) }}</span>
            <span class="name">{{ file.path.slice(file.path.lastIndexOf("/") + 1) }}</span>
          </span>
          <span class="file-count mono">{{ sign(file) }}</span>
        </li>
      </ol>
    </template>
  </section>
</template>

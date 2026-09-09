<script setup lang="ts">
// One commit: what it says, who made it, and what it changed.

import { computed } from "vue";
import { authored, exact, when } from "../format";
import { app, selectCommitFile } from "../state";

const detail = computed(() => (app.commit.status === "ready" ? app.commit.value : null));

function sign(row: { added: number; removed: number; reason: string | null }): string {
  return row.reason ?? `+${row.added} −${row.removed}`;
}
</script>

<template>
  <section class="detail">
    <header class="pane-head">
      <span>Commit</span>
      <span v-if="detail" class="pane-head-title mono">{{ detail.id.short }}</span>
    </header>

    <p v-if="app.commit.status === 'idle'" class="pane-empty">Aucun commit sélectionné</p>
    <p v-else-if="app.commit.status === 'loading'" class="pane-empty">Lecture du commit…</p>
    <p v-else-if="app.commit.status === 'failed'" class="pane-error mono">{{ app.commit.error }}</p>

    <template v-else-if="detail">
      <div class="detail-head">
        <p class="detail-summary">{{ detail.summary }}</p>
        <pre v-if="detail.body" class="detail-body mono">{{ detail.body }}</pre>

        <p class="detail-who">
          <span class="mono">{{ detail.author.name }}</span>
          <span class="dim">&lt;{{ detail.author.email }}&gt;</span>
          <span class="dim" :title="authored(detail.author.when, detail.author.offset)">
            {{ when(detail.author.when) }}
          </span>
        </p>
        <!-- Only when it differs from the author, which is the case worth
             seeing: a rebase, a cherry-pick, a patch applied by someone else. -->
        <p v-if="detail.committer" class="detail-who">
          <span class="dim">commité par</span>
          <span class="mono">{{ detail.committer.name }}</span>
          <span class="dim" :title="exact(detail.committer.when)">
            {{ when(detail.committer.when) }}
          </span>
        </p>

        <p class="detail-parents mono">
          <span class="dim">{{ detail.parents.length > 1 ? "parents" : "parent" }}</span>
          <span v-if="detail.parents.length === 0" class="dim">aucun — commit racine</span>
          <span v-for="parent in detail.parents" :key="parent.full">{{ parent.short }}</span>
        </p>
      </div>

      <header class="pane-head">
        <span>Fichiers</span>
        <span class="pane-head-count">{{ detail.files.length }}</span>
      </header>
      <p v-if="detail.files.length === 0" class="pane-empty">Ce commit ne change aucun fichier</p>
      <ol v-else class="detail-files">
        <li
          v-for="file in detail.files"
          :key="file.path"
          class="file-row"
          :class="{ selected: app.commitFile === file.path }"
          @click="selectCommitFile(file.path)"
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

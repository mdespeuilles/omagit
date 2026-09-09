<script setup lang="ts">
// The working copy's files, one row each.

import { app, selectFile } from "../state";
import VirtualList from "./VirtualList.vue";

const ROW_HEIGHT = 24;
</script>

<template>
  <section class="files">
    <header class="pane-head">STATUS</header>

    <p v-if="app.status.status === 'loading'" class="pane-empty">Lecture du statut…</p>
    <p v-else-if="app.status.status === 'failed'" class="pane-error mono">{{ app.status.error }}</p>
    <p v-else-if="app.status.status === 'ready' && app.status.value.length === 0" class="pane-empty">
      Rien de modifié
    </p>

    <VirtualList
      v-else-if="app.status.status === 'ready'"
      :items="app.status.value"
      :row-height="ROW_HEIGHT"
      v-slot="{ item }"
    >
      <div
        class="file-row"
        :class="{ selected: app.selected?.path === item.path }"
        @click="selectFile(item)"
      >
        <span class="file-code mono">{{ item.code }}</span>
        <span class="file-path mono">{{ item.path }}</span>
      </div>
    </VirtualList>
  </section>
</template>

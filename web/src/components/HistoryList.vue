<script setup lang="ts">
// Board 05's history: the graph, the message, who and when.
//
// Virtualised like every other list here, and paged on top of that: SPEC §12
// asks for a hundred thousand commits, and the backend hands out five hundred
// at a time. The list asks for the next page when it nears the end of the one
// it has, which is what `nearEnd` is for.

import { computed } from "vue";
import { when, exact } from "../format";
import { app, moreHistory, selectCommit, setQuery } from "../state";
import GraphGutter from "./GraphGutter.vue";
import HistoryFilters from "./HistoryFilters.vue";
import VirtualList from "./VirtualList.vue";

const ROW_HEIGHT = 24;

const rows = computed(() => (app.history.status === "ready" ? app.history.value : []));
</script>

<template>
  <section class="history">
    <header class="pane-head">
      <span>Historique</span>
      <span class="pane-head-count"> {{ rows.length }}{{ app.historyDone ? "" : "+" }} </span>
      <span class="pane-head-spacer" />
      <label class="toggle">
        <input
          type="checkbox"
          :checked="app.query.all"
          @change="setQuery({ all: ($event.target as HTMLInputElement).checked })"
        />
        Toutes les branches
      </label>
      <label class="toggle" title="Ne suivre que le premier parent de chaque fusion">
        <input
          type="checkbox"
          :checked="app.query.firstParent"
          @change="setQuery({ firstParent: ($event.target as HTMLInputElement).checked })"
        />
        Tronc
      </label>
    </header>
    <HistoryFilters />

    <p v-if="app.history.status === 'idle'" class="pane-empty">Historique non chargé</p>
    <p v-else-if="app.history.status === 'loading'" class="pane-empty">Lecture de l'historique…</p>
    <p v-else-if="app.history.status === 'failed'" class="pane-error mono">
      {{ app.history.error }}
    </p>
    <p v-else-if="rows.length === 0" class="pane-empty">Aucun commit</p>

    <VirtualList
      v-else
      :items="rows"
      :row-height="ROW_HEIGHT"
      @near-end="moreHistory()"
      v-slot="{ item }"
    >
      <div
        class="commit-row"
        :class="{
          selected: app.commit.status === 'ready' && app.commit.value.id.full === item.id.full,
        }"
        @click="selectCommit(item.id.full)"
      >
        <GraphGutter v-if="item.graph" :row="item" :height="ROW_HEIGHT" />
        <span v-else class="gutter-none" />
        <span class="commit-author">{{ item.author }}</span>
        <span
          v-for="label in item.labels"
          :key="label.kind + label.name"
          class="ref"
          :class="label.kind"
          >{{ label.name }}</span
        >
        <span class="commit-when" :title="exact(item.when)">{{ when(item.when) }}</span>
        <span class="commit-hash mono">{{ item.id.short }}</span>
        <span class="commit-summary">{{ item.summary }}</span>
      </div>
    </VirtualList>

    <footer v-if="rows.length > 0" class="files-foot">
      {{ rows.length }} commits{{ app.historyDone ? "" : " chargés" }}
      <span v-if="app.historyLoading"> · suite…</span>
    </footer>
  </section>
</template>

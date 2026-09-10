<script setup lang="ts">
// Board 05's history: the graph, the message, who and when.
//
// Virtualised like every other list here, and paged on top of that: SPEC §12
// asks for a hundred thousand commits, and the backend hands out five hundred
// at a time. The list asks for the next page when it nears the end of the one
// it has, which is what `nearEnd` is for.

import { computed } from "vue";
import { when, exact } from "../format";
import {
  app,
  clearFilters,
  compareWith,
  isFilteringHistory,
  markCompareFrom,
  moreHistory,
  selectCommit,
  goToZone,
  setQuery,
  showHeadHistory,
  toggleFilters,
  zoneActive,
} from "../state";
import GraphGutter from "./GraphGutter.vue";
import HistoryFilters from "./HistoryFilters.vue";
import { doubleRowHeight } from "../metrics";
import VirtualList from "./VirtualList.vue";

/// Two lines per commit.
///
/// Board 05 draws one, and this is a deliberate departure from it: on a real
/// history the single line has six things competing for it — author, refs,
/// date, hash and message — and the message, which is what the list is read
/// for, is the one that loses. Two lines give the message the whole of the
/// second one.
const ROW_HEIGHT = doubleRowHeight();

const rows = computed(() => (app.history.status === "ready" ? app.history.value : []));

/// `Maxence d'Espeuilles` → `MD`. The first letters of the first two words,
/// which is what board 01's avatar shows.
function initials(name: string): string {
  return [...name.trim().split(/\s+/).slice(0, 2)]
    .map((word) => [...word][0] ?? "")
    .join("")
    .toUpperCase();
}

/// A plain click opens a commit; a shift-click compares it with the marked one.
///
/// Shift-click rather than a mode: comparing is something you do to two rows
/// you can see, and a mode would have to be entered, remembered and left again
/// for an action that takes two clicks.
function open(id: string, extend: boolean): void {
  if (extend && app.compareFrom && app.compareFrom !== id) {
    void compareWith(id);
    return;
  }
  markCompareFrom(id);
  void selectCommit(id);
}
</script>

<template>
  <section class="history">
    <header class="pane-head">
      <span>Historique</span>
      <span class="pane-head-count"> {{ rows.length }}{{ app.historyDone ? "" : "+" }} </span>
      <!-- Whose history this is, when it is not HEAD's. A list scoped to a
           branch looks exactly like the ordinary one, and the row that asked
           for it is in another column. -->
      <button
        v-if="app.query.branch"
        class="link on"
        title="Revenir à l'historique de la branche courante"
        @click="showHeadHistory()"
      >
        {{ app.query.branch }} ✕
      </button>
      <span class="pane-head-spacer" />
      <label class="toggle">
        <input
          type="checkbox"
          :checked="app.query.all"
          @change="setQuery({ all: ($event.target as HTMLInputElement).checked })"
        /><span class="check" aria-hidden="true">✓</span>
        Toutes les branches
      </label>
      <label class="toggle" title="Ne suivre que le premier parent de chaque fusion">
        <input
          type="checkbox"
          :checked="app.query.firstParent"
          @change="setQuery({ firstParent: ($event.target as HTMLInputElement).checked })"
        /><span class="check" aria-hidden="true">✓</span>
        Tronc
      </label>
      <span class="pane-head-rule" />
      <!-- Folded away by default: five fields answering a question most
           readers are not asking, in a column they want for commits. -->
      <button
        class="link"
        :class="{ on: app.showFilters || isFilteringHistory() }"
        :aria-expanded="app.showFilters || isFilteringHistory()"
        @click="toggleFilters()"
      >
        Filtrer{{ isFilteringHistory() ? " ·" : "" }}
      </button>
    </header>
    <HistoryFilters v-if="app.showFilters || isFilteringHistory()" />

    <p v-if="app.history.status === 'idle'" class="pane-empty">Historique non chargé</p>
    <p v-else-if="app.history.status === 'loading'" class="pane-empty">Lecture de l'historique…</p>
    <p v-else-if="app.history.status === 'failed'" class="pane-error mono">
      {{ app.history.error }}
    </p>
    <!-- Three ways to have no commits, and only one of them means the list is
         telling you about the repository. "Aucun commit" under a filter reads
         as an empty repository, which is a lie the filter row is too small to
         correct on its own. -->
    <p v-else-if="rows.length === 0 && isFilteringHistory()" class="pane-empty">
      Aucun commit ne correspond.
      <button class="link" @click="clearFilters()">Effacer les filtres</button>
    </p>
    <p v-else-if="rows.length === 0 && app.summary?.head_kind === 'unborn'" class="pane-empty">
      Ce dépôt n'a pas encore de commit — le premier se fait depuis la copie de travail.
    </p>
    <p v-else-if="rows.length === 0" class="pane-empty">Aucun commit</p>

    <VirtualList
      v-else
      tabindex="0"
      data-zone="2"
      @focus="goToZone(2)"
      :items="rows"
      :row-height="ROW_HEIGHT"
      @near-end="moreHistory()"
      v-slot="{ item }"
    >
      <div
        class="row commit-row"
        :class="{
          selected: app.commit.status === 'ready' && app.commit.value.id.full === item.id.full,
          focused:
            zoneActive(2) &&
            app.commit.status === 'ready' &&
            app.commit.value.id.full === item.id.full,
          'compare-from': app.compareFrom === item.id.full && app.compare.status !== 'idle',
          'compare-to':
            app.compare.status === 'ready' && app.compare.value.to.full === item.id.full,
        }"
        :title="
          app.compareFrom && app.compareFrom !== item.id.full
            ? `Maj-clic : comparer avec ${app.compareFrom.slice(0, 7)}`
            : ''
        "
        @click="
          goToZone(2);
          open(item.id.full, $event.shiftKey);
        "
      >
        <GraphGutter v-if="item.graph" :row="item" :height="ROW_HEIGHT" />
        <span v-else class="gutter-none" />
        <span class="avatar mono" :title="item.author">{{ initials(item.author) }}</span>
        <span class="commit-lines">
          <span class="commit-line-top">
            <span class="commit-author">{{ item.author }}</span>
            <span class="pane-head-spacer" />
            <span class="commit-when" :title="exact(item.when)">{{ when(item.when) }}</span>
          </span>
          <span class="commit-line-bottom">
            <span class="commit-hash mono">{{ item.id.short }}</span>
            <span
              v-for="label in item.labels"
              :key="label.kind + label.name"
              class="ref"
              :class="label.kind"
              >{{ label.name }}</span
            >
            <span class="commit-summary">{{ item.summary }}</span>
          </span>
        </span>
      </div>
    </VirtualList>

    <footer v-if="rows.length > 0" class="files-foot">
      {{ rows.length }} commits{{ app.historyDone ? "" : " chargés" }}
      <span v-if="app.historyLoading"> · suite…</span>
    </footer>
  </section>
</template>

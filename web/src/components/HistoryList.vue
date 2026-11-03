<script setup lang="ts">
// Board 05's history: the graph, the message, who and when.
//
// Virtualised like every other list here, and paged on top of that: SPEC §12
// asks for a hundred thousand commits, and the backend hands out five hundred
// at a time. The list asks for the next page when it nears the end of the one
// it has, which is what `nearEnd` is for.

import { computed } from "vue";
import { branchColor, when, exact } from "../format";
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
import { count, t } from "../i18n";
import GraphGutter from "./GraphGutter.vue";
import HistoryFilters from "./HistoryFilters.vue";
import { commitRowHeight } from "../metrics";
import VirtualList from "./VirtualList.vue";

/// Two lines per commit (DESIGN v2): the message on the first, because it is
/// what the list is read for; who, which hash and which branches on the
/// second, smaller.
const ROW_HEIGHT = commitRowHeight();

const rows = computed(() => (app.history.status === "ready" ? app.history.value : []));
/// The widest the graph gets in what is loaded, capped: one octopus merge
/// across twenty lanes should not push every message off the screen.
const MAX_COLUMNS = 8;
const columns = computed(() =>
  Math.min(
    MAX_COLUMNS,
    rows.value.reduce((widest, row) => Math.max(widest, row.width), 1),
  ),
);

/// `Maxence d'Espeuilles` → `MD`. The first letters of the first two words,
/// which is what board 01's avatar shows.
function initials(name: string): string {
  return [...name.trim().split(/\s+/).slice(0, 2)]
    .map((word) => [...word][0] ?? "")
    .join("")
    .toUpperCase();
}

/// The commits HEAD has that its upstream does not — marked "not pushed yet"
/// — and where the two parted, marked when they have both moved on. From the
/// divergence the overview already read, so no second walk.
const unpushed = computed(() => new Set(app.divergence?.outgoing.map((c) => c.id.full) ?? []));
const parted = computed(() => {
  const tracking = app.summary?.tracking;
  if (!tracking || tracking.ahead === 0 || tracking.behind === 0) return null;
  return app.divergence?.base?.full ?? null;
});

/// The colour a label wears: its branch's, the same as on the overview and the
/// map; a remote branch wears the local name's colour, outlined.
function labelColour(label: { name: string; kind: string }): string {
  if (label.kind === "tag") return "var(--warning)";
  if (label.kind === "remote") return branchColor(label.name.slice(label.name.indexOf("/") + 1));
  return branchColor(label.name);
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
      <span>{{ t("history.title") }}</span>
      <span class="pane-head-count"> {{ rows.length }}{{ app.historyDone ? "" : "+" }} </span>
      <!-- Whose history this is, when it is not HEAD's. A list scoped to a
           branch looks exactly like the ordinary one, and the row that asked
           for it is in another column. -->
      <button
        v-if="app.query.branch"
        class="link on"
        :title="t('history.onBranch')"
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
        {{ t("history.allBranches") }}
      </label>
      <label class="toggle" :title="t('history.firstParent')">
        <input
          type="checkbox"
          :checked="app.query.firstParent"
          @change="setQuery({ firstParent: ($event.target as HTMLInputElement).checked })"
        /><span class="check" aria-hidden="true">✓</span>
        {{ t("history.trunk") }}
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
        {{ t("history.filter") }}{{ isFilteringHistory() ? " ·" : "" }}
      </button>
    </header>
    <HistoryFilters v-if="app.showFilters || isFilteringHistory()" />

    <p v-if="app.history.status === 'idle'" class="pane-empty">{{ t("history.idle") }}</p>
    <p v-else-if="app.history.status === 'loading'" class="pane-empty">
      {{ t("history.reading") }}
    </p>
    <p v-else-if="app.history.status === 'failed'" class="pane-error mono">
      {{ app.history.error }}
    </p>
    <!-- Three ways to have no commits, and only one of them means the list is
         telling you about the repository. "Aucun commit" under a filter reads
         as an empty repository, which is a lie the filter row is too small to
         correct on its own. -->
    <p v-else-if="rows.length === 0 && isFilteringHistory()" class="pane-empty">
      {{ t("history.noMatch") }}
      <button class="link" @click="clearFilters()">{{ t("history.clearFilters") }}</button>
    </p>
    <p v-else-if="rows.length === 0 && app.summary?.head_kind === 'unborn'" class="pane-empty">
      {{ t("history.unborn") }}
    </p>
    <p v-else-if="rows.length === 0" class="pane-empty">{{ t("history.empty") }}</p>

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
        <GraphGutter
          v-if="item.graph"
          :row="item"
          :height="ROW_HEIGHT"
          :head="item.labels.some((label) => label.kind === 'head')"
          :columns="columns"
        />
        <span v-else class="gutter-none" />
        <span
          class="avatar commit-avatar"
          :title="item.author"
          :style="{ '--chip': branchColor(item.author) }"
          >{{ initials(item.author) }}</span
        >
        <span class="commit-lines">
          <span class="commit-summary" :class="{ unpushed: unpushed.has(item.id.full) }">{{
            item.summary
          }}</span>
          <span class="commit-meta">
            <span class="commit-hash mono">{{ item.id.short }}</span>
            <span class="commit-author">{{ item.author }}</span>
            <span
              v-for="label in item.labels"
              :key="label.kind + label.name"
              class="ref"
              :class="label.kind"
              :style="{ '--chip': labelColour(label) }"
              >{{ label.name }}</span
            >
            <span v-if="unpushed.has(item.id.full)" class="commit-mark ahead">{{
              t("history.unpushed")
            }}</span>
            <span
              v-if="parted === item.id.full"
              class="commit-mark behind"
              :title="t('history.base', { upstream: app.summary?.tracking?.upstream ?? '' })"
              >{{ t("history.baseShort") }}</span
            >
          </span>
        </span>
        <span class="commit-when" :title="exact(item.when)">{{ when(item.when) }}</span>
      </div>
    </VirtualList>

    <footer v-if="rows.length > 0" class="files-foot">
      {{
        app.historyDone
          ? count("history.commits", rows.length)
          : t("history.loaded", { n: rows.length })
      }}
      <span v-if="app.historyLoading"> {{ t("history.more") }}</span>
    </footer>
  </section>
</template>

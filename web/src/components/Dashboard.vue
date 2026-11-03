<script setup lang="ts">
// The right of the Repositories screen when no repository is picked (DESIGN
// v2, board "Dépôts"): what needs a hand, first; how fresh the arrows are; and
// the rest, only counted.
//
// The action on a row is "Open", never "Push" or "Pull" from here: the
// overview a repository opens on names the right one for its state and shows
// what it would move. A push launched from a list of twenty, one row away from
// the repository you meant, is the click this screen should not make easy.

import { computed, onBeforeUnmount, ref } from "vue";
import { triage, type Need, type Triaged } from "../dashboard";
import { ago, branchColor } from "../format";
import { app, checkAll, openRepository, showCard } from "../state";
import { count, t, worded } from "../i18n";

const now = ref(Math.floor(Date.now() / 1000));
const tick = setInterval(() => (now.value = Math.floor(Date.now() / 1000)), 60_000);
onBeforeUnmount(() => clearInterval(tick));

const sorted = computed(() => triage(app.repositories, app.library, now.value));
const loading = computed(() =>
  app.repositories.some((row) => !row.missing && app.library[row.path]?.status === "loading"),
);
/// Every repository with a remote, for "check all".
const fetchable = computed(() =>
  [...sorted.value.needs, ...sorted.value.clean]
    .filter((entry) => entry.summary.remotes.length > 0)
    .map((entry) => entry.row.path),
);

const TONE: Record<Need, string> = {
  operation: "danger",
  conflicts: "danger",
  diverged: "warning",
  behind: "behind",
  ahead: "ahead",
  local: "accent",
  changes: "",
};

function pill(entry: Triaged): string {
  const { summary, need } = entry;
  if (need === "operation") return t("dash.need.operation", { operation: summary.operation ?? "" });
  if (need === "conflicts") return count("dash.need.conflicts", summary.counts.conflicted);
  return t(`dash.need.${need}`);
}

function changed(entry: Triaged): number {
  const { modified, added, deleted, renamed, untracked } = entry.summary.counts;
  return modified + added + deleted + renamed + untracked;
}

function fetched(entry: Triaged): string {
  const at = entry.summary.fetched_at;
  return at === null ? t("dash.never") : t("dash.checked", { ago: ago(at, now.value * 1000) });
}

function group(entry: Triaged): string {
  return worded(app.groups[entry.row.group]?.name ?? "");
}
</script>

<template>
  <main class="dashboard">
    <header class="dash-head">
      <div>
        <h1 class="dash-title">{{ t("dash.title") }}</h1>
        <p class="ov-muted">{{ count("dash.count", app.repositories.length) }}</p>
      </div>
      <div class="dash-actions">
        <button :disabled="!!app.checking || fetchable.length === 0" @click="checkAll(fetchable)">
          {{ t("dash.checkAll") }}
        </button>
      </div>
    </header>

    <section v-if="app.checking" class="dash-banner">
      <span>{{ t("dash.checking", { done: app.checking.done, total: app.checking.total }) }}</span>
      <span class="dash-progress">
        <span :style="{ width: `${(100 * app.checking.done) / app.checking.total}%` }" />
      </span>
    </section>
    <section v-else-if="sorted.stale.length > 0" class="dash-banner warning-banner">
      <div>
        <strong>{{ count("dash.stale", sorted.stale.length) }}</strong>
        <span class="ov-muted">{{ t("dash.staleDetail") }}</span>
      </div>
      <button @click="checkAll(sorted.stale.map((entry) => entry.row.path))">
        {{ t("dash.checkStale") }}
      </button>
    </section>

    <section class="ov-card dash-section">
      <h2 class="ov-section">
        {{ t("dash.needs") }}
        <span class="ov-muted">{{ sorted.needs.length }}</span>
      </h2>
      <p v-if="loading && sorted.needs.length === 0" class="ov-muted">{{ t("dash.loading") }}</p>
      <p v-else-if="sorted.needs.length === 0" class="synced">{{ t("dash.allGood") }}</p>
      <ul v-else class="dash-needs">
        <li v-for="entry in sorted.needs" :key="entry.row.path" class="dash-need">
          <span class="dash-who">
            <button class="dash-name" @click="showCard(entry.row.path)">
              {{ entry.row.name }}
            </button>
            <span class="dash-meta ov-muted">{{ group(entry) }} · {{ fetched(entry) }}</span>
          </span>
          <span class="ov-chip mono" :style="{ '--chip': branchColor(entry.summary.head) }">
            {{ entry.summary.head }}
          </span>
          <span class="ov-pill" :class="TONE[entry.need]">{{ pill(entry) }}</span>
          <span class="dash-detail">
            <span v-if="entry.summary.tracking?.ahead" class="ahead"
              >↑{{ entry.summary.tracking.ahead }}</span
            >
            <span v-if="entry.summary.tracking?.behind" class="behind"
              >↓{{ entry.summary.tracking.behind }}</span
            >
            <span v-if="changed(entry) > 0" class="ov-muted">{{
              count("dash.changes", changed(entry))
            }}</span>
          </span>
          <button @click="openRepository(entry.row.path)">{{ t("dash.open") }}</button>
        </li>
      </ul>
    </section>

    <section v-if="sorted.clean.length > 0" class="ov-card dash-section">
      <h2 class="ov-section">
        {{ t("dash.clean") }}
        <span class="ov-muted">{{ sorted.clean.length }}</span>
      </h2>
      <div class="dash-clean">
        <button
          v-for="entry in sorted.clean"
          :key="entry.row.path"
          class="dash-clean-row"
          :title="fetched(entry)"
          @click="showCard(entry.row.path)"
          @dblclick="openRepository(entry.row.path)"
        >
          <span class="dash-dot" :style="{ background: branchColor(entry.summary.head) }" />
          {{ entry.row.name }}
        </button>
      </div>
    </section>
  </main>
</template>

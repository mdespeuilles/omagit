<script setup lang="ts">
// The branch map (DESIGN v2, board "Carte"): the trunk as a line through
// time, and every other branch as a line from where it left the trunk to its
// last commit — so "which branches are still open, how old, how far apart" is
// one look rather than a graph of a thousand rows to read.
//
// A first version of the board: open branches as lines, merged ones as a mark
// where they ended, a range to zoom, and a side panel with the selected
// branch's own commits. The board's folded runs of commits and its merge
// check are for later.

import { computed, onMounted, ref, watch } from "vue";
import { api, type BranchMap, type LastCommit } from "../ipc";
import { ago, branchColor, when } from "../format";
import { app, checkoutBranch } from "../state";
import { count, t } from "../i18n";

const DAY = 24 * 60 * 60;
const RANGES = [
  { key: "map.range.month", days: 31 },
  { key: "map.range.quarter", days: 92 },
  { key: "map.range.half", days: 183 },
  { key: "map.range.all", days: 0 },
] as const;
/// How long without a commit before a branch is drawn as idle.
const IDLE_AFTER = 30 * DAY;

const map = ref<BranchMap | null>(null);
const failed = ref<string | null>(null);
const loading = ref(true);
const range = ref(1);
const everything = ref(false);
const selected = ref<string | null>(null);
const commits = ref<LastCommit[] | null>(null);

async function read(): Promise<void> {
  const path = app.open;
  if (!path) return;
  loading.value = true;
  try {
    const answer = await api.branchMap(path);
    if (app.open !== path) return;
    map.value = answer;
    failed.value = null;
    const names = answer?.branches.map((branch) => branch.name) ?? [];
    if (!selected.value || !names.includes(selected.value)) {
      selected.value = answer?.branches.find((branch) => branch.head)?.name ?? null;
    }
  } catch (error) {
    failed.value = error instanceof Error ? error.message : String(error);
  } finally {
    loading.value = false;
  }
}
onMounted(read);
// The tips move under every commit, pull and checkout.
watch(
  () => [app.open, app.summary?.head, app.summary?.last_commit?.id.full],
  () => void read(),
);

/// Whether the selected branch would merge into the trunk: the files it would
/// conflict on, `[]` for clean, `"failed"` when git could not say (older than
/// 2.38), `null` while asking.
const conflicts = ref<string[] | "failed" | null>(null);

watch([selected, map], async () => {
  const path = app.open;
  const branch = selected.value;
  const trunk = map.value?.trunk;
  commits.value = null;
  conflicts.value = null;
  if (!path || !branch || !trunk) return;
  void (async () => {
    try {
      const files = await api.mergeCheck(path, branch, trunk);
      if (selected.value === branch) conflicts.value = files;
    } catch {
      if (selected.value === branch) conflicts.value = "failed";
    }
  })();
  try {
    const list = await api.branchCommits(path, branch, trunk, 30);
    if (selected.value === branch) commits.value = list;
  } catch {
    commits.value = [];
  }
});

/// The tags inside the range, for the marks along the trunk.
const tags = computed(() =>
  (map.value?.tags ?? []).filter((tag) => tag.time >= start.value && tag.time <= now),
);

const now = Math.floor(Date.now() / 1000);
const open = computed(() => map.value?.branches.filter((branch) => branch.ahead > 0) ?? []);
const shown = computed(() => {
  const all = map.value?.branches ?? [];
  const rows = everything.value ? all : all.filter((branch) => branch.ahead > 0);
  // Newest work at the top, next to where the eye starts.
  return [...rows].sort((a, b) => b.tip_time - a.tip_time);
});

/// The time the left edge stands for.
const start = computed(() => {
  const days = RANGES[range.value]!.days;
  if (days > 0) return now - days * DAY;
  const times = shown.value.map((branch) => branch.base_time ?? branch.tip_time);
  return Math.min(now - 7 * DAY, ...times);
});
/// A time as a percentage of the track, clamped to it.
function x(time: number): number {
  const span = Math.max(now - start.value, 1);
  return Math.min(100, Math.max(0, ((time - start.value) / span) * 100));
}
/// One tick per month boundary inside the range.
const months = computed(() => {
  const ticks: { at: number; label: string }[] = [];
  const format = new Intl.DateTimeFormat(undefined, { month: "short" });
  const cursor = new Date(start.value * 1000);
  cursor.setDate(1);
  cursor.setHours(0, 0, 0, 0);
  cursor.setMonth(cursor.getMonth() + 1);
  while (cursor.getTime() / 1000 < now) {
    ticks.push({ at: x(cursor.getTime() / 1000), label: format.format(cursor) });
    cursor.setMonth(cursor.getMonth() + 1);
  }
  // A year of ticks is twelve labels in a narrow track; keep every other.
  return ticks.length > 8 ? ticks.filter((_, at) => at % 2 === 0) : ticks;
});

const current = computed(
  () => map.value?.branches.find((branch) => branch.name === selected.value) ?? null,
);
</script>

<template>
  <main class="bmap">
    <section class="bmap-main">
      <header class="dash-head">
        <div>
          <h1 class="dash-title">{{ t("map.title") }}</h1>
          <p v-if="map" class="ov-muted">
            {{ count("map.count", open.length, { total: map.branches.length + 1 }) }}
          </p>
        </div>
        <div class="bmap-controls">
          <div class="segmented">
            <button :class="{ on: !everything }" @click="everything = false">
              {{ t("map.open") }}
            </button>
            <button :class="{ on: everything }" @click="everything = true">
              {{ t("map.all") }}
            </button>
          </div>
          <div class="segmented">
            <button
              v-for="(choice, at) in RANGES"
              :key="choice.key"
              :class="{ on: range === at }"
              @click="range = at"
            >
              {{ t(choice.key) }}
            </button>
          </div>
        </div>
      </header>

      <p v-if="failed" class="pane-error">{{ failed }}</p>
      <p v-else-if="loading && !map" class="ov-muted">{{ t("map.loading") }}</p>
      <p v-else-if="!map" class="ov-muted">{{ t("map.noTrunk") }}</p>

      <div v-else class="ov-card bmap-card">
        <div class="bmap-axis">
          <span />
          <div class="bmap-track">
            <span
              v-for="tick in months"
              :key="tick.at"
              class="bmap-tick"
              :style="{ left: `${tick.at}%` }"
              >{{ tick.label }}</span
            >
          </div>
        </div>

        <p v-if="shown.length === 0" class="ov-muted bmap-empty">
          {{ t("map.none", { trunk: map.trunk }) }}
        </p>
        <button
          v-for="branch in shown"
          :key="branch.name"
          class="bmap-row"
          :class="{
            selected: branch.name === selected,
            merged: branch.ahead === 0,
            idle: branch.ahead > 0 && now - branch.tip_time > IDLE_AFTER,
          }"
          :style="{ '--chip': branchColor(branch.name) }"
          @click="selected = branch.name"
        >
          <span class="bmap-label">
            <span class="mono bmap-name">{{ branch.name }}</span>
            <span class="bmap-sub ov-muted">
              <template v-if="branch.ahead === 0">{{ t("map.merged") }}</template>
              <template v-else-if="now - branch.tip_time > IDLE_AFTER">
                {{ t("map.idle", { ago: ago(branch.tip_time) }) }}
              </template>
              <template v-else>
                <span class="ahead">↑{{ branch.ahead }}</span>
                <span v-if="branch.behind" class="behind">↓{{ branch.behind }}</span>
              </template>
              <span v-if="branch.head" class="ov-pill accent">{{ t("map.here") }}</span>
            </span>
          </span>
          <span class="bmap-track">
            <span class="bmap-trunk-ghost" />
            <template v-if="branch.ahead > 0">
              <span
                class="bmap-line"
                :style="{
                  left: `${x(branch.base_time ?? branch.tip_time)}%`,
                  width: `${Math.max(x(branch.tip_time) - x(branch.base_time ?? branch.tip_time), 0.6)}%`,
                }"
              />
              <span
                class="bmap-fork"
                :style="{ left: `${x(branch.base_time ?? branch.tip_time)}%` }"
              />
              <span class="bmap-tip" :style="{ left: `${x(branch.tip_time)}%` }" />
            </template>
            <span v-else class="bmap-merge" :style="{ left: `${x(branch.tip_time)}%` }" />
          </span>
        </button>

        <div class="bmap-row bmap-trunk" :style="{ '--chip': branchColor(map.trunk) }">
          <span class="bmap-label">
            <span class="mono bmap-name">{{ map.trunk }}</span>
            <span class="bmap-sub ov-muted">{{ t("map.trunk") }}</span>
          </span>
          <span class="bmap-track">
            <span class="bmap-line" :style="{ left: '0%', width: `${x(map.trunk_time)}%` }" />
            <span class="bmap-tip" :style="{ left: `${x(map.trunk_time)}%` }" />
            <template v-for="tag in tags" :key="tag.name">
              <span class="bmap-tag" :style="{ left: `${x(tag.time)}%` }" :title="tag.name" />
              <span
                class="bmap-tag-name mono"
                :class="{ end: x(tag.time) > 90 }"
                :style="{ left: `${x(tag.time)}%` }"
                >{{ tag.name }}</span
              >
            </template>
          </span>
        </div>
      </div>
    </section>

    <aside class="bmap-side">
      <template v-if="map && current">
        <p class="ov-label">{{ t("map.selected") }}</p>
        <span class="ov-chip mono" :style="{ '--chip': branchColor(current.name) }">
          {{ current.name }}
        </span>
        <p v-if="current.base_time" class="ov-muted">
          {{ t("map.left", { trunk: map.trunk, when: when(current.base_time) }) }}
        </p>
        <ul class="bmap-facts">
          <li v-if="current.ahead > 0" class="ahead">
            {{ count("map.ahead", current.ahead, { trunk: map.trunk }) }}
          </li>
          <li v-else class="synced">{{ t("map.mergedLong", { trunk: map.trunk }) }}</li>
          <li v-if="current.behind > 0" class="behind">
            {{ count("map.behind", current.behind, { trunk: map.trunk }) }}
          </li>
        </ul>
        <p v-if="current.ahead > 0" class="bmap-check">
          <span v-if="conflicts === null" class="ov-muted">{{
            t("map.checking", { trunk: map.trunk })
          }}</span>
          <span v-else-if="conflicts === 'failed'" class="ov-muted">{{
            t("map.checkFailed")
          }}</span>
          <span v-else-if="conflicts.length === 0" class="synced">
            ✓ {{ t("map.clean", { trunk: map.trunk }) }}
          </span>
          <template v-else>
            <span class="danger-text">
              ✗ {{ count("map.conflicts", conflicts.length, { trunk: map.trunk }) }}
            </span>
            <span v-for="file in conflicts.slice(0, 5)" :key="file" class="mono ov-muted">{{
              file
            }}</span>
          </template>
        </p>
        <button v-if="!current.head" class="primary" @click="checkoutBranch(current.name)">
          {{ t("map.checkout") }}
        </button>

        <template v-if="current.ahead > 0">
          <p class="ov-label">{{ t("map.commits") }}</p>
          <p v-if="commits === null" class="ov-muted">{{ t("map.loading") }}</p>
          <div v-else class="ov-list">
            <div v-for="commit in commits" :key="commit.id.full" class="ov-commit">
              <span class="mono ov-hash">{{ commit.id.short }}</span>
              <span class="ov-subject">{{ commit.summary }}</span>
              <span class="ov-who">{{ commit.author }} · {{ ago(commit.when) }}</span>
            </div>
          </div>
        </template>
      </template>
      <p v-else-if="map" class="ov-muted">{{ t("map.select") }}</p>
    </aside>
  </main>
</template>

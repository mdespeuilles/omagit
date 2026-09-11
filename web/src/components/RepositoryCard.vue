<script setup lang="ts">
// Board 06's card: everything about one repository that is not its history.
//
// It reads nothing of its own. `omagit_git::Summary` was written for this card
// and gathers all of it in one pass, so the card draws what the list already
// asked for rather than asking again.

import { computed } from "vue";
import { exact, tildify, when } from "../format";
import { app, fileRepository, forgetRepository, openRepository } from "../state";
import { count, t, worded, type Plural } from "../i18n";

const row = computed(() => app.repositories.find((entry) => entry.path === app.card) ?? null);
const summary = computed(() => {
  const path = app.card;
  if (!path) return null;
  const held = path in app.library ? app.library[path] : null;
  return held?.status === "ready" ? held.value : null;
});
const failure = computed(() => {
  const path = app.card;
  const held = path && path in app.library ? app.library[path] : null;
  return held?.status === "failed" ? held.error : null;
});

/// The folders, for the one control on this screen that files a repository
/// without a pointer.
///
/// The list itself is dragged, which board 06 asks for and which nothing on a
/// keyboard can do. A select is reachable with Tab, says what the folders are
/// called without hovering anything, and is the obvious shape for "one of
/// these": the Preferences screen learned the same lesson about rows that only
/// looked like buttons.
const folders = computed(() =>
  app.groups.map((folder, index) => ({ index, name: worded(folder.name) })),
);

/// The status line, spelled out rather than summed: board 06 lists the kinds
/// separately and colours each, because they are four different amounts of
/// work.
const counted = computed(() => {
  const counts = summary.value?.counts;
  if (!counts) return [];
  const kinds: { key: Plural; n: number; kind: string }[] = [
    { key: "card.modified", n: counts.modified, kind: "changed" },
    { key: "card.added", n: counts.added, kind: "added" },
    { key: "card.deleted", n: counts.deleted, kind: "deleted" },
    { key: "card.renamed", n: counts.renamed, kind: "renamed" },
    { key: "library.conflicts", n: counts.conflicted, kind: "conflict" },
    { key: "library.untracked", n: counts.untracked, kind: "untracked" },
  ];
  return kinds
    .filter((entry) => entry.n > 0)
    .map((entry) => ({ ...entry, label: count(entry.key, entry.n) }));
});

/// The chip beside the name: how much is in the working copy, at a glance.
const changed = computed(() => {
  const counts = summary.value?.counts;
  if (!counts) return null;
  if (counts.conflicted > 0) {
    return { text: count("library.conflicts", counts.conflicted), kind: "conflict" };
  }
  const total = counts.modified + counts.added + counts.deleted + counts.renamed;
  if (total === 0) return null;
  return { text: count("card.modified", total), kind: "changed" };
});

const divergence = computed(() => {
  const tracking = summary.value?.tracking;
  if (!tracking || tracking.gone) return "";
  return [
    tracking.ahead > 0 ? `↑${tracking.ahead}` : "",
    tracking.behind > 0 ? `↓${tracking.behind}` : "",
  ]
    .filter(Boolean)
    .join(" ");
});

/// Board 06's sparkline: 5px bars, scaled to the busiest bucket. The shape is
/// the point, so an absolute scale — which would flatten every repository that
/// is not the busiest one — would say nothing.
const spark = computed(() => {
  const buckets = summary.value?.activity ?? [];
  const peak = Math.max(1, ...buckets);
  const recent = buckets.length - 3;
  return buckets.map((count, at) => ({
    height: count === 0 ? 2 : Math.max(3, Math.round((count / peak) * 30)),
    // The last three buckets are the ones being asked about.
    kind: count === 0 ? "none" : at >= recent ? "now" : "past",
  }));
});
</script>

<template>
  <section class="card">
    <p v-if="!row" class="pane-empty">{{ t("card.none") }}</p>

    <template v-else>
      <div class="card-head">
        <div class="card-identity">
          <div class="card-title">
            <span class="card-name">{{ row.name }}</span>
            <span v-if="changed" class="chip" :class="changed.kind">
              <span class="chip-dot" />{{ changed.text }}
            </span>
            <span v-if="divergence" class="chip mono">{{ divergence }}</span>
          </div>
          <span v-if="row.description" class="card-note mono">{{ row.description }}</span>
        </div>

        <span class="pane-head-spacer" />

        <div class="card-side">
          <div class="card-actions">
            <button class="danger" @click="forgetRepository(row)">{{ t("card.forget") }}</button>
            <button class="primary" :disabled="row.missing" @click="openRepository(row.path)">
              {{ t("card.open") }}<span class="hint">⏎</span>
            </button>
          </div>
          <div v-if="summary && summary.commits > 0" class="card-activity">
            <span class="card-window">{{ t("card.days") }}</span>
            <span class="card-spark">
              <span
                v-for="(bar, at) in spark"
                :key="at"
                class="spark-bar"
                :class="bar.kind"
                :style="{ height: `${bar.height}px` }"
              />
            </span>
            <span class="mono dim">{{ summary.commits }} commits</span>
          </div>
        </div>
      </div>

      <!-- DESIGN §4: a repository that has moved says so and keeps its entry.
           It is never removed on the app's own initiative. -->
      <p v-if="row.missing" class="card-missing">
        {{ t("card.missing") }}
      </p>
      <p v-else-if="failure" class="pane-error">{{ failure }}</p>

      <div v-else-if="summary" class="card-body">
        <div class="card-section">{{ t("card.repository") }}</div>
        <dl class="card-facts">
          <dt>{{ t("card.folder") }}</dt>
          <dd>
            <select
              class="card-folder"
              :value="row.group"
              :title="t('card.folderTitle')"
              @change="fileRepository(row.path, Number(($event.target as HTMLSelectElement).value))"
            >
              <option v-for="folder in folders" :key="folder.index" :value="folder.index">
                {{ folder.name }}
              </option>
            </select>
          </dd>
          <dt>{{ t("card.location") }}</dt>
          <dd class="mono">{{ tildify(row.path) }}</dd>
          <dt>{{ t("card.lastOpened") }}</dt>
          <dd class="mono">
            <span v-if="row.last_opened" :title="exact(row.last_opened)">
              {{ when(row.last_opened) }}
            </span>
            <span v-else class="dim">{{ t("card.never") }}</span>
          </dd>
          <template v-if="summary.last_commit">
            <dt>{{ t("card.lastCommit") }}</dt>
            <dd class="card-commit">
              <span class="mono dim">{{ summary.last_commit.id.short }}</span>
              <span class="mono card-commit-summary">{{ summary.last_commit.summary }}</span>
              <span class="mono dim" :title="exact(summary.last_commit.when)">
                {{ when(summary.last_commit.when) }}
              </span>
            </dd>
          </template>
          <template v-if="row.description">
            <dt>{{ t("card.description") }}</dt>
            <dd>{{ row.description }}</dd>
          </template>
          <template v-if="summary.committer">
            <dt>{{ t("card.identity") }}</dt>
            <dd class="card-who">
              <span class="avatar mono">{{ summary.committer.initials }}</span>
              <span>{{ summary.committer.name }}</span>
              <span class="mono dim">&lt;{{ summary.committer.email }}&gt;</span>
              <!-- A per-repository identity is a deliberate act, so an
                   inherited one is worth saying out loud. -->
              <span v-if="summary.committer.inherited" class="dim">{{ t("card.inherited") }}</span>
            </dd>
          </template>
        </dl>

        <div class="card-section">{{ t("card.workingCopy") }}</div>
        <dl class="card-facts">
          <dt>{{ t("card.branch") }}</dt>
          <dd class="card-branch">
            <span class="mono">{{ summary.operation ?? summary.head }}</span>
            <span v-if="summary.tracking" class="ref remote">{{ summary.tracking.upstream }}</span>
            <span v-if="summary.tracking?.gone" class="mono dim">{{ t("card.gone") }}</span>
            <span v-else-if="divergence" class="mono dim">{{ divergence }}</span>
            <span v-else-if="summary.tracking" class="mono dim">{{ t("library.upToDate") }}</span>
          </dd>
          <dt>{{ t("card.status") }}</dt>
          <dd class="card-status mono">
            <span v-if="counted.length === 0" class="dim">{{ t("library.clean") }}</span>
            <span v-for="entry in counted" :key="entry.label" :class="entry.kind">
              {{ entry.n }} {{ entry.label }}{{ entry.n > 1 ? "s" : "" }}
            </span>
          </dd>
          <dt>{{ t("card.stashes") }}</dt>
          <dd class="mono">{{ summary.stashes }}</dd>
        </dl>

        <template v-if="summary.remotes.length > 0">
          <div class="card-section">{{ t("card.remotes") }}</div>
          <dl class="card-facts">
            <template v-for="remote in summary.remotes" :key="remote.name">
              <dt class="mono">{{ remote.name }}</dt>
              <dd class="mono">{{ remote.url ?? t("card.noRemoteUrl") }}</dd>
            </template>
          </dl>
        </template>
      </div>

      <footer class="card-foot mono">
        <span>{{ count("library.repositories", app.repositories.length) }}</span>
        <span class="rule">│</span>
        <span class="dim">{{ t("card.openHint") }}</span>
        <span class="pane-head-spacer" />
        <span v-if="app.repositories.some((entry) => entry.missing)" class="gone">
          ! {{ app.repositories.filter((entry) => entry.missing).length }} introuvable
        </span>
      </footer>
    </template>
  </section>
</template>

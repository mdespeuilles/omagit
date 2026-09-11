<script setup lang="ts">
// One commit: what it says, who made it, and what it changed.

import { computed } from "vue";
import { authored, exact, when } from "../format";
import {
  app,
  goToZone,
  paneArranged,
  paneWidth,
  selectCommitFile,
  selectCompareFile,
  stopComparing,
  zoneActive,
} from "../state";
import { count, t } from "../i18n";
import Splitter from "./Splitter.vue";

const detail = computed(() => (app.commit.status === "ready" ? app.commit.value : null));
const comparison = computed(() => (app.compare.status === "ready" ? app.compare.value : null));

/// A comparison takes the pane over while it is on: it answers a different
/// question from the one below it, and showing both would leave the file list
/// ambiguous about which of the two it belongs to.
const comparing = computed(() => app.compare.status !== "idle");

function sign(row: { added: number; removed: number; reason: string | null }): string {
  return row.reason ?? `+${row.added} −${row.removed}`;
}

// ── The two edges this panel can be dragged by ─────────────────────────────
//
// Board 06's right-hand side is three stacked zones — the commit, its files,
// and the diff — and which of them deserves the room depends entirely on the
// commit you are looking at: a merge with a paragraph of message, a rename
// across forty files, one line changed in one file. So both boundaries move.
//
// Sized by the stylesheet until somebody drags one, and from then on by what
// they dragged it to. `max-height` stays a percentage in the CSS, so a stored
// height cannot crush the pane below it on a shorter window — which is the
// complaint the branch list already made once.
const messageHeight = computed(() => paneWidth("detail-message", 0));
const panelHeight = computed(() => paneWidth("detail", 0));
const messageStyle = computed(() =>
  paneArranged("detail-message") ? { flex: "none", height: `${messageHeight.value}px` } : {},
);
const panelStyle = computed(() =>
  paneArranged("detail") ? { flex: "none", height: `${panelHeight.value}px` } : {},
);
</script>

<template>
  <section class="detail" :class="{ arranged: paneArranged('detail') }" :style="panelStyle">
    <header class="pane-head">
      <span>{{ comparing ? t("commitDetail.comparison") : t("commitDetail.title") }}</span>
      <span v-if="comparing && comparison" class="pane-head-title mono">
        {{ comparison.from.short }} ↔ {{ comparison.to.short }}
      </span>
      <span v-else-if="detail" class="pane-head-title mono">{{ detail.id.short }}</span>
    </header>

    <template v-if="comparing">
      <p v-if="app.compare.status === 'loading'" class="pane-empty">
        {{ t("commitDetail.readingComparison") }}
      </p>
      <p v-else-if="app.compare.status === 'failed'" class="pane-error mono">
        {{ app.compare.error }}
      </p>
      <template v-else-if="comparison">
        <div
          class="detail-head"
          :class="{ arranged: paneArranged('detail-message') }"
          :style="messageStyle"
        >
          <p class="detail-summary mono">{{ comparison.from.short }} ↔ {{ comparison.to.short }}</p>
          <p class="detail-who">
            <span class="dim">
              {{ count("commitDetail.between", comparison.files.length) }}
            </span>
            <button class="link" @click="stopComparing()">
              {{ t("commitDetail.stopComparing") }}
            </button>
          </p>
        </div>
        <Splitter
          pane="detail-message"
          sizes="height"
          :size="messageHeight || 120"
          :min="48"
          :max="600"
        />

        <header class="pane-head">
          <span>{{ t("commitDetail.files") }}</span>
          <span class="pane-head-count">{{ comparison.files.length }}</span>
        </header>
        <p v-if="comparison.files.length === 0" class="pane-empty">
          {{ t("commitDetail.same") }}
        </p>
        <ol v-else class="detail-files">
          <li
            v-for="file in comparison.files"
            :key="file.path"
            class="file-row"
            :class="{
              selected: app.commitFile === file.path,
              focused: app.commitFile === file.path && zoneActive(3),
            }"
            @click="
              goToZone(3);
              selectCompareFile(file.path);
            "
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
    </template>

    <p v-else-if="app.commit.status === 'idle'" class="pane-empty">{{ t("commitDetail.none") }}</p>
    <p v-else-if="app.commit.status === 'loading'" class="pane-empty">
      {{ t("commitDetail.reading") }}
    </p>
    <p v-else-if="app.commit.status === 'failed'" class="pane-error mono">{{ app.commit.error }}</p>

    <template v-else-if="detail">
      <div
        class="detail-head"
        :class="{ arranged: paneArranged('detail-message') }"
        :style="messageStyle"
      >
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
          <span class="dim">{{ t("commitDetail.committedBy") }}</span>
          <span class="mono">{{ detail.committer.name }}</span>
          <span class="dim" :title="exact(detail.committer.when)">
            {{ when(detail.committer.when) }}
          </span>
        </p>

        <p class="detail-parents mono">
          <span class="dim">{{
            detail.parents.length > 1 ? t("commitDetail.parents") : t("commitDetail.parent")
          }}</span>
          <span v-if="detail.parents.length === 0" class="dim">{{ t("commitDetail.root") }}</span>
          <span v-for="parent in detail.parents" :key="parent.full">{{ parent.short }}</span>
        </p>
      </div>
      <Splitter
        pane="detail-message"
        sizes="height"
        :size="messageHeight || 120"
        :min="48"
        :max="600"
      />

      <header class="pane-head">
        <span>{{ t("commitDetail.files") }}</span>
        <span class="pane-head-count">{{ detail.files.length }}</span>
      </header>
      <p v-if="detail.files.length === 0" class="pane-empty">{{ t("commitDetail.noFiles") }}</p>
      <ol v-else class="detail-files">
        <li
          v-for="file in detail.files"
          :key="file.path"
          class="file-row"
          :class="{
            selected: app.commitFile === file.path,
            focused: app.commitFile === file.path && zoneActive(3),
          }"
          @click="
            goToZone(3);
            selectCommitFile(file.path);
          "
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
    <Splitter pane="detail" sizes="height" :size="panelHeight || 240" :min="80" :max="900" />
  </section>
</template>

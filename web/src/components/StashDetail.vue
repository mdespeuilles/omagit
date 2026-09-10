<script setup lang="ts">
// What one stash holds, above its diff.
//
// The same shape as the commit detail, because a stash *is* a commit — with one
// thing said out loud that a commit never has to say: which side of the index
// its files came from stops mattering here, and whether it carries files that
// were never tracked starts to.

import { computed } from "vue";
import { app, goToZone, selectStashFile, shown, zoneActive } from "../state";
import { t } from "../i18n";
import { when } from "../format";

const entry = computed(() => shown(app.stashes)?.find((row) => row.id.full === app.stash) ?? null);
const files = computed(() => (app.stashFiles.status === "ready" ? app.stashFiles.value : []));

function sign(row: { added: number; removed: number; reason: string | null }): string {
  return row.reason ?? `+${row.added} −${row.removed}`;
}
</script>

<template>
  <section class="detail">
    <header class="pane-head">
      <span>{{ t("stash.one") }}</span>
      <span v-if="entry" class="pane-head-title mono">{{ entry.id.short }}</span>
    </header>

    <p v-if="!entry" class="pane-empty">{{ t("stash.none") }}</p>

    <template v-else>
      <div class="detail-head">
        <p class="detail-summary">{{ entry.message }}</p>
        <p class="detail-who">
          <span class="dim">{{ t("stash.from") }}</span>
          <span class="mono">{{ entry.branch ?? t("stash.detached") }}</span>
          <span class="dim">{{ when(entry.when) }}</span>
        </p>
        <!-- Said here and not only as a mark on the row: these files are in no
             commit and no index anywhere else, so a preview that showed them
             without saying where they come from would be the same trap the
             other way round. -->
        <p v-if="entry.untracked" class="detail-who">
          <span class="dim">{{ t("stash.holdsUntracked") }}</span>
        </p>
      </div>

      <header class="pane-head">
        <span>{{ t("stash.files") }}</span>
        <span class="pane-head-count">{{ files.length }}</span>
      </header>

      <p v-if="app.stashFiles.status === 'loading'" class="pane-empty">
        {{ t("stash.readingOne") }}
      </p>
      <p v-else-if="app.stashFiles.status === 'failed'" class="pane-error mono">
        {{ app.stashFiles.error }}
      </p>
      <p v-else-if="files.length === 0" class="pane-empty">{{ t("stash.noFiles") }}</p>
      <ol v-else class="detail-files">
        <li
          v-for="file in files"
          :key="file.path"
          class="file-row"
          :class="{
            selected: app.stashFile === file.path,
            focused: app.stashFile === file.path && zoneActive(3),
          }"
          @click="
            goToZone(3);
            selectStashFile(file.path);
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
  </section>
</template>

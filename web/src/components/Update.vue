<script setup lang="ts">
// A newer build, offered — and never taken (SPEC §11, M10).
//
// The band of `Notice.vue` rather than a dialog, and for the same reason: what
// somebody wants on being told a new version exists is to go on with what they
// were doing. A modal would interrupt a commit message to announce something
// nobody asked about, and the one modal shape this app has is reserved for
// SPEC §3 rule 7's questions — the ones asked before something is lost.
//
// It is the accent colour and not the danger one. `Notice.vue` is red because
// a write did not happen; nothing has gone wrong here.
//
// Three states in one band, because they are one sentence being finished: the
// offer, the download, and the build waiting to be started. The fourth is the
// failure, which keeps the version in the text — "omagit 0.2.0 could not be
// installed" says what was being attempted, and the row underneath says what
// the updater said about it, verbatim, the same rule the journal follows.

import { computed } from "vue";
import { app, dismissUpdate, installUpdate, restartForUpdate } from "../state";
import { t } from "../i18n";

const update = computed(() => app.update);
const version = computed(() => ({ version: update.value?.offer.version ?? "" }));

// The download is the only state with a bar, and it runs indeterminate until
// the server says how large the file is — the same decision the fetch overlay
// makes, for the same reason: a bar frozen at zero reads as a stall.
const percent = computed(() => update.value?.percent ?? null);
</script>

<template>
  <aside v-if="update" class="update" role="status" aria-live="polite">
    <span class="update-mark" aria-hidden="true">↑</span>

    <span class="update-body">
      <span class="update-what">
        <template v-if="update.phase === 'downloading'">
          {{ t("update.downloading", version) }}
        </template>
        <template v-else-if="update.phase === 'ready'">{{ t("update.ready", version) }}</template>
        <template v-else-if="update.phase === 'failed'">{{ t("update.failed", version) }}</template>
        <template v-else>{{ t("update.available", version) }}</template>
      </span>

      <!-- What the updater said, in its own words and never re-worded (SPEC §3
           rule 3) — the same treatment `Notice.vue` gives Git's refusals. -->
      <span v-if="update.said" class="update-said mono">{{ update.said }}</span>

      <span
        v-if="update.phase === 'downloading'"
        class="progress-track"
        :class="{ indeterminate: percent === null }"
      >
        <span
          class="progress-fill"
          :style="percent === null ? undefined : { width: `${percent}%` }"
        />
      </span>
    </span>

    <span v-if="update.phase === 'downloading' && percent !== null" class="update-percent mono">
      {{ percent }}%
    </span>

    <button v-if="update.phase === 'ready'" class="link" @click="restartForUpdate()">
      {{ t("update.restart") }}
    </button>
    <button v-else-if="update.phase !== 'downloading'" class="link" @click="installUpdate()">
      {{ update.phase === "failed" ? t("update.retry") : t("update.install") }}
    </button>

    <!-- Absent while the download runs and once the build is on disk: there is
         nothing to skip any more, and a button that took the band away would
         leave the new version installed with no way back to "Restart". -->
    <button
      v-if="update.phase === 'offered' || update.phase === 'failed'"
      class="icon"
      :title="t('update.skip')"
      :aria-label="t('update.skip')"
      @click="dismissUpdate()"
    >
      ✕
    </button>
  </aside>
</template>

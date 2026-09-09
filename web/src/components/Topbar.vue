<script setup lang="ts">
// Board 02's topbar. Its content and order never change between platforms;
// only the edge reserves do, and a reserve is a spacer rather than a padding
// (DESIGN-TOKENS §9) so nothing else shifts when the platform does.

import { computed } from "vue";
import { app } from "../state";

const crumb = computed(() =>
  app.screen === "history" ? "Historique" : app.open ? "Working Copy" : "Dépôts",
);
</script>

<template>
  <header class="topbar">
    <span
      v-if="app.platform && app.platform.reserve.leading > 0"
      class="reserve"
      :style="{ width: `${app.platform.reserve.leading}px` }"
    />
    <span class="topbar-name mono">omagit</span>
    <template v-if="app.summary">
      <span class="topbar-sep">│</span>
      <span class="topbar-repo">{{ app.summary.name }}</span>
      <span class="topbar-head mono">⑂ {{ app.summary.head }}</span>
    </template>
    <span class="topbar-sep">│</span>
    <span class="topbar-crumb">{{ crumb }}</span>

    <!-- Before anything is typed rather than after (SPEC §8): an app that only
         says `git` is missing when Commit is pressed has wasted the message. -->
    <span v-if="app.gitUnusable" class="banner danger">
      git indisponible — {{ app.gitUnusable }}
    </span>

    <span
      v-if="app.platform && app.platform.reserve.trailing > 0"
      class="reserve"
      :style="{ width: `${app.platform.reserve.trailing}px` }"
    />
  </header>
</template>

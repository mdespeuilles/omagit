<script setup lang="ts">
// The cancellable progress overlay of SPEC §11.
//
// A bar at the foot of the window rather than a modal over it. A fetch does not
// stop you reading the diff you were reading, and a dialog that blocked the
// window for two minutes of network would be the app hanging with extra steps.
//
// The percentage is often absent, and the bar says so by running indeterminate
// rather than sitting at zero — `git` spends the first part of a fetch counting
// with no total to count against, and a bar frozen at 0% reads as a stall.

import { computed } from "vue";
import { app, stopNetwork } from "../state";

const running = computed(() => app.running);
const modifier = computed(() => app.platform?.modifier_label ?? "Ctrl");
</script>

<template>
  <div v-if="running" class="progress" role="status" aria-live="polite">
    <span class="progress-what">{{ running.what }}</span>
    <span class="progress-phase mono">{{ app.stopping ? "arrêt…" : running.phase }}</span>

    <span class="progress-track" :class="{ indeterminate: running.percent === null }">
      <span
        class="progress-fill"
        :style="running.percent === null ? undefined : { width: `${running.percent}%` }"
      />
    </span>

    <span v-if="running.percent !== null" class="progress-percent mono">
      {{ running.percent }}%
    </span>

    <button class="link danger" :disabled="app.stopping" @click="stopNetwork()">
      Annuler<span class="hint">{{ modifier }}.</span>
    </button>
  </div>
</template>

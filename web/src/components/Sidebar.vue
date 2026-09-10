<script setup lang="ts">
// Board 03's sidebar: which view of this repository, then its branches.
//
// It belongs to the shell rather than to a screen, and that is the whole point.
// In the GPUI build History replaced the window and left no way back, because
// the chrome belonged to whichever screen had drawn it.
//
// Every row here is a screen now. Board 03 drew the later ones dimmed and
// labelled while they waited, which was right: an entry that will exist reads
// better as not-yet than as absent, and a sidebar that filled itself in over
// six milestones would move under the reader every time. None is waiting any
// more.

import { computed } from "vue";
import { app, changedCount, showScreen } from "../state";
import BranchTree from "./BranchTree.vue";

const changes = computed(() => (app.open ? changedCount() : 0));
const conflicts = computed(() => app.summary?.counts.conflicted ?? 0);
const stashes = computed(() => app.summary?.stashes ?? 0);
</script>

<template>
  <nav class="sidebar">
    <div class="group-head"><span class="chevron">▾</span><span>Workspace</span></div>

    <button
      class="row sidebar-row"
      :class="{ selected: app.screen === 'working-copy' }"
      @click="showScreen('working-copy')"
    >
      <span class="sidebar-glyph mono">◱</span>
      <span>Working Copy</span>
      <span class="pane-head-spacer" />
      <span v-if="conflicts > 0" class="sidebar-count conflict mono">{{ conflicts }} ⚠</span>
      <span v-else-if="changes > 0" class="sidebar-count mono">{{ changes }}</span>
    </button>

    <button
      class="row sidebar-row"
      :class="{ selected: app.screen === 'history' }"
      @click="showScreen('history')"
    >
      <span class="sidebar-glyph mono">⌸</span>
      <span>History</span>
    </button>

    <button
      class="row sidebar-row"
      :class="{ selected: app.screen === 'stashes' }"
      @click="showScreen('stashes')"
    >
      <span class="sidebar-glyph mono">⌥</span>
      <span>Stashes</span>
      <span class="pane-head-spacer" />
      <span v-if="stashes > 0" class="sidebar-count mono">{{ stashes }}</span>
    </button>

    <button
      class="row sidebar-row"
      :class="{ selected: app.screen === 'settings' }"
      @click="showScreen('settings')"
    >
      <span class="sidebar-glyph mono">⚙</span>
      <span>Réglages</span>
    </button>

    <BranchTree />

    <span class="pane-head-spacer" />

    <button class="sidebar-foot" @click="showScreen('repositories')">
      <span class="mono">◧</span><span>Tous les dépôts</span>
    </button>
  </nav>
</template>

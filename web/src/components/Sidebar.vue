<script setup lang="ts">
// Board 03's sidebar: which view of this repository, then its branches.
//
// It belongs to the shell rather than to a screen, and that is the whole point.
// In the GPUI build History replaced the window and left no way back, because
// the chrome belonged to whichever screen had drawn it.
//
// Everything below Workspace is a later milestone. Board 03 draws those rows
// anyway, dimmed and labelled, and it is right to: an entry that will exist
// reads better as not-yet than as absent, and a sidebar that filled itself in
// over six milestones would move under the reader every time.

import { computed } from "vue";
import { app, showScreen, stagedCount, unstagedCount } from "../state";
import BranchTree from "./BranchTree.vue";

const changes = computed(() => (app.open ? unstagedCount() + stagedCount() : 0));
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

    <div class="row sidebar-row deferred" title="Jalon M8">
      <span class="sidebar-glyph mono">⌥</span>
      <span>Stashes</span>
      <span class="pane-head-spacer" />
      <span v-if="stashes > 0" class="sidebar-count mono">{{ stashes }}</span>
      <span class="tag">M8</span>
    </div>

    <div class="row sidebar-row deferred" title="Jalon M9">
      <span class="sidebar-glyph mono">⚙</span>
      <span>Settings</span>
      <span class="pane-head-spacer" />
      <span class="tag">M9</span>
    </div>

    <BranchTree />

    <span class="pane-head-spacer" />

    <button class="sidebar-foot" @click="showScreen('repositories')">
      <span class="mono">◧</span><span>Tous les dépôts</span>
    </button>
  </nav>
</template>

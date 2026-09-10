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
import { app, changedCount, goToZone, isCollapsed, showScreen, toggleBranchGroup } from "../state";
import Glyph from "./Glyph.vue";
import BranchTree from "./BranchTree.vue";

const changes = computed(() => (app.open ? changedCount() : 0));
const conflicts = computed(() => app.summary?.counts.conflicted ?? 0);
const stashes = computed(() => app.summary?.stashes ?? 0);
</script>

<template>
  <!-- The whole sidebar is one stop, and `j` `k` walk the branches inside it
       (board 09, stop 5). The three screen buttons above them keep their own
       stops: nothing else reaches them, and `⌘1` `⌘2` `⌘3` is a shortcut, not a
       path for someone crossing the window with Tab. -->
  <nav class="sidebar" tabindex="0" data-zone="1" @focus="goToZone(1)">
    <!-- It folds, because it draws a chevron. Four rows is not much to gain —
         but a repository with forty branches under them makes it worth having,
         and a triangle that answers nothing is worse than no triangle. -->
    <button class="group-head as-button" @click="toggleBranchGroup('workspace')">
      <Glyph :name="isCollapsed('workspace') ? 'chevron-right' : 'chevron-down'" class="chevron" />
      <span>Workspace</span>
    </button>

    <template v-if="!isCollapsed('workspace')">
      <button
        class="row sidebar-row"
        :class="{ selected: app.screen === 'working-copy' }"
        @click="showScreen('working-copy')"
      >
        <Glyph name="working-copy" class="sidebar-glyph" />
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
        <Glyph name="history" class="sidebar-glyph" />
        <span>History</span>
      </button>

      <button
        class="row sidebar-row"
        :class="{ selected: app.screen === 'stashes' }"
        @click="showScreen('stashes')"
      >
        <Glyph name="stashes" class="sidebar-glyph" />
        <span>Stashes</span>
        <span class="pane-head-spacer" />
        <span v-if="stashes > 0" class="sidebar-count mono">{{ stashes }}</span>
      </button>

      <button
        class="row sidebar-row"
        :class="{ selected: app.screen === 'settings' }"
        @click="showScreen('settings')"
      >
        <Glyph name="settings" class="sidebar-glyph" />
        <span>Réglages</span>
      </button>
    </template>

    <BranchTree />

    <span class="pane-head-spacer" />

    <button class="sidebar-foot" @click="showScreen('repositories')">
      <Glyph name="panel" class="sidebar-glyph" /><span>Tous les dépôts</span>
    </button>
  </nav>
</template>

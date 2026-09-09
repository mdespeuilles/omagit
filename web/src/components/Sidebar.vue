<script setup lang="ts">
// Board 03's sidebar: which view of this repository, then which repository.
//
// The switcher lives here rather than in a screen, and that is the whole point.
// In the GPUI build History replaced the window and left no way back, because
// the chrome belonged to whichever screen had drawn it. The shell owns it now,
// so no screen can take it away.

import { computed } from "vue";
import { app, openRepository, showScreen, stagedCount, unstagedCount } from "../state";

const changes = computed(() => (app.open ? unstagedCount() + stagedCount() : 0));
</script>

<template>
  <nav class="sidebar">
    <template v-if="app.open">
      <header class="pane-head">Workspace</header>
      <button
        class="sidebar-row"
        :class="{ selected: app.screen === 'working-copy' }"
        @click="showScreen('working-copy')"
      >
        <span class="sidebar-glyph">◱</span>
        Working Copy
        <span v-if="changes > 0" class="sidebar-count">{{ changes }}</span>
      </button>
      <button
        class="sidebar-row"
        :class="{ selected: app.screen === 'history' }"
        @click="showScreen('history')"
      >
        <span class="sidebar-glyph">⌸</span>
        History
      </button>
    </template>

    <header class="pane-head">
      <span>Dépôts</span>
      <span class="pane-head-spacer" />
      <button class="link" @click="showScreen('repositories')">Tous</button>
    </header>
    <button
      v-for="row in app.repositories"
      :key="row.path"
      class="sidebar-row"
      :class="{ selected: app.open === row.path }"
      @click="openRepository(row.path)"
    >
      <span class="sidebar-glyph">⑂</span>
      {{ row.name || row.path }}
    </button>
    <p v-if="app.repositories.length === 0" class="pane-empty">Aucun dépôt</p>
  </nav>
</template>

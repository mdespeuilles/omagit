<script setup lang="ts">
// The repositories the window has open, one click apart.
//
// Board 02 draws no tab strip: the topbar names the repository you are in, and
// the way to another was the Dépôts screen. That is a detour for the thing
// people do most — moving between two or three repositories they are working in
// at the same time — so this is a deliberate addition to the board, asked for
// and recorded in DESIGN §6.
//
// A row of its own rather than the topbar's empty middle: that middle is the
// window's drag region on macOS, and tabs filling it would leave nowhere to
// take hold of the window.

import { app, closeTab, openRepository } from "../state";
import Glyph from "./Glyph.vue";
</script>

<template>
  <nav v-if="app.tabs.length > 0" class="tabbar">
    <!-- A `<div>` holding a `<button>`, not a button inside a button. -->
    <div
      v-for="tab in app.tabs"
      :key="tab.path"
      class="tabbar-item"
      :class="{ on: tab.path === app.open }"
      :title="tab.path"
      @click="openRepository(tab.path)"
      @auxclick.middle.prevent="closeTab(tab.path)"
    >
      <span class="tabbar-name">{{ tab.name }}</span>
      <!-- In the tab order only for the tab you are on, the same rule as a
           row's own actions (board 09): the strip is a handful of stops, not
           two per repository ever opened. -->
      <button
        class="tabbar-close"
        :tabindex="tab.path === app.open ? 0 : -1"
        :title="`Fermer ${tab.name}`"
        :aria-label="`Fermer ${tab.name}`"
        @click.stop="closeTab(tab.path)"
      >
        <Glyph name="close" />
      </button>
    </div>
  </nav>
</template>

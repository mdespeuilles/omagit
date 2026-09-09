<script setup lang="ts">
// The repository list. The chrome belongs to the shell rather than to a screen
// — the mistake the GPUI build made, where opening History left a window with
// no way out of it.

import { app, openRepository } from "../state";
</script>

<template>
  <nav class="sidebar">
    <button
      v-for="row in app.repositories"
      :key="row.path"
      class="sidebar-row"
      :class="{ selected: app.open === row.path }"
      @click="openRepository(row.path)"
    >
      {{ row.name || row.path }}
    </button>
    <p v-if="app.repositories.length === 0" class="pane-empty">Aucun dépôt</p>
  </nav>
</template>

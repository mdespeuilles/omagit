<script setup lang="ts">
// The `?` sheet: every binding the window answers, printed from the table that
// answers them.
//
// Not a written list. A sheet maintained by hand is a sheet that is wrong by
// the second slice — and this project has the receipt: `KEYMAP.md` spent three
// milestones describing a build that had been deleted. Reading the table has a
// second effect worth having: printing it is what found `Tab`, documented since
// M3 and bound to nothing.

import { onBeforeUnmount, onMounted } from "vue";
import { app, closeShortcuts } from "../state";
import { ACTIONS, MOVEMENTS, binding, hint } from "../keymap";

const modifier = app.platform?.modifier_label ?? "Ctrl";

function onKey(event: KeyboardEvent): void {
  if (event.key === "Escape" || event.key === "?") {
    event.preventDefault();
    closeShortcuts();
  }
}

onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div v-if="app.shortcuts" class="overlay" @click.self="closeShortcuts()">
    <section class="dialog sheet" role="dialog" aria-modal="true" aria-label="Raccourcis clavier">
      <header class="dialog-head">
        <span class="dialog-title">Raccourcis</span>
        <span class="pane-head-spacer" />
        <button class="link" @click="closeShortcuts()">Esc</button>
      </header>

      <div class="sheet-body">
        <section class="sheet-block">
          <h3 class="settings-subtitle">Commandes</h3>
          <dl class="sheet-list">
            <template v-for="entry in ACTIONS" :key="entry.id">
              <dt class="sheet-key mono">{{ hint(binding(entry), modifier) }}</dt>
              <dd class="sheet-label">{{ entry.label }}</dd>
            </template>
          </dl>
        </section>

        <section class="sheet-block">
          <h3 class="settings-subtitle">Déplacement</h3>
          <!-- Bare keys, so none of them fires while the caret is in a field —
               which is what makes a single letter safe as a binding at all. -->
          <dl class="sheet-list">
            <template v-for="move in MOVEMENTS" :key="move.print">
              <dt class="sheet-key mono">{{ move.print }}</dt>
              <dd class="sheet-label">{{ move.label }}</dd>
            </template>
          </dl>
          <p class="settings-note">
            Aucune de ces touches ne se déclenche pendant que le curseur est dans un champ : c'est
            ce qui rend une lettre seule utilisable comme raccourci.
          </p>
        </section>
      </div>
    </section>
  </div>
</template>

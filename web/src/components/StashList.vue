<script setup lang="ts">
// The shelf: what has been put aside, and the three things one can do with it.
//
// Board 03 draws the row in the sidebar and nothing beyond it, so the screen
// follows the shape the other two already have: a list in the middle column, a
// preview to its right. A stash is a commit, so the preview is the same file
// list and the same diff viewer History uses — with one difference the row
// carries, `+ non suivis`, because a stash made with `--include-untracked`
// holds files that are in no commit anywhere.

import { computed } from "vue";
import type { StashRow } from "../ipc";
import {
  address,
  app,
  closeStashForm,
  dropStash,
  goToZone,
  openStashForm,
  restoreStash,
  selectStash,
  setStashMessage,
  setStashUntracked,
  stashChanges,
  zoneActive,
} from "../state";
import { when } from "../format";

const rows = computed(() => (app.stashes.status === "ready" ? app.stashes.value : []));
const busy = computed(() => !!app.busy || !!app.gitUnusable);

/// What the second line says: where it came from, and when.
///
/// The branch is a plain name and not a link: switching to it is a different
/// act from reading what was put aside on it, and a stash can outlive the
/// branch it was made on.
function origin(row: StashRow): string {
  return [row.branch ?? "HEAD détaché", when(row.when)].join(" · ");
}
</script>

<template>
  <section class="stashes">
    <header class="pane-head">
      <span>Stashes</span>
      <span class="pane-head-count">{{ rows.length }}</span>
      <span class="pane-head-spacer" />
      <button class="link" :disabled="busy || !!app.stashing" @click="openStashForm()">
        Remiser…
      </button>
    </header>

    <!-- The form sits at the top of the list it adds to, rather than in a
         dialog: it is two fields, and the row it creates appears directly
         underneath. -->
    <form v-if="app.stashing" class="stash-new" @submit.prevent="stashChanges()">
      <input
        :value="app.stashing.message"
        type="text"
        placeholder="Message (facultatif)"
        spellcheck="false"
        autofocus
        @input="setStashMessage(($event.target as HTMLInputElement).value)"
        @keydown.esc="closeStashForm()"
      />
      <label class="toggle">
        <input
          type="checkbox"
          :checked="app.stashing.untracked"
          @change="setStashUntracked(($event.target as HTMLInputElement).checked)"
        /><span class="check" aria-hidden="true">✓</span>
        <!-- Named for what it does to the disk, not for the flag: these files
             are in no index and no commit, and the entry about to be created is
             the only copy of them. -->
        <span>Emporter aussi les fichiers non suivis</span>
      </label>
      <div class="stash-new-actions">
        <button type="button" @click="closeStashForm()">Annuler</button>
        <button class="primary" type="submit" :disabled="busy">Remiser</button>
      </div>
    </form>

    <p v-if="app.stashes.status === 'loading'" class="pane-empty">Lecture des remises…</p>
    <p v-else-if="app.stashes.status === 'failed'" class="pane-error mono">
      {{ app.stashes.error }}
    </p>
    <p v-else-if="rows.length === 0" class="pane-empty">
      Rien de remisé. « Remiser » met la copie de travail de côté et laisse la branche propre.
    </p>

    <ol v-else class="stash-rows">
      <!-- The row is a button, and its three actions are buttons *beside* it
           rather than inside it: a `<button>` in a `<button>` is invalid. So
           the `<li>` is the row — it carries hover and selected — and the
           button inside it stays transparent. Elsewhere the row itself is the
           `<div>` and the actions sit inside it; either shape is valid, and
           this one is what a list of `<li>` wanted. -->
      <li
        v-for="row in rows"
        :key="row.id.full"
        class="stash-entry"
        :class="{
          selected: app.stash === row.id.full,
          focused: app.stash === row.id.full && zoneActive(2),
        }"
      >
        <button
          class="row stash-row"
          @click="
            goToZone(2);
            selectStash(row.id.full);
          "
        >
          <span class="stash-lines">
            <span class="stash-line">
              <span class="stash-address mono">{{ address(row) }}</span>
              <span class="stash-message">{{ row.message }}</span>
            </span>
            <span class="stash-line">
              <span class="stash-origin">{{ origin(row) }}</span>
              <span v-if="row.untracked" class="stash-note">+ non suivis</span>
            </span>
          </span>
        </button>

        <span class="row-actions">
          <button
            class="row-action"
            :disabled="busy"
            title="Remettre ces modifications dans la copie de travail. La remise reste sur l'étagère."
            @click="restoreStash(row, true)"
          >
            Appliquer
          </button>
          <button
            class="row-action"
            :disabled="busy"
            title="Remettre ces modifications dans la copie de travail, puis retirer la remise de l'étagère."
            @click="restoreStash(row, false)"
          >
            Appliquer et retirer
          </button>
          <button
            class="row-action danger"
            :disabled="busy"
            title="Jeter la remise sans l'appliquer. Son contenu ne sera plus joignable que par le reflog."
            @click="dropStash(row)"
          >
            Supprimer
          </button>
        </span>
      </li>
    </ol>
  </section>
</template>

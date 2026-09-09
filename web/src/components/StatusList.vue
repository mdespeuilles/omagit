<script setup lang="ts">
// The working copy's files, one row each, with the checkbox that stages them.
//
// The checkbox has three states because the repository has three: a file can be
// wholly in the index, wholly out of it, or half in — which is what staging one
// hunk leaves behind. Drawing "half in" as unchecked was the bug that made
// partial staging look as though it had done nothing.

import { computed } from "vue";
import type { StatusRow } from "../ipc";
import {
  app,
  discardFile,
  plural,
  selectFile,
  stagedCount,
  stageEverything,
  stageFile,
  unstagedCount,
} from "../state";
import { rowHeight } from "../metrics";
import VirtualList from "./VirtualList.vue";

const ROW_HEIGHT = rowHeight();

const rows = computed(() => (app.status.status === "ready" ? app.status.value : []));
const staged = computed(() => stagedCount());

type Mark = "none" | "partial" | "all";

function mark(row: StatusRow): Mark {
  if (row.staged === null) return "none";
  return row.unstaged === null ? "all" : "partial";
}

/// Clicking the box stages what is left, and only a wholly staged file
/// unstages. Anything else makes a half-staged file ambiguous: the same click
/// would mean "finish" to one reader and "undo" to another.
function toggle(row: StatusRow): void {
  stageFile(row, mark(row) === "all");
}

/// Which side of the index a row opens on. A file with staged changes opens on
/// them; anything else opens on the working tree.
function open(row: StatusRow): void {
  void selectFile(row.path, row.staged !== null);
}

/// The letter a status badge shows, and the class that colours it.
///
/// Board 01 §06: a letter *and* a colour, never a colour alone — the badge has
/// to survive greyscale, and "which of these is the deletion" is a question a
/// hue cannot answer for everyone.
function badge(row: StatusRow): { letter: string; kind: string } {
  if (row.conflict) return { letter: "U", kind: "conflict" };
  const change = row.unstaged ?? row.staged ?? "";
  const letters: Record<string, string> = {
    modified: "M",
    added: "A",
    deleted: "D",
    renamed: "R",
    copied: "C",
    "type-changed": "T",
    untracked: "?",
    ignored: "!",
  };
  return { letter: letters[change] ?? "•", kind: change };
}

function directory(path: string): string {
  const cut = path.lastIndexOf("/");
  return cut < 0 ? "" : path.slice(0, cut + 1);
}

function name(path: string): string {
  const cut = path.lastIndexOf("/");
  return cut < 0 ? path : path.slice(cut + 1);
}
</script>

<template>
  <section class="files">
    <header class="pane-head">
      <span>Status</span>
      <span class="pane-head-count">{{ rows.length }}</span>
      <span class="pane-head-spacer" />
      <button
        class="link"
        :disabled="!!app.busy || unstagedCount() === 0"
        @click="stageEverything(false)"
      >
        Tout indexer
      </button>
    </header>

    <p v-if="app.status.status === 'loading'" class="pane-empty">Lecture du statut…</p>
    <p v-else-if="app.status.status === 'failed'" class="pane-error mono">
      {{ app.status.error }}
    </p>
    <p v-else-if="app.status.status === 'ready' && rows.length === 0" class="pane-empty">
      Rien de modifié
    </p>

    <VirtualList
      v-else-if="app.status.status === 'ready'"
      :items="rows"
      :row-height="ROW_HEIGHT"
      v-slot="{ item }"
    >
      <div
        class="file-row"
        :class="{
          selected: app.selected?.path === item.path,
          conflict: item.conflict !== null,
        }"
        @click="open(item)"
      >
        <!-- A button rather than an <input type=checkbox>: the third state is
             not something a checkbox can be told to draw, and this one is a
             target with a label rather than a control with a hidden meaning. -->
        <button
          class="check"
          :class="mark(item)"
          :disabled="!!app.busy"
          :title="mark(item) === 'all' ? 'Désindexer ce fichier' : 'Indexer ce fichier'"
          :aria-label="mark(item) === 'all' ? 'Désindexer ce fichier' : 'Indexer ce fichier'"
          @click.stop="toggle(item)"
        >
          {{ mark(item) === "all" ? "✓" : mark(item) === "partial" ? "–" : "" }}
        </button>

        <span class="badge" :class="badge(item).kind" :title="item.code">
          {{ badge(item).letter }}
        </span>
        <span class="file-path mono">
          <span class="dir">{{ directory(item.path) }}</span>
          <span class="name">{{ name(item.path) }}</span>
        </span>

        <!-- Only on the row being pointed at: a discard button on every row is
             a destructive target under every stray click. -->
        <button
          class="row-action danger"
          :disabled="!!app.busy || item.unstaged === null"
          title="Rejeter les modifications non indexées"
          @click.stop="discardFile(item)"
        >
          Rejeter
        </button>
      </div>
    </VirtualList>

    <footer v-if="rows.length > 0" class="files-foot">
      {{ plural(staged, "indexé") }} · {{ unstagedCount() }} non indexé
    </footer>
  </section>
</template>

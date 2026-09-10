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
  goToZone,
  openConflict,
  plural,
  resolveConflict,
  selectFile,
  stagedCount,
  stageEverything,
  stageFile,
  unstagedCount,
  zoneActive,
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

/// What each side's button says, and what its title explains.
///
/// The branch, not the pronoun. `ours` and `theirs` are only honest during a
/// merge: on a replay — a rebase, a cherry-pick — `ours` is the side already in
/// place and `theirs` is the work being replayed, which is usually your own.
/// Board 07 names both by their branch for exactly this reason.
function label(side: "ours" | "theirs"): string {
  const sides = app.sides;
  if (!sides) return side === "ours" ? "la version en place" : "celle qui arrive";
  return side === "ours" ? sides.ours : sides.theirs;
}

function explain(side: "ours" | "theirs"): string {
  const sides = app.sides;
  const what = `Garder la version de ${label(side)} (${side})`;
  if (!sides?.replayed) return what;
  return side === "ours"
    ? `${what} — le côté déjà en place, sur lequel les commits sont rejoués`
    : `${what} — le côté rejoué, celui des commits en cours de replacement`;
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
          focused: app.selected?.path === item.path && zoneActive(2),
          conflict: item.conflict !== null,
        }"
        @click="
          goToZone(2);
          open(item);
        "
      >
        <!-- A button rather than an <input type=checkbox>: the third state is
             not something a checkbox can be told to draw, and this one is a
             target with a label rather than a control with a hidden meaning.
             On a conflicted row it means something else again — `git add` on an
             unmerged path is what marks it resolved — so it says so, rather
             than sending the reader looking for a button that does not exist. -->
        <button
          class="check"
          :class="mark(item)"
          :disabled="!!app.busy"
          :title="
            item.conflict
              ? 'Marquer ce fichier résolu'
              : mark(item) === 'all'
                ? 'Désindexer ce fichier'
                : 'Indexer ce fichier'
          "
          :aria-label="
            item.conflict
              ? 'Marquer ce fichier résolu'
              : mark(item) === 'all'
                ? 'Désindexer ce fichier'
                : 'Indexer ce fichier'
          "
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

        <!-- Over the end of the row rather than beside it: invisible at rest,
             but a flex child takes its width all the same, and three of them
             were squeezing the path the row is about. A conflicted row offers
             the two sides instead of a discard — `git checkout -- <path>`
             refuses an unmerged path, so that button would have been one that
             always fails. -->
        <span class="row-actions">
          <template v-if="item.conflict">
            <!-- The two sides settle the whole file in one click; the dialog is
                 for a file whose conflicts do not all want the same answer. -->
            <button
              class="row-action"
              :disabled="!!app.busy"
              title="Résoudre conflit par conflit"
              @click.stop="openConflict(item.path)"
            >
              Résoudre…
            </button>
            <button
              class="row-action"
              :disabled="!!app.busy"
              :title="explain('ours')"
              @click.stop="resolveConflict(item, 'ours')"
            >
              {{ label("ours") }}
            </button>
            <button
              class="row-action"
              :disabled="!!app.busy"
              :title="explain('theirs')"
              @click.stop="resolveConflict(item, 'theirs')"
            >
              {{ label("theirs") }}
            </button>
          </template>
          <button
            v-else
            class="row-action danger"
            :disabled="!!app.busy || item.unstaged === null"
            title="Rejeter les modifications non indexées"
            @click.stop="discardFile(item)"
          >
            Rejeter
          </button>
        </span>
      </div>
    </VirtualList>

    <footer v-if="rows.length > 0" class="files-foot">
      {{ plural(staged, "indexé") }} · {{ unstagedCount() }} non indexé
    </footer>
  </section>
</template>

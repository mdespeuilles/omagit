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
  resolveConflict,
  selectFile,
  stagedCount,
  stageEverything,
  stageFile,
  unstagedCount,
  zoneActive,
} from "../state";
import { count, t } from "../i18n";
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
  if (!sides) return side === "ours" ? t("conflict.inPlace") : t("conflict.arriving");
  return side === "ours" ? sides.ours : sides.theirs;
}

function explain(side: "ours" | "theirs"): string {
  const sides = app.sides;
  const what = t("conflict.keep", { side: label(side), pronoun: side });
  if (!sides?.replayed) return what;
  return side === "ours"
    ? `${what} — ${t("conflict.oursReplayed")}`
    : `${what} — ${t("conflict.theirsReplayed")}`;
}

function directory(path: string): string {
  const cut = path.lastIndexOf("/");
  return cut < 0 ? "" : path.slice(0, cut + 1);
}

function name(path: string): string {
  const cut = path.lastIndexOf("/");
  return cut < 0 ? path : path.slice(cut + 1);
}

/// Whether a per-row control is in the tab order.
///
/// Board 09: the list is one stop and its rows are walked with `j` and `k`, so
/// only the row the keyboard is on offers its controls to Tab. Not hidden from
/// it entirely — a control reachable by the mouse alone is unreachable — and
/// not all of them at once either, which would be three stops per row.
function stop(path: string): 0 | -1 {
  return app.selected?.path === path ? 0 : -1;
}
</script>

<template>
  <section class="files">
    <header class="pane-head">
      <span>{{ t("status.title") }}</span>
      <span class="pane-head-count">{{ rows.length }}</span>
      <span class="pane-head-spacer" />
      <button
        class="link"
        :disabled="!!app.busy || unstagedCount() === 0"
        @click="stageEverything(false)"
      >
        {{ t("status.stageAll") }}
      </button>
    </header>

    <p v-if="app.status.status === 'loading'" class="pane-empty">{{ t("status.reading") }}</p>
    <p v-else-if="app.status.status === 'failed'" class="pane-error mono">
      {{ app.status.error }}
    </p>
    <p v-else-if="app.status.status === 'ready' && rows.length === 0" class="pane-empty">
      {{ t("status.clean") }}
    </p>

    <!-- One tab stop for the whole list (board 09): Tab enters the zone, `j`
         and `k` move inside it. A list that put every row in the tab order
         would take forty presses to cross. -->
    <VirtualList
      v-else-if="app.status.status === 'ready'"
      :items="rows"
      :row-height="ROW_HEIGHT"
      tabindex="0"
      data-zone="2"
      @focus="goToZone(2)"
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
          :tabindex="stop(item.path)"
          :class="mark(item)"
          :disabled="!!app.busy"
          :title="
            item.conflict
              ? t('status.resolved')
              : mark(item) === 'all'
                ? t('status.unstageFile')
                : t('status.stageFile')
          "
          :aria-label="
            item.conflict
              ? t('status.resolved')
              : mark(item) === 'all'
                ? t('status.unstageFile')
                : t('status.stageFile')
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
        <!-- In the tab order only for the row the keyboard is on (board 09):
             the list is one stop, and the actions of its current row are the
             next. Every row's actions in the order would be a hundred and
             twenty presses to cross a list of forty files. -->
        <span class="row-actions" :data-current="app.selected?.path === item.path">
          <template v-if="item.conflict">
            <!-- The two sides settle the whole file in one click; the dialog is
                 for a file whose conflicts do not all want the same answer. -->
            <button
              class="row-action"
              :tabindex="stop(item.path)"
              :disabled="!!app.busy"
              :title="t('status.resolveTitle')"
              @click.stop="openConflict(item.path)"
            >
              {{ t("status.resolve") }}
            </button>
            <button
              class="row-action"
              :tabindex="stop(item.path)"
              :disabled="!!app.busy"
              :title="explain('ours')"
              @click.stop="resolveConflict(item, 'ours')"
            >
              {{ label("ours") }}
            </button>
            <button
              class="row-action"
              :tabindex="stop(item.path)"
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
            :tabindex="stop(item.path)"
            :disabled="!!app.busy || item.unstaged === null"
            :title="t('status.discardTitle')"
            @click.stop="discardFile(item)"
          >
            {{ t("status.discard") }}
          </button>
        </span>
      </div>
    </VirtualList>

    <footer v-if="rows.length > 0" class="files-foot">
      {{ count("statusbar.staged", staged) }} · {{ count("statusbar.unstaged", unstagedCount()) }}
    </footer>
  </section>
</template>

<script setup lang="ts">
// Board 06's list: 300px, a filter box, groups, and one row per repository.
//
// A row is two lines — the name, then what the repository is *now* — plus a
// status square on the right. The square repeats what the second line says, in
// a form that survives at a glance and in greyscale: reading nine rows of text
// to find the one with conflicts is not glancing.

import { computed, nextTick, ref } from "vue";
import type { LibraryRow } from "../ipc";
import {
  addRepository,
  app,
  createGroup,
  dismissAddError,
  dragRepository,
  dropRepository,
  dropTarget,
  forgetRepository,
  goToZone,
  openClone,
  openRepository,
  removeGroup,
  renameGroup,
  setGroupName,
  setLibraryFilter,
  showCard,
  toggleGroup,
  visibleRepositories,
  zoneActive,
} from "../state";
import { count, t, worded } from "../i18n";
import { nearest, targets, type Measured, type Target } from "../filing";
import Glyph from "./Glyph.vue";

/// What the filter leaves, which is what the empty state has to talk about.
const rows = computed(() => visibleRepositories());

/// The folders, read from the folders and not from the rows.
///
/// Derived from the rows — which is how this was written while nothing could
/// make a folder — a folder with nothing in it does not exist, and the one the
/// user just made disappears the moment it is made.
const groups = computed(() => {
  const filtering = app.libraryFilter.trim() !== "";
  return (
    app.groups
      .map((folder, group) => ({
        group,
        // A group's name is data — renameable — and the default one is a key
        // until somebody renames it.
        name: worded(folder.name),
        collapsed: folder.collapsed,
        rows: rows.value.filter((row) => row.group === group),
      }))
      // Under a filter an empty folder is noise: narrowing is what the box is
      // for. With no filter it is a folder waiting to be filled.
      .filter((folder) => !filtering || folder.rows.length > 0)
  );
});

// ── Dragging a repository into a folder ────────────────────────────────────
//
// Pointer events, not HTML5 drag and drop: `dragDropEnabled` is what makes a
// folder dropped from the Finder arrive as a path, and the same switch turns
// off drag and drop inside the page (ARCHITECTURE §2.53). Where the line goes
// is `filing.ts`, which is where it can be tested — jsdom measures nothing.

const list = ref<HTMLElement | null>(null);
const naming = ref<HTMLInputElement | null>(null);
/// The press that has not yet travelled far enough to be a drag, and the
/// element holding the pointer once it has.
let pressed: { path: string; x: number; y: number; on: HTMLElement } | null = null;
/// Measured once, when the drag starts: the list does not move under it, and
/// measuring on every pointermove would be a layout per pixel.
let places: Target[] = [];

function press(event: PointerEvent, row: LibraryRow): void {
  if (event.button !== 0) return;
  pressed = {
    path: row.path,
    x: event.clientX,
    y: event.clientY,
    on: event.currentTarget as HTMLElement,
  };
}

function drag(event: PointerEvent): void {
  if (!pressed) return;
  if (!app.draggedRepository) {
    // A click is a click until the pointer has travelled: every row here is
    // also a thing you select, and a list where a 1px wobble files a
    // repository somewhere else is a list nobody trusts.
    const far = Math.abs(event.clientY - pressed.y) > 4 || Math.abs(event.clientX - pressed.x) > 4;
    if (!far) return;
    places = measure();
    pressed.on.setPointerCapture(event.pointerId);
    dragRepository(pressed.path);
  }
  dropTarget(nearest(places, event.clientY));
}

function drop(): void {
  pressed = null;
  if (app.draggedRepository) void dropRepository();
}

/// `Esc` while the button is still down, and whatever the browser does when it
/// takes the pointer back.
function cancel(): void {
  pressed = null;
  dragRepository(null);
}

function measure(): Target[] {
  const root = list.value;
  if (!root) return [];
  const measured: Measured[] = [];
  for (const element of root.querySelectorAll<HTMLElement>("[data-group]")) {
    const box = element.getBoundingClientRect();
    if (element.dataset["row"] === undefined) {
      measured.push({ group: Number(element.dataset["group"]), head: box.bottom, rows: [] });
    } else {
      measured[measured.length - 1]?.rows.push({ top: box.top, bottom: box.bottom });
    }
  }
  return targets(measured);
}

/// The 2px line board 06 draws, as a border on the row it would go above —
/// which needs no geometry once the drag has started, and stays put when the
/// list is scrolled.
function lineAbove(group: number, index: number): boolean {
  return app.dropAt?.group === group && app.dropAt.index === index;
}

function lineBelow(group: number, index: number, total: number): boolean {
  return index === total - 1 && app.dropAt?.group === group && app.dropAt.index === total;
}

/// A folder with no rows to draw a line between: the header itself answers.
function lineInside(group: number, total: number): boolean {
  return total === 0 && app.dropAt?.group === group;
}

/// Open a folder's name for typing, and put the caret in it.
async function rename(group: number): Promise<void> {
  renameGroup(group);
  await nextTick();
  naming.value?.select();
}

/// The second line of a row, and the colour it and the square share.
///
/// Board 06 draws five: clean, modified, conflicted, detached, and gone. Each
/// is a different question — "nothing to do", "work in progress", "stuck",
/// "not on a branch", "not here" — and collapsing them into a count would lose
/// the only one that is urgent.
type State = { text: string; kind: "clean" | "changed" | "conflict" | "detached" | "gone" };

function state(row: LibraryRow): State {
  if (row.missing) return { text: t("library.missing"), kind: "gone" };
  const held = app.library[row.path];
  if (!held || held.status === "loading") return { text: "…", kind: "clean" };
  if (held.status === "failed") return { text: held.error, kind: "gone" };
  if (held.status !== "ready") return { text: "", kind: "clean" };

  const { head, head_kind, operation, counts, tracking } = held.value;
  const where = operation ?? head;
  if (counts.conflicted > 0) {
    return {
      text: `${where} · ${count("library.conflicts", counts.conflicted)}`,
      kind: "conflict",
    };
  }

  const changed = counts.modified + counts.added + counts.deleted + counts.renamed;
  // The backend's answer, not a sniff at the label: a branch called
  // `detached-head-fix` was drawn as a detached HEAD.
  const detached = head_kind === "detached";
  const divergence =
    tracking && !tracking.gone
      ? [
          tracking.ahead > 0 ? `↑${tracking.ahead}` : "",
          tracking.behind > 0 ? `↓${tracking.behind}` : "",
        ]
          .filter(Boolean)
          .join(" ")
      : "";

  if (changed + counts.untracked === 0) {
    return {
      text: [where, divergence || t("library.clean")].filter(Boolean).join(" "),
      kind: detached ? "detached" : "clean",
    };
  }
  const what = [
    changed > 0 ? count("library.changed", changed) : "",
    counts.untracked > 0 ? count("library.untracked", counts.untracked) : "",
  ]
    .filter(Boolean)
    .join(" · ");
  return { text: `${where} · ${what}`, kind: detached ? "detached" : "changed" };
}
</script>

<template>
  <nav class="library" :class="{ dropping: app.dragging }">
    <div class="library-filter">
      <input
        type="search"
        :placeholder="t('library.filter')"
        :title="t('library.filterTitle')"
        spellcheck="false"
        :value="app.libraryFilter"
        @input="setLibraryFilter(($event.target as HTMLInputElement).value)"
      />
    </div>

    <!-- A folder that turned out not to be a repository, or a clone that
         failed. It was being set and never shown, which made "Ajouter un dépôt"
         look as though the button did nothing. -->
    <button v-if="app.addError" class="library-error" @click="dismissAddError()">
      {{ app.addError }} ✗
    </button>

    <!-- A filter with nothing behind it is not an empty library, and offering
         "Ajouter un dépôt local" there would answer a question nobody asked:
         the repositories are still in the list, one word away. -->
    <div v-if="rows.length === 0 && app.libraryFilter.trim() !== ''" class="library-empty">
      <p class="library-empty-title">{{ t("library.noMatch") }}</p>
      <p class="library-empty-text">
        {{
          t("library.noMatchText", {
            n: count("library.repositories", app.repositories.length),
            needle: app.libraryFilter.trim(),
          })
        }}
      </p>
      <span class="library-empty-actions">
        <button @click="setLibraryFilter('')">{{ t("library.clearFilter") }}</button>
      </span>
    </div>

    <!-- Board 06's empty state: what to do, not just that there is nothing. -->
    <div v-else-if="app.repositories.length === 0" class="library-empty">
      <p class="library-empty-title">{{ t("library.empty") }}</p>
      <p class="library-empty-text">{{ t("library.emptyText") }}</p>
      <span class="library-empty-actions">
        <button class="primary" @click="addRepository()">{{ t("topbar.addRepository") }}</button>
        <button @click="openClone()">{{ t("topbar.clone") }}</button>
      </span>
    </div>

    <div
      v-else
      ref="list"
      class="library-groups"
      tabindex="0"
      data-zone="1"
      @focus="goToZone(1)"
      @pointermove="drag"
      @pointerup="drop"
      @pointercancel="cancel"
      @keydown.esc="cancel"
    >
      <template v-for="folder in groups" :key="folder.group">
        <!-- A folder, and the two things you do to one. The header is the drop
             target for an empty folder, so it carries `data-group` even though
             it holds no row. -->
        <div
          class="group-head library-group"
          :class="{ into: lineInside(folder.group, folder.rows.length) }"
          :data-group="folder.group"
        >
          <input
            v-if="app.renamingGroup === folder.group"
            ref="naming"
            type="text"
            class="group-name-box"
            :value="folder.name"
            spellcheck="false"
            @keydown.enter="setGroupName(folder.group, ($event.target as HTMLInputElement).value)"
            @keydown.esc="renameGroup(null)"
            @blur="setGroupName(folder.group, ($event.target as HTMLInputElement).value)"
          />
          <template v-else>
            <button
              class="as-button group-fold"
              :aria-expanded="!folder.collapsed"
              @click="toggleGroup(folder.group)"
            >
              <Glyph :name="folder.collapsed ? 'chevron-right' : 'chevron-down'" class="chevron" />
              <!-- DESIGN §6: the folder is the group's shape, the bound
                   rectangle the repository's. Shape carries meaning here, so
                   neither may wear the other's. -->
              <Glyph name="folder" class="group-folder" />
              <span class="group-label">{{ folder.name }}</span>
            </button>
            <span class="pane-head-count">{{ folder.rows.length }}</span>
            <button
              class="row-action"
              :title="t('library.renameGroup')"
              @click="rename(folder.group)"
            >
              <Glyph name="rename" />
            </button>
            <button
              class="row-action danger"
              :disabled="app.groups.length < 2"
              :title="app.groups.length < 2 ? t('library.lastGroup') : t('library.removeGroup')"
              @click="removeGroup(folder.group)"
            >
              <Glyph name="close" />
            </button>
          </template>
        </div>

        <!-- A folder somebody just made, with the sentence that says what to do
             with it. Drawn only when nothing is filtered: under a filter an
             empty folder is not shown at all. -->
        <p v-if="!folder.collapsed && folder.rows.length === 0" class="group-empty">
          {{ t("library.emptyGroup") }}
        </p>

        <!-- A `<div>`, not a `<button>`: the row carries a button of its own
             ("Retirer"), and a button inside a button is invalid markup the
             compiler warns about and a parser undoes. `.file-row` and
             `.commit-row` were already this shape. -->
        <div
          v-for="(row, at) in folder.collapsed ? [] : folder.rows"
          :key="row.path"
          class="row library-row"
          :class="{
            selected: app.card === row.path,
            focused: app.card === row.path && zoneActive(1),
            gone: row.missing,
            dragged: app.draggedRepository === row.path,
            'line-above': lineAbove(folder.group, at),
            'line-below': lineBelow(folder.group, at, folder.rows.length),
          }"
          :data-group="folder.group"
          data-row=""
          :title="t('library.dragTitle')"
          @pointerdown="press($event, row)"
          @click="
            goToZone(1);
            showCard(row.path);
          "
          @dblclick="openRepository(row.path)"
        >
          <Glyph :name="row.missing ? 'missing' : 'repository'" class="library-icon" />
          <span class="library-lines">
            <span class="library-name">{{ row.name }}</span>
            <span class="library-state mono" :class="state(row).kind">{{ state(row).text }}</span>
          </span>
          <span class="library-dot" :class="state(row).kind" aria-hidden="true" />
          <button
            class="row-action danger"
            :tabindex="app.card === row.path ? 0 : -1"
            :title="t('library.forgetTitle')"
            @click.stop="forgetRepository(row)"
          >
            {{ t("library.forget") }}
          </button>
        </div>
      </template>
    </div>

    <!-- Board 06's "+ Nouveau groupe", which stood here labelled "jalon M9" and
         was taken down when M9 ended without it. It is built now, and the
         second line is the one that matters: a folder is made empty, and
         nothing about an empty folder says how anything gets into it. -->
    <button
      v-if="app.repositories.length > 0 || app.groups.length > 0"
      class="library-new-group"
      :title="t('library.newGroupTitle')"
      @click="createGroup()"
    >
      <Glyph name="folder-plus" />
      <span>{{ t("library.newGroup") }}</span>
      <span class="pane-head-spacer" />
      <span class="dim">{{ t("library.dragHint") }}</span>
    </button>
  </nav>
</template>

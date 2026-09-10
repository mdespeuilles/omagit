<script setup lang="ts">
// The diff, virtualised, and the place staging actually happens.
//
// The pane the stack change was decided without a measurement of, so it was
// built first — and it is also where every write except "the whole file" is
// addressed from, because a hunk and a line have no name outside the diff they
// are in.

import { computed } from "vue";
import {
  app,
  clearPicked,
  discardHunk,
  discardPicked,
  goToZone,
  lineKey,
  pickLine,
  selectFile,
  shown,
  stageHunk,
  stagePicked,
} from "../state";
import { count, t } from "../i18n";
import { lineHeight } from "../metrics";
import VirtualList from "./VirtualList.vue";

/// Board 03's interline, from the density: 17px compact, 20px comfortable.
/// Fixed rows are what lets the list place them without measuring any of them —
/// and reading the token is what makes the density actually move them.
const ROW_HEIGHT = lineHeight();

/// The diff on screen, which during a re-read is still the one it is about to
/// replace: a pane that empties itself every time the repository settles makes
/// the whole window jump (see `shown`).
const diff = computed(() => shown(app.diff));
const rows = computed(() => diff.value?.rows ?? []);
const staged = computed(() => app.selected?.staged ?? false);

/// The row of the open file, which is what says whether the other side of the
/// index has anything to show.
const entry = computed(() => {
  const file = app.selected?.path;
  if (!file || app.status.status !== "ready") return null;
  return app.status.value.find((row) => row.path === file) ?? null;
});

const picked = computed(() => app.picked.size);

/// A conflicted file is read here and settled elsewhere.
///
/// Staging a hunk of an unmerged path cannot work — there is no side of the
/// index to build the patch from — and `git checkout -- <path>` refuses one
/// outright, so "Rejeter" would be a button that always fails. Both are hidden
/// rather than disabled: the row and the dialog (§2.37) are where a conflict is
/// answered, and a control that is never usable teaches nothing by staying.
const conflicted = computed(() => !!entry.value?.conflict);

// Takes the side rather than the row: `readonly()` makes the state deeply
// immutable, and a parameter typed as the mutable row would not accept one.
function sign(side: "context" | "added" | "removed"): string {
  return side === "added" ? "+" : side === "removed" ? "−" : " ";
}

/// Only a changed line can be staged on its own: a context line is in both
/// versions, so picking it would name nothing.
function selectable(side: string): boolean {
  return side !== "context";
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/// The line split at its refinement boundaries, for the word-level highlight
/// of DESIGN-TOKENS §5.
///
/// The ranges are **byte** offsets — Rust's — and a JavaScript string is
/// indexed in UTF-16 units, so slicing the string directly would land in the
/// wrong place on the first line containing an accent. Encoding first is the
/// only version of this that is right for text that is not ASCII.
function segments(
  text: string,
  refined: readonly (readonly [number, number])[],
): { text: string; mark: boolean }[] {
  if (refined.length === 0) return [{ text, mark: false }];
  const bytes = encoder.encode(text);
  const parts: { text: string; mark: boolean }[] = [];
  let at = 0;
  for (const [from, to] of refined) {
    if (from > at) parts.push({ text: decoder.decode(bytes.slice(at, from)), mark: false });
    parts.push({ text: decoder.decode(bytes.slice(from, to)), mark: true });
    at = to;
  }
  if (at < bytes.length) parts.push({ text: decoder.decode(bytes.slice(at)), mark: false });
  return parts;
}
</script>

<template>
  <section class="diff">
    <header class="pane-head">
      <!-- The two sides of the index, as the two tabs of Board 03. A side with
           nothing on it is disabled rather than hidden: a tab that appears and
           disappears as you stage moves the other one under the pointer. -->
      <!-- A conflicted path has no side of the index to be on: it has no
           stage-0 entry at all. Two tabs where there is one answer would be two
           controls that do the same thing, so it says what it is instead. -->
      <span v-if="app.screen === 'working-copy' && entry?.conflict" class="tab on conflict">
        {{ t("diff.conflicted") }}
      </span>
      <template v-else-if="app.screen === 'working-copy' && app.selected">
        <button
          class="tab"
          :class="{ on: !staged }"
          :disabled="!entry || entry.unstaged === null"
          @click="selectFile(app.selected.path, false)"
        >
          {{ t("diff.unstaged") }}
        </button>
        <button
          class="tab"
          :class="{ on: staged }"
          :disabled="!entry || entry.staged === null"
          @click="selectFile(app.selected.path, true)"
        >
          {{ t("diff.staged") }}
        </button>
      </template>
      <span class="pane-head-title">
        {{ diff?.header ?? "—" }}
      </span>
    </header>

    <!-- The four states of SPEC §10, each drawn as itself: no spinner over the
         window, no blank pane, no error swallowed into an empty list. -->
    <p v-if="app.diff.status === 'idle'" class="pane-empty">{{ t("diff.none") }}</p>
    <p v-else-if="app.diff.status === 'failed'" class="pane-error mono">
      {{ app.diff.error }}
    </p>
    <p v-else-if="!diff" class="pane-empty">{{ t("diff.reading") }}</p>
    <p v-else-if="diff.reason" class="pane-empty">
      {{ diff.reason }}
    </p>
    <p v-else-if="rows.length === 0" class="pane-empty">{{ t("diff.nothing") }}</p>

    <VirtualList
      v-else
      :items="rows"
      :row-height="ROW_HEIGHT"
      tabindex="0"
      data-zone="3"
      @focus="goToZone(3)"
      v-slot="{ item }"
    >
      <div v-if="item.kind === 'header'" class="diff-header mono">
        <span class="diff-header-text">{{ item.text }}</span>
        <!-- Never fully hidden (DESIGN §3): at rest these sit at a low opacity
             so the target stays reachable by keyboard, and come up on hover. -->
        <span v-if="app.screen === 'working-copy' && !conflicted" class="hunk-actions">
          <button :disabled="!!app.busy" @click="stageHunk(item.hunk, staged)">
            {{ staged ? t("diff.unstageHunk") : t("diff.stageHunk") }}
          </button>
          <button
            v-if="!staged"
            class="danger"
            :disabled="!!app.busy"
            @click="discardHunk(item.hunk)"
          >
            {{ t("diff.discard") }}
          </button>
        </span>
      </div>

      <div v-else-if="item.kind === 'fold'" class="diff-fold mono">
        ⌄ {{ count("diff.folded", item.lines) }}
      </div>

      <div
        v-else
        class="diff-line mono"
        :class="[
          item.side,
          {
            picked: app.picked.has(lineKey(item.hunk, item.index)),
            inert: app.screen !== 'working-copy' || conflicted,
          },
        ]"
        @click="
          app.screen === 'working-copy' &&
          !conflicted &&
          selectable(item.side) &&
          pickLine(item.hunk, item.index, $event.shiftKey)
        "
      >
        <span class="gutter">{{ item.old ?? "" }}</span>
        <span class="gutter">{{ item.new ?? "" }}</span>
        <span class="sign">{{ sign(item.side) }}</span>
        <span class="text"
          ><span
            v-for="(part, at) in segments(item.text, item.refined)"
            :key="at"
            :class="{ word: part.mark }"
            >{{ part.text }}</span
          ><span v-if="item.no_newline" class="no-newline">{{ t("diff.noNewline") }}</span></span
        >
      </div>
    </VirtualList>

    <!-- Appears only once lines are picked, and says what it would act on
         before it acts: this is the one bar in the app whose buttons write. -->
    <footer v-if="picked > 0 && app.screen === 'working-copy' && !conflicted" class="picked-bar">
      <span>{{ count("diff.picked", picked) }}</span>
      <span class="pane-head-spacer" />
      <button :disabled="!!app.busy" @click="stagePicked(staged)">
        {{ staged ? t("diff.unstage") : t("diff.stage") }}
      </button>
      <button v-if="!staged" class="danger" :disabled="!!app.busy" @click="discardPicked()">
        {{ t("diff.discard") }}
      </button>
      <button @click="clearPicked()">{{ t("diff.clear") }}</button>
    </footer>
  </section>
</template>

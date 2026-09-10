<script setup lang="ts">
// Board 07's command palette: a 640px overlay, the query, and what it finds
// grouped by what it is.
//
// The ranking lives in `palette.ts` — this draws it. Two things here are the
// board's and worth keeping: the matched characters are marked in accent, and
// on the selected row, whose ground is already accent, they are marked in
// weight alone ("jamais deux fois la même couleur l'une sur l'autre"). And each
// row says what ⏎ will do to it, because ⏎ does four different things.

import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { app, closePalette, movePalette, runPaletteRow, setPaletteQuery } from "../state";
import { ACTIONS, hint } from "../keymap";
import { flatten, search, type Row } from "../palette";

const box = ref<HTMLInputElement | null>(null);

const groups = computed(() =>
  search(app.palette?.query ?? "", {
    actions: ACTIONS.filter((action) => action.where === "always" || !!app.open),
    repositories: [...app.repositories],
    branches: app.refs.status === "ready" ? [...app.refs.value.branches] : [],
    files: app.status.status === "ready" ? [...app.status.value] : [],
  }),
);

const rows = computed(() => flatten(groups.value));
const at = computed(() => app.palette?.at ?? 0);

/// Where a row sits in the flat order, which is what ↑ ↓ walk and what the
/// selection is counted in.
function position(group: number, row: number): number {
  let index = 0;
  for (let before = 0; before < group; before += 1) index += groups.value[before]!.rows.length;
  return index + row;
}

/// A label cut into marked and unmarked pieces, so the matched characters can
/// be drawn without putting HTML together by hand.
function pieces(row: Row): { text: string; mark: boolean }[] {
  const marked = new Set(row.marks);
  const out: { text: string; mark: boolean }[] = [];
  for (let index = 0; index < row.label.length; index += 1) {
    const mark = marked.has(index);
    const last = out[out.length - 1];
    if (last && last.mark === mark) last.text += row.label[index];
    else out.push({ text: row.label[index]!, mark });
  }
  return out;
}

/// The shortcut an action row carries, when it has one — the palette is also
/// where the bindings are learnt.
function shortcut(row: Row): string {
  if (row.kind !== "action") return "";
  const action = ACTIONS.find((entry) => entry.id === row.key);
  return action ? hint(action.binding, app.platform?.modifier_label ?? "Ctrl") : "";
}

function run(row: Row): void {
  runPaletteRow(row, ACTIONS);
}

function onKey(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    event.preventDefault();
    closePalette();
    return;
  }
  if (event.key === "ArrowDown" || (event.key === "n" && event.ctrlKey)) {
    event.preventDefault();
    movePalette(1, rows.value.length);
    return;
  }
  if (event.key === "ArrowUp" || (event.key === "p" && event.ctrlKey)) {
    event.preventDefault();
    movePalette(-1, rows.value.length);
    return;
  }
  if (event.key === "Enter") {
    event.preventDefault();
    const row = rows.value[at.value];
    if (row) run(row);
  }
}

onMounted(() => {
  box.value?.focus();
  window.addEventListener("keydown", onKey);
});
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div v-if="app.palette" class="overlay top" @click.self="closePalette()">
    <section class="palette" role="dialog" aria-modal="true" aria-label="Palette de commandes">
      <header class="palette-head">
        <input
          ref="box"
          class="palette-query"
          type="text"
          placeholder="Une action, un dépôt, une branche, un fichier…"
          spellcheck="false"
          :value="app.palette.query"
          @input="setPaletteQuery(($event.target as HTMLInputElement).value)"
        />
        <span class="palette-count"
          >{{ rows.length }} résultat{{ rows.length > 1 ? "s" : "" }}</span
        >
        <button class="link" @click="closePalette()">Esc</button>
      </header>

      <p v-if="rows.length === 0" class="pane-empty">Rien de ce nom-là ici.</p>

      <div v-else class="palette-rows">
        <template v-for="(group, index) in groups" :key="group.name">
          <div class="group-head">
            <span>{{ group.name }}</span>
            <span class="pane-head-spacer" />
            <span class="pane-head-count">{{ group.rows.length }}</span>
          </div>
          <button
            v-for="(row, offset) in group.rows"
            :key="`${row.kind}:${row.key}`"
            class="row palette-row"
            :class="{ selected: position(index, offset) === at, off: !row.enabled }"
            @click="run(row)"
            @mousemove="movePalette(position(index, offset) - at, rows.length)"
          >
            <span class="palette-label">
              <span
                v-for="(piece, part) in pieces(row)"
                :key="part"
                :class="{ mark: piece.mark }"
                >{{ piece.text }}</span
              >
            </span>
            <span v-if="row.detail" class="palette-detail mono">{{ row.detail }}</span>
            <span class="pane-head-spacer" />
            <span class="palette-hint">{{ shortcut(row) || row.hint }}</span>
          </button>
        </template>
      </div>
    </section>
  </div>
</template>

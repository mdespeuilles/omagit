<script setup lang="ts">
// SPEC §11's filters: author, message, path, date range.
//
// Two rules shape it. Typing re-walks the history — but not once per keystroke:
// a walk of a hundred thousand commits per letter is not a filter, it is a
// hang. The first version answered that by waiting for Enter or for the field
// to be left, and that was the wrong end of the trade: you type, the list does
// not move, nothing on screen says it is waiting, and the box reads as broken.
// A quarter of a second after the last keystroke is one walk per pause in the
// typing rather than one per letter, and Enter still applies at once.
//
// And a filtered history says so: the graph is gone while one is on, and a bar
// that did not explain that would look like a bug.

import { computed, onBeforeUnmount, ref, watch } from "vue";
import { app, clearFilters, isFilteringHistory, setQuery } from "../state";
import { t } from "../i18n";

const author = ref(app.query.author);
const text = ref(app.query.text);
const path = ref(app.query.path);
const since = ref(asDate(app.query.since));
const until = ref(asDate(app.query.until));

// The boxes follow the state rather than owning it: `clearFilters` empties the
// query, and a box still holding what was typed would say otherwise.
watch(
  () => app.query,
  (query) => {
    author.value = query.author;
    text.value = query.text;
    path.value = query.path;
    since.value = asDate(query.since);
    until.value = asDate(query.until);
  },
  { deep: true },
);

const filtering = computed(() => isFilteringHistory());

/// Long enough that a burst of typing is one walk, short enough that the list
/// answers while the hand is still on the keyboard.
const PAUSE = 250;
let pending: ReturnType<typeof setTimeout> | undefined;

function applySoon(): void {
  clearTimeout(pending);
  pending = setTimeout(apply, PAUSE);
}

// A walk asked for by a screen nobody is looking at any more is a walk nobody
// will read.
onBeforeUnmount(() => clearTimeout(pending));

function apply(): void {
  clearTimeout(pending);
  void setQuery({
    author: author.value,
    text: text.value,
    path: path.value,
    since: asSeconds(since.value, 0),
    // Inclusive at both ends, and a date box means the whole day: `until` is
    // the last second of the day chosen, not its first.
    until: asSeconds(until.value, 86_399),
  });
}

/// `yyyy-mm-dd` for an `<input type="date">`, empty when unset.
function asDate(seconds: number): string {
  if (seconds <= 0) return "";
  return new Date(seconds * 1000).toISOString().slice(0, 10);
}

/// Midnight local time, plus `offset` seconds. Zero when the box is empty,
/// which is what the backend reads as unset.
function asSeconds(value: string, offset: number): number {
  if (value === "") return 0;
  const parsed = new Date(`${value}T00:00:00`);
  return Number.isNaN(parsed.getTime()) ? 0 : Math.floor(parsed.getTime() / 1000) + offset;
}
</script>

<template>
  <div class="filters">
    <input
      v-model="author"
      class="filter-box"
      type="text"
      :placeholder="t('history.author')"
      spellcheck="false"
      @input="applySoon()"
      @keydown.enter="apply()"
      @blur="apply()"
    />
    <input
      v-model="text"
      class="filter-box wide"
      type="text"
      :placeholder="t('history.message')"
      spellcheck="false"
      @input="applySoon()"
      @keydown.enter="apply()"
      @blur="apply()"
    />
    <input
      v-model="path"
      class="filter-box"
      type="text"
      :placeholder="t('history.path')"
      spellcheck="false"
      @input="applySoon()"
      @keydown.enter="apply()"
      @blur="apply()"
    />
    <input
      v-model="since"
      class="filter-date"
      type="date"
      :title="t('history.since')"
      @change="apply()"
    />
    <input
      v-model="until"
      class="filter-date"
      type="date"
      :title="t('history.until')"
      @change="apply()"
    />
    <button class="link" :disabled="!filtering" @click="clearFilters()">
      {{ t("history.clear") }}
    </button>
  </div>

  <!-- Said out loud rather than left to be noticed: the gutter disappearing
       without a reason reads as a bug. -->
  <p v-if="filtering" class="filters-note">
    {{ t("history.filtered") }}
  </p>
</template>

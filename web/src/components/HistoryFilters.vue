<script setup lang="ts">
// SPEC §11's filters: author, message, path, date range.
//
// Two rules shape it. Typing does not re-walk the history on every keystroke —
// a walk of a hundred thousand commits per letter is not a filter, it is a
// hang — so the text boxes commit on Enter or on leaving the field. And a
// filtered history says so: the graph is gone while one is on, and a bar that
// did not explain that would look like a bug.

import { computed, ref, watch } from "vue";
import { app, clearFilters, isFilteringHistory, setQuery } from "../state";

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

function apply(): void {
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
      placeholder="Auteur"
      spellcheck="false"
      @keydown.enter="apply()"
      @blur="apply()"
    />
    <input
      v-model="text"
      class="filter-box wide"
      type="text"
      placeholder="Message"
      spellcheck="false"
      @keydown.enter="apply()"
      @blur="apply()"
    />
    <input
      v-model="path"
      class="filter-box"
      type="text"
      placeholder="Chemin"
      spellcheck="false"
      @keydown.enter="apply()"
      @blur="apply()"
    />
    <input v-model="since" class="filter-date" type="date" title="Depuis" @change="apply()" />
    <input v-model="until" class="filter-date" type="date" title="Jusqu'à" @change="apply()" />
    <button class="link" :disabled="!filtering" @click="clearFilters()">Effacer</button>
  </div>

  <!-- Said out loud rather than left to be noticed: the gutter disappearing
       without a reason reads as a bug. -->
  <p v-if="filtering" class="filters-note">
    Résultat de recherche — pas de graphe : les lignes voisines ne sont pas parentes, seulement les
    suivantes qui correspondent.
  </p>
</template>

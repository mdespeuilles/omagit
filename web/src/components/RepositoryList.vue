<script setup lang="ts">
// Board 06's list: 300px, a filter box, groups, and one row per repository.
//
// A row is two lines — the name, then what the repository is *now* — plus a
// status square on the right. The square repeats what the second line says, in
// a form that survives at a glance and in greyscale: reading nine rows of text
// to find the one with conflicts is not glancing.

import { computed } from "vue";
import type { LibraryRow } from "../ipc";
import {
  addRepository,
  app,
  dismissAddError,
  forgetRepository,
  openClone,
  openRepository,
  showCard,
} from "../state";

const groups = computed(() => {
  const seen = new Map<number, { name: string; rows: LibraryRow[] }>();
  for (const row of app.repositories) {
    const group = seen.get(row.group) ?? { name: row.group_name, rows: [] };
    group.rows.push(row as LibraryRow);
    seen.set(row.group, group);
  }
  return [...seen.entries()].sort(([a], [b]) => a - b).map(([, group]) => group);
});

/// The second line of a row, and the colour it and the square share.
///
/// Board 06 draws five: clean, modified, conflicted, detached, and gone. Each
/// is a different question — "nothing to do", "work in progress", "stuck",
/// "not on a branch", "not here" — and collapsing them into a count would lose
/// the only one that is urgent.
type State = { text: string; kind: "clean" | "changed" | "conflict" | "detached" | "gone" };

function state(row: LibraryRow): State {
  if (row.missing) return { text: "introuvable sur le disque", kind: "gone" };
  const held = app.library[row.path];
  if (!held || held.status === "loading") return { text: "…", kind: "clean" };
  if (held.status === "failed") return { text: held.error, kind: "gone" };
  if (held.status !== "ready") return { text: "", kind: "clean" };

  const { head, operation, counts, tracking } = held.value;
  const where = operation ?? head;
  if (counts.conflicted > 0) {
    return { text: `${where} · ${plural(counts.conflicted, "conflit")}`, kind: "conflict" };
  }

  const changed = counts.modified + counts.added + counts.deleted + counts.renamed;
  const detached = head.startsWith("detached");
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
      text: [where, divergence || "propre"].filter(Boolean).join(" "),
      kind: detached ? "detached" : "clean",
    };
  }
  const what = [
    changed > 0 ? plural(changed, "modifié") : "",
    counts.untracked > 0 ? plural(counts.untracked, "non suivi") : "",
  ]
    .filter(Boolean)
    .join(" · ");
  return { text: `${where} · ${what}`, kind: detached ? "detached" : "changed" };
}

const plural = (count: number, word: string): string => `${count} ${word}${count > 1 ? "s" : ""}`;
</script>

<template>
  <nav class="library">
    <div class="library-filter">
      <input type="search" placeholder="Filtrer les dépôts" disabled title="Filtre — jalon M9" />
    </div>

    <!-- A folder that turned out not to be a repository, or a clone that
         failed. It was being set and never shown, which made "Ajouter un dépôt"
         look as though the button did nothing. -->
    <button v-if="app.addError" class="library-error" @click="dismissAddError()">
      {{ app.addError }} ✗
    </button>

    <!-- Board 06's empty state: what to do, not just that there is nothing. -->
    <div v-if="app.repositories.length === 0" class="library-empty">
      <p class="library-empty-title">Aucun dépôt pour l'instant</p>
      <p class="library-empty-text">
        Ajoute un dossier déjà versionné. Il reste sur le disque : cette liste n'en garde que le
        chemin.
      </p>
      <span class="library-empty-actions">
        <button class="primary" @click="addRepository()">Ajouter un dépôt local</button>
        <button @click="openClone()">Cloner…</button>
      </span>
    </div>

    <div v-else class="library-groups">
      <template v-for="(group, at) in groups" :key="at">
        <div class="group-head">
          <span class="chevron">▾</span><span>{{ group.name }}</span>
          <span class="pane-head-spacer" />
          <span class="pane-head-count">{{ group.rows.length }}</span>
        </div>
        <button
          v-for="row in group.rows"
          :key="row.path"
          class="row library-row"
          :class="{ selected: app.card === row.path, gone: row.missing }"
          @click="showCard(row.path)"
          @dblclick="openRepository(row.path)"
        >
          <span class="library-icon" aria-hidden="true">{{ row.missing ? "⊘" : "▤" }}</span>
          <span class="library-lines">
            <span class="library-name">{{ row.name }}</span>
            <span class="library-state mono" :class="state(row).kind">{{ state(row).text }}</span>
          </span>
          <span class="library-dot" :class="state(row).kind" aria-hidden="true" />
          <button
            class="row-action danger"
            title="Retirer de la liste — le dépôt reste sur le disque"
            @click.stop="forgetRepository(row)"
          >
            Retirer
          </button>
        </button>
      </template>
    </div>

    <div class="library-foot">
      <span class="mono">+</span><span>Nouveau groupe</span>
      <span class="pane-head-spacer" />
      <span class="dim">jalon M9</span>
    </div>
  </nav>
</template>

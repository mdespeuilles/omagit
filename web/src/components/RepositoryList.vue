<script setup lang="ts">
// Board 06's list: the repositories the user has added, in the groups they
// arranged them in.
//
// A row says what the repository is *now* — the branch, what is modified, a
// half-finished merge — and that comes from a summary read per row after the
// list has drawn. Waiting for all of them would mean a status walk over every
// repository ever added before anything appeared.

import { computed } from "vue";
import type { LibraryRow } from "../ipc";
import { addRepository, app, forgetRepository, openRepository, showCard } from "../state";

/// The rows in their groups, in the order the library holds them.
const groups = computed(() => {
  const seen = new Map<number, { name: string; rows: LibraryRow[] }>();
  for (const row of app.repositories) {
    const group = seen.get(row.group) ?? { name: row.group_name, rows: [] };
    group.rows.push(row as LibraryRow);
    seen.set(row.group, group);
  }
  return [...seen.entries()].sort(([a], [b]) => a - b).map(([, group]) => group);
});

/// What the row says under the name, which is the whole point of reading a
/// summary per row: "main · propre" and "merge · 2 conflits" are different
/// enough to be worth a Git read each.
function state(row: LibraryRow): string {
  const summary = app.library[row.path];
  if (!summary || summary.status === "loading") return "…";
  if (summary.status === "failed") return summary.error;
  if (summary.status !== "ready") return "";

  const { head, operation, counts } = summary.value;
  // The operation replaces the branch rather than joining it: "on main" is
  // misleading while a merge is stuck.
  const where = operation ?? head;
  const parts: string[] = [];
  if (counts.conflicted > 0) parts.push(`${counts.conflicted} conflit${plural(counts.conflicted)}`);
  const changed = counts.modified + counts.added + counts.deleted + counts.renamed;
  if (changed > 0) parts.push(`${changed} modifié${plural(changed)}`);
  if (counts.untracked > 0) parts.push(`${counts.untracked} non suivi${plural(counts.untracked)}`);
  return parts.length === 0 ? `${where} · propre` : `${where} · ${parts.join(" · ")}`;
}

function ahead(row: LibraryRow): string {
  const summary = app.library[row.path];
  if (summary?.status !== "ready") return "";
  const tracking = summary.value.tracking;
  if (!tracking || tracking.gone) return "";
  const parts: string[] = [];
  if (tracking.ahead > 0) parts.push(`↑${tracking.ahead}`);
  if (tracking.behind > 0) parts.push(`↓${tracking.behind}`);
  return parts.join(" ");
}

const plural = (count: number): string => (count > 1 ? "s" : "");
</script>

<template>
  <section class="library">
    <header class="pane-head">
      <span>Dépôts</span>
      <span class="pane-head-count">{{ app.repositories.length }}</span>
      <span class="pane-head-spacer" />
      <button class="link" @click="addRepository()">Ajouter un dépôt local</button>
    </header>

    <p v-if="app.addError" class="pane-error">{{ app.addError }}</p>

    <!-- The empty state of board 06: what to do, not just that there is
         nothing. -->
    <div v-if="app.repositories.length === 0" class="library-empty">
      <p class="library-empty-title">Aucun dépôt pour l'instant</p>
      <p class="library-empty-text">
        Ajoute un dossier déjà versionné. Il reste sur le disque : cette liste n'en garde que le
        chemin.
      </p>
      <button @click="addRepository()">Ajouter un dépôt local</button>
    </div>

    <div v-else class="library-groups">
      <template v-for="(group, at) in groups" :key="at">
        <header class="pane-head">
          <span>{{ group.name }}</span>
          <span class="pane-head-count">{{ group.rows.length }}</span>
        </header>
        <div
          v-for="row in group.rows"
          :key="row.path"
          class="library-row"
          :class="{ selected: app.card === row.path, open: app.open === row.path }"
          @click="showCard(row.path)"
          @dblclick="openRepository(row.path)"
        >
          <span class="library-glyph" :class="{ missing: row.missing }">⑂</span>
          <span class="library-name" :class="{ missing: row.missing }">{{ row.name }}</span>
          <span class="library-state">{{ state(row) }}</span>
          <span class="library-ahead mono">{{ ahead(row) }}</span>
          <button
            class="row-action danger"
            title="Retirer de la liste — le dépôt reste sur le disque"
            @click.stop="forgetRepository(row)"
          >
            Retirer
          </button>
        </div>
      </template>
    </div>
  </section>
</template>

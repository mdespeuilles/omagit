<script setup lang="ts">
// Board 03's branch tree.
//
// Grouping on `/` happens here and not in `omagit-git`, which says so in as
// many words: a tree is a rendering decision, and the crate that reads Git has
// no business holding one.

import { computed, ref } from "vue";
import type { BranchRow } from "../ipc";
import {
  app,
  checkoutBranch,
  createBranch,
  deleteBranch,
  isCollapsed,
  mergeBranch,
  rebaseOnto,
  showBranchHistory,
  toggleBranchGroup,
} from "../state";

/// Whether this row's history is the one History is showing, which is what a
/// click on it asked for.
function showing(name: string): boolean {
  return app.screen === "history" && app.query.branch === name;
}

/// A branch nobody has touched in this long carries its age instead of its
/// divergence — board 03's "7 mois". Ninety days, because a quarter is the
/// shortest span for which "nobody is working on this" is a fair reading.
const STALE = 90 * 86_400;

const refs = computed(() => (app.refs.status === "ready" ? app.refs.value : null));

type Group = { prefix: string; rows: BranchRow[] };

/// Branches with no `/` first, then one group per prefix.
///
/// Only the first segment groups. `feature/ui/topbar` lands under `feature/`
/// with its remaining path shown, rather than nesting three deep for a tree
/// nobody arranged that way.
const grouped = computed(() => {
  const loose: BranchRow[] = [];
  const groups = new Map<string, BranchRow[]>();
  for (const row of refs.value?.branches ?? []) {
    const cut = row.name.indexOf("/");
    if (cut < 0) loose.push(row as BranchRow);
    else {
      const prefix = row.name.slice(0, cut + 1);
      groups.set(prefix, [...(groups.get(prefix) ?? []), row as BranchRow]);
    }
  }
  // Sorted here, like the grouping: the order `gix` enumerates references in is
  // an implementation detail of a hash map, and a tree whose rows moved between
  // two runs of the same repository would be unusable.
  const byName = (a: BranchRow, b: BranchRow) => a.name.localeCompare(b.name);
  return {
    loose: [...loose].sort(byName),
    groups: [...groups.entries()]
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([prefix, rows]): Group => ({ prefix, rows: [...rows].sort(byName) })),
  };
});

const remotes = computed(() => {
  const groups = new Map<string, { remote: string; name: string }[]>();
  for (const row of refs.value?.remote_branches ?? []) {
    groups.set(row.remote, [
      ...(groups.get(row.remote) ?? []),
      { remote: row.remote, name: row.name },
    ]);
  }
  return [...groups.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([remote, rows]): [string, { remote: string; name: string }[]] => [
      remote,
      [...rows].sort((a, b) => a.name.localeCompare(b.name)),
    ]);
});

const total = computed(() => refs.value?.branches.length ?? 0);

/// The last segment: what the row is called inside its group.
const leaf = (name: string): string => name.slice(name.indexOf("/") + 1);

function divergence(row: BranchRow): string {
  const tracking = row.tracking;
  if (!tracking) return "";
  if (tracking.gone) return "disparu";
  return [
    tracking.ahead > 0 ? `↑${tracking.ahead}` : "",
    tracking.behind > 0 ? `↓${tracking.behind}` : "",
  ]
    .filter(Boolean)
    .join("");
}

/// Months, roughly, and only when it is worth saying.
function stale(row: BranchRow): string {
  if (row.head || row.age < STALE) return "";
  const months = Math.round(row.age / (30 * 86_400));
  return months >= 12 ? `${Math.round(months / 12)} ans` : `${months} mois`;
}

// ── Creating one ────────────────────────────────────────────────────────────

const naming = ref(false);
const name = ref("");

function submit(): void {
  createBranch(name.value, "", true);
  name.value = "";
  naming.value = false;
}
</script>

<template>
  <template v-if="refs">
    <div class="group-head">
      <span class="chevron">▾</span><span>Branches</span>
      <span class="pane-head-spacer" />
      <span class="pane-head-count">{{ total }}</span>
    </div>

    <button
      v-for="row in grouped.loose"
      :key="row.name"
      class="row branch-row"
      :class="{ selected: showing(row.name) }"
      :title="`${row.name} — clic : son historique, double-clic : basculer dessus`"
      @click="showBranchHistory(row.name)"
      @dblclick="checkoutBranch(row.name)"
    >
      <span class="branch-name">{{ row.name }}</span>
      <span v-if="row.head" class="ref head">HEAD</span>
      <span class="pane-head-spacer" />
      <span v-if="stale(row)" class="branch-note">{{ stale(row) }}</span>
      <span v-else-if="row.merged && !row.head" class="ref merged">Merged</span>
      <span v-if="divergence(row)" class="branch-note mono">{{ divergence(row) }}</span>
      <span class="row-actions">
        <button
          v-if="!row.head"
          class="row-action"
          :title="`Fusionner ${row.name} dans la branche courante`"
          @click.stop="mergeBranch(row.name)"
        >
          Fusionner
        </button>
        <button
          v-if="!row.head"
          class="row-action"
          :title="`Rebaser la branche courante sur ${row.name}`"
          @click.stop="rebaseOnto(row.name)"
        >
          Rebaser
        </button>
        <button
          v-if="!row.head"
          class="row-action danger"
          title="Supprimer cette branche"
          @click.stop="deleteBranch(row)"
        >
          Suppr.
        </button>
      </span>
    </button>

    <template v-for="group in grouped.groups" :key="group.prefix">
      <button class="group-head as-button" @click="toggleBranchGroup(group.prefix)">
        <span class="chevron">{{ isCollapsed(group.prefix) ? "▸" : "▾" }}</span>
        <!-- A prefix is part of a branch's name, so it keeps its case. The
             headers around it are labels and take the section styling; board
             03 uppercases this one too, and uppercasing a name that Git treats
             case-sensitively is the kind of tidiness that misleads. -->
        <span class="group-name">{{ group.prefix }}</span>
        <span class="pane-head-spacer" />
        <span class="pane-head-count">{{ group.rows.length }}</span>
      </button>
      <template v-if="!isCollapsed(group.prefix)">
        <button
          v-for="row in group.rows"
          :key="row.name"
          class="row branch-row nested"
          :class="{ selected: showing(row.name) }"
          :title="`${row.name} — clic : son historique, double-clic : basculer dessus`"
          @click="showBranchHistory(row.name)"
          @dblclick="checkoutBranch(row.name)"
        >
          <span class="branch-name">{{ leaf(row.name) }}</span>
          <span v-if="row.head" class="ref head">HEAD</span>
          <span class="pane-head-spacer" />
          <span v-if="stale(row)" class="branch-note">{{ stale(row) }}</span>
          <span v-else-if="row.merged && !row.head" class="ref merged">Merged</span>
          <span v-if="divergence(row)" class="branch-note mono">{{ divergence(row) }}</span>
          <span class="row-actions">
            <button
              v-if="!row.head"
              class="row-action"
              :title="`Fusionner ${row.name} dans la branche courante`"
              @click.stop="mergeBranch(row.name)"
            >
              Fusionner
            </button>
            <button
              v-if="!row.head"
              class="row-action"
              :title="`Rebaser la branche courante sur ${row.name}`"
              @click.stop="rebaseOnto(row.name)"
            >
              Rebaser
            </button>
            <button
              v-if="!row.head"
              class="row-action danger"
              title="Supprimer cette branche"
              @click.stop="deleteBranch(row)"
            >
              Suppr.
            </button>
          </span>
        </button>
      </template>
    </template>

    <template v-if="refs.tags.length > 0">
      <button class="group-head as-button" @click="toggleBranchGroup('tags')">
        <span class="chevron">{{ isCollapsed("tags") ? "▸" : "▾" }}</span>
        <span>Tags</span>
        <span class="pane-head-spacer" />
        <span class="pane-head-count">{{ refs.tags.length }}</span>
      </button>
      <template v-if="!isCollapsed('tags')">
        <span v-for="tag in refs.tags" :key="tag.name" class="row branch-row nested">
          <span class="branch-name mono">{{ tag.name }}</span>
        </span>
      </template>
    </template>

    <template v-if="remotes.length > 0">
      <button class="group-head as-button" @click="toggleBranchGroup('remotes')">
        <span class="chevron">{{ isCollapsed("remotes") ? "▸" : "▾" }}</span>
        <span>Remotes</span>
        <span class="pane-head-spacer" />
        <span class="pane-head-count">{{ remotes.length }}</span>
      </button>
      <template v-if="!isCollapsed('remotes')">
        <template v-for="[remote, rows] in remotes" :key="remote">
          <div class="group-head nested">
            <span>{{ remote }}/</span>
          </div>
          <button
            v-for="row in rows"
            :key="`${row.remote}/${row.name}`"
            class="row branch-row nested twice"
            :class="{ selected: showing(`${row.remote}/${row.name}`) }"
            :title="`${row.remote}/${row.name} — clic : son historique`"
            @click="showBranchHistory(`${row.remote}/${row.name}`)"
            @dblclick="checkoutBranch(row.name)"
          >
            <span class="branch-name mono">{{ row.name }}</span>
          </button>
        </template>
      </template>
    </template>

    <form v-if="naming" class="branch-new" @submit.prevent="submit">
      <input
        v-model="name"
        type="text"
        placeholder="Nom de la branche"
        spellcheck="false"
        autofocus
        @keydown.esc="naming = false"
      />
      <button class="primary" type="submit" :disabled="name.trim() === ''">Créer</button>
    </form>
    <button v-else class="sidebar-foot" @click="naming = true">
      <span class="mono">+</span><span>Nouvelle branche</span>
    </button>
  </template>

  <p v-else-if="app.refs.status === 'failed'" class="pane-error">{{ app.refs.error }}</p>
  <p v-else class="sidebar-note">Lecture des branches…</p>
</template>

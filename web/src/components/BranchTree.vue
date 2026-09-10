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
  shown,
  toggleBranchGroup,
} from "../state";
import { count as plural, t } from "../i18n";
import Glyph from "./Glyph.vue";

/// Whether this row's history is the one History is showing, which is what a
/// click on it asked for.
function showing(name: string): boolean {
  return app.screen === "history" && app.query.branch === name;
}

/// A branch nobody has touched in this long carries its age instead of its
/// divergence — board 03's "7 mois". Ninety days, because a quarter is the
/// shortest span for which "nobody is working on this" is a fair reading.
const STALE = 90 * 86_400;

const refs = computed(() => shown(app.refs));

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
  if (tracking.gone) return t("card.gone");
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
  return months >= 12
    ? plural("branches.years", Math.round(months / 12))
    : plural("branches.months", months);
}

// ── Creating one ────────────────────────────────────────────────────────────

const naming = ref(false);
const name = ref("");

function submit(): void {
  createBranch(name.value, "", true);
  name.value = "";
  naming.value = false;
}

/// What a group's folded state is stored under.
///
/// Namespaced, because the names in this tree are not ours to choose. A
/// prefix keeps its slash, so `tags/` could never reach the `tags` section by
/// accident — but a **remote's** name is a bare word, `git remote add tags` is
/// legal, and that one did collide. The prefixes are namespaced with it so the
/// three families cannot meet at all rather than happening not to.
const key = (prefix: string): string => `prefix/${prefix}`;

/// Whether a branch's actions are in the tab order: only for the row the
/// keyboard is on (board 09). The sidebar is one stop; `j` and `k` walk it.
function stop(name: string): 0 | -1 {
  return app.branchCursor === name ? 0 : -1;
}
</script>

<template>
  <template v-if="refs">
    <!-- The tree scrolls, the foot does not. On a short window the rows used to
         be *compressed* instead: they are flex children of the sidebar, and a
         flex child shrinks before its container overflows — so `overflow-y`
         never got the chance to do anything. Here they are in a block that
         scrolls, and the button below it stays reachable. -->
    <div class="branches">
      <!-- A head with a chevron folds, or it should not carry one. This was the
         one that did not: Tags, Remotes and every `feat/` prefix folded, and
         "Branches" — the longest section of the three, the one worth folding —
         drew the same triangle and answered nothing. -->
      <button class="group-head as-button" @click="toggleBranchGroup('branches')">
        <Glyph :name="isCollapsed('branches') ? 'chevron-right' : 'chevron-down'" class="chevron" />
        <span>{{ t("branches.title") }}</span>
        <span class="pane-head-spacer" />
        <span class="pane-head-count">{{ total }}</span>
      </button>

      <template v-if="!isCollapsed('branches')">
        <!-- SPEC §13's repository with no commit: `git init` and nothing since.
         There is no branch yet — `main` is a name `HEAD` points at, not a ref —
         and a count of 0 with no rows under it reads as a tree that failed to
         load rather than as a repository waiting for its first commit. -->
        <p v-if="total === 0" class="pane-empty">{{ t("branches.empty") }}</p>

        <!-- A `<div>` for the same reason as the library row: these rows hold
         their own action buttons, and a button inside a button is invalid. -->
        <div
          v-for="row in grouped.loose"
          :key="row.name"
          class="row branch-row"
          :class="{ selected: showing(row.name) }"
          :title="t('branches.row', { branch: row.name })"
          @click="showBranchHistory(row.name)"
          @dblclick="checkoutBranch(row.name)"
        >
          <Glyph name="branch" class="branch-glyph" />
          <span class="branch-name">{{ row.name }}</span>
          <span v-if="row.head" class="ref head">HEAD</span>
          <span class="pane-head-spacer" />
          <span v-if="stale(row)" class="branch-note">{{ stale(row) }}</span>
          <span v-else-if="row.merged && !row.head" class="ref merged">{{
            t("branches.merged")
          }}</span>
          <span v-if="divergence(row)" class="branch-note mono">{{ divergence(row) }}</span>
          <span class="row-actions">
            <button
              v-if="!row.head"
              class="row-action"
              :tabindex="stop(row.name)"
              :title="t('branches.merge', { branch: row.name })"
              @click.stop="mergeBranch(row.name)"
            >
              {{ t("branches.mergeShort") }}
            </button>
            <button
              v-if="!row.head"
              class="row-action"
              :tabindex="stop(row.name)"
              :title="`Rebaser la branche courante sur ${row.name}`"
              @click.stop="rebaseOnto(row.name)"
            >
              {{ t("branches.rebaseShort") }}
            </button>
            <button
              v-if="!row.head"
              class="row-action danger"
              :tabindex="stop(row.name)"
              :title="t('branches.delete')"
              @click.stop="deleteBranch(row)"
            >
              {{ t("branches.deleteShort") }}
            </button>
          </span>
        </div>

        <template v-for="group in grouped.groups" :key="group.prefix">
          <button class="group-head as-button prefix" @click="toggleBranchGroup(key(group.prefix))">
            <Glyph
              :name="isCollapsed(key(group.prefix)) ? 'chevron-right' : 'chevron-down'"
              class="chevron"
            />
            <!-- A prefix is part of a branch's name, so it keeps its case. The
             headers around it are labels and take the section styling; board
             03 uppercases this one too, and uppercasing a name that Git treats
             case-sensitively is the kind of tidiness that misleads. -->
            <Glyph name="folder" class="branch-glyph" />
            <span class="group-name">{{ group.prefix }}</span>
            <span class="pane-head-spacer" />
            <span class="pane-head-count">{{ group.rows.length }}</span>
          </button>
          <template v-if="!isCollapsed(key(group.prefix))">
            <div
              v-for="row in group.rows"
              :key="row.name"
              class="row branch-row nested"
              :class="{ selected: showing(row.name) }"
              :title="`${row.name} — clic : son historique, double-clic : basculer dessus`"
              @click="showBranchHistory(row.name)"
              @dblclick="checkoutBranch(row.name)"
            >
              <Glyph name="branch" class="branch-glyph" />
              <span class="branch-name">{{ leaf(row.name) }}</span>
              <span v-if="row.head" class="ref head">HEAD</span>
              <span class="pane-head-spacer" />
              <span v-if="stale(row)" class="branch-note">{{ stale(row) }}</span>
              <span v-else-if="row.merged && !row.head" class="ref merged">{{
                t("branches.merged")
              }}</span>
              <span v-if="divergence(row)" class="branch-note mono">{{ divergence(row) }}</span>
              <span class="row-actions">
                <button
                  v-if="!row.head"
                  class="row-action"
                  :tabindex="stop(row.name)"
                  :title="t('branches.merge', { branch: row.name })"
                  @click.stop="mergeBranch(row.name)"
                >
                  {{ t("branches.mergeShort") }}
                </button>
                <button
                  v-if="!row.head"
                  class="row-action"
                  :tabindex="stop(row.name)"
                  :title="`Rebaser la branche courante sur ${row.name}`"
                  @click.stop="rebaseOnto(row.name)"
                >
                  {{ t("branches.rebaseShort") }}
                </button>
                <button
                  v-if="!row.head"
                  class="row-action danger"
                  :tabindex="stop(row.name)"
                  :title="t('branches.delete')"
                  @click.stop="deleteBranch(row)"
                >
                  {{ t("branches.deleteShort") }}
                </button>
              </span>
            </div>
          </template>
        </template>
      </template>

      <template v-if="refs.tags.length > 0">
        <button class="group-head as-button" @click="toggleBranchGroup('tags')">
          <Glyph :name="isCollapsed('tags') ? 'chevron-right' : 'chevron-down'" class="chevron" />
          <Glyph name="tag" class="branch-glyph" /><span>{{ t("branches.tags") }}</span>
          <span class="pane-head-spacer" />
          <span class="pane-head-count">{{ refs.tags.length }}</span>
        </button>
        <template v-if="!isCollapsed('tags')">
          <span v-for="tag in refs.tags" :key="tag.name" class="row branch-row">
            <Glyph name="tag" class="branch-glyph" />
            <span class="branch-name">{{ tag.name }}</span>
          </span>
        </template>
      </template>

      <template v-if="remotes.length > 0">
        <button class="group-head as-button" @click="toggleBranchGroup('remotes')">
          <Glyph
            :name="isCollapsed('remotes') ? 'chevron-right' : 'chevron-down'"
            class="chevron"
          />
          <span>{{ t("branches.remotes") }}</span>
          <span class="pane-head-spacer" />
          <span class="pane-head-count">{{ remotes.length }}</span>
        </button>
        <template v-if="!isCollapsed('remotes')">
          <template v-for="[remote, rows] in remotes" :key="remote">
            <!-- A remote holds branches under a prefix, which is what `feature/`
               is: the same head, so it folds the same way and its name starts
               at the same place. It was a `<div>` with no chevron, which left
               the two siblings a chevron's width out of line. -->
            <button
              class="group-head as-button prefix"
              @click="toggleBranchGroup(`remote/${remote}`)"
            >
              <Glyph
                :name="isCollapsed(`remote/${remote}`) ? 'chevron-right' : 'chevron-down'"
                class="chevron"
              />
              <Glyph name="folder" class="branch-glyph" />
              <span class="group-name">{{ remote }}/</span>
              <span class="pane-head-spacer" />
              <span class="pane-head-count">{{ rows.length }}</span>
            </button>
            <button
              v-for="row in isCollapsed(`remote/${remote}`) ? [] : rows"
              :key="`${row.remote}/${row.name}`"
              class="row branch-row nested"
              :class="{ selected: showing(`${row.remote}/${row.name}`) }"
              :title="t('branches.remoteRow', { branch: `${row.remote}/${row.name}` })"
              @click="showBranchHistory(`${row.remote}/${row.name}`)"
              @dblclick="checkoutBranch(row.name)"
            >
              <Glyph name="branch" class="branch-glyph" />
              <span class="branch-name">{{ row.name }}</span>
            </button>
          </template>
        </template>
      </template>
    </div>

    <form v-if="naming" class="branch-new" @submit.prevent="submit">
      <input
        v-model="name"
        type="text"
        :placeholder="t('branches.name')"
        spellcheck="false"
        autofocus
        @keydown.esc="naming = false"
      />
      <button class="primary" type="submit" :disabled="name.trim() === ''">
        {{ t("branches.create") }}
      </button>
    </form>
    <button v-else class="sidebar-foot" @click="naming = true">
      <span class="mono">+</span><span>{{ t("branches.new") }}</span>
    </button>
  </template>

  <p v-else-if="app.refs.status === 'failed'" class="pane-error">{{ app.refs.error }}</p>
  <p v-else class="sidebar-note">{{ t("branches.reading") }}</p>
</template>

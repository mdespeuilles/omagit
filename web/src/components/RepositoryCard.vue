<script setup lang="ts">
// Board 06's card: everything about one repository that is not its history.
//
// It reads nothing of its own. `omagit_git::Summary` was written for this card
// and gathers all of it in one pass, so the card draws what the list already
// asked for rather than asking again.

import { computed } from "vue";
import { exact, when } from "../format";
import { app, openRepository } from "../state";

const row = computed(() => app.repositories.find((entry) => entry.path === app.card) ?? null);
const summary = computed(() => {
  const path = app.card;
  if (!path) return null;
  const held = app.library[path];
  return held?.status === "ready" ? held.value : null;
});
const failure = computed(() => {
  const path = app.card;
  const held = path ? app.library[path] : null;
  return held?.status === "failed" ? held.error : null;
});

/// The status line, spelled out rather than summed: board 06 lists the four
/// kinds separately because they are four different amounts of work.
const counted = computed(() => {
  const counts = summary.value?.counts;
  if (!counts) return [];
  return [
    { label: "modifié", n: counts.modified },
    { label: "ajouté", n: counts.added },
    { label: "supprimé", n: counts.deleted },
    { label: "renommé", n: counts.renamed },
    { label: "non suivi", n: counts.untracked },
    { label: "conflit", n: counts.conflicted },
  ].filter((entry) => entry.n > 0);
});

/// The sparkline of board 06, as a bar per bucket.
///
/// Scaled to the busiest bucket, because the shape is the point and an absolute
/// scale would flatten every repository that is not the busiest one. Empty
/// buckets keep a stub so the baseline is visible, but a dim one: given the
/// same colour they line up into a solid rule that reads as a divider rather
/// than as "nothing happened here".
const spark = computed(() => {
  const buckets = summary.value?.activity ?? [];
  const peak = Math.max(1, ...buckets);
  return buckets.map((count) => ({
    height: count === 0 ? 8 : Math.max(12, Math.round((count / peak) * 100)),
    empty: count === 0,
  }));
});
</script>

<template>
  <section class="card">
    <header class="pane-head">
      <span>Dépôt</span>
      <span v-if="row" class="pane-head-title">{{ row.name }}</span>
    </header>

    <p v-if="!row" class="pane-empty">Aucun dépôt sélectionné</p>

    <template v-else>
      <div class="card-head">
        <p class="card-name">{{ row.name }}</p>
        <p v-if="row.description" class="card-note">{{ row.description }}</p>
        <p class="card-path mono">{{ row.path }}</p>

        <!-- DESIGN §4: a repository that has moved keeps its row and says so.
             It is never removed on the app's own initiative — an unmounted disk
             comes back. -->
        <p v-if="row.missing" class="card-missing">
          Le dossier n'existe plus à l'emplacement enregistré. L'entrée reste : un disque démonté
          revient.
        </p>
        <p v-else-if="failure" class="pane-error">{{ failure }}</p>

        <div class="card-actions">
          <button :disabled="row.missing" @click="openRepository(row.path)">Ouvrir</button>
        </div>
      </div>

      <template v-if="summary">
        <header class="pane-head">Working Copy</header>
        <dl class="card-facts">
          <dt>Branche</dt>
          <dd class="mono">
            {{ summary.operation ?? summary.head }}
            <span v-if="summary.tracking" class="dim">
              → {{ summary.tracking.upstream }}
              <template v-if="summary.tracking.gone">(disparu)</template>
              <template v-else>
                <template v-if="summary.tracking.ahead > 0">↑{{ summary.tracking.ahead }}</template>
                <template v-if="summary.tracking.behind > 0"
                  >↓{{ summary.tracking.behind }}</template
                >
                <template v-if="summary.tracking.ahead + summary.tracking.behind === 0">
                  à jour
                </template>
              </template>
            </span>
          </dd>

          <dt>Statut</dt>
          <dd>
            <span v-if="counted.length === 0" class="dim">propre</span>
            <span v-for="entry in counted" :key="entry.label" class="card-count">
              {{ entry.n }} {{ entry.label }}{{ entry.n > 1 ? "s" : "" }}
            </span>
          </dd>

          <dt>Stashes</dt>
          <dd>{{ summary.stashes }}</dd>

          <dt v-if="summary.last_commit">Dernier commit</dt>
          <dd v-if="summary.last_commit">
            <span class="mono">{{ summary.last_commit.id.short }}</span>
            {{ summary.last_commit.summary }}
            <span class="dim" :title="exact(summary.last_commit.when)">
              · {{ when(summary.last_commit.when) }}
            </span>
          </dd>

          <dt>Dernière ouverture</dt>
          <dd>
            <span v-if="row.last_opened" :title="exact(row.last_opened)">
              {{ when(row.last_opened) }}
            </span>
            <span v-else class="dim">jamais</span>
          </dd>

          <dt v-if="summary.committer">Identité</dt>
          <dd v-if="summary.committer">
            <span class="card-avatar">{{ summary.committer.initials }}</span>
            {{ summary.committer.name }}
            <span class="dim mono">&lt;{{ summary.committer.email }}&gt;</span>
            <!-- A per-repository identity is a deliberate act, so the inherited
                 one is worth saying out loud. -->
            <span v-if="summary.committer.inherited" class="dim">· hérité du global</span>
          </dd>

          <dt v-if="summary.commits > 0">Activité</dt>
          <dd v-if="summary.commits > 0" class="card-spark">
            <span
              v-for="(bar, at) in spark"
              :key="at"
              class="spark-bar"
              :class="{ empty: bar.empty }"
              :style="{ height: `${bar.height}%` }"
            />
            <span class="dim">{{ summary.commits }} commits</span>
          </dd>
        </dl>

        <template v-if="summary.remotes.length > 0">
          <header class="pane-head">
            <span>Remotes</span>
            <span class="pane-head-count">{{ summary.remotes.length }}</span>
          </header>
          <dl class="card-facts">
            <template v-for="remote in summary.remotes" :key="remote.name">
              <dt class="mono">{{ remote.name }}</dt>
              <dd class="mono dim">{{ remote.url ?? "aucune URL configurée" }}</dd>
            </template>
          </dl>
        </template>
      </template>
    </template>
  </section>
</template>

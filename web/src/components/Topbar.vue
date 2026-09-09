<script setup lang="ts">
// Boards 02 and 06's topbar.
//
// Two shapes, because the app has two situations. With a repository open it is
// board 02's: the window controls, the repository named on two lines, then the
// network actions and the search. With none, it is board 06's: the app's name,
// and the actions that make a repository appear.
//
// The platform reserves are spacers rather than padding (DESIGN-TOKENS §9), so
// nothing else shifts when the platform does — a control placed 12px from the
// left edge would sit under the macOS traffic lights.
//
// Actions that belong to a later milestone are drawn **disabled rather than
// hidden**: board 02 fixes the topbar's content, and an action that will exist
// reads better as not-yet than as absent.

import { computed } from "vue";
import { addRepository, app, showScreen } from "../state";
import { tildify } from "../format";

const modifier = computed(() => app.platform?.modifier_label ?? "Ctrl");

/// The second line of the repository block: where it is, and where HEAD is.
const where = computed(() => {
  const summary = app.summary;
  if (!summary) return "";
  const head = summary.operation ?? summary.head;
  const tracking = summary.tracking;
  const divergence =
    tracking && !tracking.gone
      ? [
          tracking.ahead > 0 ? `↑${tracking.ahead}` : "",
          tracking.behind > 0 ? `↓${tracking.behind}` : "",
        ]
          .filter(Boolean)
          .join(" ")
      : "";
  return [tildify(summary.path), [head, divergence].filter(Boolean).join(" ")]
    .filter(Boolean)
    .join(" · ");
});
</script>

<template>
  <header class="topbar" :class="{ compact: !app.open }">
    <span
      v-if="app.platform && app.platform.reserve.leading > 0"
      class="reserve"
      :style="{ width: `${app.platform.reserve.leading}px` }"
    />

    <template v-if="app.open && app.summary">
      <!-- Board 02: the repository, named on two lines. -->
      <button
        class="icon"
        title="Retour aux dépôts"
        aria-label="Retour aux dépôts"
        @click="showScreen('repositories')"
      >
        ◧
      </button>
      <span class="topbar-rule" />
      <span class="topbar-repo">
        <span class="topbar-repo-name mono">{{ app.summary.name }}</span>
        <span class="topbar-repo-where mono">{{ where }}</span>
      </span>
    </template>

    <template v-else>
      <span class="topbar-name mono">omagit</span>
      <span class="topbar-rule" />
      <span class="topbar-crumb">Dépôts</span>
    </template>

    <span class="topbar-spacer" />

    <span v-if="app.gitUnusable" class="banner danger">
      git indisponible — {{ app.gitUnusable }}
    </span>

    <!-- The block on the left says which repository the window is in; the
         actions on the right belong to the screen. Both are true at once on
         Dépôts with a repository open, and they answer different questions. -->
    <template v-if="app.screen !== 'repositories'">
      <!-- M7's network actions. Drawn now, disabled, because board 02 fixes
           what the topbar contains and an action that will exist reads better
           as not-yet than as absent. -->
      <button disabled title="Réseau — jalon M7">
        Fetch<span class="hint">{{ modifier }}F</span>
      </button>
      <button disabled title="Réseau — jalon M7">Pull</button>
      <button disabled title="Réseau — jalon M7">Push</button>
    </template>
    <template v-else>
      <button @click="addRepository()">
        Ajouter un dépôt local<span class="hint">{{ modifier }}O</span>
      </button>
      <button disabled title="Clonage — jalon M7">
        Cloner…<span class="hint">⇧{{ modifier }}N</span>
      </button>
    </template>

    <span class="topbar-rule" />
    <button disabled title="Palette de commandes — jalon M9">
      Rechercher<span class="hint">{{ modifier }}K</span>
    </button>

    <span
      v-if="app.platform && app.platform.reserve.trailing > 0"
      class="reserve"
      :style="{ width: `${app.platform.reserve.trailing}px` }"
    />
  </header>
</template>

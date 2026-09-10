<script setup lang="ts">
// Boards 02 and 06's topbar.
//
// Two shapes, because the app has two situations, and the situation is **which
// screen you are on** — not which repository happens to still be loaded. Inside
// a repository it is board 02's: the window controls, the repository named on
// two lines, then the network actions and the search. On Dépôts it is board
// 06's, which that board labels "topbar réduite à 40px · pas de dépôt ouvert":
// the app's name, and the actions that make a repository appear.
//
// Keyed on the open repository for a while instead, on the reasoning that the
// block says which repository the window is *in*. It does not: on Dépôts the
// window is not in one, and leaving the block there made going back look like
// it had not worked.
//
// The platform reserves are spacers rather than padding (DESIGN-TOKENS §9), so
// nothing else shifts when the platform does — a control placed 12px from the
// left edge would sit under the macOS traffic lights.
//
// Actions that belong to a later milestone are drawn **disabled rather than
// hidden**: board 02 fixes the topbar's content, and an action that will exist
// reads better as not-yet than as absent.
//
// The empty parts of the bar carry `data-tauri-drag-region`, because on Linux
// the window has no system title bar to grab (board 02: "aucune décoration
// système"). Tauri only drags when the *target* itself carries the attribute,
// so the buttons keep their clicks; what drags is the spacer, the gaps, and the
// repository block. Under Hyprland tiled none of it is reachable — the
// compositor moves windows — but on GNOME or KDE it is the only way.
//
// The window buttons that go with it are `Caption.vue`, at the edge the backend
// names.

import { computed } from "vue";
import Caption from "./Caption.vue";
import {
  addRepository,
  app,
  fetchRemote,
  openClone,
  pullRemote,
  pushBranch,
  showScreen,
} from "../state";
import { tildify } from "../format";
import { ACTIONS, hint } from "../keymap";

const modifier = computed(() => app.platform?.modifier_label ?? "Ctrl");

/// The hint a button prints, read from the table that answers the key.
///
/// Not written out beside the label: this bar has been drawing `⌘F` and `⌘O`
/// since the port beside buttons that answered the mouse and nothing else, and
/// a hint nobody can check against the binding drifts the moment one moves.
function shortcut(id: string): string {
  const action = ACTIONS.find((entry) => entry.id === id);
  return action ? hint(action.binding, modifier.value) : "";
}

/// The window buttons the app has to draw, and on which edge (board 02). None
/// until the platform has answered, and none at all on macOS.
const caption = computed(() => app.platform?.caption ?? null);
const drawsCaption = computed(
  () =>
    caption.value !== null &&
    (caption.value.minimize || caption.value.maximize || caption.value.close),
);

/// Nothing on the network while something else is.
const busy = computed(() => app.running !== null);
const tracking = computed(() => app.summary?.tracking ?? null);
const detached = computed(() => app.summary?.head.startsWith("detached") ?? false);

/// The counts are the reason to press Pull and Push.
const behind = computed(() => (tracking.value && !tracking.value.gone ? tracking.value.behind : 0));
const ahead = computed(() => (tracking.value && !tracking.value.gone ? tracking.value.ahead : 0));

/// Why a button is off, said where the button is rather than after the fact.
const cannotPush = computed(() => {
  if (app.gitUnusable) return app.gitUnusable;
  if (busy.value) return "une opération réseau est déjà en cours";
  if (detached.value) return "HEAD est détaché : il n'y a pas de branche à publier";
  return null;
});

/// Whether the window is looking *into* a repository, which is what decides the
/// topbar's whole shape.
const inRepository = computed(() => app.screen !== "repositories" && app.open !== null);

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
  <header class="topbar" :class="{ compact: !inRepository }" data-tauri-drag-region>
    <span
      v-if="app.platform && app.platform.reserve.leading > 0"
      class="reserve"
      :style="{ width: `${app.platform.reserve.leading}px` }"
    />

    <Caption v-if="drawsCaption && caption!.side === 'leading'" :caption="caption!" />

    <template v-if="inRepository && app.summary">
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
      <span class="topbar-repo" data-tauri-drag-region>
        <span class="topbar-repo-name mono">{{ app.summary.name }}</span>
        <span class="topbar-repo-where mono">{{ where }}</span>
      </span>
    </template>

    <template v-else>
      <span class="topbar-name mono">omagit</span>
      <span class="topbar-rule" />
      <span class="topbar-crumb">Dépôts</span>
    </template>

    <span class="topbar-spacer" data-tauri-drag-region />

    <span v-if="app.gitUnusable" class="banner danger">
      git indisponible — {{ app.gitUnusable }}
    </span>

    <template v-if="inRepository">
      <!-- Board 02's three, and each carries the number that is the reason to
           press it. -->
      <button :disabled="busy || !!app.gitUnusable" @click="fetchRemote()">
        Fetch<span class="hint">{{ shortcut("network.fetch") }}</span>
      </button>
      <button
        :disabled="busy || !!app.gitUnusable || !tracking"
        :title="tracking ? '' : 'Cette branche ne suit aucune branche distante'"
        @click="pullRemote()"
      >
        Pull<span v-if="behind > 0" class="hint">↓{{ behind }}</span>
      </button>
      <!-- Primary only when there is something to send: an accent-filled button
           that does nothing is the loudest thing on the screen saying the least. -->
      <button
        :class="{ primary: ahead > 0 }"
        :disabled="!!cannotPush"
        :title="cannotPush ?? ''"
        @click="pushBranch(false)"
      >
        Push<span v-if="ahead > 0" class="hint">↑{{ ahead }}</span>
      </button>
    </template>
    <template v-else>
      <button @click="addRepository()">
        Ajouter un dépôt local<span class="hint">{{ shortcut("repository.add") }}</span>
      </button>
      <button :disabled="busy || !!app.gitUnusable" @click="openClone()">
        Cloner…<span class="hint">{{ shortcut("repository.clone") }}</span>
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

    <Caption v-if="drawsCaption && caption!.side === 'trailing'" :caption="caption!" />
  </header>
</template>

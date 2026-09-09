<script setup lang="ts">
// Board 07's conflict dialog: the file, its conflicts one after another, and
// three answers for each.
//
// It draws what is *in the file* rather than a diff. `git` has already written
// both versions between its markers, so the two bands here are the two sides as
// they will be kept — and "Both" keeps them in the order it shows them, ours
// first, which is the only way that button can be read at a glance.
//
// The sides are named by their branch and not by the pronoun. `ours` and
// `theirs` mean the opposite of what they read like during a rebase, and board
// 07 asks for the names for exactly that reason.

import { computed, onBeforeUnmount, onMounted } from "vue";
import {
  app,
  applyResolution,
  chooseSide,
  closeConflict,
  conflictSettled,
  nextConflict,
  openInEditor,
} from "../state";

const resolving = computed(() => app.resolving);
const body = computed(() =>
  app.resolving?.body.status === "ready" ? app.resolving.body.value : null,
);

/// What each band is called: the branch the operation named, falling back to
/// the label `git` wrote into the file itself.
function name(side: "ours" | "theirs", label: string): string {
  const sides = app.sides;
  const named = side === "ours" ? sides?.ours : sides?.theirs;
  // `HEAD` is what git writes on our side when there is nothing better to say;
  // the branch is better to say.
  return named ?? (label === "HEAD" ? "la version en place" : label);
}

/// The label `git` wrote into the file, when it adds anything to the name.
///
/// It usually does not: on our side it writes `HEAD`, which the band already
/// says better, and on theirs it writes the branch — the very name resolved
/// above it. Printed anyway, the band read "Theirs — feature/theme-runtime
/// feature/theme-runtime".
function marker(side: "ours" | "theirs", label: string): string {
  if (label === "" || label === "HEAD") return "";
  return label === name(side, label) ? "" : label;
}

function chosen(index: number): string | null {
  return app.resolving?.choices[index] ?? null;
}

function onKey(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    event.preventDefault();
    closeConflict();
    return;
  }
  // Board 07's `n`, and the primary's ⌘⏎. Both belong to this dialog rather
  // than to the keymap: M9 makes bindings reassignable, and a dialog that is on
  // screen answers its own keys in the meantime.
  if (event.key === "n" && !(event.target instanceof HTMLInputElement)) {
    event.preventDefault();
    nextConflict();
  }
  if (event.key === "Enter" && (event.metaKey || event.ctrlKey) && conflictSettled()) {
    event.preventDefault();
    applyResolution();
  }
}

onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div v-if="resolving" class="overlay" @click.self="closeConflict()">
    <section class="dialog form conflict-dialog" role="dialog" aria-modal="true">
      <header class="dialog-head">
        <span class="dialog-title">Résoudre un conflit</span>
        <span class="mono dim">{{ resolving.file }}</span>
        <span class="pane-head-spacer" />
        <template v-if="body && body.regions > 0">
          <span class="dim">conflit {{ resolving.at + 1 }} / {{ body.regions }}</span>
          <button class="link" :disabled="body.regions < 2" @click="nextConflict()">
            conflit suivant <span class="hint">n</span>
          </button>
        </template>
      </header>

      <p v-if="resolving.body.status === 'loading'" class="pane-empty">Lecture du fichier…</p>
      <p v-else-if="resolving.body.status === 'failed'" class="pane-error mono">
        {{ resolving.body.error }}
      </p>

      <!-- A file somebody already settled by hand: no markers left, and the
           only thing missing is the `git add`. Said rather than drawn as an
           empty panel. -->
      <p v-else-if="body && body.regions === 0" class="pane-empty">
        Ce fichier ne contient plus de marqueurs de conflit — il a été résolu ailleurs. « Marquer
        résolu et indexer » suffit.
      </p>

      <div v-if="body" class="conflict-body">
        <template v-for="(segment, at) in body.segments" :key="at">
          <div v-if="segment.kind === 'agreed'" class="conflict-agreed">
            <div v-for="(line, offset) in segment.lines" :key="offset" class="conflict-line">
              <span class="gutter mono">{{ segment.start + offset }}</span>
              <span class="sign" />
              <span class="text mono">{{ line }}</span>
            </div>
          </div>

          <div
            v-else
            class="conflict-region"
            :class="{ current: resolving.at === segment.index, answered: !!chosen(segment.index) }"
          >
            <header class="conflict-choices">
              <span class="dim mono">@@ ligne {{ segment.start }} @@</span>
              <span class="pane-head-spacer" />
              <button
                :class="{ primary: chosen(segment.index) === 'ours' }"
                :title="`Garder la version de ${name('ours', segment.ours_label)} (ours)`"
                @click="chooseSide(segment.index, 'ours')"
              >
                Ours
              </button>
              <button
                :class="{ primary: chosen(segment.index) === 'theirs' }"
                :title="`Garder la version de ${name('theirs', segment.theirs_label)} (theirs)`"
                @click="chooseSide(segment.index, 'theirs')"
              >
                Theirs
              </button>
              <button
                :class="{ primary: chosen(segment.index) === 'both' }"
                title="Garder les deux côtés, Ours puis Theirs"
                @click="chooseSide(segment.index, 'both')"
              >
                Both
              </button>
            </header>

            <div class="conflict-side">
              <span class="conflict-side-name">
                Ours — {{ name("ours", segment.ours_label) }}
                <span v-if="marker('ours', segment.ours_label)" class="mono dim">
                  {{ marker("ours", segment.ours_label) }}
                </span>
              </span>
            </div>
            <div
              v-for="(line, offset) in segment.ours"
              :key="`o${offset}`"
              class="conflict-line added"
            >
              <span class="gutter mono" />
              <span class="sign mono">+</span>
              <span class="text mono">{{ line }}</span>
            </div>

            <!-- Under `merge.conflictStyle = diff3` only, and never a choice:
                 it is what both sides started from, which is context for the
                 decision rather than one of its answers. -->
            <template v-if="segment.base">
              <div class="conflict-side">
                <span class="conflict-side-name dim">Base — l'ancêtre commun</span>
              </div>
              <div v-for="(line, offset) in segment.base" :key="`b${offset}`" class="conflict-line">
                <span class="gutter mono" />
                <span class="sign" />
                <span class="text mono dim">{{ line }}</span>
              </div>
            </template>

            <div class="conflict-side">
              <span class="conflict-side-name">
                Theirs — {{ name("theirs", segment.theirs_label) }}
                <span v-if="marker('theirs', segment.theirs_label)" class="mono dim">
                  {{ marker("theirs", segment.theirs_label) }}
                </span>
              </span>
            </div>
            <div
              v-for="(line, offset) in segment.theirs"
              :key="`t${offset}`"
              class="conflict-line removed"
            >
              <span class="gutter mono" />
              <span class="sign mono">−</span>
              <span class="text mono">{{ line }}</span>
            </div>
          </div>
        </template>
      </div>

      <footer class="dialog-foot">
        <button @click="openInEditor()">Ouvrir dans l'éditeur</button>
        <span class="dialog-foot-spacer" />
        <button @click="closeConflict()">Annuler</button>
        <button
          class="primary"
          :disabled="!conflictSettled() || !!app.busy"
          :title="conflictSettled() ? '' : 'Chaque conflit attend une réponse'"
          @click="applyResolution()"
        >
          Marquer résolu et indexer
        </button>
      </footer>
    </section>
  </div>
</template>

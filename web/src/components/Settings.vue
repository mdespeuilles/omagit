<script setup lang="ts">
// The Preferences screen — the one DESIGN §7 lists as not yet designed, so it
// is built from the vocabulary the rest of the window already uses: pane heads,
// rows, the same toggles as the commit box, no invented control.
//
// Two rules shape what it shows.
//
// **A source this machine cannot offer is drawn disabled with the reason**, not
// hidden. "There is no Omarchy here" is an answer; an option that vanishes is
// one nobody can ask about — and this is the screen where somebody comes
// *because* the theme is not what they expected.
//
// **It says what is on screen, not only what was asked for.** SPEC §6.1's
// sources fall back: asking for Omarchy on a machine without it resolves to
// something else, and a screen showing the request rather than the answer would
// be lying about what you are looking at.

import { computed, onBeforeUnmount, ref } from "vue";
import {
  app,
  chooseDensity,
  chooseScale,
  chooseTheme,
  readPreferences,
  setBinding,
  toggleShortcuts,
} from "../state";
import { ACTIONS, binding, capture, hint, reassigned, refuse } from "../keymap";

const prefs = computed(() => (app.preferences.status === "ready" ? app.preferences.value : null));

/// The scale in whole percentages, which is what a person thinks in.
const percent = computed(() => Math.round((prefs.value?.scale ?? 1) * 100));

function scaleBy(step: number): void {
  const wanted = Math.min(Math.max((prefs.value?.scale ?? 1) + step, 0.8), 2);
  chooseScale(Number(wanted.toFixed(2)));
}

// ── Reassignment (SPEC §11) ───────────────────────────────────────────────
//
// The way to say which keys you want is to press them, so a row being changed
// listens on the window: a key press is not something a text box could report
// without inventing a spelling for it, and half the interesting bindings —
// `⌘,`, `⌘.` — are ones the box would swallow.

/// The action whose binding is being pressed, or `null`.
const capturing = ref<string | null>(null);
/// Why the last press was refused, printed under the row it was refused for.
const refused = ref<string | null>(null);

const modifier = computed(() => app.platform?.modifier_label ?? "Ctrl");
const primary = computed(() => (app.platform?.modifier === "command" ? "meta" : "control"));

function listenFor(id: string): void {
  if (capturing.value === id) return stopCapture();
  capturing.value = id;
  refused.value = null;
  // Captured, so nothing else in the window answers the press being offered —
  // `⌘W` while listening must not close the window, `Esc` must cancel here
  // rather than in the shell.
  window.addEventListener("keydown", onKey, true);
}

function stopCapture(): void {
  capturing.value = null;
  refused.value = null;
  window.removeEventListener("keydown", onKey, true);
}

// A screen that vanished mid-capture would leave a handler on the window
// answering keys for a row nobody can see.
onBeforeUnmount(stopCapture);

function onKey(event: KeyboardEvent): void {
  const id = capturing.value;
  if (!id) return;
  event.preventDefault();
  event.stopPropagation();
  if (event.key === "Escape") return stopCapture();

  const chosen = capture(event, primary.value);
  // A modifier on its own is the first half of an answer: keep listening.
  if (!chosen) return;

  const no = refuse(id, chosen);
  if (no) {
    refused.value = `${hint(chosen, modifier.value)} : ${no}`;
    return;
  }
  setBinding(id, chosen);
  stopCapture();
}

function reset(id: string): void {
  setBinding(id, null);
  stopCapture();
}

/// Whether a source is the one in force. `automatic` is a request, never an
/// answer: it means "whatever this machine offers", so the row that is *on* is
/// the one it resolved to.
function on(source: string, name = ""): boolean {
  if (!prefs.value) return false;
  if (source === "user-override")
    return prefs.value.source === source && prefs.value.theme === name;
  if (source === "embedded-dark")
    return prefs.value.source === "embedded" && prefs.value.mode === "dark";
  if (source === "embedded-light")
    return prefs.value.source === "embedded" && prefs.value.mode === "light";
  return prefs.value.source === source;
}
</script>

<template>
  <section class="settings">
    <header class="pane-head">
      <span>Réglages</span>
      <span class="pane-head-spacer" />
      <button class="link" @click="readPreferences()">Relire</button>
    </header>

    <p v-if="app.preferences.status === 'loading'" class="pane-empty">Lecture des réglages…</p>
    <p v-else-if="app.preferences.status === 'failed'" class="pane-error mono">
      {{ app.preferences.error }}
    </p>

    <div v-else-if="prefs" class="settings-body">
      <section class="settings-block">
        <h2 class="settings-title">Thème</h2>
        <p class="settings-note">
          À l'écran en ce moment : <strong>{{ prefs.resolved }}</strong
          >. Un seul choix ici : suivre quelque chose, ou nommer un thème — nommer un thème arrête
          tout suivi.
        </p>

        <!-- One question, one answer. The five sources and the eight themes set
             the same setting, and drawing them as two lists let both look
             chosen at once. They are one radio group now: a mark on every
             option, exactly one of them filled, whichever half it is in. -->
        <div class="settings-picks" role="radiogroup" aria-label="Thème">
          <button
            class="row settings-row"
            role="radio"
            :aria-checked="on('automatic')"
            :class="{ selected: on('automatic') }"
            @click="chooseTheme('automatic')"
          >
            <span class="pick" :class="{ on: on('automatic') }" aria-hidden="true" />
            <span>Automatique</span>
            <span class="settings-detail">
              la première source qui répond : Omarchy, puis le système, sinon le thème embarqué
            </span>
          </button>

          <button
            class="row settings-row"
            role="radio"
            :aria-checked="on('omarchy')"
            :class="{ selected: on('omarchy') }"
            :disabled="!prefs.omarchy"
            @click="chooseTheme('omarchy')"
          >
            <span class="pick" :class="{ on: on('omarchy') }" aria-hidden="true" />
            <span>Suivre Omarchy</span>
            <span class="settings-detail">
              {{
                prefs.omarchy
                  ? "la palette Quattro, suivie en direct"
                  : "aucun Omarchy sur cette machine"
              }}
            </span>
          </button>

          <button
            class="row settings-row"
            role="radio"
            :aria-checked="on('system-appearance')"
            :class="{ selected: on('system-appearance') }"
            :disabled="!prefs.system_appearance"
            @click="chooseTheme('system-appearance')"
          >
            <span class="pick" :class="{ on: on('system-appearance') }" aria-hidden="true" />
            <span>Suivre le système</span>
            <span class="settings-detail">
              {{
                prefs.system_appearance
                  ? "clair ou sombre, bascule comprise"
                  : "cette plateforme ne rapporte pas de préférence"
              }}
            </span>
          </button>

          <button
            class="row settings-row"
            role="radio"
            :aria-checked="on('embedded-dark')"
            :class="{ selected: on('embedded-dark') }"
            @click="chooseTheme('embedded-dark')"
          >
            <span class="pick" :class="{ on: on('embedded-dark') }" aria-hidden="true" />
            <span>Embarqué — sombre</span>
            <span class="settings-detail">le thème sombre par défaut, sans rien suivre</span>
          </button>

          <button
            class="row settings-row"
            role="radio"
            :aria-checked="on('embedded-light')"
            :class="{ selected: on('embedded-light') }"
            @click="chooseTheme('embedded-light')"
          >
            <span class="pick" :class="{ on: on('embedded-light') }" aria-hidden="true" />
            <span>Embarqué — clair</span>
            <span class="settings-detail">le thème clair par défaut, sans rien suivre</span>
          </button>

          <h3 class="settings-subtitle">Ou un thème précis</h3>
          <div class="settings-catalogue">
            <button
              v-for="theme in prefs.catalogue"
              :key="theme.name"
              class="row settings-swatch"
              role="radio"
              :aria-checked="on('user-override', theme.name)"
              :class="{ selected: on('user-override', theme.name) }"
              @click="chooseTheme('user-override', theme.name)"
            >
              <span
                class="pick"
                :class="{ on: on('user-override', theme.name) }"
                aria-hidden="true"
              />
              <span>{{ theme.name }}</span>
              <span class="pane-head-spacer" />
              <span class="settings-detail">{{ theme.mode === "light" ? "clair" : "sombre" }}</span>
            </button>
          </div>
        </div>
      </section>

      <section class="settings-block">
        <h2 class="settings-title">Densité</h2>
        <p class="settings-note">
          Board 08 : la densité ne change ni la taille de police ni la graisse — elle change les
          hauteurs de ligne, les marges et les écarts. Aucune cible cliquable ne descend sous 24×24
          px, même en compact.
        </p>
        <div class="settings-choice">
          <button
            class="row settings-row"
            :class="{ selected: prefs.density === 'comfortable' }"
            @click="chooseDensity('comfortable')"
          >
            <span>Confortable</span>
            <span class="settings-detail">ce que les maquettes dessinent</span>
          </button>
          <button
            class="row settings-row"
            :class="{ selected: prefs.density === 'compact' }"
            @click="chooseDensity('compact')"
          >
            <span>Compact</span>
            <span class="settings-detail">à côté d'un terminal dense</span>
          </button>
        </div>
      </section>

      <section class="settings-block">
        <h2 class="settings-title">Échelle</h2>
        <p class="settings-note">
          L'échelle de type est un contrat (DESIGN-TOKENS §8) : les proportions ne bougent pas, tout
          grandit ensemble. Ce qui change est la taille absolue, qui est une propriété de l'écran et
          non du dessin.
        </p>
        <div class="settings-scale">
          <button :disabled="percent <= 80" @click="scaleBy(-0.05)">−</button>
          <span class="settings-percent mono">{{ percent }} %</span>
          <button :disabled="percent >= 200" @click="scaleBy(0.05)">+</button>
          <button class="link" @click="chooseScale(1.15)">Défaut</button>
        </div>
      </section>

      <section class="settings-block">
        <h2 class="settings-title">Clavier</h2>
        <p class="settings-note">
          Une seule table, lue par quatre choses : le clavier, la palette, la feuille des raccourcis
          et la barre de menus. Changer une liaison ici les change toutes les quatre.
        </p>
        <button class="row settings-row" @click="toggleShortcuts()">
          <span>Voir tous les raccourcis</span>
          <span class="settings-detail">?</span>
        </button>

        <ol class="keymap-list">
          <li v-for="action in ACTIONS" :key="action.id" class="keymap-row">
            <div class="row settings-row" :class="{ listening: capturing === action.id }">
              <span>{{ action.label }}</span>
              <span class="pane-head-spacer" />
              <span v-if="reassigned(action)" class="keymap-was mono">
                {{ hint(action.binding, modifier) }}
              </span>
              <button
                class="keymap-key mono"
                :class="{ listening: capturing === action.id }"
                @click="listenFor(action.id)"
              >
                {{ capturing === action.id ? "Appuie…" : hint(binding(action), modifier) }}
              </button>
              <button
                class="link"
                :disabled="!reassigned(action)"
                title="Remettre la liaison de départ"
                @click="reset(action.id)"
              >
                Défaut
              </button>
            </div>
            <p v-if="capturing === action.id && refused" class="keymap-refused">{{ refused }}</p>
            <p v-else-if="capturing === action.id" class="settings-note keymap-hint">
              Appuie sur la combinaison voulue. Échap annule.
            </p>
          </li>
        </ol>
      </section>

      <section class="settings-block">
        <h2 class="settings-title">Git</h2>
        <p class="settings-note">
          Ce que l'app lit de ta configuration, et ce qu'elle en fait. Rien ici ne s'écrit : `git
          config` reste le seul endroit où ces choix se prennent.
        </p>
        <dl class="settings-facts">
          <dt>Binaire</dt>
          <dd class="mono">{{ prefs.git }}</dd>
          <dt>Éditeur</dt>
          <dd class="mono">{{ prefs.editor }}</dd>
          <dt>Identifiants</dt>
          <dd class="mono">{{ prefs.credential_helper }}</dd>
        </dl>
      </section>
    </div>
  </section>
</template>

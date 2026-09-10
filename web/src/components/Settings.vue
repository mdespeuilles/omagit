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

import { computed } from "vue";
import { app, chooseDensity, chooseScale, chooseTheme, readPreferences } from "../state";

const prefs = computed(() => (app.preferences.status === "ready" ? app.preferences.value : null));

/// The scale in whole percentages, which is what a person thinks in.
const percent = computed(() => Math.round((prefs.value?.scale ?? 1) * 100));

function scaleBy(step: number): void {
  const wanted = Math.min(Math.max((prefs.value?.scale ?? 1) + step, 0.8), 2);
  chooseScale(Number(wanted.toFixed(2)));
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
          >. Les sources sont celles de SPEC §6.1, dans leur ordre de priorité — la première qui
          répond gagne.
        </p>

        <button
          class="row settings-row"
          :class="{ selected: on('automatic') }"
          @click="chooseTheme('automatic')"
        >
          <span>Automatique</span>
          <span class="settings-detail">ce que cette machine offre de mieux</span>
        </button>

        <button
          class="row settings-row"
          :class="{ selected: on('omarchy') }"
          :disabled="!prefs.omarchy"
          @click="chooseTheme('omarchy')"
        >
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
          :class="{ selected: on('system-appearance') }"
          :disabled="!prefs.system_appearance"
          @click="chooseTheme('system-appearance')"
        >
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
          :class="{ selected: on('embedded-dark') }"
          @click="chooseTheme('embedded-dark')"
        >
          <span>Embarqué — sombre</span>
          <span class="settings-detail">le thème sombre par défaut, sans rien suivre</span>
        </button>

        <button
          class="row settings-row"
          :class="{ selected: on('embedded-light') }"
          @click="chooseTheme('embedded-light')"
        >
          <span>Embarqué — clair</span>
          <span class="settings-detail">le thème clair par défaut, sans rien suivre</span>
        </button>

        <h3 class="settings-subtitle">Ou un thème précis, qui désactive tout suivi</h3>
        <div class="settings-catalogue">
          <button
            v-for="theme in prefs.catalogue"
            :key="theme.name"
            class="row settings-swatch"
            :class="{ selected: on('user-override', theme.name) }"
            @click="chooseTheme('user-override', theme.name)"
          >
            <span>{{ theme.name }}</span>
            <span class="settings-detail">{{ theme.mode === "light" ? "clair" : "sombre" }}</span>
          </button>
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

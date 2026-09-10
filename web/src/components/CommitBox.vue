<script setup lang="ts">
// Board 03's commit box: the message, the three flags, who it will be from,
// and the two buttons.
//
// The rule that shapes it is SPEC §11: everything that can stop a commit is
// said *before* the message is typed. `git` has no identity, `git` is missing,
// nothing is staged — each of those is on screen while the box is still empty,
// because an app that accepts a paragraph and then refuses it has wasted the
// only part of this the user actually wrote.

import { computed } from "vue";
import {
  app,
  canCommit,
  commit,
  setAmend,
  setMessage,
  setNoVerify,
  setSignOff,
  stagedCount,
  stageEverything,
  plural,
} from "../state";

/// Git's own conventions, and the only two numbers in this component: a
/// subject over 50 columns is long, over 72 it no longer fits the tools that
/// read it.
const SUBJECT_COMFORTABLE = 50;
const SUBJECT_LIMIT = 72;

const subject = computed(() => app.message.split("\n", 1)[0] ?? "");

/// The counter appears only past the comfortable length: a number that is
/// always there is a number nobody reads (DESIGN §3).
const counter = computed(() => {
  const length = subject.value.length;
  if (length <= SUBJECT_COMFORTABLE) return null;
  return { length, over: length > SUBJECT_LIMIT };
});

const staged = computed(() => stagedCount());

const label = computed(() =>
  app.amend ? "Corriger le commit" : `Commiter ${plural(staged.value, "fichier")}`,
);

/// Why the button is off, in the words of the thing that is wrong. An
/// explanation belongs next to the control it disables, not in a dialog after
/// the fact.
/// What is worth saying before a commit that will work anyway.
///
/// A detached `HEAD` was promised a warning at M5 — `Head::Detached` says so in
/// `repo.rs` — and never got one: the commit is made, no branch moves, and it
/// is reachable only by hash until something else points at it. Not blocked,
/// because it is a legitimate thing to do; said, because it is not what most
/// people mean.
const caution = computed(() => {
  if (app.summary?.head_kind !== "detached") return null;
  return `${app.summary.head} : ce commit n'appartiendra à aucune branche.`;
});

const blocked = computed(() => {
  if (app.gitUnusable) return `git indisponible — ${app.gitUnusable}`;
  // Nothing to amend in a repository whose first commit has not been made.
  if (app.amend && app.summary?.head_kind === "unborn") {
    return "Aucun commit à corriger : celui-ci sera le premier";
  }
  if (!app.committer)
    return "Aucune identité Git : git config --global user.name && git config --global user.email";
  if (staged.value === 0 && !app.amend) return "Rien n'est indexé";
  if (app.message.trim() === "") return "Le message est vide";
  return null;
});

const modifier = computed(() => app.platform?.modifier_label ?? "Ctrl");
</script>

<template>
  <section class="commit">
    <div class="commit-field">
      <textarea
        class="commit-message mono"
        :value="app.message"
        :placeholder="app.amend ? 'Message du commit corrigé' : 'Message du commit'"
        spellcheck="false"
        @input="setMessage(($event.target as HTMLTextAreaElement).value)"
        @keydown.meta.enter.prevent="commit()"
        @keydown.ctrl.enter.prevent="commit()"
      />
      <!-- The 72-column rule is the right edge of the field itself, never a
           line floating in the middle of the text (DESIGN §3). -->
      <span class="commit-rule">{{ SUBJECT_LIMIT }}</span>
      <span v-if="counter" class="commit-count mono" :class="{ over: counter.over }">
        {{ counter.length }}
      </span>
    </div>

    <div class="commit-flags">
      <label class="toggle">
        <input
          type="checkbox"
          :checked="app.amend"
          @change="setAmend(($event.target as HTMLInputElement).checked)"
        /><span class="check" aria-hidden="true">✓</span>
        Amend
      </label>
      <label class="toggle">
        <input
          type="checkbox"
          :checked="app.signOff"
          @change="setSignOff(($event.target as HTMLInputElement).checked)"
        /><span class="check" aria-hidden="true">✓</span>
        Sign off
      </label>
      <label class="toggle" title="Ignore les hooks pre-commit et commit-msg">
        <input
          type="checkbox"
          :checked="app.noVerify"
          @change="setNoVerify(($event.target as HTMLInputElement).checked)"
        /><span class="check" aria-hidden="true">✓</span>
        No verify
      </label>
    </div>

    <div class="commit-line">
      <span class="commit-who mono" :class="{ missing: !app.committer }">
        {{ app.committer ?? "aucune identité configurée" }}
      </span>
      <span class="commit-hint">{{ modifier }}⏎</span>
    </div>

    <div class="commit-actions">
      <button :disabled="!!app.busy || staged === 0" @click="stageEverything(true)">
        Tout désindexer
      </button>
      <!-- No `title`: the reason sits one line below, on screen, permanently.
           A tooltip repeating it adds nothing — and WKWebView drew this one in
           the window's top-left corner after the button under the pointer
           changed, which is a sentence about staging floating over the traffic
           lights. -->
      <button class="primary" :disabled="!canCommit()" @click="commit()">
        {{ label }}
      </button>
    </div>

    <p v-if="caution" class="commit-caution">{{ caution }}</p>
    <p v-if="blocked" class="commit-blocked">{{ blocked }}</p>
  </section>
</template>

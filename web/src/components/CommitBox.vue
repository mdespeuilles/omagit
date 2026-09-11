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
  canDraft,
  commit,
  draftMessage,
  hasAgent,
  setAmend,
  setMessage,
  setNoVerify,
  setSignOff,
  stagedCount,
  stageEverything,
} from "../state";
import { count, t } from "../i18n";
import Glyph from "./Glyph.vue";

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

/// What the "Draft" button says it will do, which changes with the box.
///
/// A message already typed is work, and a button that silently replaces it is
/// the loss SPEC §3 rule 7 is about arriving through a control nobody thinks of
/// as destructive. It is not blocked — asking again after an edit is exactly
/// what somebody does — but it says so before the click.
const draftTitle = computed(() => {
  if (staged.value === 0) return t("commit.draftNothing");
  const agent = app.agent;
  return app.message.trim() === ""
    ? t("commit.draftTitle", { agent })
    : t("commit.draftReplace", { agent });
});

const label = computed(() => (app.amend ? t("commit.doAmend") : count("commit.do", staged.value)));

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
  return t("commit.detached", { head: app.summary.head });
});

const blocked = computed(() => {
  if (app.gitUnusable) return t("commit.blockedGit", { reason: app.gitUnusable });
  // Nothing to amend in a repository whose first commit has not been made.
  if (app.amend && app.summary?.head_kind === "unborn") {
    return t("commit.blockedUnborn");
  }
  if (!app.committer) return t("commit.blockedIdentity");
  if (staged.value === 0 && !app.amend) return t("commit.blockedEmpty");
  if (app.message.trim() === "") return t("commit.blockedMessage");
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
        :placeholder="app.amend ? t('commit.amendMessage') : t('commit.message')"
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
        {{ t("commit.amend") }}
      </label>
      <label class="toggle">
        <input
          type="checkbox"
          :checked="app.signOff"
          @change="setSignOff(($event.target as HTMLInputElement).checked)"
        /><span class="check" aria-hidden="true">✓</span>
        {{ t("commit.signOff") }}
      </label>
      <label class="toggle" :title="t('commit.noVerifyTitle')">
        <input
          type="checkbox"
          :checked="app.noVerify"
          @change="setNoVerify(($event.target as HTMLInputElement).checked)"
        /><span class="check" aria-hidden="true">✓</span>
        {{ t("commit.noVerify") }}
      </label>
    </div>

    <div class="commit-line">
      <span class="commit-who mono" :class="{ missing: !app.committer }">
        {{ app.committer ?? t("commit.noIdentity") }}
      </span>
      <span class="commit-hint">{{ modifier }}⏎</span>
    </div>

    <div class="commit-actions">
      <button :disabled="!!app.busy || staged === 0" @click="stageEverything(true)">
        {{ t("status.unstageAll") }}
      </button>
      <!-- No `title`: the reason sits one line below, on screen, permanently.
           A tooltip repeating it adds nothing — and WKWebView drew this one in
           the window's top-left corner after the button under the pointer
           changed, which is a sentence about staging floating over the traffic
           lights. -->
      <!-- Only when an agent is configured. An affordance for a feature that is
           off is the dead code SPEC §2 forbids, wearing a button. -->
      <button v-if="hasAgent()" :disabled="!canDraft()" :title="draftTitle" @click="draftMessage()">
        <Glyph name="draft" />{{ t("commit.draft") }}
      </button>
      <button class="primary" :disabled="!canCommit()" @click="commit()">
        {{ label }}
      </button>
    </div>

    <p v-if="caution" class="commit-caution">{{ caution }}</p>
    <p v-if="blocked" class="commit-blocked">{{ blocked }}</p>
  </section>
</template>

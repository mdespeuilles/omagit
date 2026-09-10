<script setup lang="ts">
// A write that did not happen, said where it can be read.
//
// It lived in the status bar — 22 pixels at the foot of the window, one line,
// cut wherever it ran out. `git`'s refusals are the useful half of this
// application's error messages and they are two hundred characters long:
// "your local changes to the following files would be overwritten by merge:
// src/render.rs". Truncated to the width of a bar, what survives is the part
// the reader already knew.
//
// Not a modal, and that is a decision rather than an omission. A modal blocks,
// and what someone wants on reading that sentence is to go and look at the file
// it names. Worse, the modals of this app are the confirmations of SPEC §3
// rule 7 — the questions that must be answered before something is lost. Using
// the same shape to *report* teaches people to dismiss them, and the one that
// matters goes with the rest.
//
// So: a band, in the vocabulary the progress overlay already established. It
// wraps rather than truncates, it stays until it is dismissed or until the next
// write succeeds, and it points at the journal, where the exact command line is.

import { app, dismissWriteError, toggleJournal } from "../state";
</script>

<template>
  <aside v-if="app.writeError" class="notice" role="alert">
    <span class="notice-mark" aria-hidden="true">✗</span>
    <span class="notice-body">
      <span class="notice-what">{{ app.writeError.what }}</span>
      <span class="notice-said mono">{{ app.writeError.said }}</span>
    </span>
    <button class="link" @click="toggleJournal()">Journal</button>
    <button class="icon" title="Fermer" aria-label="Fermer" @click="dismissWriteError()">✕</button>
  </aside>
</template>

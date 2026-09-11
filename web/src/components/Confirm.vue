<script setup lang="ts">
// The confirmation SPEC §3 rule 7 requires, and the only modal in the app.
//
// Three things make it a real confirmation rather than a speed bump: it names
// what is at stake in the title, it says why the loss is final in the body, and
// its default is Cancel — the destructive button is never what Enter presses.

import { onMounted, onBeforeUnmount, ref } from "vue";
import { answer, answerAlternative, app } from "../state";
import { t } from "../i18n";

const cancel = ref<HTMLButtonElement | null>(null);

function onKey(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    event.preventDefault();
    answer(false);
  }
}

onMounted(() => {
  cancel.value?.focus();
  window.addEventListener("keydown", onKey);
});
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div class="overlay" @click.self="answer(false)">
    <div class="dialog" role="alertdialog" aria-modal="true">
      <h2 class="dialog-title">{{ app.question?.title }}</h2>
      <p class="dialog-detail">{{ app.question?.detail }}</p>
      <div class="dialog-actions">
        <button ref="cancel" @click="answer(false)">{{ t("ask.cancel") }}</button>
        <!-- A second way of doing it, for the question that is a choice rather
             than a warning: neither merging nor rebasing is the dangerous one,
             so neither wears `danger`. -->
        <button v-if="app.question?.alternative" @click="answerAlternative()">
          {{ app.question.alternative }}
        </button>
        <button
          :class="app.question?.alternative ? 'primary' : 'danger solid'"
          @click="answer(true)"
        >
          {{ app.question?.verb }}
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
// Making a tag: a name, what it goes on, and a message that decides what kind
// of tag it is.
//
// The kind is not a choice of its own, and that is deliberate. `git` decides it
// by whether there is a message — `-m` implies `-a` — and offering a separate
// "annotated" switch would let the two disagree: an annotated tag with no
// message is a prompt for an editor that never opens here, and a lightweight
// one with a message is a message thrown away. So the box says which one you
// are about to make, under the field that decides it.
//
// A name already taken is not pre-empted either. `git` refuses it and names the
// tag in the way, which is better than anything composed here (SPEC §3 rule 3),
// and the refusal is what offers to move it — because "move it" is a question
// nobody can answer before being told there is something to move.

import { computed, onMounted, onBeforeUnmount, ref } from "vue";
import { app, closeTag, createTag, setTagField, setTagForce } from "../state";
import { t } from "../i18n";

const field = ref<HTMLInputElement | null>(null);

const form = computed(() => app.tagging);
const annotated = computed(() => (form.value?.message ?? "").trim() !== "");

function onKey(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    event.preventDefault();
    closeTag();
  }
}

onMounted(() => {
  // On the name: it is the only field with nothing in it that cannot be
  // guessed, and the commit is already decided by where this was opened from.
  field.value?.focus();
  window.addEventListener("keydown", onKey);
});
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div v-if="form" class="overlay" @click.self="closeTag()">
    <form
      class="dialog form"
      role="dialog"
      aria-modal="true"
      :aria-label="t('tag.title')"
      @submit.prevent="createTag()"
    >
      <div class="dialog-head">{{ t("tag.title") }}</div>

      <div class="dialog-body">
        <label class="dialog-label" for="tag-name">{{ t("tag.name") }}</label>
        <input
          id="tag-name"
          ref="field"
          type="text"
          class="mono"
          spellcheck="false"
          autocomplete="off"
          :placeholder="t('tag.namePlaceholder')"
          :value="form.name"
          @input="setTagField('name', ($event.target as HTMLInputElement).value)"
        />

        <!-- What it goes on, said rather than chosen: this dialog is opened
             from a place that already knows — the sidebar means the current
             commit, a commit row means that one. A field here would be a second
             way to say something already said. -->
        <span class="dialog-label">{{ t("tag.at") }}</span>
        <span class="mono dim">{{ form.where }}</span>

        <label class="dialog-label" for="tag-message">{{ t("tag.message") }}</label>
        <textarea
          id="tag-message"
          rows="3"
          :placeholder="t('tag.messagePlaceholder')"
          :value="form.message"
          @input="setTagField('message', ($event.target as HTMLTextAreaElement).value)"
        />

        <span></span>
        <span class="tag-kind">{{
          annotated ? t("tag.kindAnnotated") : t("tag.kindLightweight")
        }}</span>

        <!-- Only after `git` has refused: its own words, and the offer that
             those words make possible. -->
        <template v-if="form.refused">
          <span></span>
          <span class="pane-error mono">{{ form.refused }}</span>
          <span></span>
          <label class="tag-force">
            <input
              type="checkbox"
              :checked="form.force"
              @change="setTagForce(($event.target as HTMLInputElement).checked)"
            />
            {{ t("tag.force") }}
          </label>
        </template>
      </div>

      <footer class="dialog-foot">
        <span class="dialog-foot-spacer" />
        <button type="button" @click="closeTag()">{{ t("tag.cancel") }}</button>
        <button class="primary" type="submit" :disabled="form.name.trim() === '' || !!app.busy">
          {{ t("tag.create") }}
        </button>
      </footer>
    </form>
  </div>
</template>

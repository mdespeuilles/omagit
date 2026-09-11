<script setup lang="ts">
// Making a tag: a name, what it goes on, and a message.
//
// **It does not name Git's two kinds of tag, and that is the point.** It used
// to: a line under the message field said "Légère — un nom qui désigne le
// commit" or "Annotée — un objet avec un auteur, une date et ce message",
// switching as you typed. Reported, and rightly: "je ne connaissais pas ce
// terme et je trouve que ça embrouille sans apporter de plus-value".
//
// The distinction is real in Git and it is not the user's to make. There is no
// control here for it — writing a message or not is the whole of it, and that
// is identical in every client; Tower's own dialog is Name, Revision, Message
// and says nothing either. Naming the outcome taught the vocabulary Git uses
// internally and added no choice. The one place it bites is `git describe`
// without `--tags`, which passes over a tag that is only a name — a terminal
// concern, and not a reason to put the word in a dialog.
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

<script setup lang="ts">
// Board 07's clone dialog: URL, destination, group, two options, and a line
// saying whether the remote answered.
//
// The reachability line is the part worth arguing for. It costs one
// `git ls-remote` — DNS, the transport, and the credential helper, which is the
// whole path a clone takes — and it is asked *before* the clone rather than
// after, because the alternative is finding out from a four-minute operation
// that failed on its first second. With `GIT_TERMINAL_PROMPT=0` set for every
// invocation, a repository whose credentials no helper can supply fails rather
// than prompting, so this tests authentication and not just the network.

import { computed, onMounted, onBeforeUnmount, ref } from "vue";
import {
  app,
  browseCloneParent,
  cloneBlocker,
  closeClone,
  setCloneGroup,
  setCloneName,
  setCloneOption,
  setCloneUrl,
  startClone,
} from "../state";
import { t } from "../i18n";
import { tildify } from "../format";

const url = ref<HTMLInputElement | null>(null);

const form = computed(() => app.clone);
const blocker = computed(() => (form.value ? cloneBlocker(form.value) : t("clone.nothing")));

/// The full path the clone will land at, which is the two fields joined.
const destination = computed(() => {
  const current = form.value;
  if (!current) return "";
  const parent = current.parent.replace(/\/+$/, "");
  return tildify(current.name ? `${parent}/${current.name}` : parent);
});

/// The groups the library already has, so a clone can join one as it arrives
/// rather than be dragged there afterwards.
const groups = computed(() => {
  const seen = new Map<number, string>();
  for (const row of app.repositories) if (!seen.has(row.group)) seen.set(row.group, row.group_name);
  return [...seen].map(([index, name]) => ({ index, name }));
});

function onKey(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    event.preventDefault();
    closeClone();
  }
}

onMounted(() => {
  // On the URL, because it is the only field with nothing in it that can be
  // guessed — everything below fills itself in from what is typed here.
  url.value?.focus();
  window.addEventListener("keydown", onKey);
});
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div v-if="form" class="overlay" @click.self="closeClone()">
    <form
      class="dialog form"
      role="dialog"
      aria-modal="true"
      :aria-label="t('clone.title')"
      @submit.prevent="startClone()"
    >
      <div class="dialog-head">{{ t("clone.title") }}</div>

      <div class="dialog-body">
        <label class="dialog-label" for="clone-url">{{ t("clone.url") }}</label>
        <input
          id="clone-url"
          ref="url"
          type="text"
          class="mono"
          spellcheck="false"
          autocomplete="off"
          placeholder="git@github.com:owner/repo.git"
          :value="form.url"
          @input="setCloneUrl(($event.target as HTMLInputElement).value)"
        />

        <label class="dialog-label" for="clone-name">{{ t("clone.destination") }}</label>
        <div class="clone-destination">
          <input
            id="clone-name"
            type="text"
            class="mono"
            spellcheck="false"
            autocomplete="off"
            :value="form.name"
            @input="setCloneName(($event.target as HTMLInputElement).value)"
          />
          <button type="button" @click="browseCloneParent()">{{ t("clone.browse") }}</button>
        </div>

        <span></span>
        <span class="clone-path mono" :title="destination">{{ destination }}</span>

        <template v-if="groups.length > 1">
          <label class="dialog-label" for="clone-group">{{ t("clone.group") }}</label>
          <select
            id="clone-group"
            :value="form.group ?? ''"
            @change="
              setCloneGroup(
                ($event.target as HTMLSelectElement).value === ''
                  ? null
                  : Number(($event.target as HTMLSelectElement).value),
              )
            "
          >
            <option value="">{{ t("clone.default") }}</option>
            <option v-for="group in groups" :key="group.index" :value="group.index">
              {{ group.name }}
            </option>
          </select>
        </template>

        <span></span>
        <div class="clone-options">
          <label>
            <input
              type="checkbox"
              :checked="form.shallow"
              @change="setCloneOption('shallow', ($event.target as HTMLInputElement).checked)"
            />
            {{ t("clone.shallow") }} <code>--depth 1</code>
          </label>
          <label>
            <input
              type="checkbox"
              :checked="form.submodules"
              @change="setCloneOption('submodules', ($event.target as HTMLInputElement).checked)"
            />
            {{ t("clone.submodules") }}
          </label>
        </div>

        <span></span>
        <!-- Board 07's status line. Silent until there is something to say:
             a red line under a field somebody is still typing into is noise. -->
        <span
          v-if="form.probe !== 'idle'"
          class="clone-probe mono"
          :class="{
            ok: form.probe === 'reachable',
            bad: typeof form.probe === 'object',
          }"
        >
          <template v-if="form.probe === 'checking'">{{ t("clone.probing") }}</template>
          <template v-else-if="form.probe === 'reachable'">{{ t("clone.reachable") }}</template>
          <template v-else>✗ {{ form.probe.error }}</template>
        </span>
      </div>

      <div class="dialog-foot">
        <span class="hint">{{ t("clone.escape") }}</span>
        <span class="spacer" />
        <button type="button" @click="closeClone()">{{ t("clone.cancel") }}</button>
        <button type="submit" class="primary" :disabled="!!blocker" :title="blocker ?? ''">
          {{ t("clone.confirm") }}<span class="hint">⏎</span>
        </button>
      </div>
    </form>
  </div>
</template>

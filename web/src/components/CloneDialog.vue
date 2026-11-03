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

import { computed, onMounted, onBeforeUnmount, ref, watch } from "vue";
import {
  app,
  browseCloneParent,
  cloneBlocker,
  closeClone,
  openRepository,
  setCloneGroup,
  setCloneName,
  setCloneOption,
  setCloneUrl,
  startClone,
} from "../state";
import { count, t, worded } from "../i18n";
import { ago, tildify } from "../format";
import { hosted, loadHosted, remoteKey, search } from "../hosted";
import type { HostedRepo } from "../ipc";

const url = ref<HTMLInputElement | null>(null);

// ── Searching the user's GitHub (DESIGN v2, board "Clone") ─────────────────

const finder = ref<HTMLInputElement | null>(null);
const results = ref<HTMLElement | null>(null);
const query = ref("");
const cursor = ref(0);
/// How many matches are drawn. The search is over all of them; drawing two
/// hundred buttons to show eight is work nobody sees.
const SHOWN = 60;

const found = computed(() => search(hosted.list?.repos ?? [], query.value).slice(0, SHOWN));
watch(query, () => (cursor.value = 0));

/// Where each repository already in the library came from, by remote, so a
/// row can say "already cloned" and open that one instead.
const cloned = computed(() => {
  const by = new Map<string, string>();
  for (const [path, held] of Object.entries(app.library)) {
    if (held.status !== "ready") continue;
    for (const remote of held.value.remotes) if (remote.url) by.set(remoteKey(remote.url), path);
  }
  return by;
});
const clonedAt = (repo: HostedRepo) => cloned.value.get(remoteKey(repo.ssh_url)) ?? null;
const urlOf = (repo: HostedRepo) => (hosted.list?.ssh === false ? repo.clone_url : repo.ssh_url);

function pick(repo: HostedRepo): void {
  const there = clonedAt(repo);
  if (there) {
    closeClone();
    void openRepository(there);
    return;
  }
  setCloneUrl(urlOf(repo));
}

function onFinderKey(event: KeyboardEvent): void {
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    const step = event.key === "ArrowDown" ? 1 : -1;
    cursor.value = Math.min(Math.max(cursor.value + step, 0), Math.max(found.value.length - 1, 0));
    results.value?.children[cursor.value]?.scrollIntoView({ block: "nearest" });
  } else if (event.key === "Enter") {
    // The first Enter picks; the second, with the URL already filled from the
    // same row, clones — the form's own submit.
    const repo = found.value[cursor.value];
    if (repo && (clonedAt(repo) || urlOf(repo) !== form.value?.url)) {
      event.preventDefault();
      pick(repo);
    }
  } else if (event.key === "Escape" && query.value) {
    // One level at a time: the search first, the dialog after.
    event.stopPropagation();
    query.value = "";
  }
}

function pushed(repo: HostedRepo): string {
  return repo.pushed_at ? ago(Date.parse(repo.pushed_at) / 1000) : "";
}

const form = computed(() => app.clone);
const blocker = computed(() => (form.value ? cloneBlocker(form.value) : t("clone.nothing")));

/// The full path the clone will land at, which is the two fields joined.
const destination = computed(() => {
  const current = form.value;
  if (!current) return "";
  const parent = current.parent.replace(/\/+$/, "");
  return tildify(current.name ? `${parent}/${current.name}` : parent);
});

/// The folders the library already has, so a clone can join one as it arrives
/// rather than be dragged there afterwards.
///
/// From the folders, not from the rows they hold: a folder made for the clone
/// that is about to be fetched holds nothing yet, and is exactly the one the
/// user means to put it in.
const groups = computed(() =>
  app.groups.map((folder, index) => ({ index, name: worded(folder.name) })),
);

function onKey(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    event.preventDefault();
    closeClone();
  }
}

onMounted(() => {
  // On the search, which is how most clones start; on the URL when there is
  // no `gh` to search with — it is the only field nothing can guess.
  void loadHosted();
  (hosted.error === "gh-missing" ? url : finder).value?.focus();
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

      <div class="clone-finder">
        <div class="clone-finder-head">
          <span class="dialog-label">{{ t("clone.hosted.title") }}</span>
          <span v-if="hosted.list" class="ov-muted">
            {{ count("clone.hosted.count", hosted.list.repos.length) }}
          </span>
          <span class="spacer" />
          <button type="button" class="link" :disabled="hosted.loading" @click="loadHosted(true)">
            {{ hosted.loading ? t("clone.hosted.refreshing") : t("clone.hosted.refresh") }}
          </button>
        </div>
        <input
          ref="finder"
          v-model="query"
          type="search"
          spellcheck="false"
          autocomplete="off"
          :placeholder="t('clone.hosted.search')"
          @keydown="onFinderKey"
        />
        <p v-if="hosted.error === 'gh-missing'" class="ov-muted clone-finder-note">
          {{ t("clone.hosted.missing") }}
        </p>
        <p v-else-if="hosted.error && !hosted.list" class="pane-error clone-finder-note">
          {{ hosted.error }}
        </p>
        <p v-else-if="!hosted.list" class="ov-muted clone-finder-note">
          {{ t("clone.hosted.loading") }}
        </p>
        <p v-else-if="found.length === 0" class="ov-muted clone-finder-note">
          {{ t("clone.hosted.none") }}
        </p>
        <ul v-else ref="results" class="clone-results">
          <li v-for="(repo, at) in found" :key="repo.full_name">
            <button
              type="button"
              class="clone-result"
              :class="{ active: at === cursor, picked: urlOf(repo) === form?.url }"
              @click="pick(repo)"
              @mouseenter="cursor = at"
            >
              <span class="clone-result-name">
                <span class="ov-muted">{{ repo.full_name.split("/")[0] }}/</span
                ><strong>{{ repo.full_name.split("/")[1] }}</strong>
              </span>
              <span class="clone-result-tags">
                <span v-if="clonedAt(repo)" class="ov-pill success">{{
                  t("clone.hosted.cloned")
                }}</span>
                <span v-if="repo.private" class="ov-pill">{{ t("clone.hosted.private") }}</span>
                <span v-if="repo.fork" class="ov-pill">{{ t("clone.hosted.fork") }}</span>
                <span v-if="repo.archived" class="ov-pill warning">{{
                  t("clone.hosted.archived")
                }}</span>
              </span>
              <span class="clone-result-description ov-muted">{{ repo.description }}</span>
              <span class="clone-result-when ov-muted">{{ pushed(repo) }}</span>
            </button>
          </li>
        </ul>
      </div>

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

<script setup lang="ts">
// The branch pill's popover (DESIGN v2): which branch you are on, the others
// within one keystroke, and each with where it stands against its remote — so
// switching is not a trip to the sidebar's tree, and "which one was ahead" is
// answered before choosing.
//
// Typing a name nothing has offers to create it from here, which is how most
// branches start: "I am on main, I want a branch for this".

import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { ago, branchColor } from "../format";
import { app, checkoutBranch, createBranch, showScreen } from "../state";
import { t } from "../i18n";

const emit = defineEmits<{ close: [] }>();

const box = ref<HTMLElement | null>(null);
const input = ref<HTMLInputElement | null>(null);
const list = ref<HTMLElement | null>(null);
const query = ref("");
const cursor = ref(0);

const branches = computed(() => (app.refs.status === "ready" ? app.refs.value.branches : []));
const matching = computed(() => {
  const words = query.value.toLowerCase().split(/\s+/).filter(Boolean);
  return [...branches.value]
    .filter((row) => words.every((word) => row.name.toLowerCase().includes(word)))
    .sort((a, b) => Number(b.head) - Number(a.head) || a.age - b.age);
});
/// Offered when what was typed is a name no branch has.
const creatable = computed(() => {
  const name = query.value.trim();
  return name !== "" && !/\s/.test(name) && !branches.value.some((row) => row.name === name);
});
const rows = computed(() => matching.value.length + (creatable.value ? 1 : 0));
watch(query, () => (cursor.value = 0));

function choose(at: number): void {
  const row = matching.value[at];
  if (row) {
    if (!row.head) checkoutBranch(row.name);
  } else if (creatable.value) {
    createBranch(query.value.trim(), "HEAD", true);
  }
  emit("close");
}

function onKey(event: KeyboardEvent): void {
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    const step = event.key === "ArrowDown" ? 1 : -1;
    cursor.value = Math.min(Math.max(cursor.value + step, 0), Math.max(rows.value - 1, 0));
    void nextTick(() => list.value?.querySelector(".active")?.scrollIntoView({ block: "nearest" }));
  } else if (event.key === "Enter") {
    event.preventDefault();
    choose(cursor.value);
  } else if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    emit("close");
  }
}

function outside(event: PointerEvent): void {
  const target = event.target as Node;
  if (box.value?.contains(target)) return;
  if ((target as HTMLElement).closest?.(".branch-pill")) return;
  emit("close");
}

onMounted(() => {
  input.value?.focus();
  window.addEventListener("pointerdown", outside, true);
});
onBeforeUnmount(() => window.removeEventListener("pointerdown", outside, true));
</script>

<template>
  <div ref="box" class="switcher" role="dialog" :aria-label="t('switcher.title')">
    <input
      ref="input"
      v-model="query"
      type="search"
      class="mono"
      spellcheck="false"
      autocomplete="off"
      :placeholder="t('switcher.search')"
      @keydown="onKey"
    />
    <ul ref="list" class="switcher-list">
      <li v-for="(row, at) in matching" :key="row.name">
        <button
          class="switcher-row"
          :class="{ active: at === cursor, current: row.head }"
          @click="choose(at)"
          @mouseenter="cursor = at"
        >
          <span class="dash-dot" :style="{ background: branchColor(row.name) }" />
          <span class="mono switcher-name">{{ row.name }}</span>
          <span class="switcher-state">
            <template v-if="row.tracking && !row.tracking.gone">
              <span v-if="row.tracking.ahead" class="ahead">↑{{ row.tracking.ahead }}</span>
              <span v-if="row.tracking.behind" class="behind">↓{{ row.tracking.behind }}</span>
              <span v-if="!row.tracking.ahead && !row.tracking.behind" class="synced">✓</span>
            </template>
            <span v-else-if="row.tracking?.gone" class="warning">{{
              t("overview.branch.gone")
            }}</span>
            <span v-else class="ov-muted">{{ t("overview.branch.local") }}</span>
          </span>
          <span class="ov-muted switcher-when">
            {{ row.head ? t("overview.branch.current") : ago(Date.now() / 1000 - row.age) }}
          </span>
        </button>
      </li>
      <li v-if="creatable">
        <button
          class="switcher-row"
          :class="{ active: cursor === matching.length }"
          @click="choose(matching.length)"
          @mouseenter="cursor = matching.length"
        >
          <span class="switcher-plus">+</span>
          <span>{{ t("switcher.create", { name: query.trim() }) }}</span>
        </button>
      </li>
    </ul>
    <div class="switcher-foot">
      <button
        class="link"
        @click="
          showScreen('overview');
          emit('close');
        "
      >
        {{ t("switcher.overview") }}
      </button>
      <span class="hint">{{ t("switcher.keys") }}</span>
    </div>
  </div>
</template>

<script setup lang="ts" generic="T">
// A virtualised list.
//
// SPEC §12 asks for a hundred thousand rows and constant 120fps, so only the
// visible window exists as elements. Rows are positioned by `transform` inside
// a spacer of the full height, which means a scroll never triggers layout.
//
// The recycling is Vue's: `v-for` over the slot *positions* rather than over
// the items keeps the same elements and changes what is in them, which is the
// same thing a hand-written pool does and rather less code.

import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";

const props = withDefaults(
  defineProps<{
    items: readonly T[];
    rowHeight: number;
    /// Rows drawn past each edge, so a fast scroll shows no gaps.
    overscan?: number;
  }>(),
  { overscan: 8 },
);

const emit = defineEmits<{ nearEnd: [] }>();

const viewport = ref<HTMLElement | null>(null);
const scrollTop = ref(0);
const height = ref(0);

const first = computed(() =>
  Math.max(0, Math.floor(scrollTop.value / props.rowHeight) - props.overscan),
);

const count = computed(() => {
  const visible = Math.ceil((height.value || 1) / props.rowHeight);
  return Math.max(0, Math.min(props.items.length - first.value, visible + props.overscan * 2));
});

// Carries the slot as well as the index: the slot is what the `v-for` is keyed
// on, and it is stable as the window slides. Keying on the index instead would
// change every key on every scroll, which is the opposite of recycling.
const window_ = computed(() =>
  Array.from({ length: count.value }, (_, slot) => ({
    slot,
    index: first.value + slot,
    item: props.items[first.value + slot] as T,
  })),
);

function onScroll(): void {
  const element = viewport.value;
  if (!element) return;
  scrollTop.value = element.scrollTop;
  if (props.items.length > 0 && first.value + count.value >= props.items.length - props.overscan) {
    emit("nearEnd");
  }
}

// The viewport's height decides how many rows exist. Measured on mount and
// watched afterwards: a pane that is created before it has been laid out has a
// height of zero, and a list that believed that would render one row.
let observer: ResizeObserver | null = null;

onMounted(() => {
  const element = viewport.value;
  if (!element) return;
  height.value = element.clientHeight;
  observer = new ResizeObserver(() => {
    height.value = element.clientHeight;
  });
  observer.observe(element);
});

onBeforeUnmount(() => observer?.disconnect());

// A new list starts at the top.
watch(
  () => props.items,
  () => {
    const element = viewport.value;
    if (element) {
      element.scrollTop = 0;
      scrollTop.value = 0;
    }
  },
);

defineExpose({
  scrollToIndex(index: number): void {
    const element = viewport.value;
    if (!element) return;
    element.scrollTop = Math.max(
      0,
      index * props.rowHeight - element.clientHeight / 2 + props.rowHeight / 2,
    );
  },
});
</script>

<template>
  <div ref="viewport" class="vlist" @scroll.passive="onScroll">
    <div class="vlist-spacer" :style="{ height: `${items.length * rowHeight}px` }">
      <div
        v-for="entry in window_"
        :key="entry.slot"
        class="vlist-row"
        :style="{
          height: `${rowHeight}px`,
          transform: `translateY(${entry.index * rowHeight}px)`,
        }"
      >
        <slot :item="entry.item" :index="entry.index" />
      </div>
    </div>
  </div>
</template>

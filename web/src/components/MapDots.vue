<script setup lang="ts">
// The commits along one line of the branch map: dots, hollow for a merge, and
// the folded runs as "n commits" pills that open when clicked.

import type { Piece } from "../runs";
import { count, t } from "../i18n";
import { when } from "../format";

defineProps<{ pieces: readonly Piece[] }>();
const emit = defineEmits<{ open: [key: string] }>();
</script>

<template>
  <template v-for="piece in pieces" :key="piece.kind === 'run' ? piece.key : `d${piece.time}`">
    <span
      v-if="piece.kind === 'dot'"
      class="bmap-dot"
      :class="{ merge: piece.merge }"
      :style="{ left: `${piece.x}%` }"
      :title="when(piece.time)"
    />
    <!-- A span with a button's manners rather than a button: it sits inside
         the row, which is one, and buttons do not nest. -->
    <span
      v-else
      class="bmap-run"
      role="button"
      tabindex="0"
      :title="t('map.unfold')"
      :style="{ left: `${(piece.from + piece.to) / 2}%` }"
      @click.stop="emit('open', piece.key)"
      @keydown.enter.stop.prevent="emit('open', piece.key)"
      @keydown.space.stop.prevent="emit('open', piece.key)"
      >{{ count("map.run", piece.count) }}</span
    >
  </template>
</template>

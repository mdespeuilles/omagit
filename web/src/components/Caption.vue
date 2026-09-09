<script setup lang="ts">
// Board 02's window buttons, on the platforms where the app draws its own.
//
// **Drawn, not borrowed.** Linear 16px glyphs at 1.5px stroke on the app's own
// grid, right angles, 38px wide by the full height of the topbar. Neither macOS
// circles nor Segoe Fluent: the window stays an omagit window.
//
// Which buttons appear is the backend's call, not a media query — see
// `platform::Platform::caption`. Under Hyprland that is close alone: the
// compositor owns sizing, and a tiled window has no size of its own to minimise.
//
// Not in the tab order (board 02, "ordre de tabulation"): Tab belongs to the
// app's own controls, and every session already closes a window from the
// keyboard. `aria-label` still names them, so a screen reader has them.

import type { Caption } from "../ipc";

defineProps<{ caption: Caption }>();

/// The window API is imported on the click rather than at the top of the file:
/// under test this component mounts in jsdom, where there is no Tauri window to
/// get the handle of, and an import that reaches for one would fail on mount
/// rather than on use.
async function act(what: "minimize" | "toggleMaximize" | "close") {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow()[what]();
}
</script>

<template>
  <span class="caption" :class="caption.side">
    <button
      v-if="caption.minimize"
      tabindex="-1"
      title="Réduire"
      aria-label="Réduire"
      @click="act('minimize')"
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 8h10" /></svg>
    </button>
    <button
      v-if="caption.maximize"
      tabindex="-1"
      title="Agrandir"
      aria-label="Agrandir"
      @click="act('toggleMaximize')"
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 3.5h9v9h-9z" /></svg>
    </button>
    <button
      v-if="caption.close"
      class="close"
      tabindex="-1"
      title="Fermer"
      aria-label="Fermer"
      @click="act('close')"
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
    </button>
  </span>
</template>

<script setup lang="ts">
// Every icon the window draws, in one place.
//
// **Drawn, not borrowed.** `Caption.vue` states the rule for the window buttons
// and the topbar's own icon follows it; the sidebar did not, and it showed. It
// drew `◱` for the working copy, `⌸` for History, `⌥` for Stashes — the macOS
// Option key, which means something else entirely — and `▾` for a disclosure
// triangle that the fallback font rendered as a dot. A character's shape is the
// font's business, and the font is not ours.
//
// One component rather than a path per call site, for the same reason the
// bindings are one table: a glyph copied into four files is a glyph that will
// differ in four files. 16px grid, 1.5px stroke, right angles wherever a shape
// tolerates them — a clock does not, so it is round.

defineProps<{ name: keyof typeof PATHS }>();

/// The shapes. Each is the inside of a 16×16 `viewBox`; stroke and size come
/// from CSS, so one rule moves them all.
const PATHS = {
  /// The working copy: a file, because that is what is changing.
  "working-copy": '<path d="M4.5 2.5h5l3 3v8h-8z"/><path d="M9.5 2.5v3h3"/>',
  /// History: a clock. It was `⌸`, an APL symbol, which reads as a table.
  history: '<circle cx="8" cy="8" r="5.5"/><path d="M8 4.75V8l2.5 1.75"/>',
  /// Stashes: a box with a lid, which is what a shelf of them is.
  stashes:
    '<path d="M2.75 4.25h10.5v2.5h-10.5z"/><path d="M3.75 6.75v6.5h8.5v-6.5"/>' +
    '<path d="M6.5 9.5h3"/>',
  /// Preferences: a gear, which is what everybody looks for. Eight teeth
  /// radiating past the rim rather than cut into it — a gear drawn as a
  /// silhouette needs a fill, and this is the one set in the window where
  /// everything is stroked. It was two sliders, on the theory that a gear at
  /// 16px with a 1.5px stroke is a blob; a probe at 16, 20 and 32 says it is
  /// not, as long as the teeth stand outside the circle.
  settings:
    '<circle cx="8" cy="8" r="4.2"/><circle cx="8" cy="8" r="1.7"/>' +
    '<path d="M12.20 8.00L14.30 8.00M10.97 10.97L12.45 12.45M8.00 12.20L8.00 14.30M5.03 10.97L3.55 12.45M3.80 8.00L1.70 8.00M5.03 5.03L3.55 3.55M8.00 3.80L8.00 1.70M10.97 5.03L12.45 3.55"/>',
  /// A branch: the trunk, and one leaving it. The dots are round on purpose —
  /// they are commits, and a square commit reads as a stop.
  branch:
    '<circle cx="5" cy="3.25" r="1.25"/><circle cx="5" cy="12.75" r="1.25"/>' +
    '<circle cx="11" cy="3.25" r="1.25"/><path d="M5 4.5v7"/><path d="M11 4.5v2.5h-6"/>',
  /// A folder, for the groups a `/` in a branch name makes — and for a remote,
  /// which is the same thing: a prefix holding branches.
  folder: '<path d="M2.5 13.25v-9.5h4l1.5 2h5.5v7.5z"/>',
  /// A tag: the label shape, with its hole.
  tag: '<path d="M8.5 2.5h4.5v4.5l-6 6-4.5-4.5z"/><circle cx="10.75" cy="5.25" r="0.9"/>',
  /// Disclosure, both ways.
  "chevron-down": '<path d="M4.5 6.5l3.5 3.5 3.5-3.5"/>',
  "chevron-right": '<path d="M6.5 4.5l3.5 3.5-3.5 3.5"/>',
  /// The panel that means "all the repositories", in the topbar and the foot of
  /// the sidebar.
  panel:
    '<path d="M2.75 3.75h10.5v8.5h-10.5z"/><path d="M6.75 3.75v8.5"/>' +
    '<path d="M2.75 3.75h4v8.5h-4z" fill="currentcolor"/>',
  /// A repository in the library: a box holding history.
  repository: '<path d="M3.5 2.5h9v11h-9z"/><path d="M3.5 10.5h9"/><path d="M6 2.5v8"/>',
  /// Close, on a tab.
  close: '<path d="M4.5 4.5l7 7M11.5 4.5l-7 7"/>',
  /// One that is no longer where it was recorded.
  missing: '<circle cx="8" cy="8" r="5.5"/><path d="M4.5 11.5l7-7"/>',
} as const;
</script>

<template>
  <!-- eslint-disable vue/no-v-html — the paths are this file's own constants,
       none of them touches anything a user typed. -->
  <svg class="glyph" viewBox="0 0 16 16" aria-hidden="true" v-html="PATHS[name]" />
</template>

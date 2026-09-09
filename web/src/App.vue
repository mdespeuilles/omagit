<script setup lang="ts">
// The shell. Chrome belongs here rather than to a screen — the mistake the
// GPUI build made, where opening History left a window with no way out of it.
//
// The columns are resizable, and their widths live here rather than in each
// pane's own stylesheet rule: a splitter has to know which two things it sits
// between, and that is only true at this level.

import CloneDialog from "./components/CloneDialog.vue";
import CommitBox from "./components/CommitBox.vue";
import CommitDetail from "./components/CommitDetail.vue";
import Confirm from "./components/Confirm.vue";
import ConflictDialog from "./components/ConflictDialog.vue";
import DiffView from "./components/DiffView.vue";
import HistoryList from "./components/HistoryList.vue";
import Journal from "./components/Journal.vue";
import RepositoryCard from "./components/RepositoryCard.vue";
import RepositoryList from "./components/RepositoryList.vue";
import Progress from "./components/Progress.vue";
import Sidebar from "./components/Sidebar.vue";
import Splitter from "./components/Splitter.vue";
import StashDetail from "./components/StashDetail.vue";
import StashList from "./components/StashList.vue";
import StatusBar from "./components/StatusBar.vue";
import StatusList from "./components/StatusList.vue";
import Topbar from "./components/Topbar.vue";
import { app, paneWidth } from "./state";

/// The stylesheet's own widths, and the floor a column may be dragged to. The
/// floor is per pane because what has to stay readable differs: a file list can
/// give up more than a diff.
const widths = {
  library: 300,
  sidebar: 260,
  middle: 340,
  history: 520,
  detail: 340,
};
</script>

<template>
  <Topbar />
  <div class="shell">
    <!-- Board 06 has no workspace sidebar: with no repository open there is no
         workspace to be in, and the repository list *is* the left column. -->
    <template v-if="app.screen === 'repositories'">
      <section class="pane" :style="{ width: `${paneWidth('library', widths.library)}px` }">
        <RepositoryList />
        <Splitter pane="library" :width="paneWidth('library', widths.library)" :min="240" />
      </section>
      <RepositoryCard />
    </template>

    <template v-else>
      <section class="pane" :style="{ width: `${paneWidth('sidebar', widths.sidebar)}px` }">
        <Sidebar />
        <Splitter pane="sidebar" :width="paneWidth('sidebar', widths.sidebar)" :min="200" />
      </section>

      <template v-if="app.screen === 'history'">
        <section class="pane" :style="{ width: `${paneWidth('history', widths.history)}px` }">
          <HistoryList />
          <Splitter pane="history" :width="paneWidth('history', widths.history)" :min="360" />
        </section>
        <!-- The commit and its diff, stacked. Side by side they were two
             narrow columns fighting for the same width, and the diff — which
             is the wider of the two by nature, because it holds code — got the
             worse half. Stacked, the diff has the window's full width and the
             commit keeps only the height it needs. -->
        <section class="stack">
          <CommitDetail />
          <DiffView />
        </section>
      </template>

      <!-- The shelf, in the shape History already has: the list in the middle
           column, and what one entry holds stacked above its diff. A stash is a
           commit, so the right-hand side is the same two panes. -->
      <template v-else-if="app.screen === 'stashes'">
        <section class="pane" :style="{ width: `${paneWidth('middle', widths.middle)}px` }">
          <StashList />
          <Splitter pane="middle" :width="paneWidth('middle', widths.middle)" :min="280" />
        </section>
        <section class="stack">
          <StashDetail />
          <DiffView />
        </section>
      </template>

      <template v-else>
        <section class="pane" :style="{ width: `${paneWidth('middle', widths.middle)}px` }">
          <div class="middle">
            <CommitBox />
            <StatusList />
          </div>
          <Splitter pane="middle" :width="paneWidth('middle', widths.middle)" :min="280" />
        </section>
      </template>

      <DiffView v-if="app.screen === 'working-copy'" />
      <Journal v-if="app.showJournal" />
    </template>
  </div>
  <Progress />
  <StatusBar />
  <Confirm v-if="app.question" />
  <CloneDialog v-if="app.clone" />
  <ConflictDialog v-if="app.resolving" />
</template>

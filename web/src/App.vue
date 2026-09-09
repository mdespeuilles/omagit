<script setup lang="ts">
// The shell. Chrome belongs here rather than to a screen — the mistake the
// GPUI build made, where opening History left a window with no way out of it.
//
// Only the middle changes between screens. The topbar, the sidebar, the status
// bar and the diff pane are the same elements throughout, which is also why the
// diff pane can be one component: what it draws comes from `state.diff`, and
// both screens put something there.

import CommitBox from "./components/CommitBox.vue";
import CommitDetail from "./components/CommitDetail.vue";
import Confirm from "./components/Confirm.vue";
import DiffView from "./components/DiffView.vue";
import HistoryList from "./components/HistoryList.vue";
import Journal from "./components/Journal.vue";
import RepositoryCard from "./components/RepositoryCard.vue";
import RepositoryList from "./components/RepositoryList.vue";
import Sidebar from "./components/Sidebar.vue";
import StatusBar from "./components/StatusBar.vue";
import StatusList from "./components/StatusList.vue";
import Topbar from "./components/Topbar.vue";
import { app } from "./state";
</script>

<template>
  <Topbar />
  <div class="shell">
    <!-- Board 06 has no workspace sidebar: with no repository open there is no
         workspace to be in, and the repository list *is* the left column. -->
    <template v-if="app.screen === 'repositories'">
      <RepositoryList />
      <RepositoryCard />
    </template>

    <template v-else>
      <Sidebar />
      <template v-if="app.screen === 'history'">
        <HistoryList />
        <CommitDetail />
        <DiffView />
      </template>
      <template v-else>
        <section class="middle">
          <CommitBox />
          <StatusList />
        </section>
        <DiffView />
      </template>
      <Journal v-if="app.showJournal" />
    </template>
  </div>
  <StatusBar />
  <Confirm v-if="app.question" />
</template>

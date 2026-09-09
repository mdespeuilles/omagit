<script setup lang="ts">
// The shell. Chrome belongs here rather than to a screen — the mistake the
// GPUI build made, where opening History left a window with no way out of it.

import CommitBox from "./components/CommitBox.vue";
import Confirm from "./components/Confirm.vue";
import DiffView from "./components/DiffView.vue";
import Journal from "./components/Journal.vue";
import Sidebar from "./components/Sidebar.vue";
import StatusBar from "./components/StatusBar.vue";
import StatusList from "./components/StatusList.vue";
import Topbar from "./components/Topbar.vue";
import { app } from "./state";
</script>

<template>
  <Topbar />
  <div class="shell">
    <Sidebar />
    <section class="middle">
      <CommitBox />
      <StatusList />
    </section>
    <DiffView />
    <Journal v-if="app.showJournal" />
  </div>
  <StatusBar />
  <Confirm v-if="app.question" />
</template>

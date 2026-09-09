<script setup lang="ts">
import DiffView from "./components/DiffView.vue";
import Sidebar from "./components/Sidebar.vue";
import StatusList from "./components/StatusList.vue";
import Topbar from "./components/Topbar.vue";
</script>

<template>
  <Topbar />
  <div class="shell">
    <Sidebar />
    <StatusList />
    <DiffView />
  </div>
</template>

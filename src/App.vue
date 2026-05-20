<script setup lang="ts">
import { onMounted } from "vue";
import Home from "./pages/Home.vue";
import Compare from "./pages/Compare.vue";
import Detect from "./pages/Detect.vue";
import Settings from "./pages/Settings.vue";
import History from "./pages/History.vue";
import Calibration from "./pages/Calibration.vue";
import appLogo from "./assets/app-logo.svg";
import { useAppStore } from "./stores/app";

const store = useAppStore();

onMounted(() => {
  void store.initialize();
});
</script>

<template>
  <div class="app-shell">
    <aside class="sidebar">
      <div class="brand">
        <img :src="appLogo" alt="AI Paper Rewriter" />
        <strong>AI Paper Rewriter</strong>
      </div>
      <nav>
        <button :class="{ active: store.page === 'home' }" @click="store.page = 'home'">处理</button>
        <button :class="{ active: store.page === 'compare' }" @click="store.page = 'compare'">对比</button>
        <button :class="{ active: store.page === 'history' }" @click="store.page = 'history'">历史</button>
        <button :class="{ active: store.page === 'calibration' }" @click="store.page = 'calibration'">校准库</button>
        <button :class="{ active: store.page === 'settings' }" @click="store.page = 'settings'">设置</button>
      </nav>
      <div class="sidebar-footer">
        <span v-if="store.status">{{ store.status }}</span>
        <span v-else>本地桌面处理</span>
      </div>
    </aside>

    <section class="content">
      <Home v-if="store.page === 'home'" />
      <Detect v-else-if="store.page === 'detect'" />
      <Compare v-else-if="store.page === 'compare'" />
      <History v-else-if="store.page === 'history'" />
      <Calibration v-else-if="store.page === 'calibration'" />
      <Settings v-else />
      <div v-if="store.error" class="toast error">{{ store.error }}</div>
      <div v-else-if="store.status && !store.loading" class="toast">{{ store.status }}</div>
    </section>
  </div>
</template>

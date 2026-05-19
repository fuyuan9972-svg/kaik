<script setup lang="ts">
import { save } from "@tauri-apps/plugin-dialog";
import { Download, FileUp, History, Settings } from "lucide-vue-next";
import ParagraphCard from "../components/ParagraphCard.vue";
import { useAppStore } from "../stores/app";

const store = useAppStore();

async function exportDocx() {
  const outputPath = await save({
    defaultPath: defaultExportName(),
    filters: [{ name: "Word Document", extensions: ["docx"] }],
  });
  if (outputPath) {
    await store.exportDocx(ensureDocxExtension(outputPath));
  }
}

function defaultExportName() {
  const session = store.currentSession;
  if (!session) {
    return "改写结果.docx";
  }

  const name = session.fileName.replace(/\.[^.]+$/, "");
  const safeName = sanitizeFileName(name).slice(0, 18) || "论文";
  const profile = profileLabel(session.promptProfile);
  const sample = session.isSample ? `${session.sampleLimit || 20}段` : "整篇";
  return `${safeName}_${profile}_${sample}.docx`;
}

function sanitizeFileName(value: string) {
  return value
    .replace(/[\\/:*?"<>|]/g, "")
    .replace(/\s+/g, "")
    .trim();
}

function profileLabel(value: string) {
  if (value === "sample_calibrated_17_v2") return "成功链路17-2.0";
  if (value === "sample_calibrated_17_success") return "成功链路17基线";
  if (value === "sample_calibrated_17") return "样本校准17%旧版";
  if (value === "sample_calibrated_28") return "旧样本校准28%";
  if (value === "directive_aigc_reduce") return "旧指令降AIGC";
  if (value === "directive_aigc_reduce_legacy") return "旧指令降AIGC";
  if (value === "doubao_plain_humanize") return "豆包降AI";
  if (value === "plain_spoken_humanize") return "大白话";
  if (value === "local_light_rewrite") return "局部轻改";
  if (value === "local_depattern") return "旧局部";
  if (value === "paperpass_restructure") return "旧PP重构";
  if (value === "conservative_rewrite") return "旧保守";
  if (value === "light_rewrite") return "旧轻改";
  return "改写";
}

function aiRateLabel(session: { currentAiRate?: number | null; targetAiRate?: number | null }) {
  if (session.currentAiRate == null || session.targetAiRate == null) {
    return "";
  }
  return `，AI ${session.currentAiRate} -> ${session.targetAiRate}`;
}

function ensureDocxExtension(path: string) {
  return path.toLowerCase().endsWith(".docx") ? path : `${path}.docx`;
}
</script>

<template>
  <main class="page">
    <section class="toolbar sticky">
      <div>
        <h1>对比结果</h1>
        <p>
          {{ store.visibleResults.length }} 个段落，改写 {{ store.acceptedCount }} 个，跳过
          {{ store.skippedResultCount }} 个，失败 {{ store.failedCount }} 个。
          <template v-if="store.currentSession">
            {{ profileLabel(store.currentSession.promptProfile) }}，
            {{ store.currentSession.isSample ? `测试 ${store.currentSession.sampleLimit} 段` : "整篇记录" }}
            {{ aiRateLabel(store.currentSession) }}
          </template>
        </p>
      </div>
      <div class="button-row">
        <button class="secondary-button" type="button" @click="store.page = 'home'">
          <FileUp :size="16" />
          上传
        </button>
        <button class="secondary-button" type="button" @click="store.page = 'settings'">
          <Settings :size="16" />
          设置
        </button>
        <button class="secondary-button" type="button" @click="store.page = 'history'">
          <History :size="16" />
          历史
        </button>
        <button class="primary-button" type="button" :disabled="store.results.length === 0" @click="exportDocx">
          <Download :size="16" />
          导出 .docx
        </button>
      </div>
    </section>

    <div v-if="store.visibleResults.length === 0" class="empty-state">还没有改写结果。</div>
    <div v-else class="paragraph-list">
      <ParagraphCard
        v-for="result in store.visibleResults"
        :key="result.index"
        :result="result"
        @accept="store.setAccepted($event, true)"
        @reject="store.setAccepted($event, false)"
      />
    </div>
  </main>
</template>

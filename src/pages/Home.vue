<script setup lang="ts">
import { FlaskConical, Play, Settings } from "lucide-vue-next";
import FileUpload from "../components/FileUpload.vue";
import ProgressBar from "../components/ProgressBar.vue";
import { useAppStore } from "../stores/app";

const store = useAppStore();

function fileName(path: string) {
  return path.split("/").pop() || path;
}
</script>

<template>
  <main class="page">
    <section class="toolbar">
      <div>
        <h1>AI 论文降重工具</h1>
        <p>本地解析论文，逐段调用 OpenAI 兼容 API 轻度改写。</p>
      </div>
      <button class="secondary-button" type="button" @click="store.page = 'settings'">
        <Settings :size="16" />
        设置
      </button>
    </section>

    <FileUpload @selected="store.parseFile" />

    <section v-if="store.filePath" class="panel">
      <div class="file-card">
        <span>当前文件</span>
        <strong :title="fileName(store.filePath)">{{ fileName(store.filePath) }}</strong>
        <small :title="store.filePath">{{ store.filePath }}</small>
      </div>

      <div class="metric-grid">
        <div>
          <span>总段落</span>
          <strong>{{ store.paragraphs.length }}</strong>
        </div>
        <div>
          <span>实际改写</span>
          <strong>{{ store.rewriteableCount }}</strong>
        </div>
        <div>
          <span>已跳过</span>
          <strong>{{ store.skippedParagraphCount }}</strong>
        </div>
      </div>

      <div class="rewrite-profile-card">
        <label>
          <span>改写模式</span>
          <select v-model="store.config.promptProfile">
            <option value="sample_calibrated_17_v2">成功链路17 2.0（推荐）</option>
            <option value="sample_calibrated_17_success">成功链路17（基线）</option>
          </select>
        </label>
        <p>
          2.0 更偏短句和生涩感；基线用于复现 fn11。当前会调用 API {{ store.rewriteableCount }} 段，跳过
          {{ store.skippedParagraphCount }} 段低收益内容。
        </p>
      </div>

      <ProgressBar
        :current="store.loading ? store.progressCurrent : 0"
        :total="store.loading ? store.progressTotal : store.rewriteableCount"
        :label="store.loading ? store.status : `等待开始，实际调用 API ${store.rewriteableCount} 段`"
      />
      <div class="metric-grid ai-rate-grid">
        <label>
          <span>当前AI率</span>
          <input v-model.number="store.currentAiRate" type="number" min="0" max="100" step="1" />
        </label>
        <label>
          <span>目标AI率</span>
          <input v-model.number="store.targetAiRate" type="number" min="0" max="100" step="1" />
        </label>
      </div>
      <p v-if="store.loading && store.results.length" class="progress-note">
        本轮已收到 {{ store.results.length }} 个段落结果；失败段落会自动保留原文并继续处理。
      </p>

      <div class="action-row">
        <button
          class="primary-button"
          type="button"
          :disabled="store.loading || store.rewriteableCount === 0"
          @click="() => store.rewrite()"
        >
          <Play :size="17" />
          开始改写
        </button>
        <button
          class="secondary-button"
          type="button"
          :disabled="store.loading || store.rewriteableCount === 0"
          @click="store.runSampleTest"
        >
          <FlaskConical :size="17" />
          测试 20 段
        </button>
      </div>
    </section>

    <section v-if="store.paragraphs.length" class="panel run-summary">
      <h2>处理说明</h2>
      <div>
        <p>导出会保留全文结构，只替换已接受的正文改写段。</p>
        <p>建议先用检测页判断 AI 率，再决定跑 20 段测试还是整篇改写。</p>
      </div>
    </section>
  </main>
</template>

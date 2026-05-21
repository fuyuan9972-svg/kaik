<script setup lang="ts">
import { open } from "@tauri-apps/plugin-dialog";
import { FileSearch, FlaskConical, Gauge, Play, Settings, Square } from "lucide-vue-next";
import FileUpload from "../components/FileUpload.vue";
import ProgressBar from "../components/ProgressBar.vue";
import { useAppStore } from "../stores/app";
import type { AigcAnalysis } from "../types";

const store = useAppStore();

function fileName(path: string) {
  return path.split("/").pop() || path;
}

function formatAiRate(analysis: AigcAnalysis | null) {
  return analysis ? `${analysis.estimatedAigc.toFixed(1)}%` : "--";
}

function formatAiRange(analysis: AigcAnalysis | null) {
  return analysis ? `${analysis.rangeLow.toFixed(1)}% - ${analysis.rangeHigh.toFixed(1)}%` : "等待混合检测";
}

async function choosePaperPassReport() {
  const selected = await open({
    multiple: false,
    directory: false,
    filters: [
      {
        name: "PaperPass 报告",
        extensions: ["html", "htm"],
      },
    ],
  });
  if (typeof selected === "string") {
    await store.importPaperPassReport(selected);
  }
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

    <FileUpload @selected="store.startUnifiedTask" />

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
          2.0 更偏短句和生涩感；基线作为稳定对照组。当前会调用 API {{ store.rewriteableCount }} 段，跳过
          {{ store.skippedParagraphCount }} 段低收益内容。
        </p>
      </div>

      <section class="report-guided-card">
        <div>
          <span>PP 报告定向改写</span>
          <p>
            适合“原稿先过 PP，再按报告命中正文段改写”。已低于 20 的改写稿不建议继续精修。
          </p>
        </div>
        <div v-if="store.activeExternalReport" class="report-guided-summary">
          <strong>
            {{ (store.activeExternalReport.totalSuspectedRatio ?? store.activeExternalReport.reportScore ?? 0).toFixed(2) }}%
          </strong>
          <small>
            正文命中 {{ store.reportGuidedBodySegmentCount }} 段，当前匹配 {{ store.reportGuidedIndices.length }} 段
          </small>
        </div>
        <button class="secondary-button" type="button" :disabled="store.loading" @click="choosePaperPassReport">
          <FileSearch :size="17" />
          导入 PP 报告
        </button>
      </section>

      <section v-if="store.aigcAnalysis" class="task-analysis-card">
        <div class="section-heading">
          <span>原稿 AI 混合检测</span>
          <small>{{ store.aigcAnalysis.detectionMode === "ai_mixed" ? "检测完成" : "本地兜底" }}</small>
        </div>
        <div class="analysis-grid compact">
          <article class="score-card">
            <span>AI 混合检测率</span>
            <strong>{{ (store.originalAigcAnalysis ?? store.aigcAnalysis).estimatedAigc.toFixed(1) }}%</strong>
            <small>
              {{ (store.originalAigcAnalysis ?? store.aigcAnalysis).rangeLow.toFixed(1) }}% -
              {{ (store.originalAigcAnalysis ?? store.aigcAnalysis).rangeHigh.toFixed(1) }}%
            </small>
          </article>
          <article class="score-card">
            <span>风险等级</span>
            <strong>{{ store.aigcAnalysis.riskLevel }}</strong>
            <small>{{ store.aigcAnalysis.profile }}</small>
          </article>
          <article class="score-card">
            <span>试跑目标</span>
            <strong>{{ store.trialTargetAigc().toFixed(1) }}%</strong>
            <small>先比原稿降低约15点</small>
          </article>
          <article class="score-card">
            <span>测试20段后AI率</span>
            <strong>{{ formatAiRate(store.trialDraftAigcAnalysis) }}</strong>
            <small>{{ formatAiRange(store.trialDraftAigcAnalysis) }}</small>
          </article>
          <article class="score-card">
            <span>叠加全篇后AI率</span>
            <strong>{{ formatAiRate(store.stackedFullAigcAnalysis) }}</strong>
            <small>{{ formatAiRange(store.stackedFullAigcAnalysis) }}</small>
          </article>
          <article class="score-card">
            <span>直接全篇后AI率</span>
            <strong>{{ formatAiRate(store.directFullAigcAnalysis) }}</strong>
            <small>{{ formatAiRange(store.directFullAigcAnalysis) }}</small>
          </article>
          <article v-if="store.reportGuidedAigcAnalysis" class="score-card">
            <span>PP定向后AI率</span>
            <strong>{{ formatAiRate(store.reportGuidedAigcAnalysis) }}</strong>
            <small>{{ formatAiRange(store.reportGuidedAigcAnalysis) }}</small>
          </article>
        </div>
        <p>{{ store.aigcAnalysis.summary }}</p>
        <p v-if="store.aigcAnalysis.calibrationSummary">{{ store.aigcAnalysis.calibrationSummary }}</p>
      </section>

      <section class="task-flow-card">
        <div class="flow-step" :class="{ done: Boolean(store.aigcAnalysis), active: store.taskStage === 'detected' }">
          <strong>1</strong>
          <span>AI 混合检测</span>
          <small>{{ store.aigcAnalysis ? "已完成" : "上传后自动检测" }}</small>
        </div>
        <div class="flow-step" :class="{ done: Boolean(store.trialEvaluation), active: store.activeRewriteKind === 'sample' }">
          <strong>2</strong>
          <span>试跑 {{ Math.min(20, store.rewriteableCount) }} 段</span>
          <small>{{ store.trialEvaluation ? "已评估" : "先看策略是否适合" }}</small>
        </div>
        <div class="flow-step" :class="{ active: store.activeRewriteKind === 'full' || store.activeRewriteKind === 'stacked', done: store.taskStage === 'completed' }">
          <strong>3</strong>
          <span>全文改写</span>
          <small>{{ store.trialEvaluation ? "叠加跑全文或直接全篇" : "可直接全篇" }}</small>
        </div>
        <div class="flow-step" :class="{ active: store.activeRewriteKind === 'report', done: Boolean(store.reportGuidedAigcAnalysis) }">
          <strong>4</strong>
          <span>PP 报告定向</span>
          <small>{{ store.activeExternalReport ? `匹配 ${store.reportGuidedIndices.length} 段` : "先导入报告" }}</small>
        </div>
      </section>

      <section v-if="store.trialEvaluation" class="task-analysis-card">
        <div class="section-heading">
          <span>试跑结论</span>
          <small>置信度 {{ store.trialEvaluation.confidence.toFixed(0) }}%</small>
        </div>
        <div class="trial-verdict">
          <strong>{{ store.trialEvaluation.verdict }}</strong>
          <span>{{ store.trialEvaluation.summary }}</span>
        </div>
        <div class="scope-summary" v-if="store.trialEvaluation.risks.length">
          <span>主要风险</span>
          <div>
            <small v-for="risk in store.trialEvaluation.risks" :key="risk">{{ risk }}</small>
          </div>
        </div>
        <p>
          推荐策略：{{ store.trialEvaluation.recommendedProfile === "sample_calibrated_17_success" ? "成功链路17（基线）" : "成功链路17 2.0" }}。
          可选择叠加跑全文：先把试跑20段合成底稿，再基于这个底稿跑完整正文。
        </p>
      </section>

      <ProgressBar
        :current="store.loading ? store.progressCurrent : 0"
        :total="store.loading ? store.progressTotal : store.rewriteableCount"
        :label="store.loading ? store.status : `等待开始，实际调用 API ${store.rewriteableCount} 段`"
      />
      <div class="ai-rate-panel">
        <div class="section-heading">
          <span>改写强度</span>
          <small>用于提示词判断力度，默认按 60 → 10。</small>
        </div>
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
      </div>
      <p v-if="store.loading && store.results.length" class="progress-note">
        本轮已收到 {{ store.results.length }} 个段落结果；失败段落会自动保留原文并继续处理。
      </p>

      <div class="action-row">
        <button
          class="primary-button"
          type="button"
          :disabled="
            store.rewriteableCount === 0 ||
            (store.loading && store.activeRewriteKind !== 'sample')
          "
          @click="store.activeRewriteKind === 'sample' ? store.cancelRewrite() : store.runSampleTest()"
        >
          <Square v-if="store.activeRewriteKind === 'sample'" :size="17" />
          <FlaskConical v-else :size="17" />
          {{ store.activeRewriteKind === "sample" ? "中止试跑" : `试跑 ${Math.min(20, store.rewriteableCount)} 段` }}
        </button>
        <button
          class="secondary-button"
          type="button"
          :disabled="
            !store.trialEvaluation ||
            store.rewriteableCount === 0 ||
            (store.loading && store.activeRewriteKind !== 'stacked')
          "
          @click="store.activeRewriteKind === 'stacked' ? store.cancelRewrite() : store.rewriteStackedFull()"
        >
          <Square v-if="store.activeRewriteKind === 'stacked'" :size="17" />
          <Play v-else :size="17" />
          {{ store.activeRewriteKind === "stacked" ? "中止叠加" : "叠加跑全文" }}
        </button>
        <button
          class="secondary-button"
          type="button"
          :disabled="store.loading || store.rewriteableCount === 0"
          @click="store.rewrite()"
        >
          <Gauge :size="17" />
          直接整篇改写
        </button>
        <button
          class="secondary-button"
          type="button"
          :disabled="
            !store.activeExternalReport ||
            store.reportGuidedIndices.length === 0 ||
            (store.loading && store.activeRewriteKind !== 'report')
          "
          @click="store.activeRewriteKind === 'report' ? store.cancelRewrite() : store.rewriteByPaperPassReport()"
        >
          <Square v-if="store.activeRewriteKind === 'report'" :size="17" />
          <FileSearch v-else :size="17" />
          {{ store.activeRewriteKind === "report" ? "中止定向" : "按 PP 报告改写" }}
        </button>
      </div>
    </section>

    <section v-if="store.paragraphs.length" class="panel run-summary">
      <h2>处理说明</h2>
      <div>
        <p>导出会保留全文结构，只替换已接受的正文改写段。</p>
        <p>当前主流程是自动检测、试跑 20 段，再以试跑底稿为基础叠加跑全文；也可以跳过试跑直接整篇。</p>
      </div>
    </section>
  </main>
</template>

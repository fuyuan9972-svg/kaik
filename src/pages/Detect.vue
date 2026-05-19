<script setup lang="ts">
import { computed, reactive } from "vue";
import { Gauge, RefreshCw, Save, Trash2 } from "lucide-vue-next";
import FileUpload from "../components/FileUpload.vue";
import { useAppStore } from "../stores/app";

const store = useAppStore();

const draft = reactive({
  measuredAigc: 0,
  plagiarismRate: null as number | null,
  strategy: "",
  round: "",
  note: "",
});

const analysis = computed(() => store.aigcAnalysis);
const measuredValid = computed(() => draft.measuredAigc >= 0 && draft.measuredAigc <= 100);
const aiScores = computed(() => analysis.value?.aiAssessment?.paragraphScores ?? []);

function formatPercent(value: number) {
  return `${value.toFixed(1)}%`;
}

function sampleMeta(sample: { strategy?: string | null; round?: string | null; plagiarismRate?: number | null }) {
  const parts = [];
  if (sample.strategy) parts.push(sample.strategy);
  if (sample.round) parts.push(sample.round);
  if (sample.plagiarismRate != null) parts.push(`查重 ${formatPercent(sample.plagiarismRate)}`);
  return parts.join(" · ") || "本地校准样本";
}

function modeLabel(value?: string) {
  if (value === "ai_mixed") return "AI 混合检测";
  if (value === "ai_mixed_failed") return "AI 检测失败，已回退本地";
  return "本地快速检测";
}

async function saveCalibration() {
  if (!store.detectFilePath || !measuredValid.value) return;
  const plagiarismRate =
    typeof draft.plagiarismRate === "number" && Number.isFinite(draft.plagiarismRate)
      ? draft.plagiarismRate
      : null;
  await store.saveAigcCalibration({
    filePath: store.detectFilePath,
    measuredAigc: draft.measuredAigc,
    plagiarismRate,
    strategy: draft.strategy.trim() || null,
    round: draft.round.trim() || null,
    note: draft.note.trim() || null,
  });
}
</script>

<template>
  <main class="page detect-page">
    <section class="toolbar">
      <div>
        <h1>AI 混合 AIGC 检测</h1>
        <p>抽样调用检测模型，并融合本地指标和已知 PaperPass 实测样本。</p>
      </div>
      <button
        class="secondary-button"
        type="button"
        :disabled="store.loading || !store.detectFilePath"
        @click="store.analyzeAigcFileAi(store.detectFilePath)"
      >
        <RefreshCw :size="16" />
        重新 AI 检测
      </button>
    </section>

    <FileUpload @selected="store.analyzeAigcFileAi" />

    <section v-if="!analysis" class="empty-state detect-empty">
      <div>
        <Gauge :size="30" />
        <strong>上传论文后会自动进行 AI 混合检测。</strong>
        <span>检测模型配置在设置页；结果用于判断是否继续改写，最终仍以 PaperPass 实测为准。</span>
      </div>
    </section>

    <template v-else>
      <section class="analysis-grid">
        <article class="score-card">
          <span>预计 AI 率</span>
          <strong>{{ formatPercent(analysis.estimatedAigc) }}</strong>
          <small>
            {{ modeLabel(analysis.detectionMode) }} · 区间 {{ formatPercent(analysis.rangeLow) }} -
            {{ formatPercent(analysis.rangeHigh) }}
          </small>
        </article>
        <article class="score-card">
          <span>风险等级</span>
          <strong>{{ analysis.riskLevel }}</strong>
          <small>{{ analysis.profile }}</small>
        </article>
        <article class="score-card">
          <span>置信度</span>
          <strong>{{ formatPercent(analysis.confidence) }}</strong>
          <small v-if="analysis.localEstimatedAigc != null">本地 {{ formatPercent(analysis.localEstimatedAigc) }}</small>
          <small v-else>样本越多越准</small>
        </article>
      </section>

      <section v-if="analysis.aiError || analysis.aiAssessment" class="panel analysis-section">
        <h2>AI 判断</h2>
        <p v-if="analysis.aiError" class="paragraph-error-inline">
          {{ analysis.aiError }}。已保留本地检测结果作为兜底。
        </p>
        <template v-else-if="analysis.aiAssessment">
          <p>{{ analysis.aiAssessment.summary }}</p>
          <div class="scope-summary">
            <span>AI 判断原因</span>
            <div>
              <small v-for="reason in analysis.aiAssessment.reasons" :key="reason">{{ reason }}</small>
            </div>
          </div>
        </template>
      </section>

      <section class="panel analysis-section">
        <div>
          <h2>{{ analysis.fileName }}</h2>
          <p>{{ analysis.summary }}</p>
          <p>{{ analysis.nextAction }}</p>
        </div>
        <div class="metric-grid detector-metrics">
          <div>
            <span>总段落</span>
            <strong>{{ analysis.metrics.totalParagraphs }}</strong>
          </div>
          <div>
            <span>正文候选</span>
            <strong>{{ analysis.metrics.bodyParagraphs }}</strong>
          </div>
          <div>
            <span>正文字数</span>
            <strong>{{ analysis.metrics.cjkChars }}</strong>
          </div>
          <div>
            <span>平均段长</span>
            <strong>{{ analysis.metrics.avgParagraphLen }}</strong>
          </div>
          <div>
            <span>平均句长</span>
            <strong>{{ analysis.metrics.avgSentenceLen }}</strong>
          </div>
          <div>
            <span>AI套话/万字</span>
            <strong>{{ analysis.metrics.aiTermsPer10k }}</strong>
          </div>
          <div>
            <span>缓冲词/万字</span>
            <strong>{{ analysis.metrics.bufferTermsPer10k }}</strong>
          </div>
          <div>
            <span>朴素表达/万字</span>
            <strong>{{ analysis.metrics.plainTermsPer10k }}</strong>
          </div>
        </div>
      </section>

      <section class="detect-columns">
        <article class="panel analysis-section">
          <h2>最相似样本</h2>
          <div class="sample-list">
            <div v-for="sample in analysis.similarSamples" :key="sample.fileName" class="sample-row">
              <span>
                <strong>{{ sample.fileName }}</strong>
                <small>实测 {{ formatPercent(sample.measuredAigc) }}</small>
              </span>
              <b>{{ formatPercent(sample.similarity) }}</b>
            </div>
          </div>
        </article>

        <article class="panel analysis-section">
          <h2>录入 PaperPass 实测</h2>
          <div class="calibration-form">
            <label>
              <span>实测 AIGC 率</span>
              <input v-model.number="draft.measuredAigc" type="number" min="0" max="100" step="0.01" />
            </label>
            <label>
              <span>查重率</span>
              <input v-model.number="draft.plagiarismRate" type="number" min="0" max="100" step="0.01" />
            </label>
            <label>
              <span>策略</span>
              <input v-model="draft.strategy" placeholder="成功链路17 2.0" />
            </label>
            <label>
              <span>轮次</span>
              <input v-model="draft.round" placeholder="测试20后整篇 / 直跑整篇" />
            </label>
            <label class="wide">
              <span>备注</span>
              <input v-model="draft.note" placeholder="例如：基于 fn11 二次处理" />
            </label>
          </div>
          <button
            class="primary-button"
            type="button"
            :disabled="store.loading || !store.detectFilePath || !measuredValid"
            @click="saveCalibration"
          >
            <Save :size="16" />
            保存校准
          </button>
        </article>
      </section>

      <section class="panel analysis-section">
        <h2>{{ aiScores.length ? "AI 抽样段落" : "高风险段落" }}</h2>
        <div v-if="aiScores.length" class="risk-list">
          <article v-for="score in aiScores" :key="score.index" class="risk-item">
            <header>
              <strong>段落 {{ score.index + 1 }}</strong>
              <span>{{ formatPercent(score.risk) }}</span>
            </header>
            <p>{{ score.text }}</p>
            <div>
              <small>{{ score.profile }}</small>
              <small v-for="reason in score.reasons" :key="reason">{{ reason }}</small>
            </div>
          </article>
        </div>
        <div v-else-if="analysis.paragraphRisks.length === 0" class="muted-line">没有明显高风险正文段。</div>
        <div v-else class="risk-list">
          <article v-for="risk in analysis.paragraphRisks" :key="risk.index" class="risk-item">
            <header>
              <strong>段落 {{ risk.index + 1 }}</strong>
              <span>{{ formatPercent(risk.risk) }}</span>
            </header>
            <p>{{ risk.text }}</p>
            <div>
              <small v-for="reason in risk.reasons" :key="reason">{{ reason }}</small>
            </div>
          </article>
        </div>
      </section>

      <section class="panel analysis-section">
        <h2>本地校准库</h2>
        <div v-if="store.aigcCalibrations.length === 0" class="muted-line">还没有手动录入的实测样本。</div>
        <div v-else class="sample-list">
          <div v-for="sample in store.aigcCalibrations" :key="sample.id" class="sample-row calibration-row">
            <span>
              <strong>{{ sample.fileName }}</strong>
              <small>实测 {{ formatPercent(sample.measuredAigc) }} · {{ sampleMeta(sample) }}</small>
            </span>
            <button class="icon-button" type="button" title="删除校准" @click="store.deleteAigcCalibration(sample.id)">
              <Trash2 :size="16" />
            </button>
          </div>
        </div>
      </section>
    </template>
  </main>
</template>

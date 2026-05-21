<script setup lang="ts">
import { computed, reactive } from "vue";
import { Database, Save } from "lucide-vue-next";
import { useAppStore } from "../stores/app";

const store = useAppStore();

const draft = reactive({
  measuredAigc: 0,
  plagiarismRate: null as number | null,
  originalSnapshotId: "",
  rewrittenSnapshotId: "",
  sessionId: "",
  strategy: "",
  round: "",
  customRound: "",
  note: "",
});

const avgError = computed(() => {
  const values = store.feedbackRecords
    .map((record) => record.estimationError)
    .filter((value): value is number => typeof value === "number");
  if (!values.length) return null;
  return values.reduce((sum, value) => sum + value, 0) / values.length;
});

const feedbackBins = computed(() => {
  const bins = [
    { label: "0-15%", min: 0, max: 15, count: 0 },
    { label: "15-22%", min: 15, max: 22, count: 0 },
    { label: "22-35%", min: 22, max: 35, count: 0 },
    { label: "35-55%", min: 35, max: 55, count: 0 },
    { label: "55%+", min: 55, max: 100, count: 0 },
  ];
  for (const record of store.feedbackRecords) {
    const bin = bins.find((item) => record.measuredAigc >= item.min && record.measuredAigc <= item.max);
    if (bin) bin.count += 1;
  }
  return bins;
});

const pairedRecords = computed(() =>
  store.feedbackRecords.filter((record) => record.originalSnapshotId && record.rewrittenSnapshotId),
);

const externalReportRecords = computed(() =>
  store.feedbackRecords.filter((record) => (record.provider ?? "paperpass") !== "paperpass"),
);

const avgDrop = computed(() => {
  const values = store.feedbackRecords
    .map((record) => record.estimatedDrop)
    .filter((value): value is number => typeof value === "number");
  if (!values.length) return null;
  return values.reduce((sum, value) => sum + value, 0) / values.length;
});

function formatPercent(value: number | null | undefined) {
  if (value == null) return "未记录";
  return `${value.toFixed(2)}%`;
}

function snapshotLabel(id: string | null | undefined) {
  if (!id) return "未匹配";
  const snapshot = store.detectionSnapshots.find((item) => item.id === id);
  return snapshot ? snapshot.fileName : id;
}

function profileLabel(value: string | null | undefined) {
  if (value === "sample_calibrated_17_v2") return "成功链路17 2.0";
  if (value === "sample_calibrated_17_success") return "成功链路17（基线）";
  if (value === "sample_calibrated_17") return "旧版17";
  if (value === "sample_calibrated_28") return "旧样本校准28%";
  if (value === "directive_aigc_reduce" || value === "directive_aigc_reduce_legacy") return "旧指令降AIGC";
  return value || "未标策略";
}

function providerLabel(value: string | null | undefined) {
  if (!value || value === "paperpass") return "PP";
  if (value === "weipu" || value === "vip") return "维普";
  return value;
}

async function saveFeedback() {
  const round = draft.round === "手动备注" ? draft.customRound.trim() : draft.round.trim();
  await store.saveFeedbackRecord({
    measuredAigc: draft.measuredAigc,
    plagiarismRate: draft.plagiarismRate,
    originalSnapshotId: draft.originalSnapshotId || null,
    rewrittenSnapshotId: draft.rewrittenSnapshotId || null,
    sessionId: draft.sessionId || null,
    strategy: draft.strategy.trim() || null,
    round: round || null,
    note: draft.note.trim() || null,
  });
}

function fillCurrentTask() {
  draft.originalSnapshotId = store.originalSnapshotId || draft.originalSnapshotId;
  draft.rewrittenSnapshotId = store.rewrittenSnapshotId || draft.rewrittenSnapshotId;
  draft.strategy = store.currentSession?.promptProfile || store.config.promptProfile;
  draft.sessionId = store.currentSessionId || draft.sessionId;
  draft.round = roundLabel(store.currentSession?.taskType, Boolean(store.trialEvaluation));
}

function roundLabel(taskType: string | null | undefined, hasTrial: boolean) {
  if (taskType === "reportGuidedRewrite") return "PP报告定向改写";
  if (taskType === "stackedFullRewrite") return "试跑后叠加全文";
  if (taskType === "guidedRewrite") return "试跑后补剩余全文";
  if (taskType === "fullRewrite") return hasTrial ? "试跑后叠加全文" : "直接全文";
  if (taskType === "sampleTrial") return "测试20段";
  return hasTrial ? "试跑后叠加全文" : "直接全文";
}

function comparisonSummary(record: { reportComparison?: { summary: string } | null }) {
  return record.reportComparison?.summary ?? "";
}
</script>

<template>
  <main class="page calibration-page">
    <section class="toolbar">
      <div>
        <h1>校准库</h1>
        <p>按“原稿 → 改写稿 → PaperPass 实测”记录闭环，让后续混合检测更接近 PP。</p>
      </div>
    </section>

    <section class="analysis-grid">
      <article class="score-card">
        <span>闭环记录</span>
        <strong>{{ pairedRecords.length }}</strong>
        <small>同时包含原稿和改写稿快照</small>
      </article>
      <article class="score-card">
        <span>检测快照</span>
        <strong>{{ store.detectionSnapshots.length }}</strong>
        <small>每次 AI 混合检测自动保存</small>
      </article>
      <article class="score-card">
        <span>PP反馈</span>
        <strong>{{ store.feedbackRecords.length }}</strong>
        <small>PP和外部报告实测记录</small>
      </article>
      <article class="score-card">
        <span>外部报告</span>
        <strong>{{ externalReportRecords.length }}</strong>
        <small>维普等带片段标注的报告</small>
      </article>
      <article class="score-card">
        <span>平均预估降幅</span>
        <strong>{{ avgDrop == null ? "--" : avgDrop.toFixed(2) }}</strong>
        <small>原稿混合AI - 改写稿混合AI</small>
      </article>
      <article class="score-card">
        <span>平均误差</span>
        <strong>{{ avgError == null ? "--" : avgError.toFixed(2) }}</strong>
        <small>App估算 - PP实测</small>
      </article>
    </section>

    <section class="panel analysis-section">
      <div class="section-heading">
        <span>PP实测分布</span>
        <small>用区间统计校准，不用单篇论文当模板</small>
      </div>
      <div class="bin-grid">
        <div v-for="bin in feedbackBins" :key="bin.label">
          <span>{{ bin.label }}</span>
          <strong>{{ bin.count }}</strong>
        </div>
      </div>
    </section>

    <section class="panel analysis-section">
      <div class="section-heading">
        <span>录入一条闭环数据</span>
        <small>选择原稿快照和基于它导出的改写稿快照，再填 PP 实测</small>
      </div>
      <button class="secondary-button compact-action" type="button" @click="fillCurrentTask">
        使用当前任务快照
      </button>
      <div class="calibration-form">
        <label>
          <span>原稿快照</span>
          <select v-model="draft.originalSnapshotId">
            <option value="">未选择</option>
            <option v-for="snapshot in store.detectionSnapshots" :key="snapshot.id" :value="snapshot.id">
              {{ snapshot.fileName }} · {{ formatPercent(snapshot.analysis.estimatedAigc) }}
            </option>
          </select>
        </label>
        <label>
          <span>改写稿快照</span>
          <select v-model="draft.rewrittenSnapshotId">
            <option value="">未选择</option>
            <option v-for="snapshot in store.detectionSnapshots" :key="snapshot.id" :value="snapshot.id">
              {{ snapshot.fileName }} · {{ formatPercent(snapshot.analysis.estimatedAigc) }}
            </option>
          </select>
        </label>
        <label>
          <span>PP AIGC率</span>
          <input v-model.number="draft.measuredAigc" type="number" min="0" max="100" step="0.01" />
        </label>
        <label>
          <span>查重率</span>
          <input v-model.number="draft.plagiarismRate" type="number" min="0" max="100" step="0.01" />
        </label>
        <label>
          <span>策略</span>
          <select v-model="draft.strategy">
            <option value="">未选择</option>
            <option value="sample_calibrated_17_v2">成功链路17 2.0</option>
            <option value="sample_calibrated_17_success">成功链路17（基线）</option>
          </select>
        </label>
        <label>
          <span>轮次</span>
          <select v-model="draft.round">
            <option value="">未选择</option>
            <option value="直接全文">直接全文</option>
            <option value="试跑后补剩余全文">试跑后补剩余全文</option>
            <option value="试跑后叠加全文">试跑后叠加全文</option>
            <option value="二次全文">二次全文</option>
            <option value="多轮全文">多轮全文</option>
            <option value="测试20段">测试20段</option>
            <option value="手动备注">手动备注</option>
          </select>
        </label>
        <label v-if="draft.round === '手动备注'">
          <span>自定义轮次</span>
          <input v-model="draft.customRound" placeholder="例如：基于25.98稿二跑基线" />
        </label>
        <label class="wide">
          <span>备注</span>
          <input v-model="draft.note" placeholder="例如：测试20后整篇 / 直跑整篇 / 二次全文处理" />
        </label>
      </div>
      <button class="primary-button" type="button" :disabled="store.loading" @click="saveFeedback">
        <Save :size="16" />
        保存反馈
      </button>
    </section>

    <section class="panel analysis-section">
      <div class="section-heading">
        <span>校准规则</span>
        <small>根据闭环反馈自动修正混合检测</small>
      </div>
      <div v-if="!store.calibrationRules.length" class="muted-line">还没有足够反馈生成规则。</div>
      <div v-else class="sample-list">
        <div v-for="rule in store.calibrationRules" :key="rule.id" class="sample-row">
          <span>
            <strong>{{ rule.pattern }}</strong>
            <small>{{ rule.summary }}</small>
          </span>
          <b>{{ rule.correction > 0 ? "+" : "" }}{{ rule.correction.toFixed(2) }}</b>
        </div>
      </div>
    </section>

    <section class="panel analysis-section">
      <div class="section-heading">
        <span>反馈记录</span>
        <small>原稿 → 改写稿/外部报告 → 实测</small>
      </div>
      <div v-if="!store.feedbackRecords.length" class="empty-state compact-empty">
        <div>
          <Database :size="28" />
          <strong>还没有 PP 实测反馈。</strong>
        </div>
      </div>
      <div v-else class="sample-list">
        <div v-for="record in store.feedbackRecords" :key="record.id" class="sample-row feedback-row">
          <span>
            <strong>{{ snapshotLabel(record.originalSnapshotId) }} → {{ snapshotLabel(record.rewrittenSnapshotId) }}</strong>
            <small>
              原稿App {{ formatPercent(record.originalAppEstimatedAigc) }} · 改写App {{ formatPercent(record.rewrittenAppEstimatedAigc ?? record.appEstimatedAigc) }}
              · {{ providerLabel(record.provider) }} {{ formatPercent(record.measuredAigc) }}
              · 误差 {{ record.estimationError == null ? "未计算" : record.estimationError.toFixed(2) }}
            </small>
            <small v-if="record.externalReport">
              报告片段 {{ record.externalReport.suspiciousSegmentCount }} 个 · 标注 {{ record.externalReport.markedSpanCount }} 处 ·
              重/中/轻 {{ record.externalReport.severeSegmentCount }}/{{ record.externalReport.moderateSegmentCount }}/{{ record.externalReport.mildSegmentCount }}
            </small>
            <small v-if="comparisonSummary(record)">{{ comparisonSummary(record) }}</small>
            <small v-if="record.estimatedDrop != null">App预估降幅 {{ record.estimatedDrop.toFixed(2) }} 个百分点</small>
            <small>{{ record.aiReview }}</small>
          </span>
          <b>{{ profileLabel(record.strategy) }}</b>
        </div>
      </div>
    </section>
  </main>
</template>

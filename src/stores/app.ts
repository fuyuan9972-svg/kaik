import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AigcAnalysis,
  AigcCalibrationInput,
  AigcCalibrationRule,
  AigcCalibrationSample,
  AigcDetectionSnapshot,
  AigcFeedbackInput,
  AigcFeedbackRecord,
  ApiConfig,
  PageName,
  Paragraph,
  RewriteOptions,
  RewriteProgress,
  RewriteResult,
  RewriteScopeStats,
  RewriteSession,
  TrialEvaluation,
} from "../types";

interface AppState {
  page: PageName;
  filePath: string;
  paragraphs: Paragraph[];
  results: RewriteResult[];
  sessions: RewriteSession[];
  currentSessionId: string;
  scopeStats: RewriteScopeStats | null;
  detectFilePath: string;
  aigcAnalysis: AigcAnalysis | null;
  originalSnapshotId: string;
  originalAigcAnalysis: AigcAnalysis | null;
  rewrittenSnapshotId: string;
  rewrittenAigcAnalysis: AigcAnalysis | null;
  trialDraftAigcAnalysis: AigcAnalysis | null;
  stackedFullAigcAnalysis: AigcAnalysis | null;
  directFullAigcAnalysis: AigcAnalysis | null;
  aigcCalibrations: AigcCalibrationSample[];
  detectionSnapshots: AigcDetectionSnapshot[];
  feedbackRecords: AigcFeedbackRecord[];
  calibrationRules: AigcCalibrationRule[];
  sampleIndices: number[];
  trialEvaluation: TrialEvaluation | null;
  taskStage: "idle" | "detected" | "trialReady" | "evaluated" | "rewriting" | "completed";
  progressCurrent: number;
  progressTotal: number;
  config: ApiConfig;
  currentAiRate: number;
  targetAiRate: number;
  activeRewriteKind: "full" | "sample" | "stacked" | "";
  cancelRewriteRequested: boolean;
  _saveResultsTimer: ReturnType<typeof setTimeout> | null;
  loading: boolean;
  status: string;
  error: string;
}

type DetectionTarget = "original" | "rewritten";

const defaultConfig: ApiConfig = {
  apiBase: "https://api.openai.com/v1",
  apiKey: "",
  model: "gpt-5.5",
  language: "zh",
  promptProfile: "sample_calibrated_17_v2",
  detectApiBase: "",
  detectApiKey: "",
  detectModel: "gpt-5.4",
};

function hasHistoryResults(session: RewriteSession) {
  return session.results.length > 0;
}

export const useAppStore = defineStore("app", {
  state: (): AppState => ({
    page: "home",
    filePath: "",
    paragraphs: [],
    results: [],
    sessions: [],
    currentSessionId: "",
    scopeStats: null,
    detectFilePath: "",
    aigcAnalysis: null,
    originalSnapshotId: "",
    originalAigcAnalysis: null,
    rewrittenSnapshotId: "",
    rewrittenAigcAnalysis: null,
    trialDraftAigcAnalysis: null,
    stackedFullAigcAnalysis: null,
    directFullAigcAnalysis: null,
    aigcCalibrations: [],
    detectionSnapshots: [],
    feedbackRecords: [],
    calibrationRules: [],
    sampleIndices: [],
    trialEvaluation: null,
    taskStage: "idle",
    progressCurrent: 0,
    progressTotal: 0,
    config: { ...defaultConfig },
    currentAiRate: 60,
    targetAiRate: 10,
    activeRewriteKind: "",
    cancelRewriteRequested: false,
    _saveResultsTimer: null,
    loading: false,
    status: "",
    error: "",
  }),

  getters: {
    rewriteableCount: (state) =>
      state.scopeStats?.selected ?? state.paragraphs.filter((item) => !item.skip).length,
    skippedParagraphCount: (state) =>
      state.scopeStats?.skipped ?? state.paragraphs.filter((item) => item.skip).length,
    topSkipCategories: (state) =>
      [...(state.scopeStats?.categories ?? [])].sort((a, b) => b.count - a.count).slice(0, 6),
    progressPercent: (state) =>
      state.progressTotal === 0 ? 0 : Math.round((state.progressCurrent / state.progressTotal) * 100),
    acceptedCount: (state) => state.results.filter((item) => item.accepted && !item.skipped).length,
    skippedResultCount: (state) => state.results.filter((item) => item.skipped).length,
    failedCount: (state) => state.results.filter((item) => item.failed).length,
    visibleResults: (state) => state.results.filter((item) => !item.skipped),
    historySessions: (state) => state.sessions.filter(hasHistoryResults),
    currentSession: (state) =>
      state.sessions.find((session) => session.id === state.currentSessionId) ?? null,
    latestExternalReport: (state) =>
      state.feedbackRecords.find((record) => record.externalReport)?.externalReport ?? null,
  },

  actions: {
    async initialize() {
      try {
        const loadedConfig = { ...defaultConfig, ...(await invoke<ApiConfig>("load_config")) };
        if (loadedConfig.promptProfile === "sample_calibrated_17") {
          loadedConfig.promptProfile = "sample_calibrated_17_v2";
        }
        if (
          loadedConfig.promptProfile !== "sample_calibrated_17_v2" &&
          loadedConfig.promptProfile !== "sample_calibrated_17_success"
        ) {
          loadedConfig.promptProfile = "sample_calibrated_17_v2";
        }
        if (!loadedConfig.detectModel?.trim()) {
          loadedConfig.detectModel = "gpt-5.4";
        }
        this.config = loadedConfig;
        const [sessions, calibrations, snapshots, feedback, rules] = await Promise.all([
          invoke<RewriteSession[]>("load_sessions"),
          invoke<AigcCalibrationSample[]>("load_aigc_calibrations"),
          invoke<AigcDetectionSnapshot[]>("load_detection_snapshots"),
          invoke<AigcFeedbackRecord[]>("load_feedback_records"),
          invoke<AigcCalibrationRule[]>("load_calibration_rules"),
        ]);
        this.sessions = sessions;
        this.aigcCalibrations = calibrations;
        this.detectionSnapshots = snapshots;
        this.feedbackRecords = feedback;
        this.calibrationRules = rules;
        const latest = this.sessions.find(hasHistoryResults);
        if (latest) {
          this.applySession(latest);
          this.page = "compare";
        }
      } catch (error) {
        this.error = String(error);
      }
    },

    async parseFile(filePath: string) {
      await this.flushPendingResultsSave();
      this.loading = true;
      this.activeRewriteKind = "";
      this.cancelRewriteRequested = false;
      this.error = "";
      this.status = "正在解析文件";
      try {
        this.filePath = filePath;
        this.paragraphs = await invoke<Paragraph[]>("parse_file", { filePath });
        this.resetRewriteState();
        await this.refreshScopeStats();
      } catch (error) {
        this.error = String(error);
      } finally {
        this.loading = false;
        this.status = "";
      }
    },

    async startUnifiedTask(filePath: string) {
      await this.parseFile(filePath);
      if (!this.error) {
        await this.analyzeCurrentFileAi();
      }
    },

    async rewrite(options: RewriteOptions = {}) {
      await this.flushPendingResultsSave();
      const activeKind = options.sampleLimit ? "sample" : options.stackedFull ? "stacked" : "full";
      const sourceParagraphs = options.stackedFull ? this.mergeRewriteResults(this.paragraphs, this.results) : this.paragraphs;
      const existingResults = options.sampleLimit || options.stackedFull
        ? []
        : this.results.filter((item) => item.accepted && !item.failed && !item.skipped);
      const excludeIndices = options.sampleLimit || options.stackedFull ? [] : existingResults.map((item) => item.index);
      const rewriteOptions = {
        ...options,
        externalReport: options.externalReport ?? this.latestExternalReport,
        excludeIndices,
        currentAiRate: this.currentAiRate,
        targetAiRate: this.targetAiRate,
      };
      const scopeStats = await this.estimateScopeStats(rewriteOptions, sourceParagraphs);
      this.loading = true;
      this.activeRewriteKind = activeKind;
      this.cancelRewriteRequested = false;
      this.error = "";
      this.results = [...existingResults];
      this.progressCurrent = 0;
      this.progressTotal = scopeStats.selected;
      this.status = `正在调用 API 改写段落 0/${this.progressTotal}`;
      let session = this.currentSession;
      if (session && !options.sampleLimit && (existingResults.length > 0 || options.stackedFull)) {
        session = {
          ...session,
          model: this.config.model,
          promptProfile: this.config.promptProfile,
          language: this.config.language,
          isSample: false,
          sampleLimit: null,
          currentAiRate: rewriteOptions.currentAiRate ?? null,
          targetAiRate: rewriteOptions.targetAiRate ?? null,
          taskType: options.stackedFull
            ? "stackedFullRewrite"
            : options.sampleLimit
              ? "sampleTrial"
              : options.includeIndices?.length
                ? "guidedRewrite"
                : "fullRewrite",
          paragraphs: sourceParagraphs,
          results: this.results,
          updatedAt: Date.now().toString(),
        };
      } else {
        session = this.buildSession(this.filePath, sourceParagraphs, this.results, rewriteOptions);
      }
      await this.persistSession(session);
      this.applySession(session);
      const unlisten = await listen<RewriteProgress>("rewrite-progress", (event) => {
        const progress = event.payload;
        const existingIndex = this.results.findIndex((item) => item.index === progress.result.index);
        if (existingIndex >= 0) {
          this.results[existingIndex] = progress.result;
        } else {
          this.results.push(progress.result);
        }
        this.results.sort((a, b) => a.index - b.index);
        this.updateCurrentSession({ results: this.results });
        this.progressCurrent = progress.current;
        this.progressTotal = progress.total;
        this.status = progress.status;
      });
      try {
        const finalResults = await invoke<RewriteResult[]>("rewrite_paragraphs", {
          paragraphs: sourceParagraphs,
          config: this.config,
          options: rewriteOptions,
          session: this.currentSession,
        });
        this.results = finalResults;
        this.updateCurrentSession({ results: this.results });
        if (this.cancelRewriteRequested) {
          this.status = `已中止，已保留 ${this.results.length} 个段落结果`;
        } else if (options.sampleLimit) {
          this.taskStage = "trialReady";
          await this.evaluateTrialRewrite();
        } else {
          await this.analyzeRewriteDraft(
            options.stackedFull ? "stacked" : "direct",
            sourceParagraphs,
            finalResults,
          );
          this.taskStage = "completed";
          this.page = "compare";
        }
      } catch (error) {
        this.error = String(error);
      } finally {
        unlisten();
        this.loading = false;
        if (!this.cancelRewriteRequested) {
          this.status = "";
        }
        this.activeRewriteKind = "";
        this.cancelRewriteRequested = false;
        this.progressCurrent = 0;
        this.progressTotal = 0;
      }
    },

    async runSampleTest() {
      this.config.promptProfile = this.config.promptProfile || "sample_calibrated_17_v2";
      const includeIndices = await this.refreshSampleIndices(20);
      await this.rewrite({ sampleLimit: 20, includeIndices });
    },

    async rewriteStackedFull() {
      if (!this.results.some((item) => item.accepted && !item.failed && !item.skipped)) {
        await this.runSampleTest();
        return;
      }
      await this.rewrite({ stackedFull: true });
    },

    async evaluateTrialRewrite() {
      if (!this.results.length) {
        return;
      }
      this.loading = true;
      this.error = "";
      this.status = "正在评估测试20段效果";
      try {
        this.trialEvaluation = await invoke<TrialEvaluation>("evaluate_trial_rewrite", {
          config: this.config,
          input: {
            analysis: this.aigcAnalysis,
            paragraphs: this.paragraphs,
            results: this.results,
            promptProfile: this.config.promptProfile,
            currentAiRate: this.currentAiRate,
            targetAiRate: this.targetAiRate,
            trialTargetAigc: this.trialTargetAigc(),
          },
        });
        await this.analyzeRewriteDraft("trial", this.paragraphs, this.results);
        this.alignTrialEvaluationWithMixedDetection();
        if (this.trialEvaluation?.recommendedProfile) {
          this.config.promptProfile = this.trialEvaluation.recommendedProfile;
        }
        if (this.trialEvaluation?.recommendedCurrentAiRate != null) {
          this.currentAiRate = this.trialEvaluation.recommendedCurrentAiRate;
        }
        if (this.trialEvaluation?.recommendedTargetAiRate != null) {
          this.targetAiRate = this.trialEvaluation.recommendedTargetAiRate;
        }
        this.taskStage = "evaluated";
        this.status = "试跑评估完成";
      } catch (error) {
        this.error = String(error);
      } finally {
        this.loading = false;
      }
    },

    async cancelRewrite() {
      if (!this.activeRewriteKind) {
        return;
      }
      this.cancelRewriteRequested = true;
      this.status =
        this.activeRewriteKind === "sample"
          ? "正在中止测试，当前段落返回后停止"
          : "正在中止改写，当前段落返回后停止";
      try {
        await invoke("cancel_rewrite");
      } catch (error) {
        this.error = String(error);
      }
    },

    async testConnection() {
      this.loading = true;
      this.error = "";
      this.status = "正在测试 API 连通性";
      try {
        await invoke<boolean>("test_connection", { config: this.config });
        this.status = "API 连通正常";
      } catch (error) {
        this.error = String(error);
      } finally {
        this.loading = false;
      }
    },

    async saveConfig() {
      this.error = "";
      try {
        await invoke("save_config", { config: this.config });
        this.status = "配置已保存";
      } catch (error) {
        this.error = String(error);
      }
    },

    async saveResults() {
      this.updateCurrentSession({ results: this.results });
      if (this.currentSession) {
        this.sessions = await invoke<RewriteSession[]>("save_session", { session: this.currentSession });
      }
    },

    async exportDocx(outputPath: string) {
      await this.flushPendingResultsSave();
      this.loading = true;
      this.error = "";
      this.status = "正在导出 .docx";
      try {
        if (this.currentSessionId) {
          const exportedPath = await invoke<string>("export_session_docx", {
            sessionId: this.currentSessionId,
            outputPath,
          });
          this.updateCurrentSession({ exportedPath });
          await this.analyzeExportedDocx(exportedPath);
        } else {
          const exportedPath = await invoke<string>("export_docx", {
            results: this.results,
            outputPath,
          });
          await this.analyzeExportedDocx(exportedPath);
        }
        this.status = "导出完成，已自动检测导出稿";
      } catch (error) {
        this.error = String(error);
      } finally {
        this.loading = false;
      }
    },

    async analyzeExportedDocx(filePath: string) {
      try {
        await this.runAiDetection(filePath, "rewritten");
      } catch (error) {
        this.status = `导出完成，但自动检测导出稿失败：${String(error)}`;
      }
    },

    async analyzeRewriteDraft(
      target: "trial" | "stacked" | "direct",
      sourceParagraphs: Paragraph[],
      results: RewriteResult[],
    ) {
      const paragraphs = this.mergeRewriteResults(sourceParagraphs, results);
      const label =
        target === "trial"
          ? "测试20段后"
          : target === "stacked"
            ? "叠加全文后"
            : "直接全文后";
      try {
        const analysis = await invoke<AigcAnalysis>("analyze_aigc_paragraphs_ai", {
          fileName: `${label}-${this.filePath.split("/").pop() || "当前论文"}`,
          paragraphs,
          config: this.config,
        });
        if (target === "trial") this.trialDraftAigcAnalysis = analysis;
        else if (target === "stacked") this.stackedFullAigcAnalysis = analysis;
        else this.directFullAigcAnalysis = analysis;
      } catch (error) {
        this.status = `${label}混合检测失败：${String(error)}`;
      }
    },

    alignTrialEvaluationWithMixedDetection() {
      if (!this.trialEvaluation || !this.trialDraftAigcAnalysis) {
        return;
      }
      const measured = this.trialDraftAigcAnalysis.estimatedAigc;
      const target = this.trialTargetAigc();
      const original =
        this.originalAigcAnalysis?.estimatedAigc ??
        this.aigcAnalysis?.estimatedAigc ??
        this.currentAiRate;
      const drop = Math.max(0, Math.round((original - measured) * 10) / 10);
      const gap = Math.round((measured - target) * 10) / 10;
      const verdict = gap <= 3 ? "合格" : gap <= 10 ? "接近" : "偏弱";
      const recommendedAction = gap <= 3 ? "continue_full" : gap <= 10 ? "rewrite_risky_only" : "increase_strength";
      const summary =
        gap <= 3
          ? `测试20段后混合检测为${measured.toFixed(1)}%，已接近试跑目标${target.toFixed(1)}%，可继续叠加跑全文。`
          : gap <= 10
            ? `测试20段后混合检测为${measured.toFixed(1)}%，比试跑目标${target.toFixed(1)}%高${gap.toFixed(1)}点，整体有效但建议叠加全文时优先处理高风险段。`
            : `测试20段后混合检测为${measured.toFixed(1)}%，比试跑目标${target.toFixed(1)}%高${gap.toFixed(1)}点，已降低${drop.toFixed(1)}点但力度仍偏弱。`;
      this.trialEvaluation = {
        ...this.trialEvaluation,
        verdict,
        recommendedAction,
        recommendedCurrentAiRate: measured,
        summary,
        risks: [
          `混合检测口径：${measured.toFixed(1)}%，区间${this.trialDraftAigcAnalysis.rangeLow.toFixed(1)}%-${this.trialDraftAigcAnalysis.rangeHigh.toFixed(1)}%。`,
          ...this.trialEvaluation.risks.filter((risk) => !risk.includes("48%")).slice(0, 3),
        ],
      };
    },

    setAccepted(index: number, accepted: boolean) {
      const item = this.results.find((result) => result.index === index);
      if (item) {
        item.accepted = accepted;
        this.scheduleResultsSave();
      }
    },

    async deleteSession(sessionId: string) {
      await this.flushPendingResultsSave();
      this.sessions = await invoke<RewriteSession[]>("delete_session", { sessionId });
      if (this.currentSessionId === sessionId) {
        const next = this.sessions.find(hasHistoryResults);
        if (next) {
          this.applySession(next);
          this.page = "compare";
        } else {
          this.filePath = "";
          this.paragraphs = [];
          this.scopeStats = null;
          this.resetRewriteState();
          this.page = "home";
        }
      }
    },

    async analyzeAigcFile(filePath: string) {
      this.loading = true;
      this.error = "";
      this.status = "正在本地快速检测 AIGC 风险";
      try {
        this.detectFilePath = filePath;
        this.aigcAnalysis = await invoke<AigcAnalysis>("analyze_aigc_file", { filePath });
        this.page = "detect";
      } catch (error) {
        this.error = String(error);
      } finally {
        this.loading = false;
        this.status = "";
      }
    },

    async analyzeAigcFileAi(filePath: string) {
      this.loading = true;
      this.error = "";
      this.status = "正在 AI 混合检测 AIGC 风险";
      try {
        await this.runAiDetection(filePath, "original");
        this.taskStage = "detected";
        this.page = "detect";
      } catch (error) {
        this.error = String(error);
      } finally {
        this.loading = false;
        this.status = "";
      }
    },

    async analyzeCurrentFileAi() {
      if (!this.filePath) {
        return;
      }
      this.loading = true;
      this.error = "";
      this.status = "正在 AI 混合检测 AIGC 风险";
      try {
        await this.runAiDetection(this.filePath, "original");
        this.taskStage = "detected";
      } catch (error) {
        this.error = String(error);
      } finally {
        this.loading = false;
        this.status = "";
      }
    },

    async runAiDetection(filePath: string, target: DetectionTarget) {
      this.detectFilePath = filePath;
      const analysis = await invoke<AigcAnalysis>("analyze_aigc_file_ai", {
        filePath,
        config: this.config,
      });
      this.detectionSnapshots = await invoke<AigcDetectionSnapshot[]>("load_detection_snapshots");
      const snapshotId = this.findSnapshotId(filePath, analysis);
      if (target === "original") {
        this.aigcAnalysis = analysis;
        this.originalAigcAnalysis = analysis;
        this.originalSnapshotId = snapshotId || this.originalSnapshotId;
        await this.refreshSampleIndices(20);
      } else {
        this.rewrittenAigcAnalysis = analysis;
        this.rewrittenSnapshotId = snapshotId || this.rewrittenSnapshotId;
      }
      return analysis;
    },

    async loadAigcCalibrations() {
      this.aigcCalibrations = await invoke<AigcCalibrationSample[]>("load_aigc_calibrations");
    },

    async saveAigcCalibration(input: AigcCalibrationInput) {
      this.loading = true;
      this.error = "";
      this.status = "正在保存实测 AIGC 校准样本";
      try {
        this.aigcCalibrations = await invoke<AigcCalibrationSample[]>("save_aigc_calibration", { input });
        if (this.detectFilePath) {
          this.aigcAnalysis = await invoke<AigcAnalysis>("analyze_aigc_file", {
            filePath: this.detectFilePath,
          });
        }
        this.status = "校准样本已保存";
      } catch (error) {
        this.error = String(error);
      } finally {
        this.loading = false;
      }
    },

    async deleteAigcCalibration(sampleId: string) {
      this.loading = true;
      this.error = "";
      this.status = "正在删除校准样本";
      try {
        this.aigcCalibrations = await invoke<AigcCalibrationSample[]>("delete_aigc_calibration", {
          sampleId,
        });
        if (this.detectFilePath) {
          this.aigcAnalysis = await invoke<AigcAnalysis>("analyze_aigc_file", {
            filePath: this.detectFilePath,
          });
        }
        this.status = "校准样本已删除";
      } catch (error) {
        this.error = String(error);
      } finally {
        this.loading = false;
      }
    },

    async saveFeedbackRecord(input: AigcFeedbackInput) {
      this.loading = true;
      this.error = "";
      this.status = "正在保存 PP 实测反馈";
      try {
        this.feedbackRecords = await invoke<AigcFeedbackRecord[]>("save_feedback_record", {
          config: this.config,
          input,
        });
        this.calibrationRules = await invoke<AigcCalibrationRule[]>("load_calibration_rules");
        this.detectionSnapshots = await invoke<AigcDetectionSnapshot[]>("load_detection_snapshots");
        this.status = "PP 实测反馈已保存";
      } catch (error) {
        this.error = String(error);
      } finally {
        this.loading = false;
      }
    },

    findSnapshotId(filePath: string, analysis: AigcAnalysis) {
      const fileName = filePath.split("/").pop() || analysis.fileName;
      return (
        this.detectionSnapshots.find(
          (snapshot) =>
            snapshot.filePath === filePath &&
            snapshot.analysis.estimatedAigc === analysis.estimatedAigc,
        )?.id ||
        this.detectionSnapshots.find((snapshot) => snapshot.filePath === filePath)?.id ||
        this.detectionSnapshots.find((snapshot) => snapshot.fileName === fileName)?.id ||
        ""
      );
    },

    trialTargetAigc() {
      const baseline =
        this.originalAigcAnalysis?.estimatedAigc ??
        this.aigcAnalysis?.estimatedAigc ??
        this.currentAiRate;
      return Math.max(0, Math.round((baseline - 15) * 10) / 10);
    },

    applySession(session: RewriteSession) {
      this.currentSessionId = session.id;
      this.filePath = session.filePath;
      this.paragraphs = session.paragraphs;
      this.results = session.results;
      void this.refreshScopeStats();
    },

    async openSession(session: RewriteSession) {
      await this.flushPendingResultsSave();
      this.applySession(session);
      this.page = "compare";
    },

    async refreshScopeStats(options: RewriteOptions = {}) {
      this.scopeStats = await this.estimateScopeStats(options);
    },

    resetRewriteState() {
      this.results = [];
      this.aigcAnalysis = null;
      this.originalSnapshotId = "";
      this.originalAigcAnalysis = null;
      this.rewrittenSnapshotId = "";
      this.rewrittenAigcAnalysis = null;
      this.trialDraftAigcAnalysis = null;
      this.stackedFullAigcAnalysis = null;
      this.directFullAigcAnalysis = null;
      this.sampleIndices = [];
      this.trialEvaluation = null;
      this.taskStage = "idle";
      this.progressCurrent = 0;
      this.progressTotal = 0;
      this.currentSessionId = "";
    },

    scheduleResultsSave() {
      if (this._saveResultsTimer) {
        clearTimeout(this._saveResultsTimer);
      }
      this._saveResultsTimer = setTimeout(() => {
        this._saveResultsTimer = null;
        void this.saveResults();
      }, 800);
    },

    async flushPendingResultsSave() {
      if (!this._saveResultsTimer) {
        return;
      }
      clearTimeout(this._saveResultsTimer);
      this._saveResultsTimer = null;
      await this.saveResults();
    },

    async refreshSampleIndices(limit = 20) {
      if (this.paragraphs.length === 0) {
        this.sampleIndices = [];
        return [];
      }
      this.sampleIndices = await invoke<number[]>("select_sample_indices", {
        paragraphs: this.paragraphs,
        analysis: this.aigcAnalysis,
        limit,
      });
      return this.sampleIndices;
    },

    async estimateScopeStats(options: RewriteOptions = {}, paragraphs?: Paragraph[]) {
      const sourceParagraphs = paragraphs ?? this.paragraphs;
      if (sourceParagraphs.length === 0) {
        return {
          total: 0,
          selected: 0,
          skipped: 0,
          categories: [],
        };
      }

      return await invoke<RewriteScopeStats>("estimate_rewrite_scope", {
        paragraphs: sourceParagraphs,
        options,
      });
    },

    mergeRewriteResults(paragraphs: Paragraph[], results: RewriteResult[]) {
      const replacements = new Map(
        results
          .filter((item) => item.accepted && !item.failed && !item.skipped)
          .map((item) => [item.index, item.rewritten]),
      );
      return paragraphs.map((paragraph) => ({
        ...paragraph,
        text: replacements.get(paragraph.index) ?? paragraph.text,
      }));
    },

    buildSession(
      filePath: string,
      paragraphs: Paragraph[],
      results: RewriteResult[],
      options: RewriteOptions,
    ): RewriteSession {
      const now = Date.now().toString();
      const fileName = filePath.split("/").pop() || "未命名论文";
      return {
        id: `${Date.now()}-${Math.random().toString(16).slice(2)}`,
        fileName,
        filePath,
        createdAt: now,
        updatedAt: now,
        model: this.config.model,
        promptProfile: this.config.promptProfile,
        language: this.config.language,
        isSample: Boolean(options.sampleLimit),
        sampleLimit: options.sampleLimit ?? null,
        currentAiRate: options.currentAiRate ?? null,
        targetAiRate: options.targetAiRate ?? null,
        taskType: options.stackedFull
          ? "stackedFullRewrite"
          : options.sampleLimit
            ? "sampleTrial"
            : options.includeIndices?.length
              ? "guidedRewrite"
              : "fullRewrite",
        paragraphs,
        results,
        exportedPath: null,
      };
    },

    async persistSession(session: RewriteSession) {
      this.sessions = await invoke<RewriteSession[]>("save_session", { session });
    },

    updateCurrentSession(patch: Partial<RewriteSession>) {
      const index = this.sessions.findIndex((session) => session.id === this.currentSessionId);
      if (index === -1) {
        return;
      }
      this.sessions[index] = {
        ...this.sessions[index],
        ...patch,
        updatedAt: Date.now().toString(),
      };
    },

  },
});

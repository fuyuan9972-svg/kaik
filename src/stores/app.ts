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

const activePromptProfiles = new Set(["sample_calibrated_17_v2", "sample_calibrated_17_success"]);

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
        if (!activePromptProfiles.has(loadedConfig.promptProfile)) {
          loadedConfig.promptProfile = "sample_calibrated_17_v2";
        }
        if (!loadedConfig.detectModel?.trim()) {
          loadedConfig.detectModel = "gpt-5.4";
        }
        this.config = loadedConfig;
        this.sessions = await invoke<RewriteSession[]>("load_sessions");
        this.aigcCalibrations = await invoke<AigcCalibrationSample[]>("load_aigc_calibrations");
        this.detectionSnapshots = await invoke<AigcDetectionSnapshot[]>("load_detection_snapshots");
        this.feedbackRecords = await invoke<AigcFeedbackRecord[]>("load_feedback_records");
        this.calibrationRules = await invoke<AigcCalibrationRule[]>("load_calibration_rules");
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
      this.loading = true;
      this.activeRewriteKind = "";
      this.cancelRewriteRequested = false;
      this.error = "";
      this.status = "正在解析文件";
      try {
        this.filePath = filePath;
        this.paragraphs = await invoke<Paragraph[]>("parse_file", { filePath });
        this.results = [];
        this.aigcAnalysis = null;
        this.originalSnapshotId = "";
        this.originalAigcAnalysis = null;
        this.rewrittenSnapshotId = "";
        this.rewrittenAigcAnalysis = null;
        this.sampleIndices = [];
        this.trialEvaluation = null;
        this.taskStage = "idle";
        this.progressCurrent = 0;
        this.progressTotal = 0;
        this.currentSessionId = "";
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
      const activeKind = options.sampleLimit ? "sample" : options.stackedFull ? "stacked" : "full";
      const sourceParagraphs = options.stackedFull ? this.buildTrialDraftParagraphs() : this.paragraphs;
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
        if (this.trialEvaluation.recommendedProfile) {
          this.config.promptProfile = this.trialEvaluation.recommendedProfile;
        }
        if (this.trialEvaluation.recommendedCurrentAiRate != null) {
          this.currentAiRate = this.trialEvaluation.recommendedCurrentAiRate;
        }
        if (this.trialEvaluation.recommendedTargetAiRate != null) {
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
      this.loading = true;
      this.error = "";
      this.status = "正在导出 .docx";
      try {
        if (this.currentSessionId) {
          const exportedPath = await invoke<string>("export_session_docx", {
            sessionId: this.currentSessionId,
            outputPath,
          });
          const activeId = this.currentSessionId;
          this.updateCurrentSession({ exportedPath });
          this.sessions = await invoke<RewriteSession[]>("load_sessions");
          const active = this.sessions.find((session) => session.id === activeId);
          if (active) {
            this.applySession(active);
          }
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

    setAccepted(index: number, accepted: boolean) {
      const item = this.results.find((result) => result.index === index);
      if (item) {
        item.accepted = accepted;
        void this.saveResults();
      }
    },

    async deleteSession(sessionId: string) {
      this.sessions = await invoke<RewriteSession[]>("delete_session", { sessionId });
      if (this.currentSessionId === sessionId) {
        const next = this.sessions.find(hasHistoryResults);
        if (next) {
          this.applySession(next);
          this.page = "compare";
        } else {
          this.currentSessionId = "";
          this.filePath = "";
          this.paragraphs = [];
          this.results = [];
          this.scopeStats = null;
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

    async refreshScopeStats(options: RewriteOptions = {}) {
      this.scopeStats = await this.estimateScopeStats(options);
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

    buildTrialDraftParagraphs() {
      const replacements = new Map(
        this.results
          .filter((item) => item.accepted && !item.failed && !item.skipped)
          .map((item) => [item.index, item.rewritten]),
      );
      return this.paragraphs.map((paragraph) => ({
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

export interface ParagraphStyle {
  bold: boolean;
  italic: boolean;
  fontSize: number | null;
}

export interface Paragraph {
  index: number;
  text: string;
  style: ParagraphStyle;
  skip: boolean;
  skipReason: string | null;
}

export interface RewriteResult {
  index: number;
  original: string;
  rewritten: string;
  accepted: boolean;
  skipped: boolean;
  failed: boolean;
  error: string | null;
  style: ParagraphStyle;
}

export interface RewriteProgress {
  current: number;
  total: number;
  status: string;
  result: RewriteResult;
}

export interface RewriteSkipCategory {
  reason: string;
  count: number;
}

export interface RewriteScopeStats {
  total: number;
  selected: number;
  skipped: number;
  categories: RewriteSkipCategory[];
}

export interface RewriteSession {
  id: string;
  fileName: string;
  filePath: string;
  createdAt: string;
  updatedAt: string;
  model: string;
  promptProfile: string;
  language: string;
  isSample: boolean;
  sampleLimit: number | null;
  currentAiRate: number | null;
  targetAiRate: number | null;
  taskType?: "sampleTrial" | "guidedRewrite" | "fullRewrite" | "stackedFullRewrite" | "reportGuidedRewrite" | null;
  paragraphs: Paragraph[];
  results: RewriteResult[];
  exportedPath: string | null;
}

export interface RewriteOptions {
  sampleLimit?: number | null;
  excludeIndices?: number[];
  includeIndices?: number[];
  stackedFull?: boolean;
  currentAiRate?: number | null;
  targetAiRate?: number | null;
  externalReport?: ExternalAigcReportEvidence | null;
  reportGuided?: boolean;
}

export interface ApiConfig {
  apiBase: string;
  apiKey: string;
  model: string;
  language: "zh" | "en";
  detectApiBase?: string;
  detectApiKey?: string;
  detectModel?: string;
  promptProfile:
    | "sample_calibrated_17_v2"
    | "sample_calibrated_17_success"
    | "sample_calibrated_17"
    | "sample_calibrated_28"
    | "directive_aigc_reduce"
    | "directive_aigc_reduce_legacy"
    | "doubao_plain_humanize"
    | "plain_spoken_humanize"
    | "local_light_rewrite"
    | "local_depattern"
    | "paperpass_restructure"
    | "conservative_rewrite"
    | "light_rewrite"
    | "academic_humanizer";
}

export interface AigcMetrics {
  totalParagraphs: number;
  bodyParagraphs: number;
  cjkChars: number;
  avgParagraphLen: number;
  avgSentenceLen: number;
  punctuationPer100: number;
  aiTermsPer10k: number;
  bufferTermsPer10k: number;
  plainTermsPer10k: number;
  connectorsPer10k: number;
}

export interface AigcMetricsDelta {
  cjkCharsDelta: number;
  avgParagraphLenDelta: number;
  avgSentenceLenDelta: number;
  punctuationPer100Delta: number;
  aiTermsPer10kDelta: number;
  bufferTermsPer10kDelta: number;
  plainTermsPer10kDelta: number;
  connectorsPer10kDelta: number;
}

export interface AigcCalibrationSample {
  id: string;
  fileName: string;
  measuredAigc: number;
  plagiarismRate: number | null;
  strategy: string | null;
  round: string | null;
  note: string | null;
  metrics: AigcMetrics;
  createdAt: string;
  updatedAt: string;
}

export interface AigcSimilarSample {
  fileName: string;
  measuredAigc: number;
  similarity: number;
}

export interface AigcParagraphRisk {
  index: number;
  text: string;
  risk: number;
  reasons: string[];
}

export interface AiAigcParagraphScore {
  index: number;
  risk: number;
  profile: string;
  reasons: string[];
  text: string;
}

export interface AiAigcAssessment {
  estimatedAigc: number;
  rangeLow: number;
  rangeHigh: number;
  confidence: number;
  profile: string;
  summary: string;
  nextAction: string;
  paragraphScores: AiAigcParagraphScore[];
  reasons: string[];
}

export interface AigcAnalysis {
  fileName: string;
  estimatedAigc: number;
  rangeLow: number;
  rangeHigh: number;
  confidence: number;
  riskLevel: string;
  profile: string;
  summary: string;
  nextAction: string;
  metrics: AigcMetrics;
  similarSamples: AigcSimilarSample[];
  paragraphRisks: AigcParagraphRisk[];
  localEstimatedAigc?: number | null;
  uncalibratedEstimatedAigc?: number | null;
  calibrationCorrection?: number | null;
  calibrationSummary?: string | null;
  calibrationSampleCount?: number | null;
  aiAssessment?: AiAigcAssessment | null;
  aiError?: string | null;
  detectionMode: string;
}

export interface AigcCalibrationInput {
  filePath: string;
  measuredAigc: number;
  plagiarismRate?: number | null;
  strategy?: string | null;
  round?: string | null;
  note?: string | null;
}

export interface TrialEvaluation {
  verdict: string;
  recommendedAction: string;
  recommendedProfile: ApiConfig["promptProfile"];
  recommendedCurrentAiRate?: number | null;
  recommendedTargetAiRate?: number | null;
  recommendedParagraphIndices: number[];
  summary: string;
  risks: string[];
  confidence: number;
}

export interface TrialEvaluationInput {
  analysis?: AigcAnalysis | null;
  paragraphs: Paragraph[];
  results: RewriteResult[];
  promptProfile: string;
  currentAiRate?: number | null;
  targetAiRate?: number | null;
  trialTargetAigc?: number | null;
}

export interface AigcDetectionSnapshot {
  id: string;
  fileName: string;
  filePath: string;
  createdAt: string;
  analysis: AigcAnalysis;
}

export interface AigcFeedbackInput {
  measuredAigc: number;
  plagiarismRate?: number | null;
  provider?: string | null;
  originalSnapshotId?: string | null;
  rewrittenSnapshotId?: string | null;
  sessionId?: string | null;
  strategy?: string | null;
  round?: string | null;
  note?: string | null;
  externalReport?: ExternalAigcReportEvidence | null;
}

export interface ExternalAigcReportSegment {
  no: number;
  text: string;
  suspectedChars: number;
  suspectedRatio: number;
  segmentKind?: "body" | "appendix" | "reference" | string;
}

export interface ExternalAigcReportEvidence {
  provider: string;
  reportFileName?: string | null;
  reportFilePath?: string | null;
  reportScore?: number | null;
  totalSuspectedRatio?: number | null;
  highAndMiddleSuspectedRatio?: number | null;
  highSuspectedRatio?: number | null;
  middleSuspectedRatio?: number | null;
  lowSuspectedRatio?: number | null;
  noAiSuspectedRatio?: number | null;
  humanWrittenRate?: number | null;
  suspiciousSegmentCount: number;
  bodySuspiciousSegmentCount?: number | null;
  appendixLikeSegmentCount?: number | null;
  referenceSegmentCount?: number | null;
  markedSpanCount: number;
  markedChars: number;
  severeSegmentCount: number;
  moderateSegmentCount: number;
  mildSegmentCount: number;
  riskTypes: string[];
  segments: ExternalAigcReportSegment[];
  analysisSummary?: string | null;
  rewriteGuidance?: string | null;
}

export interface AigcFeedbackRecord {
  id: string;
  measuredAigc: number;
  plagiarismRate: number | null;
  provider: string | null;
  originalSnapshotId: string | null;
  rewrittenSnapshotId: string | null;
  sessionId: string | null;
  strategy: string | null;
  round: string | null;
  note: string | null;
  originalAppEstimatedAigc: number | null;
  rewrittenAppEstimatedAigc: number | null;
  appEstimatedAigc: number | null;
  estimationError: number | null;
  estimatedDrop: number | null;
  metricsDelta: AigcMetricsDelta | null;
  aiReview: string | null;
  externalReport: ExternalAigcReportEvidence | null;
  createdAt: string;
  updatedAt: string;
}

export interface AigcCalibrationRule {
  id: string;
  pattern: string;
  correction: number;
  recommendedStrategy: string;
  confidence: number;
  sampleCount: number;
  summary: string;
  updatedAt: string;
}

export type PageName = "home" | "detect" | "compare" | "history" | "calibration" | "settings";

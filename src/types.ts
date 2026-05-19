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
  paragraphs: Paragraph[];
  results: RewriteResult[];
  exportedPath: string | null;
}

export interface RewriteOptions {
  sampleLimit?: number | null;
  excludeIndices?: number[];
  currentAiRate?: number | null;
  targetAiRate?: number | null;
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

export type PageName = "home" | "detect" | "compare" | "history" | "settings";

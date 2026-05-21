use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paragraph {
    pub index: usize,
    pub text: String,
    pub style: ParagraphStyle,
    pub skip: bool,
    pub skip_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParagraphStyle {
    pub bold: bool,
    pub italic: bool,
    pub font_size: Option<u32>,
}

impl Default for ParagraphStyle {
    fn default() -> Self {
        Self {
            bold: false,
            italic: false,
            font_size: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteResult {
    pub index: usize,
    pub original: String,
    pub rewritten: String,
    pub accepted: bool,
    #[serde(default)]
    pub skipped: bool,
    pub failed: bool,
    pub error: Option<String>,
    pub style: ParagraphStyle,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteOptions {
    #[serde(default)]
    pub sample_limit: Option<usize>,
    #[serde(default)]
    pub exclude_indices: Vec<usize>,
    #[serde(default)]
    pub include_indices: Vec<usize>,
    #[serde(default)]
    pub current_ai_rate: Option<f32>,
    #[serde(default)]
    pub target_ai_rate: Option<f32>,
    #[serde(default)]
    pub external_report: Option<ExternalAigcReportEvidence>,
    #[serde(default)]
    pub report_guided: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteProgress {
    pub current: usize,
    pub total: usize,
    pub status: String,
    pub result: RewriteResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteSkipCategory {
    pub reason: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteScopeStats {
    pub total: usize,
    pub selected: usize,
    pub skipped: usize,
    pub categories: Vec<RewriteSkipCategory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteSession {
    pub id: String,
    pub file_name: String,
    pub file_path: String,
    pub created_at: String,
    pub updated_at: String,
    pub model: String,
    pub prompt_profile: String,
    pub language: String,
    #[serde(default)]
    pub is_sample: bool,
    #[serde(default)]
    pub sample_limit: Option<usize>,
    #[serde(default)]
    pub current_ai_rate: Option<f32>,
    #[serde(default)]
    pub target_ai_rate: Option<f32>,
    #[serde(default)]
    pub task_type: Option<String>,
    pub paragraphs: Vec<Paragraph>,
    pub results: Vec<RewriteResult>,
    #[serde(default)]
    pub exported_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AigcMetrics {
    pub total_paragraphs: usize,
    pub body_paragraphs: usize,
    pub cjk_chars: usize,
    pub avg_paragraph_len: f32,
    pub avg_sentence_len: f32,
    pub punctuation_per_100: f32,
    pub ai_terms_per_10k: f32,
    pub buffer_terms_per_10k: f32,
    pub plain_terms_per_10k: f32,
    pub connectors_per_10k: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AigcMetricsDelta {
    pub cjk_chars_delta: isize,
    pub avg_paragraph_len_delta: f32,
    pub avg_sentence_len_delta: f32,
    pub punctuation_per_100_delta: f32,
    pub ai_terms_per_10k_delta: f32,
    pub buffer_terms_per_10k_delta: f32,
    pub plain_terms_per_10k_delta: f32,
    pub connectors_per_10k_delta: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AigcCalibrationSample {
    pub id: String,
    pub file_name: String,
    pub measured_aigc: f32,
    #[serde(default)]
    pub plagiarism_rate: Option<f32>,
    #[serde(default)]
    pub strategy: Option<String>,
    #[serde(default)]
    pub round: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    pub metrics: AigcMetrics,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AigcSimilarSample {
    pub file_name: String,
    pub measured_aigc: f32,
    pub similarity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AigcParagraphRisk {
    pub index: usize,
    pub text: String,
    pub risk: f32,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAigcParagraphScore {
    pub index: usize,
    pub risk: f32,
    pub profile: String,
    pub reasons: Vec<String>,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAigcAssessment {
    pub estimated_aigc: f32,
    pub range_low: f32,
    pub range_high: f32,
    pub confidence: f32,
    pub profile: String,
    pub summary: String,
    pub next_action: String,
    pub paragraph_scores: Vec<AiAigcParagraphScore>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AigcAnalysis {
    pub file_name: String,
    pub estimated_aigc: f32,
    pub range_low: f32,
    pub range_high: f32,
    pub confidence: f32,
    pub risk_level: String,
    pub profile: String,
    pub summary: String,
    pub next_action: String,
    pub metrics: AigcMetrics,
    pub similar_samples: Vec<AigcSimilarSample>,
    pub paragraph_risks: Vec<AigcParagraphRisk>,
    #[serde(default)]
    pub local_estimated_aigc: Option<f32>,
    #[serde(default)]
    pub uncalibrated_estimated_aigc: Option<f32>,
    #[serde(default)]
    pub calibration_correction: Option<f32>,
    #[serde(default)]
    pub calibration_summary: Option<String>,
    #[serde(default)]
    pub calibration_sample_count: Option<usize>,
    #[serde(default)]
    pub ai_assessment: Option<AiAigcAssessment>,
    #[serde(default)]
    pub ai_error: Option<String>,
    #[serde(default)]
    pub detection_mode: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrialEvaluation {
    pub verdict: String,
    pub recommended_action: String,
    pub recommended_profile: String,
    #[serde(default)]
    pub recommended_current_ai_rate: Option<f32>,
    #[serde(default)]
    pub recommended_target_ai_rate: Option<f32>,
    #[serde(default)]
    pub recommended_paragraph_indices: Vec<usize>,
    pub summary: String,
    #[serde(default)]
    pub risks: Vec<String>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrialEvaluationInput {
    #[serde(default)]
    pub analysis: Option<AigcAnalysis>,
    pub paragraphs: Vec<Paragraph>,
    pub results: Vec<RewriteResult>,
    pub prompt_profile: String,
    #[serde(default)]
    pub current_ai_rate: Option<f32>,
    #[serde(default)]
    pub target_ai_rate: Option<f32>,
    #[serde(default)]
    pub trial_target_aigc: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AigcDetectionSnapshot {
    pub id: String,
    pub file_name: String,
    pub file_path: String,
    pub created_at: String,
    pub analysis: AigcAnalysis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AigcFeedbackInput {
    pub measured_aigc: f32,
    #[serde(default)]
    pub plagiarism_rate: Option<f32>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub original_snapshot_id: Option<String>,
    #[serde(default)]
    pub rewritten_snapshot_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub strategy: Option<String>,
    #[serde(default)]
    pub round: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub external_report: Option<ExternalAigcReportEvidence>,
    #[serde(default)]
    pub before_external_report: Option<ExternalAigcReportEvidence>,
    #[serde(default)]
    pub after_external_report: Option<ExternalAigcReportEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAigcReportSegment {
    pub no: usize,
    pub text: String,
    pub suspected_chars: usize,
    pub suspected_ratio: f32,
    #[serde(default)]
    pub segment_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAigcReportEvidence {
    pub provider: String,
    #[serde(default)]
    pub report_file_name: Option<String>,
    #[serde(default)]
    pub report_file_path: Option<String>,
    #[serde(default)]
    pub report_score: Option<f32>,
    #[serde(default)]
    pub total_suspected_ratio: Option<f32>,
    #[serde(default)]
    pub high_and_middle_suspected_ratio: Option<f32>,
    #[serde(default)]
    pub high_suspected_ratio: Option<f32>,
    #[serde(default)]
    pub middle_suspected_ratio: Option<f32>,
    #[serde(default)]
    pub low_suspected_ratio: Option<f32>,
    #[serde(default)]
    pub no_ai_suspected_ratio: Option<f32>,
    #[serde(default)]
    pub human_written_rate: Option<f32>,
    #[serde(default)]
    pub suspicious_segment_count: usize,
    #[serde(default)]
    pub body_suspicious_segment_count: Option<usize>,
    #[serde(default)]
    pub appendix_like_segment_count: Option<usize>,
    #[serde(default)]
    pub reference_segment_count: Option<usize>,
    #[serde(default)]
    pub marked_span_count: usize,
    #[serde(default)]
    pub marked_chars: usize,
    #[serde(default)]
    pub severe_segment_count: usize,
    #[serde(default)]
    pub moderate_segment_count: usize,
    #[serde(default)]
    pub mild_segment_count: usize,
    #[serde(default)]
    pub risk_types: Vec<String>,
    #[serde(default)]
    pub segments: Vec<ExternalAigcReportSegment>,
    #[serde(default)]
    pub analysis_summary: Option<String>,
    #[serde(default)]
    pub rewrite_guidance: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAigcReportComparison {
    #[serde(default)]
    pub before_total_ratio: Option<f32>,
    #[serde(default)]
    pub after_total_ratio: Option<f32>,
    #[serde(default)]
    pub total_ratio_delta: Option<f32>,
    #[serde(default)]
    pub before_body_segment_count: usize,
    #[serde(default)]
    pub after_body_segment_count: usize,
    #[serde(default)]
    pub removed_body_segment_count: usize,
    #[serde(default)]
    pub persistent_body_segment_count: usize,
    #[serde(default)]
    pub added_body_segment_count: usize,
    #[serde(default)]
    pub removed_risk_types: Vec<String>,
    #[serde(default)]
    pub persistent_risk_types: Vec<String>,
    #[serde(default)]
    pub added_risk_types: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AigcFeedbackRecord {
    pub id: String,
    pub measured_aigc: f32,
    #[serde(default)]
    pub plagiarism_rate: Option<f32>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub original_snapshot_id: Option<String>,
    #[serde(default)]
    pub rewritten_snapshot_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub strategy: Option<String>,
    #[serde(default)]
    pub round: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub original_app_estimated_aigc: Option<f32>,
    #[serde(default)]
    pub rewritten_app_estimated_aigc: Option<f32>,
    #[serde(default)]
    pub app_estimated_aigc: Option<f32>,
    #[serde(default)]
    pub estimation_error: Option<f32>,
    #[serde(default)]
    pub estimated_drop: Option<f32>,
    #[serde(default)]
    pub metrics_delta: Option<AigcMetricsDelta>,
    #[serde(default)]
    pub ai_review: Option<String>,
    #[serde(default)]
    pub external_report: Option<ExternalAigcReportEvidence>,
    #[serde(default)]
    pub before_external_report: Option<ExternalAigcReportEvidence>,
    #[serde(default)]
    pub after_external_report: Option<ExternalAigcReportEvidence>,
    #[serde(default)]
    pub report_comparison: Option<ExternalAigcReportComparison>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AigcCalibrationRule {
    pub id: String,
    pub pattern: String,
    pub correction: f32,
    pub recommended_strategy: String,
    pub confidence: f32,
    pub sample_count: usize,
    pub summary: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AigcCalibrationInput {
    pub file_path: String,
    pub measured_aigc: f32,
    #[serde(default)]
    pub plagiarism_rate: Option<f32>,
    #[serde(default)]
    pub strategy: Option<String>,
    #[serde(default)]
    pub round: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiConfig {
    pub api_base: String,
    pub api_key: String,
    pub model: String,
    pub language: String,
    #[serde(default = "default_prompt_profile")]
    pub prompt_profile: String,
    #[serde(default)]
    pub detect_api_base: String,
    #[serde(default)]
    pub detect_api_key: String,
    #[serde(default)]
    pub detect_model: String,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            api_base: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: "gpt-5.5".to_string(),
            language: "zh".to_string(),
            prompt_profile: default_prompt_profile(),
            detect_api_base: String::new(),
            detect_api_key: String::new(),
            detect_model: String::new(),
        }
    }
}

fn default_prompt_profile() -> String {
    "sample_calibrated_17_v2".to_string()
}

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
    pub current_ai_rate: Option<f32>,
    #[serde(default)]
    pub target_ai_rate: Option<f32>,
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
    pub ai_assessment: Option<AiAigcAssessment>,
    #[serde(default)]
    pub ai_error: Option<String>,
    #[serde(default)]
    pub detection_mode: String,
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

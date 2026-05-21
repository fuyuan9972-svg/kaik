use crate::models::{
    AigcAnalysis, AigcCalibrationInput, AigcCalibrationRule, AigcCalibrationSample,
    AigcDetectionSnapshot, AigcFeedbackInput, AigcFeedbackRecord, ApiConfig, Paragraph,
    RewriteOptions, RewriteProgress, RewriteResult, RewriteScopeStats, RewriteSession,
    TrialEvaluation, TrialEvaluationInput,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::{collections::BTreeMap, path::Path};
use tauri::{AppHandle, Emitter, State};

#[derive(Clone, Default)]
pub struct RewriteCancelState {
    cancelled: Arc<AtomicBool>,
}

impl RewriteCancelState {
    fn reset(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[tauri::command]
pub fn parse_file(file_path: String) -> Result<Vec<Paragraph>, String> {
    crate::parser::parse_file(&file_path).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn estimate_rewrite_scope(
    paragraphs: Vec<Paragraph>,
    options: Option<RewriteOptions>,
) -> Result<RewriteScopeStats, String> {
    Ok(crate::rewriter::estimate_rewrite_scope(
        &paragraphs,
        options,
    ))
}

#[tauri::command]
pub fn select_sample_indices(
    paragraphs: Vec<Paragraph>,
    analysis: Option<AigcAnalysis>,
    limit: usize,
) -> Result<Vec<usize>, String> {
    Ok(crate::rewriter::select_sample_indices(
        &paragraphs,
        analysis.as_ref(),
        limit,
    ))
}

#[tauri::command]
pub async fn rewrite_paragraphs(
    app: AppHandle,
    cancel_state: State<'_, RewriteCancelState>,
    paragraphs: Vec<Paragraph>,
    config: ApiConfig,
    options: Option<RewriteOptions>,
    session: Option<RewriteSession>,
) -> Result<Vec<RewriteResult>, String> {
    cancel_state.reset();
    crate::rewriter::validate_config(&config).map_err(|error| error.to_string())?;
    let client = crate::rewriter::create_client().map_err(|error| error.to_string())?;
    let rewrite_options = options.unwrap_or_default();
    let prompt = crate::rewriter::system_prompt(
        &config.language,
        &config.prompt_profile,
        &rewrite_options,
        rewrite_options.external_report.as_ref(),
    );
    let selected = crate::rewriter::select_paragraphs(paragraphs, Some(rewrite_options.clone()));
    let total = selected.len();
    let mut active_session = session;
    let mut result_map: BTreeMap<usize, RewriteResult> = active_session
        .as_ref()
        .map(|session| {
            session
                .results
                .iter()
                .cloned()
                .map(|result| (result.index, result))
                .collect()
        })
        .unwrap_or_default();

    for (position, paragraph) in selected.into_iter().enumerate() {
        if cancel_state.is_cancelled() {
            break;
        }

        let result = crate::rewriter::rewrite_paragraph(
            &client,
            &config,
            &rewrite_options,
            paragraph,
            &prompt,
        )
        .await;
        let current = position + 1;
        let status = if result.failed {
            format!("第 {current}/{total} 段失败，已保留原文")
        } else {
            format!("已完成第 {current}/{total} 段")
        };

        app.emit(
            "rewrite-progress",
            RewriteProgress {
                current,
                total,
                status,
                result: result.clone(),
            },
        )
        .map_err(|error| error.to_string())?;

        result_map.insert(result.index, result);
        let results: Vec<RewriteResult> = result_map.values().cloned().collect();
        if position % 5 == 4 || current == total {
            if let Some(session) = active_session.as_mut() {
                session.results = results.clone();
                session.updated_at = current_timestamp();
                crate::config::upsert_session(&app, session.clone())
                    .map_err(|error| error.to_string())?;
            } else {
                crate::config::save_results(&app, &results).map_err(|error| error.to_string())?;
            }
        }

        if cancel_state.is_cancelled() {
            break;
        }
    }

    let results: Vec<RewriteResult> = result_map.values().cloned().collect();
    if let Some(mut session) = active_session {
        session.results = results.clone();
        session.updated_at = current_timestamp();
        crate::config::upsert_session(&app, session).map_err(|error| error.to_string())?;
    } else {
        crate::config::save_results(&app, &results).map_err(|error| error.to_string())?;
    }
    Ok(results)
}

#[tauri::command]
pub fn cancel_rewrite(cancel_state: State<'_, RewriteCancelState>) -> Result<(), String> {
    cancel_state.cancel();
    Ok(())
}

#[tauri::command]
pub async fn test_connection(config: ApiConfig) -> Result<bool, String> {
    crate::rewriter::test_connection(config)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn export_docx(results: Vec<RewriteResult>, output_path: String) -> Result<String, String> {
    crate::exporter::export_docx(results, output_path).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn export_session_docx(
    app: AppHandle,
    session_id: String,
    output_path: String,
) -> Result<String, String> {
    let mut sessions = crate::config::load_sessions(&app).map_err(|error| error.to_string())?;
    let session = sessions
        .iter()
        .find(|item| item.id == session_id)
        .cloned()
        .ok_or_else(|| "找不到当前历史记录，无法导出".to_string())?;
    if session.results.is_empty() {
        return Err(
            "当前历史记录没有改写结果，无法导出。请先切换到有结果的历史记录，或重新开始改写。"
                .to_string(),
        );
    }
    let exported = crate::exporter::export_full_docx(
        session.paragraphs.clone(),
        session.results.clone(),
        output_path,
    )
    .map_err(|error| error.to_string())?;

    if let Some(session) = sessions.iter_mut().find(|item| item.id == session_id) {
        session.exported_path = Some(exported.clone());
        session.updated_at = current_timestamp();
    }
    crate::config::save_sessions(&app, &sessions).map_err(|error| error.to_string())?;
    Ok(exported)
}

#[tauri::command]
pub fn save_config(app: AppHandle, config: ApiConfig) -> Result<(), String> {
    crate::config::save_config(&app, &config).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn load_config(app: AppHandle) -> Result<ApiConfig, String> {
    crate::config::load_config(&app).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_results(app: AppHandle, results: Vec<RewriteResult>) -> Result<(), String> {
    crate::config::save_results(&app, &results).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn load_results(app: AppHandle) -> Result<Vec<RewriteResult>, String> {
    crate::config::load_results(&app).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn clear_results(app: AppHandle) -> Result<(), String> {
    crate::config::clear_results(&app).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn load_sessions(app: AppHandle) -> Result<Vec<RewriteSession>, String> {
    crate::config::load_sessions(&app).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_session(
    app: AppHandle,
    session: RewriteSession,
) -> Result<Vec<RewriteSession>, String> {
    crate::config::upsert_session(&app, session).map_err(|error| error.to_string())?;
    crate::config::load_sessions(&app).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn delete_session(app: AppHandle, session_id: String) -> Result<Vec<RewriteSession>, String> {
    crate::config::delete_session(&app, &session_id).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn analyze_aigc_file(app: AppHandle, file_path: String) -> Result<AigcAnalysis, String> {
    let samples = crate::config::load_aigc_calibrations(&app).map_err(|error| error.to_string())?;
    let rules = crate::config::load_calibration_rules(&app).map_err(|error| error.to_string())?;
    let analysis = crate::aigc_detector::analyze_file(&file_path, &samples)
        .map_err(|error| error.to_string())?;
    Ok(crate::aigc_detector::apply_calibration_rules(
        analysis, &rules,
    ))
}

#[tauri::command]
pub async fn analyze_aigc_file_ai(
    app: AppHandle,
    file_path: String,
    config: ApiConfig,
) -> Result<AigcAnalysis, String> {
    let samples = crate::config::load_aigc_calibrations(&app).map_err(|error| error.to_string())?;
    let rules = crate::config::load_calibration_rules(&app).map_err(|error| error.to_string())?;
    let feedback_records =
        crate::config::load_feedback_records(&app).map_err(|error| error.to_string())?;
    let config = detection_config(config);
    let client = crate::rewriter::create_client().map_err(|error| error.to_string())?;
    let analysis = crate::ai_aigc_detector::analyze_file_with_fallback(
        &client,
        &config,
        &file_path,
        &samples,
        &feedback_records,
    )
    .await
    .map_err(|error| error.to_string())?;
    let analysis = crate::aigc_detector::apply_calibration_rules(analysis, &rules);
    let snapshot = crate::calibration::create_snapshot(&file_path, analysis.clone());
    crate::config::upsert_detection_snapshot(&app, snapshot).map_err(|error| error.to_string())?;
    Ok(analysis)
}

#[tauri::command]
pub async fn analyze_aigc_paragraphs_ai(
    app: AppHandle,
    file_name: String,
    paragraphs: Vec<Paragraph>,
    config: ApiConfig,
) -> Result<AigcAnalysis, String> {
    let samples = crate::config::load_aigc_calibrations(&app).map_err(|error| error.to_string())?;
    let rules = crate::config::load_calibration_rules(&app).map_err(|error| error.to_string())?;
    let feedback_records =
        crate::config::load_feedback_records(&app).map_err(|error| error.to_string())?;
    let config = detection_config(config);
    let client = crate::rewriter::create_client().map_err(|error| error.to_string())?;
    let local = crate::aigc_detector::analyze_paragraphs(&file_name, &paragraphs, &samples);
    let analysis = crate::ai_aigc_detector::analyze_paragraphs_with_fallback(
        &client,
        &config,
        local,
        &paragraphs,
        &samples,
        &feedback_records,
    )
    .await
    .map_err(|error| error.to_string())?;
    Ok(crate::aigc_detector::apply_calibration_rules(
        analysis, &rules,
    ))
}

#[tauri::command]
pub fn load_aigc_calibrations(app: AppHandle) -> Result<Vec<AigcCalibrationSample>, String> {
    crate::config::load_aigc_calibrations(&app).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_aigc_calibration(
    app: AppHandle,
    input: AigcCalibrationInput,
) -> Result<Vec<AigcCalibrationSample>, String> {
    let paragraphs =
        crate::parser::parse_file(&input.file_path).map_err(|error| error.to_string())?;
    let metrics = crate::aigc_detector::calculate_metrics(&paragraphs);
    let now = current_timestamp();
    let file_name = Path::new(&input.file_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(&input.file_path)
        .to_string();
    let sample = AigcCalibrationSample {
        id: file_name.clone(),
        file_name,
        measured_aigc: input.measured_aigc.clamp(0.0, 100.0),
        plagiarism_rate: input.plagiarism_rate.map(|value| value.clamp(0.0, 100.0)),
        strategy: input.strategy,
        round: input.round,
        note: input.note,
        metrics,
        created_at: now.clone(),
        updated_at: now,
    };
    crate::config::upsert_aigc_calibration(&app, sample).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn delete_aigc_calibration(
    app: AppHandle,
    sample_id: String,
) -> Result<Vec<AigcCalibrationSample>, String> {
    crate::config::delete_aigc_calibration(&app, &sample_id).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn evaluate_trial_rewrite(
    config: ApiConfig,
    input: TrialEvaluationInput,
) -> Result<TrialEvaluation, String> {
    let config = detection_config(config);
    let client = crate::rewriter::create_client().map_err(|error| error.to_string())?;
    crate::trial_evaluator::evaluate_with_fallback(&client, &config, input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn load_detection_snapshots(app: AppHandle) -> Result<Vec<AigcDetectionSnapshot>, String> {
    crate::config::load_detection_snapshots(&app).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn load_feedback_records(app: AppHandle) -> Result<Vec<AigcFeedbackRecord>, String> {
    crate::config::load_feedback_records(&app).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn save_feedback_record(
    app: AppHandle,
    config: ApiConfig,
    input: AigcFeedbackInput,
) -> Result<Vec<AigcFeedbackRecord>, String> {
    let mut record = crate::calibration::create_feedback_record(&app, input)
        .map_err(|error| error.to_string())?;
    let snapshots =
        crate::config::load_detection_snapshots(&app).map_err(|error| error.to_string())?;
    let detect_config = detection_config(config);
    if crate::rewriter::validate_config(&detect_config).is_ok() {
        let client = crate::rewriter::create_client().map_err(|error| error.to_string())?;
        if let Ok(review) = crate::calibration::review_feedback_with_ai(
            &client,
            &detect_config,
            &snapshots,
            &record,
        )
        .await
        {
            record.ai_review = Some(review);
        }
    }
    let records =
        crate::config::upsert_feedback_record(&app, record).map_err(|error| error.to_string())?;
    let rules = crate::calibration::build_rules(&records);
    crate::config::save_calibration_rules(&app, &rules).map_err(|error| error.to_string())?;
    Ok(records)
}

#[tauri::command]
pub fn load_calibration_rules(app: AppHandle) -> Result<Vec<AigcCalibrationRule>, String> {
    crate::config::load_calibration_rules(&app).map_err(|error| error.to_string())
}

fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

fn detection_config(mut config: ApiConfig) -> ApiConfig {
    let detect_api_base = config.detect_api_base.trim();
    let detect_api_key = config.detect_api_key.trim();
    let detect_model = config.detect_model.trim();
    if !detect_api_base.is_empty() {
        config.api_base = detect_api_base.to_string();
    }
    if !detect_api_key.is_empty() {
        config.api_key = detect_api_key.to_string();
    }
    config.model = if !detect_model.is_empty() {
        detect_model.to_string()
    } else {
        "gpt-5.4".to_string()
    };
    config
}

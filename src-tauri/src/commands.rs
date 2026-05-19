use crate::models::{
    AigcAnalysis, AigcCalibrationInput, AigcCalibrationSample, ApiConfig, Paragraph,
    RewriteOptions, RewriteProgress, RewriteResult, RewriteScopeStats, RewriteSession,
};
use std::path::Path;
use tauri::{AppHandle, Emitter};

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
pub async fn rewrite_paragraphs(
    app: AppHandle,
    paragraphs: Vec<Paragraph>,
    config: ApiConfig,
    options: Option<RewriteOptions>,
    session: Option<RewriteSession>,
) -> Result<Vec<RewriteResult>, String> {
    crate::rewriter::validate_config(&config).map_err(|error| error.to_string())?;
    let client = crate::rewriter::create_client().map_err(|error| error.to_string())?;
    let rewrite_options = options.unwrap_or_default();
    let selected = crate::rewriter::select_paragraphs(paragraphs, Some(rewrite_options.clone()));
    let total = selected.len();
    let mut active_session = session;
    let mut results = active_session
        .as_ref()
        .map(|session| session.results.clone())
        .unwrap_or_else(|| Vec::with_capacity(total));

    for (position, paragraph) in selected.into_iter().enumerate() {
        let result =
            crate::rewriter::rewrite_paragraph(&client, &config, &rewrite_options, paragraph).await;
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

        if let Some(index) = results.iter().position(|item| item.index == result.index) {
            results[index] = result;
        } else {
            results.push(result);
        }
        results.sort_by_key(|item| item.index);
        if let Some(session) = active_session.as_mut() {
            session.results = results.clone();
            session.updated_at = current_timestamp();
            crate::config::upsert_session(&app, session.clone())
                .map_err(|error| error.to_string())?;
        } else {
            crate::config::save_results(&app, &results).map_err(|error| error.to_string())?;
        }
    }

    if let Some(session) = active_session {
        crate::config::upsert_session(&app, session).map_err(|error| error.to_string())?;
    } else {
        crate::config::save_results(&app, &results).map_err(|error| error.to_string())?;
    }
    Ok(results)
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
    crate::aigc_detector::analyze_file(&file_path, &samples).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn analyze_aigc_file_ai(
    app: AppHandle,
    file_path: String,
    config: ApiConfig,
) -> Result<AigcAnalysis, String> {
    let samples = crate::config::load_aigc_calibrations(&app).map_err(|error| error.to_string())?;
    let config = detection_config(config);
    let client = crate::rewriter::create_client().map_err(|error| error.to_string())?;
    crate::ai_aigc_detector::analyze_file_with_fallback(&client, &config, &file_path, &samples)
        .await
        .map_err(|error| error.to_string())
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
    if !detect_model.is_empty() {
        config.model = detect_model.to_string();
    }
    config
}

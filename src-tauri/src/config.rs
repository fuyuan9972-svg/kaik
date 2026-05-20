use crate::models::{
    AigcCalibrationRule, AigcCalibrationSample, AigcDetectionSnapshot, AigcFeedbackRecord,
    ApiConfig, RewriteResult, RewriteSession,
};
use anyhow::Context;
use std::{fs, path::PathBuf};
use tauri::{AppHandle, Manager};

fn app_dir(app: &AppHandle) -> anyhow::Result<PathBuf> {
    let dir = app
        .path()
        .app_config_dir()
        .context("unable to resolve app config directory")?;
    fs::create_dir_all(&dir).with_context(|| format!("unable to create {}", dir.display()))?;
    Ok(dir)
}

pub fn load_config(app: &AppHandle) -> anyhow::Result<ApiConfig> {
    let path = app_dir(app)?.join("config.json");
    if !path.exists() {
        return Ok(ApiConfig::default());
    }
    let data =
        fs::read_to_string(&path).with_context(|| format!("unable to read {}", path.display()))?;
    let config = serde_json::from_str(&data).context("unable to parse config.json")?;
    Ok(config)
}

pub fn save_config(app: &AppHandle, config: &ApiConfig) -> anyhow::Result<()> {
    let path = app_dir(app)?.join("config.json");
    let data = serde_json::to_string_pretty(config).context("unable to serialize config")?;
    fs::write(&path, data).with_context(|| format!("unable to write {}", path.display()))?;
    Ok(())
}

pub fn load_results(app: &AppHandle) -> anyhow::Result<Vec<RewriteResult>> {
    let path = app_dir(app)?.join("rewrite-results.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data =
        fs::read_to_string(&path).with_context(|| format!("unable to read {}", path.display()))?;
    let results = serde_json::from_str(&data).context("unable to parse rewrite-results.json")?;
    Ok(results)
}

pub fn save_results(app: &AppHandle, results: &[RewriteResult]) -> anyhow::Result<()> {
    let path = app_dir(app)?.join("rewrite-results.json");
    let data =
        serde_json::to_string_pretty(results).context("unable to serialize rewrite results")?;
    fs::write(&path, data).with_context(|| format!("unable to write {}", path.display()))?;
    Ok(())
}

pub fn clear_results(app: &AppHandle) -> anyhow::Result<()> {
    let path = app_dir(app)?.join("rewrite-results.json");
    if path.exists() {
        fs::remove_file(&path).with_context(|| format!("unable to remove {}", path.display()))?;
    }
    Ok(())
}

pub fn load_sessions(app: &AppHandle) -> anyhow::Result<Vec<RewriteSession>> {
    let dir = app_dir(app)?;
    let sessions_path = dir.join("sessions.json");

    if sessions_path.exists() {
        let data = fs::read_to_string(&sessions_path)
            .with_context(|| format!("unable to read {}", sessions_path.display()))?;
        let mut sessions: Vec<RewriteSession> =
            serde_json::from_str(&data).context("unable to parse sessions.json")?;
        sort_sessions(&mut sessions);
        return Ok(sessions);
    }

    let legacy_path = dir.join("rewrite-results.json");
    if legacy_path.exists() {
        let data = fs::read_to_string(&legacy_path)
            .with_context(|| format!("unable to read {}", legacy_path.display()))?;
        let results: Vec<RewriteResult> =
            serde_json::from_str(&data).context("unable to parse legacy rewrite-results.json")?;
        if !results.is_empty() {
            let now = "legacy".to_string();
            return Ok(vec![RewriteSession {
                id: "legacy-rewrite-results".to_string(),
                file_name: "历史改写结果".to_string(),
                file_path: String::new(),
                created_at: now.clone(),
                updated_at: now,
                model: String::new(),
                prompt_profile: String::new(),
                language: String::new(),
                is_sample: false,
                sample_limit: None,
                current_ai_rate: None,
                target_ai_rate: None,
                task_type: None,
                paragraphs: Vec::new(),
                results,
                exported_path: None,
            }]);
        }
    }

    Ok(Vec::new())
}

pub fn save_sessions(app: &AppHandle, sessions: &[RewriteSession]) -> anyhow::Result<()> {
    let path = app_dir(app)?.join("sessions.json");
    let data = serde_json::to_string_pretty(sessions).context("unable to serialize sessions")?;
    fs::write(&path, data).with_context(|| format!("unable to write {}", path.display()))?;
    Ok(())
}

pub fn upsert_session(app: &AppHandle, session: RewriteSession) -> anyhow::Result<()> {
    let mut sessions = load_sessions(app)?;
    if let Some(existing) = sessions.iter_mut().find(|item| item.id == session.id) {
        *existing = session;
    } else {
        sessions.push(session);
    }
    sort_sessions(&mut sessions);
    save_sessions(app, &sessions)
}

pub fn delete_session(app: &AppHandle, session_id: &str) -> anyhow::Result<Vec<RewriteSession>> {
    let mut sessions = load_sessions(app)?;
    sessions.retain(|item| item.id != session_id);
    save_sessions(app, &sessions)?;
    Ok(sessions)
}

pub fn load_aigc_calibrations(app: &AppHandle) -> anyhow::Result<Vec<AigcCalibrationSample>> {
    let path = app_dir(app)?.join("aigc-calibrations.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data =
        fs::read_to_string(&path).with_context(|| format!("unable to read {}", path.display()))?;
    let mut samples: Vec<AigcCalibrationSample> =
        serde_json::from_str(&data).context("unable to parse aigc-calibrations.json")?;
    sort_aigc_calibrations(&mut samples);
    Ok(samples)
}

pub fn load_detection_snapshots(app: &AppHandle) -> anyhow::Result<Vec<AigcDetectionSnapshot>> {
    let path = app_dir(app)?.join("aigc-detection-snapshots.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data =
        fs::read_to_string(&path).with_context(|| format!("unable to read {}", path.display()))?;
    let mut snapshots: Vec<AigcDetectionSnapshot> =
        serde_json::from_str(&data).context("unable to parse aigc-detection-snapshots.json")?;
    snapshots.sort_by(|a, b| timestamp_value(&b.created_at).cmp(&timestamp_value(&a.created_at)));
    Ok(snapshots)
}

pub fn save_detection_snapshots(
    app: &AppHandle,
    snapshots: &[AigcDetectionSnapshot],
) -> anyhow::Result<()> {
    let path = app_dir(app)?.join("aigc-detection-snapshots.json");
    let data = serde_json::to_string_pretty(snapshots)
        .context("unable to serialize detection snapshots")?;
    fs::write(&path, data).with_context(|| format!("unable to write {}", path.display()))?;
    Ok(())
}

pub fn upsert_detection_snapshot(
    app: &AppHandle,
    snapshot: AigcDetectionSnapshot,
) -> anyhow::Result<Vec<AigcDetectionSnapshot>> {
    let mut snapshots = load_detection_snapshots(app)?;
    snapshots.retain(|item| item.id != snapshot.id);
    snapshots.push(snapshot);
    snapshots.sort_by(|a, b| timestamp_value(&b.created_at).cmp(&timestamp_value(&a.created_at)));
    save_detection_snapshots(app, &snapshots)?;
    Ok(snapshots)
}

pub fn load_feedback_records(app: &AppHandle) -> anyhow::Result<Vec<AigcFeedbackRecord>> {
    let path = app_dir(app)?.join("aigc-feedback-records.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data =
        fs::read_to_string(&path).with_context(|| format!("unable to read {}", path.display()))?;
    let mut records: Vec<AigcFeedbackRecord> =
        serde_json::from_str(&data).context("unable to parse aigc-feedback-records.json")?;
    records.sort_by(|a, b| timestamp_value(&b.updated_at).cmp(&timestamp_value(&a.updated_at)));
    Ok(records)
}

pub fn save_feedback_records(
    app: &AppHandle,
    records: &[AigcFeedbackRecord],
) -> anyhow::Result<()> {
    let path = app_dir(app)?.join("aigc-feedback-records.json");
    let data =
        serde_json::to_string_pretty(records).context("unable to serialize feedback records")?;
    fs::write(&path, data).with_context(|| format!("unable to write {}", path.display()))?;
    Ok(())
}

pub fn upsert_feedback_record(
    app: &AppHandle,
    record: AigcFeedbackRecord,
) -> anyhow::Result<Vec<AigcFeedbackRecord>> {
    let mut records = load_feedback_records(app)?;
    records.retain(|item| item.id != record.id);
    records.push(record);
    records.sort_by(|a, b| timestamp_value(&b.updated_at).cmp(&timestamp_value(&a.updated_at)));
    save_feedback_records(app, &records)?;
    Ok(records)
}

pub fn load_calibration_rules(app: &AppHandle) -> anyhow::Result<Vec<AigcCalibrationRule>> {
    let path = app_dir(app)?.join("aigc-calibration-rules.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data =
        fs::read_to_string(&path).with_context(|| format!("unable to read {}", path.display()))?;
    let rules: Vec<AigcCalibrationRule> =
        serde_json::from_str(&data).context("unable to parse aigc-calibration-rules.json")?;
    Ok(rules)
}

pub fn save_calibration_rules(
    app: &AppHandle,
    rules: &[AigcCalibrationRule],
) -> anyhow::Result<()> {
    let path = app_dir(app)?.join("aigc-calibration-rules.json");
    let data =
        serde_json::to_string_pretty(rules).context("unable to serialize calibration rules")?;
    fs::write(&path, data).with_context(|| format!("unable to write {}", path.display()))?;
    Ok(())
}

pub fn save_aigc_calibrations(
    app: &AppHandle,
    samples: &[AigcCalibrationSample],
) -> anyhow::Result<()> {
    let path = app_dir(app)?.join("aigc-calibrations.json");
    let data =
        serde_json::to_string_pretty(samples).context("unable to serialize aigc calibrations")?;
    fs::write(&path, data).with_context(|| format!("unable to write {}", path.display()))?;
    Ok(())
}

pub fn upsert_aigc_calibration(
    app: &AppHandle,
    sample: AigcCalibrationSample,
) -> anyhow::Result<Vec<AigcCalibrationSample>> {
    let mut samples = load_aigc_calibrations(app)?;
    if let Some(existing) = samples
        .iter_mut()
        .find(|item| item.id == sample.id || item.file_name == sample.file_name)
    {
        let created_at = existing.created_at.clone();
        *existing = sample;
        existing.created_at = created_at;
    } else {
        samples.push(sample);
    }
    sort_aigc_calibrations(&mut samples);
    save_aigc_calibrations(app, &samples)?;
    Ok(samples)
}

pub fn delete_aigc_calibration(
    app: &AppHandle,
    sample_id: &str,
) -> anyhow::Result<Vec<AigcCalibrationSample>> {
    let mut samples = load_aigc_calibrations(app)?;
    samples.retain(|item| item.id != sample_id);
    save_aigc_calibrations(app, &samples)?;
    Ok(samples)
}

fn sort_sessions(sessions: &mut [RewriteSession]) {
    sessions.sort_by(|a, b| timestamp_value(&b.updated_at).cmp(&timestamp_value(&a.updated_at)));
}

fn sort_aigc_calibrations(samples: &mut [AigcCalibrationSample]) {
    samples.sort_by(|a, b| timestamp_value(&b.updated_at).cmp(&timestamp_value(&a.updated_at)));
}

fn timestamp_value(value: &str) -> u128 {
    if let Ok(number) = value.parse::<u128>() {
        return number;
    }

    // Handles the ISO strings generated by the frontend well enough for ordering.
    let digits: String = value.chars().filter(|ch| ch.is_ascii_digit()).collect();
    digits.parse::<u128>().unwrap_or(0)
}

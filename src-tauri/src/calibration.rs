use crate::models::{
    AigcCalibrationRule, AigcDetectionSnapshot, AigcFeedbackInput, AigcFeedbackRecord, AigcMetrics,
    AigcMetricsDelta, ApiConfig,
};
use anyhow::Context;
use reqwest::Client;
use std::path::Path;
use tauri::AppHandle;

const WEIPU_GUIDANCE: &str = "维普报告反推规律：命中点不只是个别套话，而是“完整包装段”。高风险结构包括：摘要里一段同时写背景、理论、方法、数据、问题、建议和意义；文献综述后用“综上所述/现有研究不足/基于此”直接推出本研究；理论定义段写成“概念定义+作用意义+应用场景”的完整说明；策略段写成“一是/二是/三是”或“第一阶段/第二阶段/第三阶段”的整齐清单；结尾用“构建完整方案/提供数据支持/推动转型/形成良性循环”收束。降 AI 时不要只换词，应拆散这些完整闭环，保留事实和数据，但把总结式、方案式、阶段推进式表达改得更分散、更像学生自己解释。";

pub fn create_snapshot(
    file_path: &str,
    analysis: crate::models::AigcAnalysis,
) -> AigcDetectionSnapshot {
    let now = current_timestamp();
    let file_name = file_name(file_path);
    AigcDetectionSnapshot {
        id: format!("snapshot-{now}-{}", stable_suffix(&file_name)),
        file_name,
        file_path: file_path.to_string(),
        created_at: now,
        analysis,
    }
}

pub fn create_feedback_record(
    app: &AppHandle,
    input: AigcFeedbackInput,
) -> anyhow::Result<AigcFeedbackRecord> {
    let snapshots = crate::config::load_detection_snapshots(app)?;
    let rewritten = input
        .rewritten_snapshot_id
        .as_ref()
        .and_then(|id| snapshots.iter().find(|snapshot| &snapshot.id == id));
    let original = input
        .original_snapshot_id
        .as_ref()
        .and_then(|id| snapshots.iter().find(|snapshot| &snapshot.id == id));
    let original_estimate = original.map(|snapshot| snapshot.analysis.estimated_aigc);
    let rewritten_estimate = rewritten.map(|snapshot| snapshot.analysis.estimated_aigc);
    let measured = input.measured_aigc.clamp(0.0, 100.0);
    let now = current_timestamp();
    let provider = normalized_provider(input.provider.as_deref(), input.external_report.as_ref());
    let estimated_drop = match (original_estimate, rewritten_estimate) {
        (Some(original), Some(rewritten)) => Some(round2(original - rewritten)),
        _ => None,
    };
    let metrics_delta = match (original, rewritten) {
        (Some(original), Some(rewritten)) => Some(metrics_delta(
            &original.analysis.metrics,
            &rewritten.analysis.metrics,
        )),
        _ => None,
    };
    let mut record = AigcFeedbackRecord {
        id: format!("feedback-{now}"),
        measured_aigc: measured,
        plagiarism_rate: input.plagiarism_rate.map(|value| value.clamp(0.0, 100.0)),
        provider,
        original_snapshot_id: input.original_snapshot_id,
        rewritten_snapshot_id: input.rewritten_snapshot_id,
        session_id: input.session_id,
        strategy: input.strategy,
        round: input.round,
        note: input.note,
        original_app_estimated_aigc: original_estimate,
        rewritten_app_estimated_aigc: rewritten_estimate,
        app_estimated_aigc: rewritten_estimate,
        estimation_error: rewritten_estimate.map(|value| round2(value - measured)),
        estimated_drop,
        metrics_delta,
        ai_review: None,
        external_report: input.external_report,
        created_at: now.clone(),
        updated_at: now,
    };
    record.ai_review = Some(default_feedback_review(&record));
    Ok(record)
}

pub async fn review_feedback_with_ai(
    client: &Client,
    config: &ApiConfig,
    snapshots: &[AigcDetectionSnapshot],
    record: &AigcFeedbackRecord,
) -> anyhow::Result<String> {
    crate::rewriter::validate_config(config)?;
    let original = record
        .original_snapshot_id
        .as_ref()
        .and_then(|id| snapshots.iter().find(|snapshot| &snapshot.id == id));
    let rewritten = record
        .rewritten_snapshot_id
        .as_ref()
        .and_then(|id| snapshots.iter().find(|snapshot| &snapshot.id == id));
    let payload = serde_json::to_string(&serde_json::json!({
        "original": original.map(snapshot_payload),
        "rewritten": rewritten.map(snapshot_payload),
        "measuredPaperPassAigc": record.measured_aigc,
        "plagiarismRate": record.plagiarism_rate,
        "provider": record.provider,
        "strategy": record.strategy,
        "round": record.round,
        "appEstimationError": record.estimation_error,
        "estimatedDrop": record.estimated_drop,
        "metricsDelta": record.metrics_delta,
        "externalReport": record.external_report,
    }))
    .context("unable to serialize feedback review payload")?;
    let output = crate::rewriter::complete_once(
        client,
        config,
        feedback_review_prompt(),
        &payload,
        0.1,
        1200,
    )
    .await?;
    let cleaned = output.trim().trim_matches('`').trim().to_string();
    if cleaned.is_empty() {
        anyhow::bail!("empty feedback review")
    }
    Ok(cleaned)
}

fn snapshot_payload(snapshot: &AigcDetectionSnapshot) -> serde_json::Value {
    serde_json::json!({
        "fileName": snapshot.file_name,
        "estimatedAigc": snapshot.analysis.estimated_aigc,
        "uncalibratedEstimatedAigc": snapshot.analysis.uncalibrated_estimated_aigc,
        "rangeLow": snapshot.analysis.range_low,
        "rangeHigh": snapshot.analysis.range_high,
        "confidence": snapshot.analysis.confidence,
        "profile": snapshot.analysis.profile,
        "riskLevel": snapshot.analysis.risk_level,
        "summary": snapshot.analysis.summary,
        "metrics": snapshot.analysis.metrics,
    })
}

fn feedback_review_prompt() -> &'static str {
    "你是本地 AIGC 检测校准分析器。用户给你一条闭环数据：原稿混合AI检测、改写稿混合AI检测、外部检测实测率，以及两文指标差异。外部来源可能是 PaperPass，也可能是维普等报告；如果 payload 含 externalReport，请重点分析它命中的疑似片段和风险类型。你的任务是反推这条数据对以后检测和降 AIGC 改写有什么校准意义，不要改写文本。\n\n请用中文输出 2-4 句，必须包含：\n1. App 对外部检测是高估还是低估，误差多少。\n2. 这条样本说明哪些指标变化或报告命中类型有效/无效。\n3. 后续类似文本检测应该偏上修、下修，还是保持谨慎；降 AI 时应优先避开什么写法。\n\n不要 Markdown，不要列表，不要编造官方规则。"
}

pub fn external_report_guidance(records: &[AigcFeedbackRecord]) -> String {
    let reports: Vec<&AigcFeedbackRecord> = records
        .iter()
        .filter(|record| record.external_report.is_some())
        .collect();
    if reports.is_empty() {
        return String::new();
    }

    let report_count = reports.len();
    let segment_count = reports
        .iter()
        .filter_map(|record| record.external_report.as_ref())
        .map(|report| report.suspicious_segment_count)
        .sum::<usize>();
    let marked_count = reports
        .iter()
        .filter_map(|record| record.external_report.as_ref())
        .map(|report| report.marked_span_count)
        .sum::<usize>();
    let mut risk_types = Vec::new();
    for report in reports
        .iter()
        .filter_map(|record| record.external_report.as_ref())
    {
        for risk_type in &report.risk_types {
            if !risk_types.iter().any(|item: &String| item == risk_type) {
                risk_types.push(risk_type.clone());
            }
        }
    }
    let risk_summary = if risk_types.is_empty() {
        "摘要式、策略式、总结式、英文摘要等完整包装段".to_string()
    } else {
        risk_types.join("、")
    };
    let snippets = reports
        .iter()
        .filter_map(|record| record.external_report.as_ref())
        .flat_map(|report| report.segments.iter())
        .take(4)
        .map(|segment| compact_snippet(&segment.text, 90))
        .collect::<Vec<_>>();
    let snippet_summary = if snippets.is_empty() {
        String::new()
    } else {
        format!(" 典型命中片段包括：{}。", snippets.join(" / "))
    };
    let paperpass_reports = reports
        .iter()
        .filter(|record| feedback_provider(record) == "paperpass")
        .count();
    format!(
        "{WEIPU_GUIDANCE}\n本地已记录 {report_count} 条外部报告证据，其中 PaperPass 报告 {paperpass_reports} 条，共 {segment_count} 个疑似片段、{marked_count} 处正文标注；高频风险类型：{risk_summary}。{snippet_summary}"
    )
}

fn metrics_delta(original: &AigcMetrics, rewritten: &AigcMetrics) -> AigcMetricsDelta {
    AigcMetricsDelta {
        cjk_chars_delta: rewritten.cjk_chars as isize - original.cjk_chars as isize,
        avg_paragraph_len_delta: round1(rewritten.avg_paragraph_len - original.avg_paragraph_len),
        avg_sentence_len_delta: round1(rewritten.avg_sentence_len - original.avg_sentence_len),
        punctuation_per_100_delta: round2(
            rewritten.punctuation_per_100 - original.punctuation_per_100,
        ),
        ai_terms_per_10k_delta: round2(rewritten.ai_terms_per_10k - original.ai_terms_per_10k),
        buffer_terms_per_10k_delta: round2(
            rewritten.buffer_terms_per_10k - original.buffer_terms_per_10k,
        ),
        plain_terms_per_10k_delta: round2(
            rewritten.plain_terms_per_10k - original.plain_terms_per_10k,
        ),
        connectors_per_10k_delta: round2(
            rewritten.connectors_per_10k - original.connectors_per_10k,
        ),
    }
}

pub fn build_rules(records: &[AigcFeedbackRecord]) -> Vec<AigcCalibrationRule> {
    let usable: Vec<&AigcFeedbackRecord> = records
        .iter()
        .filter(|record| is_real_estimation_feedback(record))
        .filter(|record| matches!(feedback_provider(record), "paperpass" | "weipu" | "vip"))
        .filter(|record| record.estimation_error.is_some())
        .collect();
    if usable.is_empty() {
        return Vec::new();
    }

    let mut rules = Vec::new();
    let avg_error = usable
        .iter()
        .filter_map(|record| record.estimation_error)
        .sum::<f32>()
        / usable.len() as f32;
    let now = current_timestamp();
    rules.push(AigcCalibrationRule {
        id: "global-error-correction".to_string(),
        pattern: "全局检测误差".to_string(),
        correction: round2(-avg_error),
        recommended_strategy: best_strategy(records),
        confidence: (usable.len() as f32 * 8.0).clamp(20.0, 80.0),
        sample_count: usable.len(),
        summary: if avg_error > 0.0 {
            format!(
                "基于 {} 条 PP/维普真实反馈，当前 App 平均比外部实测估高 {:.2} 个百分点，后续检测会适当下修。",
                usable.len(),
                avg_error
            )
        } else {
            format!(
                "基于 {} 条 PP/维普真实反馈，当前 App 平均比外部实测估低 {:.2} 个百分点，后续检测会适当上修。",
                usable.len(),
                avg_error.abs()
            )
        },
        updated_at: now,
    });

    for bucket in [
        CalibrationBucket {
            id: "bucket-low-aigc",
            pattern: "低AI率桶",
            min: 0.0,
            max: 22.0,
            label: "低AI率",
        },
        CalibrationBucket {
            id: "bucket-mid-aigc",
            pattern: "中AI率桶",
            min: 22.0,
            max: 45.0,
            label: "中AI率",
        },
        CalibrationBucket {
            id: "bucket-high-aigc",
            pattern: "高AI率桶",
            min: 45.0,
            max: 100.01,
            label: "高AI率",
        },
    ] {
        if let Some(rule) = bucket_rule(&usable, bucket) {
            rules.push(rule);
        }
    }

    let successful_report_overestimates: Vec<&AigcFeedbackRecord> = usable
        .iter()
        .copied()
        .filter(|record| record.measured_aigc <= 25.0)
        .filter(|record| record.estimation_error.unwrap_or(0.0) >= 6.0)
        .filter(|record| {
            let report = record.external_report.as_ref();
            let no_high = report
                .and_then(|report| report.high_suspected_ratio)
                .map(|value| value <= 0.1)
                .unwrap_or(false);
            let good_delta = record
                .metrics_delta
                .as_ref()
                .map(|delta| {
                    delta.ai_terms_per_10k_delta <= -3.0
                        && delta.connectors_per_10k_delta <= -8.0
                        && delta.plain_terms_per_10k_delta >= 60.0
                })
                .unwrap_or(false);
            no_high || good_delta
        })
        .collect();
    if !successful_report_overestimates.is_empty() {
        let avg = successful_report_overestimates
            .iter()
            .filter_map(|record| record.estimation_error)
            .sum::<f32>()
            / successful_report_overestimates.len() as f32;
        rules.push(AigcCalibrationRule {
            id: "successful-report-overestimate".to_string(),
            pattern: "低PP成功稿高估修正".to_string(),
            correction: round2(-(avg * 0.55).clamp(3.0, 8.0)),
            recommended_strategy: "成功链路17 2.0".to_string(),
            confidence: (successful_report_overestimates.len() as f32 * 12.0).clamp(28.0, 76.0),
            sample_count: successful_report_overestimates.len(),
            summary: "外部报告显示部分成功稿高疑似片段为0，且AI套话和模板连接词已下降；后续相似改写稿不应因普通缓冲词、朴素表达和适度变长被过度判高。".to_string(),
            updated_at: current_timestamp(),
        });
    }

    let low_ratio_success_reports: Vec<&AigcFeedbackRecord> = usable
        .iter()
        .copied()
        .filter(|record| record.measured_aigc <= 18.0)
        .filter(|record| record.estimation_error.unwrap_or(0.0) >= 4.0)
        .filter(|record| {
            record
                .external_report
                .as_ref()
                .map(|report| {
                    report.total_suspected_ratio.unwrap_or(record.measured_aigc) <= 18.0
                        && report.high_suspected_ratio.unwrap_or(0.0) <= 3.0
                        && report.middle_suspected_ratio.unwrap_or(0.0) <= 8.5
                        && report.suspicious_segment_count <= 14
                })
                .unwrap_or(false)
        })
        .collect();
    if !low_ratio_success_reports.is_empty() {
        let avg = low_ratio_success_reports
            .iter()
            .filter_map(|record| record.estimation_error)
            .sum::<f32>()
            / low_ratio_success_reports.len() as f32;
        rules.push(AigcCalibrationRule {
            id: "low-ratio-success-report".to_string(),
            pattern: "15%低占比成功报告修正".to_string(),
            correction: round2(-(avg * 0.65).clamp(2.5, 7.0)),
            recommended_strategy: "成功链路17 2.0".to_string(),
            confidence: (low_ratio_success_reports.len() as f32 * 16.0).clamp(30.0, 82.0),
            sample_count: low_ratio_success_reports.len(),
            summary: "15%左右成功报告主要来自“测试20段后叠加全文”链路；少量中低疑似解释段并不代表整体失败，只要高疑似占比低、中疑似可控、命中片段少，检测应向低风险区间靠拢。".to_string(),
            updated_at: current_timestamp(),
        });
    }

    rules
}

#[derive(Clone, Copy)]
struct CalibrationBucket {
    id: &'static str,
    pattern: &'static str,
    min: f32,
    max: f32,
    label: &'static str,
}

fn bucket_rule(
    records: &[&AigcFeedbackRecord],
    bucket: CalibrationBucket,
) -> Option<AigcCalibrationRule> {
    let bucket_records = records
        .iter()
        .copied()
        .filter(|record| record.measured_aigc >= bucket.min && record.measured_aigc < bucket.max)
        .collect::<Vec<_>>();
    if bucket_records.is_empty() {
        return None;
    }
    let avg_error = bucket_records
        .iter()
        .filter_map(|record| record.estimation_error)
        .sum::<f32>()
        / bucket_records.len() as f32;
    Some(AigcCalibrationRule {
        id: bucket.id.to_string(),
        pattern: bucket.pattern.to_string(),
        correction: round2(-avg_error),
        recommended_strategy: best_strategy_for_records(&bucket_records),
        confidence: (bucket_records.len() as f32 * 14.0).clamp(24.0, 88.0),
        sample_count: bucket_records.len(),
        summary: if avg_error > 0.0 {
            format!(
                "{}样本中 App 平均估高 {:.2} 个百分点，检测同区间文本时会优先下修。",
                bucket.label, avg_error
            )
        } else {
            format!(
                "{}样本中 App 平均估低 {:.2} 个百分点，检测同区间文本时会优先上修。",
                bucket.label,
                avg_error.abs()
            )
        },
        updated_at: current_timestamp(),
    })
}

fn compact_snippet(text: &str, limit: usize) -> String {
    let compact = text.split_whitespace().collect::<String>();
    let mut output = String::new();
    for (index, ch) in compact.chars().enumerate() {
        if index >= limit {
            output.push('…');
            return output;
        }
        output.push(ch);
    }
    output
}

fn is_real_estimation_feedback(record: &AigcFeedbackRecord) -> bool {
    let note = record.note.as_deref().unwrap_or_default();
    let review = record.ai_review.as_deref().unwrap_or_default();
    !(note.contains("历史PP实测补录")
        || note.contains("历史 PP 实测补录")
        || review.contains("历史补录种子数据"))
}

fn default_feedback_review(record: &AigcFeedbackRecord) -> String {
    if feedback_provider(record) != "paperpass" {
        return default_external_report_review(record);
    }
    let pair_summary = match (
        record.original_app_estimated_aigc,
        record.rewritten_app_estimated_aigc,
        record.estimated_drop,
    ) {
        (Some(original), Some(rewritten), Some(drop)) => format!(
            "原稿 App {:.2}%，改写稿 App {:.2}%，App 预估降低 {:.2} 个百分点。",
            original, rewritten, drop
        ),
        _ => "缺少完整原稿/改写稿快照，暂只能按改写稿估算校准。".to_string(),
    };
    let error_summary = match record.estimation_error {
        Some(error) if error >= 8.0 => {
            format!(
                "App 估算比 PaperPass 高 {:.2} 个百分点，说明当前检测可能高估了这类文本。",
                error
            )
        }
        Some(error) if error <= -8.0 => {
            format!(
                "App 估算比 PaperPass 低 {:.2} 个百分点，说明当前检测可能低估了这类文本。",
                error.abs()
            )
        }
        Some(error) => format!("App 估算和 PaperPass 接近，误差 {:.2} 个百分点。", error),
        None => "缺少改写稿检测快照，已保存实测值，暂不能计算估算误差。".to_string(),
    };
    let report_summary = record
        .external_report
        .as_ref()
        .map(|report| {
            format!(
                " 报告分档：总疑似 {:.2}%，高疑似 {:.2}%，中疑似 {:.2}%，低疑似 {:.2}%，命中 {} 个片段。",
                report.total_suspected_ratio.unwrap_or(record.measured_aigc),
                report.high_suspected_ratio.unwrap_or(0.0),
                report.middle_suspected_ratio.unwrap_or(0.0),
                report.low_suspected_ratio.unwrap_or(0.0),
                report.suspicious_segment_count
            )
        })
        .unwrap_or_default();
    format!("{pair_summary} {error_summary}{report_summary}")
}

fn default_external_report_review(record: &AigcFeedbackRecord) -> String {
    let provider = provider_label(record.provider.as_deref());
    let report = record.external_report.as_ref();
    let segment_summary = report
        .map(|value| {
            format!(
                "报告命中 {} 个疑似片段、{} 处正文标注，重度 {} 个，中度 {} 个，轻度 {} 个。",
                value.suspicious_segment_count,
                value.marked_span_count,
                value.severe_segment_count,
                value.moderate_segment_count,
                value.mild_segment_count
            )
        })
        .unwrap_or_else(|| "缺少报告片段明细。".to_string());
    let guidance = report
        .and_then(|value| value.rewrite_guidance.clone())
        .or_else(|| report.and_then(|value| value.analysis_summary.clone()))
        .unwrap_or_else(|| {
            "后续应优先处理外部报告命中的摘要式、总结式和方案式高风险段。".to_string()
        });
    format!(
        "{provider} 实测 AIGC 为 {:.2}%。{segment_summary} {guidance}",
        record.measured_aigc
    )
}

fn best_strategy(records: &[AigcFeedbackRecord]) -> String {
    records
        .iter()
        .filter(|record| record.measured_aigc <= 20.0)
        .filter_map(|record| record.strategy.clone())
        .next()
        .unwrap_or_else(|| "成功链路17 2.0".to_string())
}

fn best_strategy_for_records(records: &[&AigcFeedbackRecord]) -> String {
    records
        .iter()
        .filter(|record| record.measured_aigc <= 25.0)
        .filter_map(|record| record.strategy.clone())
        .next()
        .unwrap_or_else(|| "成功链路17 2.0".to_string())
}

fn normalized_provider(
    provider: Option<&str>,
    report: Option<&crate::models::ExternalAigcReportEvidence>,
) -> Option<String> {
    let value = provider
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_lowercase())
        .or_else(|| {
            report
                .map(|value| value.provider.trim().to_lowercase())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "paperpass".to_string());
    Some(value)
}

fn feedback_provider(record: &AigcFeedbackRecord) -> &str {
    record.provider.as_deref().unwrap_or("paperpass")
}

fn provider_label(provider: Option<&str>) -> &str {
    match provider.unwrap_or("paperpass") {
        "weipu" | "vip" => "维普",
        "paperpass" => "PaperPass",
        _ => "外部检测",
    }
}

fn file_name(file_path: &str) -> String {
    Path::new(file_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(file_path)
        .to_string()
}

fn stable_suffix(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(10)
        .collect::<String>()
}

fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

fn round2(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

fn round1(value: f32) -> f32 {
    (value * 10.0).round() / 10.0
}

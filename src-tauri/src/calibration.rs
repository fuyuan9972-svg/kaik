use crate::models::{
    AigcCalibrationRule, AigcDetectionSnapshot, AigcFeedbackInput, AigcFeedbackRecord, AigcMetrics,
    AigcMetricsDelta, ApiConfig, ExternalAigcReportComparison, ExternalAigcReportEvidence,
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
    let external_report = input
        .after_external_report
        .clone()
        .or_else(|| input.external_report.clone())
        .or_else(|| input.before_external_report.clone());
    let provider = normalized_provider(input.provider.as_deref(), external_report.as_ref());
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
        external_report,
        before_external_report: input.before_external_report,
        after_external_report: input.after_external_report,
        report_comparison: None,
        created_at: now.clone(),
        updated_at: now,
    };
    record.report_comparison = report_comparison(
        record.before_external_report.as_ref(),
        record.after_external_report.as_ref(),
    );
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
        "beforeExternalReport": record.before_external_report,
        "afterExternalReport": record.after_external_report,
        "reportComparison": record.report_comparison,
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
        .filter(|record| {
            record.external_report.is_some()
                || record.before_external_report.is_some()
                || record.after_external_report.is_some()
        })
        .collect();
    if reports.is_empty() {
        return String::new();
    }

    let report_count = reports.len();
    let segment_count = reports
        .iter()
        .filter_map(|record| primary_report(record))
        .map(|report| report.suspicious_segment_count)
        .sum::<usize>();
    let appendix_like_count = reports
        .iter()
        .filter_map(|record| primary_report(record))
        .map(appendix_like_segment_count)
        .sum::<usize>();
    let body_segment_count = reports
        .iter()
        .filter_map(|record| primary_report(record))
        .map(body_suspicious_segment_count)
        .sum::<usize>();
    let marked_count = reports
        .iter()
        .filter_map(|record| primary_report(record))
        .map(|report| report.marked_span_count)
        .sum::<usize>();
    let mut risk_types = Vec::new();
    for report in reports
        .iter()
        .filter_map(|record| primary_report(record))
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
        .filter_map(|record| primary_report(record))
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
    let appendix_summary = if appendix_like_count > 0 {
        format!(
            " 其中约 {appendix_like_count} 个为问卷/访谈/参考文献/致谢等附录型命中，正文有效命中约 {body_segment_count} 个；低PP报告里这类片段不应按正文失败处理。"
        )
    } else {
        String::new()
    };
    format!(
        "{WEIPU_GUIDANCE}\n本地已记录 {report_count} 条外部报告证据，其中 PaperPass 报告 {paperpass_reports} 条，共 {segment_count} 个疑似片段、{marked_count} 处正文标注；高频风险类型：{risk_summary}。{appendix_summary}{snippet_summary}"
    )
}

fn primary_report(record: &AigcFeedbackRecord) -> Option<&ExternalAigcReportEvidence> {
    record
        .after_external_report
        .as_ref()
        .or(record.external_report.as_ref())
        .or(record.before_external_report.as_ref())
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

    let strong_success_reports: Vec<&AigcFeedbackRecord> = usable
        .iter()
        .copied()
        .filter(|record| record.measured_aigc <= 10.0)
        .filter(|record| record.estimation_error.unwrap_or(0.0) >= 8.0)
        .filter(|record| {
            record
                .external_report
                .as_ref()
                .map(|report| {
                    let total = report.total_suspected_ratio.unwrap_or(record.measured_aigc);
                    let high = report.high_suspected_ratio.unwrap_or(0.0);
                    let middle = report.middle_suspected_ratio.unwrap_or(0.0);
                    let effective_segments = effective_body_segment_count(report);
                    (total <= 10.0
                        && high <= 0.1
                        && middle <= 4.5
                        && report.suspicious_segment_count <= 12)
                        || (total <= 10.0
                            && high <= 2.0
                            && middle <= 5.0
                            && effective_segments <= 6)
                })
                .unwrap_or(false)
        })
        .collect();
    if !strong_success_reports.is_empty() {
        let avg = strong_success_reports
            .iter()
            .filter_map(|record| record.estimation_error)
            .sum::<f32>()
            / strong_success_reports.len() as f32;
        rules.push(AigcCalibrationRule {
            id: "strong-success-report".to_string(),
            pattern: "10%内强成功报告修正".to_string(),
            correction: round2(-(avg * 0.7).clamp(5.0, 10.0)),
            recommended_strategy: "成功链路17 2.0".to_string(),
            confidence: (strong_success_reports.len() as f32 * 18.0).clamp(34.0, 86.0),
            sample_count: strong_success_reports.len(),
            summary: "10%内强成功报告说明，17 2.0 经过测试20段后叠加全文或PP报告定向改写可以进入极低PP区间；当总疑似很低、正文有效命中少，即使少量高疑似来自文献定义/引用综述，也不应误判为整体失败。".to_string(),
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
                    let effective_segments = effective_body_segment_count(report);
                    report.total_suspected_ratio.unwrap_or(record.measured_aigc) <= 18.0
                        && report.high_suspected_ratio.unwrap_or(0.0) <= 3.0
                        && report.middle_suspected_ratio.unwrap_or(0.0) <= 8.5
                        && effective_segments <= 14
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
            summary: "15%左右成功报告主要来自“测试20段后叠加全文”链路；少量中低疑似解释段、问卷/访谈提纲、参考文献或致谢类命中不代表整体失败，只要高疑似占比低、中疑似可控、正文有效命中片段少，检测应向低风险区间靠拢。".to_string(),
            updated_at: current_timestamp(),
        });
    }

    let zero_high_mid_success_reports: Vec<&AigcFeedbackRecord> = usable
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
                        && report.high_suspected_ratio.unwrap_or(0.0) <= 0.1
                        && report.middle_suspected_ratio.unwrap_or(0.0) <= 15.0
                        && effective_body_segment_count(report) <= 12
                })
                .unwrap_or(false)
        })
        .collect();
    if !zero_high_mid_success_reports.is_empty() {
        let avg = zero_high_mid_success_reports
            .iter()
            .filter_map(|record| record.estimation_error)
            .sum::<f32>()
            / zero_high_mid_success_reports.len() as f32;
        rules.push(AigcCalibrationRule {
            id: "zero-high-mid-success-report".to_string(),
            pattern: "零高疑似中疑似边界成功报告".to_string(),
            correction: round2(-(avg * 0.5).clamp(2.0, 5.5)),
            recommended_strategy: "成功链路17 2.0".to_string(),
            confidence: (zero_high_mid_success_reports.len() as f32 * 14.0).clamp(26.0, 74.0),
            sample_count: zero_high_mid_success_reports.len(),
            summary: "零高疑似但中疑似仍偏高的16%左右报告，通常说明17 2.0叠加全文已经成功压掉高风险，只剩文献综述、案例观察、理论定义和数据说明等正文中风险段；后续检测可下修到低风险边界，但改写建议应转为精修命中段而非整篇重跑。".to_string(),
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

fn effective_body_segment_count(report: &crate::models::ExternalAigcReportEvidence) -> usize {
    report.body_suspicious_segment_count.unwrap_or_else(|| {
        report
            .suspicious_segment_count
            .saturating_sub(appendix_like_segment_count(report))
    })
}

fn body_suspicious_segment_count(report: &crate::models::ExternalAigcReportEvidence) -> usize {
    report.body_suspicious_segment_count.unwrap_or_else(|| {
        report
            .suspicious_segment_count
            .saturating_sub(appendix_like_segment_count(report))
    })
}

fn appendix_like_segment_count(report: &crate::models::ExternalAigcReportEvidence) -> usize {
    let explicit = report.appendix_like_segment_count.unwrap_or(0)
        + report.reference_segment_count.unwrap_or(0);
    if explicit > 0 {
        return explicit;
    }
    report
        .segments
        .iter()
        .filter(|segment| {
            matches!(
                segment.segment_kind.as_deref(),
                Some("appendix") | Some("reference")
            ) || looks_appendix_like_segment(&segment.text)
        })
        .count()
}

fn report_comparison(
    before: Option<&ExternalAigcReportEvidence>,
    after: Option<&ExternalAigcReportEvidence>,
) -> Option<ExternalAigcReportComparison> {
    let before = before?;
    let after = after?;
    let before_total = before.total_suspected_ratio.or(before.report_score);
    let after_total = after.total_suspected_ratio.or(after.report_score);
    let before_body = body_segment_signatures(before);
    let after_body = body_segment_signatures(after);
    let removed = before_body
        .iter()
        .filter(|signature| !after_body.contains(*signature))
        .count();
    let persistent = before_body
        .iter()
        .filter(|signature| after_body.contains(*signature))
        .count();
    let added = after_body
        .iter()
        .filter(|signature| !before_body.contains(*signature))
        .count();
    let before_risks = before.risk_types.clone();
    let after_risks = after.risk_types.clone();
    let removed_risk_types = before_risks
        .iter()
        .filter(|risk| !after_risks.contains(*risk))
        .cloned()
        .collect::<Vec<_>>();
    let persistent_risk_types = before_risks
        .iter()
        .filter(|risk| after_risks.contains(*risk))
        .cloned()
        .collect::<Vec<_>>();
    let added_risk_types = after_risks
        .iter()
        .filter(|risk| !before_risks.contains(*risk))
        .cloned()
        .collect::<Vec<_>>();
    let total_delta = match (before_total, after_total) {
        (Some(before), Some(after)) => Some(round2(after - before)),
        _ => None,
    };
    let summary = match total_delta {
        Some(delta) if delta < 0.0 => format!(
            "PP总疑似下降 {:.2} 个百分点；正文命中由 {} 段变为 {} 段，消失 {} 段、残留 {} 段、新增 {} 段。",
            delta.abs(),
            before_body.len(),
            after_body.len(),
            removed,
            persistent,
            added
        ),
        Some(delta) if delta > 0.0 => format!(
            "PP总疑似上升 {:.2} 个百分点；正文命中由 {} 段变为 {} 段，消失 {} 段、残留 {} 段、新增 {} 段。",
            delta,
            before_body.len(),
            after_body.len(),
            removed,
            persistent,
            added
        ),
        Some(_) => format!(
            "PP总疑似基本持平；正文命中由 {} 段变为 {} 段，消失 {} 段、残留 {} 段、新增 {} 段。",
            before_body.len(),
            after_body.len(),
            removed,
            persistent,
            added
        ),
        None => format!(
            "已生成PP报告命中对比；正文命中由 {} 段变为 {} 段，消失 {} 段、残留 {} 段、新增 {} 段。",
            before_body.len(),
            after_body.len(),
            removed,
            persistent,
            added
        ),
    };
    Some(ExternalAigcReportComparison {
        before_total_ratio: before_total,
        after_total_ratio: after_total,
        total_ratio_delta: total_delta,
        before_body_segment_count: before_body.len(),
        after_body_segment_count: after_body.len(),
        removed_body_segment_count: removed,
        persistent_body_segment_count: persistent,
        added_body_segment_count: added,
        removed_risk_types,
        persistent_risk_types,
        added_risk_types,
        summary,
    })
}

fn body_segment_signatures(report: &ExternalAigcReportEvidence) -> Vec<String> {
    report
        .segments
        .iter()
        .filter(|segment| segment.segment_kind.as_deref() == Some("body"))
        .map(|segment| segment_signature(&segment.text))
        .filter(|signature| !signature.is_empty())
        .collect()
}

fn segment_signature(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(ch))
        .take(80)
        .collect()
}

fn looks_appendix_like_segment(text: &str) -> bool {
    let compact = text.split_whitespace().collect::<String>();
    let numbered = compact.matches('.').count()
        + compact.matches('．').count()
        + compact.matches('、').count();
    let question_like = [
        "请简要介绍",
        "是否使用",
        "使用频率如何",
        "哪些体验较差",
        "具体建议",
        "通常咨询",
        "根据您的观察",
        "访谈前",
        "访谈中",
        "这份问卷",
        "填写结果",
        "感谢您参加这次访谈",
        "再次感谢您的参与",
        "本人声明",
        "原创性声明",
        "独创性声明",
        "法律结果由本人承担",
        "版权使用授权书",
    ]
    .iter()
    .any(|term| compact.contains(term));
    let reference_like = compact.contains("[J]")
        || compact.contains("[D]")
        || compact.contains("[N]")
        || compact.contains("旅游纵览")
        || compact.contains("智能城市")
        || compact.contains("西部旅游")
        || compact.contains("燕山大学")
        || compact.contains("吉林大学");
    reference_like
        || question_like
        || (numbered >= 3
            && (compact.contains('您') || compact.contains("访谈") || compact.contains("问卷")))
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
    let comparison_summary = record
        .report_comparison
        .as_ref()
        .map(|comparison| format!(" {}", comparison.summary))
        .unwrap_or_default();
    format!("{pair_summary} {error_summary}{report_summary}{comparison_summary}")
}

fn default_external_report_review(record: &AigcFeedbackRecord) -> String {
    let provider = provider_label(record.provider.as_deref());
    let report = record
        .after_external_report
        .as_ref()
        .or(record.external_report.as_ref())
        .or(record.before_external_report.as_ref());
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
    let comparison_summary = record
        .report_comparison
        .as_ref()
        .map(|comparison| format!(" {}", comparison.summary))
        .unwrap_or_default();
    format!(
        "{provider} 实测 AIGC 为 {:.2}%。{segment_summary} {guidance}",
        record.measured_aigc
    ) + &comparison_summary
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

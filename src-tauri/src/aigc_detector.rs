use crate::models::{
    AiAigcAssessment, AigcAnalysis, AigcCalibrationRule, AigcCalibrationSample, AigcMetrics,
    AigcParagraphRisk, AigcSimilarSample, Paragraph,
};
use crate::utils::{cjk_count, is_body_candidate, truncate_text};
use std::{cmp::Ordering, path::Path};

const AI_TERMS: &[&str] = &[
    "促进",
    "推动",
    "完善",
    "深度融合",
    "具有重要意义",
    "赋能",
    "实践参考",
    "调研发现",
    "旨在",
    "有效路径",
    "显著提升",
    "不断完善",
    "提供参考",
    "优化路径",
    "现状及对策",
    "高质量发展",
    "创新形式",
    "核心载体",
    "重要资源",
    "理论参考",
];

const BUFFER_TERMS: &[&str] = &[
    "比较",
    "一些",
    "还",
    "可以",
    "会",
    "方面",
    "情况",
    "过程中",
    "一定",
    "里面",
    "这样",
    "来看",
    "不够",
    "并不",
    "有的",
    "了",
    "的",
];

const PLAIN_TERMS: &[&str] = &[
    "有一点",
    "更充分一些",
    "不太",
    "不够",
    "比较",
    "一些",
    "还",
    "可以",
    "会",
    "里面",
    "这样",
];

const CONNECTORS: &[&str] = &[
    "首先", "其次", "最后", "此外", "综上", "因此", "同时", "不仅", "而且", "从而", "进而", "通过",
    "基于", "为了",
];

const VIP_REPORT_TERMS: &[&str] = &[
    "本研究基于",
    "本研究主要包括",
    "提出优化建议",
    "提出三项优化",
    "提出优化策略",
    "这些策略",
    "构建了",
    "构建",
    "完整方案",
    "研究验证了",
    "提供了数据支持",
    "提供数据支持",
    "未来可",
    "服务质量",
    "差异化竞争",
    "关键",
    "核心概念",
    "整体感知",
    "综合评价体系",
    "为策略落地提供",
    "三个阶段",
    "分阶段",
    "综合以上分析",
    "第一阶段",
    "第二阶段",
    "第三阶段",
    "良性循环",
    "速赢策略",
    "长效机制",
];

pub fn analyze_file(
    file_path: &str,
    user_samples: &[AigcCalibrationSample],
) -> anyhow::Result<AigcAnalysis> {
    let paragraphs = crate::parser::parse_file(file_path)?;
    Ok(analyze_paragraphs(file_path, &paragraphs, user_samples))
}

pub fn analyze_paragraphs(
    file_path: &str,
    paragraphs: &[Paragraph],
    user_samples: &[AigcCalibrationSample],
) -> AigcAnalysis {
    let metrics = calculate_metrics(paragraphs);
    let samples = calibration_samples(user_samples);
    let similar_samples = rank_similar_samples(&metrics, &samples);
    let estimated_aigc = estimate_aigc(&metrics, &similar_samples);
    let confidence = estimate_confidence(&similar_samples, samples.len());
    let (range_low, range_high) = estimate_range(estimated_aigc, confidence);
    let profile = classify_profile(&metrics, estimated_aigc);
    let risk_level = classify_risk_level(estimated_aigc, &profile);
    let summary = build_summary(&profile, estimated_aigc, &metrics);
    let next_action = build_next_action(&profile, estimated_aigc);
    let file_name = Path::new(file_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(file_path)
        .to_string();

    AigcAnalysis {
        file_name,
        estimated_aigc: round1(estimated_aigc),
        range_low: round1(range_low),
        range_high: round1(range_high),
        confidence: round1(confidence),
        risk_level,
        profile,
        summary,
        next_action,
        metrics,
        similar_samples: similar_samples.into_iter().take(5).collect(),
        paragraph_risks: paragraph_risks(paragraphs),
        local_estimated_aigc: None,
        uncalibrated_estimated_aigc: None,
        calibration_correction: None,
        calibration_summary: None,
        calibration_sample_count: None,
        ai_assessment: None,
        ai_error: None,
        detection_mode: "local".to_string(),
    }
}

pub fn merge_ai_assessment(mut local: AigcAnalysis, ai: AiAigcAssessment) -> AigcAnalysis {
    let local_score = local.estimated_aigc;
    let ai_score = ai.estimated_aigc.clamp(0.0, 100.0);
    let confidence = ai.confidence.clamp(35.0, 95.0);
    let ai_weight = (confidence / 100.0).clamp(0.48, 0.72);
    let estimated = (ai_score * ai_weight + local_score * (1.0 - ai_weight)).clamp(0.0, 100.0);
    let spread = ((local.range_high - local.range_low).abs() * 0.35 + (100.0 - confidence) * 0.08)
        .clamp(4.0, 10.0);

    local.local_estimated_aigc = Some(local_score);
    local.estimated_aigc = round1(estimated);
    local.uncalibrated_estimated_aigc = Some(round1(estimated));
    local.range_low = round1((estimated - spread).max(0.0).min(ai.range_low.max(0.0)));
    local.range_high = round1(
        (estimated + spread)
            .min(100.0)
            .max(ai.range_high.min(100.0)),
    );
    local.confidence = round1(((local.confidence + confidence) / 2.0).clamp(35.0, 94.0));
    local.profile = ai.profile.clone();
    local.risk_level = classify_risk_level(local.estimated_aigc, &local.profile);
    local.summary = format!(
        "AI 混合检测：{} 本地快速检测约 {:.1}%。",
        ai.summary, local_score
    );
    local.next_action = ai.next_action.clone();
    local.ai_assessment = Some(ai);
    local.ai_error = None;
    local.detection_mode = "ai_mixed".to_string();
    local
}

pub fn apply_calibration_rules(
    mut analysis: AigcAnalysis,
    rules: &[AigcCalibrationRule],
) -> AigcAnalysis {
    let correction = blended_correction(&analysis, rules);
    if correction.abs() < 0.01 {
        return analysis;
    }

    let before = analysis.estimated_aigc;
    let bounded_correction = correction.clamp(-18.0, 18.0);
    let after = (before + bounded_correction).clamp(0.0, 100.0);
    let range_low = (analysis.range_low + bounded_correction).clamp(0.0, 100.0);
    let range_high = (analysis.range_high + bounded_correction).clamp(0.0, 100.0);
    let sample_count = rules
        .iter()
        .map(|rule| rule.sample_count)
        .max()
        .unwrap_or(0);
    let summary = calibration_summary(rules, bounded_correction, sample_count);

    analysis.uncalibrated_estimated_aigc = Some(round1(before));
    analysis.calibration_correction = Some(round2(bounded_correction));
    analysis.calibration_summary = Some(summary.clone());
    analysis.calibration_sample_count = Some(sample_count);
    analysis.estimated_aigc = round1(after);
    analysis.range_low = round1(range_low.min(range_high));
    analysis.range_high = round1(range_high.max(range_low));
    analysis.risk_level = classify_risk_level(analysis.estimated_aigc, &analysis.profile);
    analysis.summary = format!(
        "{} 校准库根据 PP 反馈修正 {:.2} 个百分点，当前显示为校准后估算。",
        analysis.summary, bounded_correction
    );
    analysis.next_action = format!("{} {}", analysis.next_action, summary);
    analysis
}

fn blended_correction(analysis: &AigcAnalysis, rules: &[AigcCalibrationRule]) -> f32 {
    let mut weighted = 0.0;
    let mut weight_sum = 0.0;
    for rule in rules {
        if rule.sample_count == 0 {
            continue;
        }
        if is_bucket_rule(rule) && !matches_bucket_rule(analysis, rule) {
            continue;
        }
        if rule.id == "successful-report-overestimate"
            && !matches_successful_report_overestimate_profile(analysis)
        {
            continue;
        }
        if rule.id == "low-ratio-success-report" && !matches_low_ratio_success_profile(analysis) {
            continue;
        }
        if rule.id == "strong-success-report" && !matches_strong_success_profile(analysis) {
            continue;
        }
        let weight = (rule.confidence / 100.0).clamp(0.15, 1.0) * rule.sample_count as f32;
        weighted += rule.correction * weight;
        weight_sum += weight;
    }
    if weight_sum == 0.0 {
        0.0
    } else {
        weighted / weight_sum
    }
}

fn is_bucket_rule(rule: &AigcCalibrationRule) -> bool {
    matches!(
        rule.id.as_str(),
        "bucket-low-aigc" | "bucket-mid-aigc" | "bucket-high-aigc"
    )
}

fn matches_bucket_rule(analysis: &AigcAnalysis, rule: &AigcCalibrationRule) -> bool {
    match rule.id.as_str() {
        "bucket-low-aigc" => analysis.estimated_aigc < 24.0,
        "bucket-mid-aigc" => analysis.estimated_aigc >= 20.0 && analysis.estimated_aigc < 48.0,
        "bucket-high-aigc" => analysis.estimated_aigc >= 42.0,
        _ => false,
    }
}

fn matches_successful_report_overestimate_profile(analysis: &AigcAnalysis) -> bool {
    let metrics = &analysis.metrics;
    analysis.estimated_aigc >= 24.0
        && analysis.estimated_aigc <= 42.0
        && metrics.ai_terms_per_10k <= 4.0
        && metrics.connectors_per_10k <= 32.0
        && metrics.plain_terms_per_10k >= 180.0
        && metrics.avg_sentence_len <= 42.0
        && metrics.avg_paragraph_len <= 210.0
}

fn matches_low_ratio_success_profile(analysis: &AigcAnalysis) -> bool {
    let metrics = &analysis.metrics;
    analysis.estimated_aigc >= 18.0
        && analysis.estimated_aigc <= 36.0
        && metrics.ai_terms_per_10k <= 5.0
        && metrics.connectors_per_10k <= 36.0
        && metrics.plain_terms_per_10k >= 170.0
        && metrics.avg_sentence_len <= 48.0
        && metrics.avg_paragraph_len <= 230.0
}

fn matches_strong_success_profile(analysis: &AigcAnalysis) -> bool {
    let metrics = &analysis.metrics;
    analysis.estimated_aigc >= 18.0
        && analysis.estimated_aigc <= 34.0
        && metrics.ai_terms_per_10k <= 4.5
        && metrics.connectors_per_10k <= 30.0
        && metrics.plain_terms_per_10k >= 170.0
        && metrics.avg_sentence_len <= 44.0
}

fn calibration_summary(
    rules: &[AigcCalibrationRule],
    correction: f32,
    sample_count: usize,
) -> String {
    let direction = if correction > 0.0 { "上修" } else { "下修" };
    let rule_summary = rules
        .first()
        .map(|rule| rule.summary.clone())
        .unwrap_or_else(|| "校准库已参与本次估算。".to_string());
    format!(
        "已按 {} 条外部实测反馈{} {:.2} 个百分点；{}",
        sample_count,
        direction,
        correction.abs(),
        rule_summary
    )
}

pub fn with_ai_error(mut local: AigcAnalysis, error: String) -> AigcAnalysis {
    local.local_estimated_aigc = Some(local.estimated_aigc);
    local.uncalibrated_estimated_aigc = Some(local.estimated_aigc);
    local.ai_error = Some(error);
    local.detection_mode = "ai_mixed_failed".to_string();
    local
}

pub fn calculate_metrics(paragraphs: &[Paragraph]) -> AigcMetrics {
    let body: Vec<&Paragraph> = paragraphs
        .iter()
        .filter(|item| is_body_candidate(item))
        .collect();
    let total_paragraphs = paragraphs.len();
    let body_paragraphs = body.len();
    let mut cjk_chars = 0usize;
    let mut sentence_total = 0usize;
    let mut punctuation = 0usize;
    let mut ai_terms = 0usize;
    let mut buffer_terms = 0usize;
    let mut plain_terms = 0usize;
    let mut connectors = 0usize;
    let mut paragraph_lens = Vec::new();

    for paragraph in body {
        let text = paragraph.text.as_str();
        let cjk = cjk_count(text);
        cjk_chars += cjk;
        paragraph_lens.push(cjk);
        sentence_total += sentence_count(text).max(1);
        punctuation += text
            .chars()
            .filter(|ch| matches!(ch, '。' | '！' | '？' | '；' | ';' | '，' | ','))
            .count();
        ai_terms += count_terms(text, AI_TERMS);
        buffer_terms += count_terms(text, BUFFER_TERMS);
        plain_terms += count_terms(text, PLAIN_TERMS);
        connectors += count_terms(text, CONNECTORS);
    }

    let avg_paragraph_len = if body_paragraphs == 0 {
        0.0
    } else {
        cjk_chars as f32 / body_paragraphs as f32
    };
    let avg_sentence_len = if sentence_total == 0 {
        0.0
    } else {
        cjk_chars as f32 / sentence_total as f32
    };
    let cjk = cjk_chars.max(1) as f32;

    AigcMetrics {
        total_paragraphs,
        body_paragraphs,
        cjk_chars,
        avg_paragraph_len: round1(avg_paragraph_len),
        avg_sentence_len: round1(avg_sentence_len),
        punctuation_per_100: round2(punctuation as f32 / cjk * 100.0),
        ai_terms_per_10k: round2(ai_terms as f32 / cjk * 10000.0),
        buffer_terms_per_10k: round2(buffer_terms as f32 / cjk * 10000.0),
        plain_terms_per_10k: round2(plain_terms as f32 / cjk * 10000.0),
        connectors_per_10k: round2(connectors as f32 / cjk * 10000.0),
    }
}

fn calibration_samples(user_samples: &[AigcCalibrationSample]) -> Vec<CalibrationPoint> {
    let mut samples = builtin_calibration_samples();
    samples.extend(user_samples.iter().map(|sample| CalibrationPoint {
        file_name: sample.file_name.clone(),
        measured_aigc: sample.measured_aigc,
        metrics: sample.metrics.clone(),
    }));
    samples
}

fn builtin_calibration_samples() -> Vec<CalibrationPoint> {
    vec![
        point(
            "历史低风险成功样本A",
            17.0,
            57,
            13664,
            239.7,
            47.9,
            6.76,
            2.20,
            772.83,
            267.13,
            35.13,
        ),
        point(
            "历史低风险成功样本B",
            18.41,
            57,
            13761,
            241.4,
            47.8,
            6.92,
            1.45,
            778.29,
            273.96,
            34.15,
        ),
        point(
            "历史中风险扩写样本A",
            27.8,
            57,
            16806,
            294.8,
            46.6,
            6.72,
            2.98,
            916.34,
            352.26,
            36.89,
        ),
        point(
            "历史中风险扩写样本B",
            27.83,
            57,
            20589,
            361.2,
            57.4,
            6.18,
            1.46,
            955.85,
            369.61,
            32.54,
        ),
        point(
            "历史中风险改写样本A",
            28.8,
            57,
            16227,
            284.7,
            45.8,
            6.64,
            2.47,
            930.55,
            346.34,
            42.52,
        ),
        point(
            "历史中风险扩写样本C",
            30.81,
            57,
            15846,
            278.0,
            53.9,
            6.27,
            3.16,
            906.22,
            347.72,
            32.82,
        ),
        point(
            "历史中风险扩写样本D",
            32.03,
            57,
            17424,
            305.7,
            55.7,
            6.26,
            1.15,
            928.03,
            361.00,
            32.71,
        ),
        point(
            "历史改写不足样本A",
            43.0,
            57,
            14041,
            246.3,
            53.2,
            6.47,
            7.12,
            714.34,
            221.49,
            50.57,
        ),
        point(
            "历史改写不足样本B",
            45.2,
            57,
            12505,
            219.4,
            50.2,
            6.85,
            4.80,
            573.37,
            143.94,
            35.99,
        ),
        point(
            "历史改写不足样本C",
            50.81,
            57,
            12405,
            217.6,
            50.8,
            6.64,
            11.29,
            607.82,
            171.70,
            41.11,
        ),
        point(
            "历史较高风险样本A",
            53.58,
            57,
            12901,
            226.3,
            53.5,
            6.56,
            8.53,
            662.74,
            188.36,
            57.36,
        ),
        point(
            "历史较高风险样本B",
            53.98,
            57,
            11914,
            209.0,
            51.1,
            6.79,
            10.07,
            564.88,
            134.30,
            42.81,
        ),
        point(
            "历史高风险样本A",
            61.0,
            57,
            11155,
            195.7,
            59.7,
            6.26,
            30.48,
            581.80,
            51.10,
            35.86,
        ),
        point(
            "历史高风险样本B",
            61.7,
            57,
            10843,
            190.2,
            52.9,
            6.64,
            33.20,
            367.06,
            54.41,
            33.20,
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn point(
    file_name: &str,
    measured_aigc: f32,
    body_paragraphs: usize,
    cjk_chars: usize,
    avg_paragraph_len: f32,
    avg_sentence_len: f32,
    punctuation_per_100: f32,
    ai_terms_per_10k: f32,
    buffer_terms_per_10k: f32,
    plain_terms_per_10k: f32,
    connectors_per_10k: f32,
) -> CalibrationPoint {
    CalibrationPoint {
        file_name: file_name.to_string(),
        measured_aigc,
        metrics: AigcMetrics {
            total_paragraphs: 263,
            body_paragraphs,
            cjk_chars,
            avg_paragraph_len,
            avg_sentence_len,
            punctuation_per_100,
            ai_terms_per_10k,
            buffer_terms_per_10k,
            plain_terms_per_10k,
            connectors_per_10k,
        },
    }
}

fn rank_similar_samples(
    metrics: &AigcMetrics,
    samples: &[CalibrationPoint],
) -> Vec<AigcSimilarSample> {
    let mut ranked: Vec<AigcSimilarSample> = samples
        .iter()
        .map(|sample| {
            let distance = metrics_distance(metrics, &sample.metrics);
            AigcSimilarSample {
                file_name: sample.file_name.clone(),
                measured_aigc: round1(sample.measured_aigc),
                similarity: round1((1.0 / (1.0 + distance) * 100.0).clamp(0.0, 100.0)),
            }
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.similarity
            .partial_cmp(&a.similarity)
            .unwrap_or(Ordering::Equal)
    });
    ranked
}

fn estimate_aigc(metrics: &AigcMetrics, similar_samples: &[AigcSimilarSample]) -> f32 {
    let rule_score = rule_score(metrics);
    if let Some(top) = similar_samples.first() {
        if top.similarity >= 99.5 {
            return top.measured_aigc.clamp(5.0, 95.0);
        }
        if top.similarity >= 94.0 {
            return (top.measured_aigc * 0.92 + rule_score * 0.08).clamp(5.0, 95.0);
        }
    }

    let mut weighted = 0.0;
    let mut weight_sum = 0.0;
    for sample in similar_samples.iter().take(5) {
        let weight = (sample.similarity / 100.0).powf(3.0).max(0.01);
        weighted += sample.measured_aigc * weight;
        weight_sum += weight;
    }
    let sample_score = if weight_sum == 0.0 {
        45.0
    } else {
        weighted / weight_sum
    };
    (sample_score * 0.8 + rule_score * 0.2).clamp(5.0, 95.0)
}

fn rule_score(metrics: &AigcMetrics) -> f32 {
    let mut score = 42.0;
    score += (metrics.ai_terms_per_10k - 3.0).max(0.0) * 1.1;
    score -= (metrics.plain_terms_per_10k - 230.0).max(0.0) * 0.045;
    score += (230.0 - metrics.plain_terms_per_10k).max(0.0) * 0.07;
    score -= (metrics.buffer_terms_per_10k - 700.0).max(0.0) * 0.012;
    score += (650.0 - metrics.buffer_terms_per_10k).max(0.0) * 0.02;
    score += (metrics.avg_sentence_len - 50.0).max(0.0) * 0.95;
    score += (metrics.avg_paragraph_len - 225.0).max(0.0) * 0.16;
    score += (185.0 - metrics.avg_paragraph_len).max(0.0) * 0.18;
    score += (metrics.connectors_per_10k - 42.0).max(0.0) * 0.35;
    if metrics.avg_paragraph_len > 235.0 && metrics.buffer_terms_per_10k > 850.0 {
        score += 6.0;
    }
    score.clamp(12.0, 75.0)
}

fn metrics_distance(left: &AigcMetrics, right: &AigcMetrics) -> f32 {
    norm(left.cjk_chars as f32, right.cjk_chars as f32, 4500.0) * 1.0
        + norm(left.avg_paragraph_len, right.avg_paragraph_len, 70.0) * 1.2
        + norm(left.avg_sentence_len, right.avg_sentence_len, 14.0) * 1.0
        + norm(left.punctuation_per_100, right.punctuation_per_100, 1.0) * 0.7
        + norm(left.ai_terms_per_10k, right.ai_terms_per_10k, 16.0) * 1.5
        + norm(left.buffer_terms_per_10k, right.buffer_terms_per_10k, 260.0) * 1.2
        + norm(left.plain_terms_per_10k, right.plain_terms_per_10k, 140.0) * 1.3
        + norm(left.connectors_per_10k, right.connectors_per_10k, 18.0) * 0.7
}

fn norm(left: f32, right: f32, scale: f32) -> f32 {
    ((left - right).abs() / scale).min(3.0)
}

fn estimate_confidence(similar_samples: &[AigcSimilarSample], sample_count: usize) -> f32 {
    let top = similar_samples
        .first()
        .map(|sample| sample.similarity)
        .unwrap_or(0.0);
    let sample_bonus = (sample_count as f32 * 1.5).min(18.0);
    (top * 0.68 + sample_bonus).clamp(35.0, 92.0)
}

fn estimate_range(estimated: f32, confidence: f32) -> (f32, f32) {
    let spread = (16.0 - confidence * 0.11).clamp(5.0, 12.0);
    (
        (estimated - spread).max(0.0),
        (estimated + spread).min(100.0),
    )
}

fn classify_profile(metrics: &AigcMetrics, estimated: f32) -> String {
    if metrics.avg_paragraph_len > 235.0 && metrics.buffer_terms_per_10k > 840.0 {
        "过度扩写稿".to_string()
    } else if estimated <= 23.0
        && metrics.ai_terms_per_10k <= 4.0
        && metrics.plain_terms_per_10k >= 230.0
    {
        "低风险成功区间".to_string()
    } else if metrics.ai_terms_per_10k >= 18.0 || metrics.plain_terms_per_10k < 90.0 {
        "原稿高AI".to_string()
    } else if estimated >= 40.0 {
        "改写不足".to_string()
    } else {
        "中等风险".to_string()
    }
}

fn classify_risk_level(estimated: f32, profile: &str) -> String {
    if profile == "低风险成功区间" || estimated <= 23.0 {
        "低风险".to_string()
    } else if estimated <= 35.0 {
        "中风险".to_string()
    } else if estimated <= 55.0 {
        "较高风险".to_string()
    } else {
        "高风险".to_string()
    }
}

fn build_summary(profile: &str, estimated: f32, metrics: &AigcMetrics) -> String {
    match profile {
        "低风险成功区间" => format!(
            "整体指标接近历史低风险成功区间，AI套话密度较低，朴素表达密度较高，估算约 {:.1}%。",
            estimated
        ),
        "过度扩写稿" => format!(
            "文本比低风险成功区间更长，缓冲表达密度偏高，容易落入过度扩写型中风险，估算约 {:.1}%。",
            estimated
        ),
        "原稿高AI" => format!(
            "AI套话密度偏高或朴素表达不足，整体仍接近原稿高AI状态，估算约 {:.1}%。",
            estimated
        ),
        "改写不足" => format!(
            "已做过一定改写，但AI套话和句式规整度仍偏高，估算约 {:.1}%。",
            estimated
        ),
        _ => format!(
            "指标处在低风险区间和高风险区间之间，平均段长 {:.1} 字、平均句长 {:.1} 字，估算约 {:.1}%。",
            metrics.avg_paragraph_len, metrics.avg_sentence_len, estimated
        ),
    }
}

fn build_next_action(profile: &str, estimated: f32) -> String {
    match profile {
        "低风险成功区间" => {
            "接近成功区间，建议先去 PaperPass 实测，不要继续大幅改写。".to_string()
        }
        "过度扩写稿" => "不要继续整篇扩写，优先回到成功链路基线或做局部回压。".to_string(),
        "原稿高AI" => "仍像原稿高AI状态，可以用成功链路策略跑一轮。".to_string(),
        "改写不足" => "可以再跑一轮成功链路，但注意不要把段落扩得太长。".to_string(),
        _ if estimated <= 35.0 => "可以先抽样送检；如果仍偏高，再只处理高风险段落。".to_string(),
        _ => "建议继续处理高风险段落，再做 PaperPass 实测。".to_string(),
    }
}

fn paragraph_risks(paragraphs: &[Paragraph]) -> Vec<AigcParagraphRisk> {
    let mut risks: Vec<AigcParagraphRisk> = paragraphs
        .iter()
        .filter(|paragraph| is_body_candidate(paragraph))
        .filter_map(|paragraph| {
            let (risk, reasons) = paragraph_risk(paragraph.text.as_str());
            if risk >= 36.0 {
                Some(AigcParagraphRisk {
                    index: paragraph.index,
                    text: truncate_text(&paragraph.text, 220),
                    risk: round1(risk),
                    reasons,
                })
            } else {
                None
            }
        })
        .collect();
    risks.sort_by(|a, b| b.risk.partial_cmp(&a.risk).unwrap_or(Ordering::Equal));
    risks.truncate(20);
    risks
}

fn paragraph_risk(text: &str) -> (f32, Vec<String>) {
    let cjk = cjk_count(text).max(1) as f32;
    let ai = count_terms(text, AI_TERMS) as f32 / cjk * 10000.0;
    let buffer = count_terms(text, BUFFER_TERMS) as f32 / cjk * 10000.0;
    let plain = count_terms(text, PLAIN_TERMS) as f32 / cjk * 10000.0;
    let sentence = cjk / sentence_count(text).max(1) as f32;
    let mut risk: f32 = 18.0;
    let mut reasons = Vec::new();

    if ai > 12.0 {
        risk += 26.0;
        reasons.push("AI套话残留较多".to_string());
    }
    if plain < 120.0 {
        risk += 18.0;
        reasons.push("朴素表达不足".to_string());
    }
    if sentence > 58.0 {
        risk += 16.0;
        reasons.push("句子偏长偏顺".to_string());
    }
    if cjk > 260.0 && buffer > 850.0 {
        risk += 18.0;
        reasons.push("可能过度扩写".to_string());
    }
    if buffer > 1050.0 {
        risk += 10.0;
        reasons.push("缓冲词堆叠偏多".to_string());
    }
    if count_terms(text, CONNECTORS) as f32 / cjk * 10000.0 > 55.0 {
        risk += 8.0;
        reasons.push("连接词偏模板化".to_string());
    }
    let vip_hits = count_terms(text, VIP_REPORT_TERMS);
    let vip_structure = vip_structure_risk(text);
    if vip_hits >= 2 || vip_structure >= 2 {
        risk += (14.0 + vip_structure as f32 * 4.0).min(30.0);
        reasons.push(vip_structure_reason(vip_structure));
    }
    if reasons.is_empty() {
        reasons.push("指标略高于低风险区间".to_string());
    }
    (risk.clamp(0.0, 100.0), reasons)
}

fn vip_structure_risk(text: &str) -> usize {
    let compact: String = text.chars().filter(|ch| !ch.is_whitespace()).collect();
    let cjk = cjk_count(&compact);
    let sentence = sentence_count(&compact).max(1);
    let mut score = 0usize;

    if compact.contains("本研究")
        && (compact.contains("基于") || compact.contains("采用") || compact.contains("通过"))
        && (compact.contains("提出") || compact.contains("构建") || compact.contains("分析"))
    {
        score += 1;
    }

    if cjk > 220
        && compact.contains("数据")
        && compact.contains("问题")
        && (compact.contains("建议") || compact.contains("策略"))
    {
        score += 1;
    }

    let has_list = (compact.contains("一是") && compact.contains("二是"))
        || (compact.contains("第一阶段") && compact.contains("第二阶段"))
        || (compact.contains("首先") && compact.contains("其次"))
        || (compact.contains("三个方面") && compact.contains("方面"));
    let has_package = compact.contains("优化策略")
        || compact.contains("优化建议")
        || compact.contains("服务质量优化")
        || compact.contains("发展历程")
        || compact.contains("理论")
        || compact.contains("综上")
        || compact.contains("结论");
    let has_summary = compact.contains("为")
        && (compact.contains("提供") || compact.contains("推动") || compact.contains("构建"));
    if has_list && has_package {
        score += 2;
    } else if has_list {
        score += 1;
    }
    if has_package && has_summary {
        score += 1;
    }

    if compact.contains("概念")
        && compact.contains("指")
        && (compact.contains("评价") || compact.contains("体系") || compact.contains("维度"))
    {
        score += 1;
    }

    if compact.contains("综上所述")
        || compact.contains("基于此")
        || compact.contains("综合以上分析")
        || compact.contains("最终形成")
    {
        score += 1;
    }

    if cjk > 450 && sentence <= 6 {
        score += 1;
    }

    score
}

fn vip_structure_reason(score: usize) -> String {
    if score >= 4 {
        "高度接近维普命中的完整包装段：摘要/综述/策略/阶段推进信息被压成完整闭环".to_string()
    } else if score >= 2 {
        "接近维普命中的摘要式、方案式或阶段推进结构".to_string()
    } else {
        "含有维普报告命中过的包装式表达".to_string()
    }
}

fn count_terms(text: &str, terms: &[&str]) -> usize {
    terms.iter().map(|term| text.matches(term).count()).sum()
}

fn sentence_count(text: &str) -> usize {
    text.chars()
        .filter(|ch| matches!(ch, '。' | '！' | '？' | '；' | ';'))
        .count()
}

fn round1(value: f32) -> f32 {
    (value * 10.0).round() / 10.0
}

fn round2(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

#[derive(Debug, Clone)]
struct CalibrationPoint {
    file_name: String,
    measured_aigc: f32,
    metrics: AigcMetrics,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Paragraph, ParagraphStyle};

    fn paragraph(index: usize, text: &str) -> Paragraph {
        Paragraph {
            index,
            text: text.to_string(),
            style: ParagraphStyle::default(),
            skip: false,
            skip_reason: None,
        }
    }

    #[test]
    fn flags_template_like_paragraph() {
        let text = "幼儿园体育活动是促进幼儿身心健康发展的重要载体，民间传统游戏具有重要意义，可以推动体育活动不断完善，为后续研究提供实践参考。";
        let (_, reasons) = paragraph_risk(text);
        assert!(reasons.iter().any(|item| item.contains("AI套话")));
    }

    #[test]
    fn estimates_success_like_metrics_lower_than_original_like_metrics() {
        let success = point(
            "success", 18.0, 68, 14200, 209.0, 47.5, 6.8, 1.5, 755.0, 265.0, 33.0,
        );
        let original = point(
            "original", 61.0, 67, 11300, 168.0, 52.5, 6.6, 32.0, 355.0, 53.0, 32.0,
        );
        let success_similar =
            rank_similar_samples(&success.metrics, &builtin_calibration_samples());
        let original_similar =
            rank_similar_samples(&original.metrics, &builtin_calibration_samples());
        assert!(estimate_aigc(&success.metrics, &success_similar) < 25.0);
        assert!(estimate_aigc(&original.metrics, &original_similar) > 50.0);
    }

    #[test]
    fn skips_metadata_in_analysis() {
        let paragraphs = vec![
            paragraph(0, "关键词：民间传统游戏；幼儿园体育活动；现状调查；优化对策"),
            paragraph(1, "幼儿园体育活动原本就是幼儿身心成长过程中经常会接触到的一类活动。民间传统游戏既能让幼儿活动身体，又有比较强的游戏趣味，其中还包含一些文化内容，所以可以作为体育活动材料的补充。"),
        ];
        let metrics = calculate_metrics(&paragraphs);
        assert_eq!(metrics.body_paragraphs, 1);
    }

    #[test]
    fn flags_vip_report_packaged_structure() {
        let text = "综合以上分析，建议企业分三阶段推进服务质量优化。第一阶段聚焦速赢策略，落实政策透明化。第二阶段转向中等难度任务，搭建技术通道并开发培训课程。第三阶段构建长效机制，最终形成服务与效益相互促进的良性循环。";
        let (_, reasons) = paragraph_risk(text);
        assert!(reasons.iter().any(|item| item.contains("维普")));
    }
}

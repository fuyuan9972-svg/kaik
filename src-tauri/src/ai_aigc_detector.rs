use crate::models::{
    AiAigcAssessment, AiAigcParagraphScore, AigcAnalysis, AigcCalibrationSample,
    AigcFeedbackRecord, ApiConfig, Paragraph,
};
use anyhow::{bail, Context};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const SAMPLE_TARGET: usize = 16;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiSampleParagraph {
    index: usize,
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAiAssessment {
    estimated_aigc: Option<f32>,
    range_low: Option<f32>,
    range_high: Option<f32>,
    confidence: Option<f32>,
    profile: Option<String>,
    summary: Option<String>,
    next_action: Option<String>,
    paragraph_scores: Option<Vec<RawParagraphScore>>,
    reasons: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawParagraphScore {
    index: Option<usize>,
    risk: Option<f32>,
    profile: Option<String>,
    reasons: Option<Vec<String>>,
}

pub async fn analyze_file_with_fallback(
    client: &Client,
    config: &ApiConfig,
    file_path: &str,
    user_samples: &[AigcCalibrationSample],
    feedback_records: &[AigcFeedbackRecord],
) -> anyhow::Result<AigcAnalysis> {
    let paragraphs = crate::parser::parse_file(file_path)?;
    let local = crate::aigc_detector::analyze_paragraphs(file_path, &paragraphs, user_samples);
    if let Err(error) = crate::rewriter::validate_config(config) {
        return Ok(crate::aigc_detector::with_ai_error(
            local,
            error.to_string(),
        ));
    }

    match assess_with_ai(
        client,
        config,
        &local,
        &paragraphs,
        user_samples,
        feedback_records,
    )
    .await
    {
        Ok(assessment) => Ok(crate::aigc_detector::merge_ai_assessment(local, assessment)),
        Err(error) => Ok(crate::aigc_detector::with_ai_error(
            local,
            error.to_string(),
        )),
    }
}

pub async fn analyze_paragraphs_with_fallback(
    client: &Client,
    config: &ApiConfig,
    local: AigcAnalysis,
    paragraphs: &[Paragraph],
    user_samples: &[AigcCalibrationSample],
    feedback_records: &[AigcFeedbackRecord],
) -> anyhow::Result<AigcAnalysis> {
    if let Err(error) = crate::rewriter::validate_config(config) {
        return Ok(crate::aigc_detector::with_ai_error(
            local,
            error.to_string(),
        ));
    }

    match assess_with_ai(
        client,
        config,
        &local,
        paragraphs,
        user_samples,
        feedback_records,
    )
    .await
    {
        Ok(assessment) => Ok(crate::aigc_detector::merge_ai_assessment(local, assessment)),
        Err(error) => Ok(crate::aigc_detector::with_ai_error(
            local,
            error.to_string(),
        )),
    }
}

async fn assess_with_ai(
    client: &Client,
    config: &ApiConfig,
    local: &AigcAnalysis,
    paragraphs: &[Paragraph],
    user_samples: &[AigcCalibrationSample],
    feedback_records: &[AigcFeedbackRecord],
) -> anyhow::Result<AiAigcAssessment> {
    let samples = select_samples(paragraphs, local);
    if samples.is_empty() {
        bail!("没有可用于 AI 检测的正文段落");
    }

    let prompt = ai_detection_prompt(local, user_samples, feedback_records);
    let user_content = serde_json::json!({
        "fileName": local.file_name,
        "localEstimate": {
            "estimatedAigc": local.estimated_aigc,
            "rangeLow": local.range_low,
            "rangeHigh": local.range_high,
            "profile": local.profile,
            "riskLevel": local.risk_level,
            "metrics": local.metrics,
            "similarSamples": local.similar_samples,
        },
        "sampleParagraphs": samples,
    })
    .to_string();

    let output =
        crate::rewriter::complete_once(client, config, &prompt, &user_content, 0.15, 3800).await?;
    let raw = parse_ai_json(&output)?;
    Ok(normalize_assessment(raw, paragraphs))
}

fn select_samples(paragraphs: &[Paragraph], local: &AigcAnalysis) -> Vec<AiSampleParagraph> {
    let candidates: Vec<&Paragraph> = paragraphs
        .iter()
        .filter(|paragraph| is_body_candidate(paragraph))
        .collect();
    if candidates.is_empty() {
        return Vec::new();
    }

    let mut indices = BTreeSet::new();
    let len = candidates.len();
    for position in [
        0,
        1,
        2,
        len / 3,
        len / 2,
        len * 2 / 3,
        len.saturating_sub(3),
        len.saturating_sub(2),
        len.saturating_sub(1),
    ] {
        if let Some(paragraph) = candidates.get(position) {
            indices.insert(paragraph.index);
        }
    }

    let mut by_len = candidates.clone();
    by_len.sort_by_key(|paragraph| std::cmp::Reverse(cjk_count(&paragraph.text)));
    for paragraph in by_len.into_iter().take(4) {
        indices.insert(paragraph.index);
    }

    for risk in local.paragraph_risks.iter().take(6) {
        indices.insert(risk.index);
    }

    let stride = (len / 6).max(1);
    for paragraph in candidates.iter().step_by(stride).take(6) {
        indices.insert(paragraph.index);
    }

    indices
        .into_iter()
        .filter_map(|index| paragraphs.iter().find(|paragraph| paragraph.index == index))
        .take(SAMPLE_TARGET + 2)
        .map(|paragraph| AiSampleParagraph {
            index: paragraph.index,
            text: truncate_text(&paragraph.text, 520),
        })
        .collect()
}

fn ai_detection_prompt(
    local: &AigcAnalysis,
    user_samples: &[AigcCalibrationSample],
    feedback_records: &[AigcFeedbackRecord],
) -> String {
    let calibration = calibration_brief(user_samples);
    let external_guidance = crate::calibration::external_report_guidance(feedback_records);
    let external_section = if external_guidance.is_empty() {
        "暂无维普等外部报告片段证据。".to_string()
    } else {
        external_guidance
    };

    format!(
        "你是 AIGC 检测助手。你不是通用 AI 文本检测器，要按用户已经实测过的 PaperPass/维普经验来判断。\n\n本地 PP/维普校准库摘要：\n{calibration}\n\n外部报告片段证据：\n{external_section}\n\n当前本地快速检测：估算 {local_estimate:.1}%，画像 {profile}，风险等级 {risk_level}。\n\n判断重点：\n1. 不要把某一篇论文当模板；只按指标、抽样正文、校准区间和外部报告命中类型判断。\n2. 不要只看词表。句子过顺、过完整、过像标准论文润色稿，可能偏高。\n3. AI套话、万能意义句、模板连接词会提高风险。\n4. 过度扩写、解释腔、缓冲词堆叠会提高风险。\n5. 短句化、生涩、普通但仍学术，有时会显著降低风险。\n6. 中低AI原稿不能按高AI原稿处理；如果文本已经自然，不要因为朴素词少就判很高。\n7. 重点识别维普报告已命中的“完整包装段”：摘要一段塞进背景、理论、方法、数据、问题、建议、意义；文献综述后用“综上所述/基于此”引出研究；理论定义段写成概念定义+作用意义+维度体系；策略段写成一是二是三是或第一阶段第二阶段第三阶段；结尾写成构建方案、提供支持、推动转型、形成良性循环。\n8. 如果抽样段落接近这些结构，要提高段落风险并说明是摘要式总包、理论定义包、策略清单包、阶段推进包还是意义闭环包。\n\n用户会给你 JSON，包含本地指标和抽样正文段。请只返回一个 JSON 对象，不要 Markdown，不要解释。\nJSON 结构必须是：\n{{\n  \"estimatedAigc\": 0-100数字,\n  \"rangeLow\": 0-100数字,\n  \"rangeHigh\": 0-100数字,\n  \"confidence\": 0-100数字,\n  \"profile\": \"高AI模板稿|中低AI原稿|改写不足|过度扩写|低风险成功区间|强成功区间|中等风险\",\n  \"summary\": \"一句中文总结\",\n  \"nextAction\": \"一句中文建议\",\n  \"reasons\": [\"原因1\", \"原因2\"],\n  \"paragraphScores\": [{{\"index\": 段落index, \"risk\": 0-100数字, \"profile\": \"段落画像\", \"reasons\": [\"原因\"]}}]\n}}\n\n如果判断不确定，扩大区间并降低 confidence；不要编造 PaperPass 或维普官方规则。",
        local_estimate = local.estimated_aigc,
        profile = local.profile,
        risk_level = local.risk_level
    )
}

fn calibration_brief(user_samples: &[AigcCalibrationSample]) -> String {
    let mut lines = vec![
        "- 强成功区间（约 0%-15%）：通常短句化更明显，表达偏朴素，套话少，可能略生涩，但没有大量解释腔。".to_string(),
        "- 低风险成功区间（约 15%-22%）：仍保持论文语气，但句子不太顺滑，AI套话低，普通连接和轻微不圆滑表达较多。".to_string(),
        "- 中风险区间（约 22%-35%）：常见于改写有效但扩写偏多，或解释腔、缓冲词密度开始升高的文本。".to_string(),
        "- 改写不足区间（约 35%-55%）：文本已经动过，但仍保留模板词、顺滑长句和标准总结式表达。".to_string(),
        "- 高风险区间（约 55%+）：AI套话和浓缩论文句明显，表达规整，朴素表达不足。".to_string(),
    ];
    let bins = sample_bins(user_samples);
    if !bins.is_empty() {
        lines.push("- 用户新增外部实测分布：".to_string());
        for bin in bins {
            lines.push(bin);
        }
    }
    lines.join("\n")
}

fn sample_bins(user_samples: &[AigcCalibrationSample]) -> Vec<String> {
    let ranges = [
        ("0%-15%", 0.0, 15.0),
        ("15%-22%", 15.0, 22.0),
        ("22%-35%", 22.0, 35.0),
        ("35%-55%", 35.0, 55.0),
        ("55%+", 55.0, 100.0),
    ];
    ranges
        .iter()
        .filter_map(|(label, low, high)| {
            let values: Vec<&AigcCalibrationSample> = user_samples
                .iter()
                .filter(|sample| sample.measured_aigc >= *low && sample.measured_aigc <= *high)
                .collect();
            if values.is_empty() {
                return None;
            }
            let avg = values.iter().map(|sample| sample.measured_aigc).sum::<f32>()
                / values.len() as f32;
            Some(format!(
                "  - {label}：{} 条，PP均值 {:.1}%，平均段长 {:.1}，平均句长 {:.1}，AI套话/万字 {:.1}",
                values.len(),
                avg,
                values
                    .iter()
                    .map(|sample| sample.metrics.avg_paragraph_len)
                    .sum::<f32>()
                    / values.len() as f32,
                values
                    .iter()
                    .map(|sample| sample.metrics.avg_sentence_len)
                    .sum::<f32>()
                    / values.len() as f32,
                values
                    .iter()
                    .map(|sample| sample.metrics.ai_terms_per_10k)
                    .sum::<f32>()
                    / values.len() as f32
            ))
        })
        .collect()
}

fn parse_ai_json(output: &str) -> anyhow::Result<RawAiAssessment> {
    let trimmed = output.trim();
    let json_text = if let Some(start) = trimmed.find('{') {
        let end = trimmed.rfind('}').context("AI 返回内容不是 JSON 对象")?;
        &trimmed[start..=end]
    } else {
        trimmed
    };
    serde_json::from_str(json_text).context("AI 检测返回了非 JSON 内容")
}

fn normalize_assessment(raw: RawAiAssessment, paragraphs: &[Paragraph]) -> AiAigcAssessment {
    let estimated = raw.estimated_aigc.unwrap_or(45.0).clamp(0.0, 100.0);
    let low = raw.range_low.unwrap_or(estimated - 8.0).clamp(0.0, 100.0);
    let high = raw.range_high.unwrap_or(estimated + 8.0).clamp(0.0, 100.0);
    let paragraph_text = paragraphs
        .iter()
        .map(|paragraph| (paragraph.index, truncate_text(&paragraph.text, 180)))
        .collect::<BTreeMap<_, _>>();
    let paragraph_scores = raw
        .paragraph_scores
        .unwrap_or_default()
        .into_iter()
        .filter_map(|score| {
            let index = score.index?;
            Some(AiAigcParagraphScore {
                index,
                risk: score.risk.unwrap_or(45.0).clamp(0.0, 100.0),
                profile: clean_text(score.profile).unwrap_or_else(|| "未分类".to_string()),
                reasons: clean_vec(score.reasons),
                text: paragraph_text.get(&index).cloned().unwrap_or_default(),
            })
        })
        .collect();

    AiAigcAssessment {
        estimated_aigc: estimated,
        range_low: low.min(high),
        range_high: high.max(low),
        confidence: raw.confidence.unwrap_or(65.0).clamp(0.0, 100.0),
        profile: clean_text(raw.profile).unwrap_or_else(|| "AI混合检测".to_string()),
        summary: clean_text(raw.summary).unwrap_or_else(|| "AI 已完成抽样判断。".to_string()),
        next_action: clean_text(raw.next_action)
            .unwrap_or_else(|| "建议结合 PaperPass 实测继续校准。".to_string()),
        paragraph_scores,
        reasons: clean_vec(raw.reasons),
    }
}

fn clean_text(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn clean_vec(value: Option<Vec<String>>) -> Vec<String> {
    value
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| clean_text(Some(item)))
        .take(6)
        .collect()
}

fn is_body_candidate(paragraph: &Paragraph) -> bool {
    if paragraph.skip {
        return false;
    }
    let cjk = cjk_count(&paragraph.text);
    cjk >= 35 && paragraph.text.chars().count() >= 45
}

fn cjk_count(text: &str) -> usize {
    text.chars()
        .filter(|ch| ('\u{4e00}'..='\u{9fff}').contains(ch))
        .count()
}

fn truncate_text(text: &str, limit: usize) -> String {
    let mut output = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= limit {
            output.push('…');
            return output;
        }
        output.push(ch);
    }
    output
}

use crate::models::{
    AiAigcAssessment, AiAigcParagraphScore, AigcAnalysis, AigcCalibrationSample, ApiConfig,
    Paragraph,
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
) -> anyhow::Result<AigcAnalysis> {
    let paragraphs = crate::parser::parse_file(file_path)?;
    let local = crate::aigc_detector::analyze_paragraphs(file_path, &paragraphs, user_samples);
    if let Err(error) = crate::rewriter::validate_config(config) {
        return Ok(crate::aigc_detector::with_ai_error(
            local,
            error.to_string(),
        ));
    }

    match assess_with_ai(client, config, &local, &paragraphs, user_samples).await {
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
) -> anyhow::Result<AiAigcAssessment> {
    let samples = select_samples(paragraphs, local);
    if samples.is_empty() {
        bail!("没有可用于 AI 检测的正文段落");
    }

    let prompt = ai_detection_prompt(local, user_samples);
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

fn ai_detection_prompt(local: &AigcAnalysis, user_samples: &[AigcCalibrationSample]) -> String {
    let mut calibration = vec![
        "513原稿=61.7，高AI模板明显",
        "513成功稿=17.0，适度解释拆句但不太顺",
        "513 fn11=18.41，成功链路基线",
        "513 fn8=30.81，过度扩写后卡在30%左右",
        "史斯颖原稿=26.85，中低AI原稿，不应按高AI处理",
        "史斯颖fn1=9.5，短句化、压短、表达略笨拙，强成功",
        "史斯颖fn2=16.74，基线稿更完整顺滑，仍成功但高于fn1",
    ]
    .join("\n- ");
    if !user_samples.is_empty() {
        let extra = user_samples
            .iter()
            .take(8)
            .map(|sample| {
                format!(
                    "{}={}，{}{}",
                    sample.file_name,
                    sample.measured_aigc,
                    sample
                        .strategy
                        .clone()
                        .unwrap_or_else(|| "本地录入".to_string()),
                    sample
                        .round
                        .as_ref()
                        .map(|round| format!("，{round}"))
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n- ");
        calibration.push_str("\n- ");
        calibration.push_str(&extra);
    }

    format!(
        "你是 PaperPass 风格 AIGC 检测助手。你不是通用 AI 文本检测器，要按用户已经实测过的 PaperPass 经验来判断。\n\n已知校准样本：\n- {calibration}\n\n当前本地快速检测：估算 {local_estimate:.1}%，画像 {profile}，风险等级 {risk_level}。\n\n判断重点：\n1. 不要只看词表。句子过顺、过完整、过像标准论文润色稿，可能偏高。\n2. AI套话、万能意义句、模板连接词会提高风险。\n3. 过度扩写、解释腔、缓冲词堆叠会提高风险。\n4. 短句化、生涩、普通但仍学术，有时会显著降低风险。\n5. 中低AI原稿不能按高AI原稿处理；如果文本已经自然，不要因为朴素词少就判很高。\n\n用户会给你 JSON，包含本地指标和抽样正文段。请只返回一个 JSON 对象，不要 Markdown，不要解释。\nJSON 结构必须是：\n{{\n  \"estimatedAigc\": 0-100数字,\n  \"rangeLow\": 0-100数字,\n  \"rangeHigh\": 0-100数字,\n  \"confidence\": 0-100数字,\n  \"profile\": \"原稿高AI|中低AI原稿|改写不足|过度扩写|接近成功稿|强成功稿|中等风险\",\n  \"summary\": \"一句中文总结\",\n  \"nextAction\": \"一句中文建议\",\n  \"reasons\": [\"原因1\", \"原因2\"],\n  \"paragraphScores\": [{{\"index\": 段落index, \"risk\": 0-100数字, \"profile\": \"段落画像\", \"reasons\": [\"原因\"]}}]\n}}\n\n如果判断不确定，扩大区间并降低 confidence；不要编造 PaperPass 官方规则。",
        local_estimate = local.estimated_aigc,
        profile = local.profile,
        risk_level = local.risk_level
    )
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

use crate::models::{AigcAnalysis, TrialEvaluation, TrialEvaluationInput};
use crate::utils::{cjk_count, extract_json_object, truncate_text};
use anyhow::{bail, Context};
use reqwest::Client;
use serde::Deserialize;
use std::collections::BTreeSet;
use tokio::time::{timeout, Duration};

const TRIAL_EVALUATION_TIMEOUT_SECS: u64 = 25;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTrialEvaluation {
    verdict: Option<String>,
    recommended_action: Option<String>,
    recommended_profile: Option<String>,
    recommended_current_ai_rate: Option<f32>,
    recommended_target_ai_rate: Option<f32>,
    recommended_paragraph_indices: Option<Vec<usize>>,
    summary: Option<String>,
    risks: Option<Vec<String>>,
    confidence: Option<f32>,
}

pub async fn evaluate_with_fallback(
    client: &Client,
    config: &crate::models::ApiConfig,
    input: TrialEvaluationInput,
) -> anyhow::Result<TrialEvaluation> {
    if input.results.is_empty() {
        bail!("没有测试20段结果，无法评估");
    }

    match timeout(
        Duration::from_secs(TRIAL_EVALUATION_TIMEOUT_SECS),
        evaluate_with_ai(client, config, &input),
    )
    .await
    {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Ok(fallback_evaluation(&input, Some(error.to_string()))),
        Err(_) => Ok(fallback_evaluation(
            &input,
            Some(format!(
                "AI试跑评估超过{}秒，已使用本地规则评估",
                TRIAL_EVALUATION_TIMEOUT_SECS
            )),
        )),
    }
}

async fn evaluate_with_ai(
    client: &Client,
    config: &crate::models::ApiConfig,
    input: &TrialEvaluationInput,
) -> anyhow::Result<TrialEvaluation> {
    crate::rewriter::validate_config(config)?;
    let prompt = trial_prompt();
    let payload = build_payload(input)?;
    let output =
        crate::rewriter::complete_once(client, config, prompt, &payload, 0.15, 2600).await?;
    let raw = parse_json(&output)?;
    Ok(normalize(raw, input))
}

fn build_payload(input: &TrialEvaluationInput) -> anyhow::Result<String> {
    let pairs = input
        .results
        .iter()
        .filter(|result| !result.skipped)
        .take(24)
        .map(|result| {
            serde_json::json!({
                "index": result.index,
                "original": truncate_text(&result.original, 360),
                "rewritten": truncate_text(&result.rewritten, 420),
                "originalChars": cjk_count(&result.original),
                "rewrittenChars": cjk_count(&result.rewritten),
                "lengthDelta": cjk_count(&result.rewritten) as isize - cjk_count(&result.original) as isize,
                "failed": result.failed,
                "error": result.error,
            })
        })
        .collect::<Vec<_>>();
    let analysis = input.analysis.as_ref().map(compact_analysis);
    serde_json::to_string(&serde_json::json!({
        "initialAnalysis": analysis,
        "promptProfile": input.prompt_profile,
        "currentAiRate": input.current_ai_rate,
        "targetAiRate": input.target_ai_rate,
        "trialTargetAigc": input.trial_target_aigc,
        "trialPairs": pairs,
    }))
    .context("unable to serialize trial evaluation payload")
}

fn compact_analysis(analysis: &AigcAnalysis) -> serde_json::Value {
    serde_json::json!({
        "estimatedAigc": analysis.estimated_aigc,
        "rangeLow": analysis.range_low,
        "rangeHigh": analysis.range_high,
        "confidence": analysis.confidence,
        "profile": analysis.profile,
        "riskLevel": analysis.risk_level,
        "summary": analysis.summary,
        "nextAction": analysis.next_action,
        "metrics": analysis.metrics,
        "paragraphRisks": analysis.paragraph_risks.iter().take(12).collect::<Vec<_>>(),
    })
}

fn trial_prompt() -> &'static str {
    "你是 PaperPass 风格降 AIGC 试跑评估器，使用检测 API 做决策，不负责改写。用户会给你一篇论文的原稿混合检测结果、试跑目标 AI 率，以及测试20段的原文和改写后文本。\n\n核心流程已经固定：成功链路17 2.0 默认使用 60→10 强度；测试20段后，叠加跑全文仍保持 60→10，不根据试跑检测值自动改 currentAiRate 或 targetAiRate。你的任务只判断是否继续叠加、是否只改高风险段、是否过度或暂停，不要建议改写强度数值。\n\n核心目标：试跑不是追最终目标，而是判断这套策略能否先把原稿混合AI率降低约10-15个百分点。payload 里的 trialTargetAigc 就是本轮试跑评估目标，例如原稿60%，试跑评估目标约45%。payload 里的 targetAiRate 是固定全文目标，只用于说明当前强度，不要改它。\n\n判断重点：\n1. 是否朝 trialTargetAigc 的方向有效降低 AI 味，而不是简单同义替换。\n2. 是否改得太弱，仍保留高 AI 模板句、万能意义句和标准润色腔。\n3. 是否过度扩写、解释腔过重、缓冲词堆叠，导致后续全文可能像模型执行提示词。\n4. 是否过于顺滑、过完整、像标准论文润色稿。\n5. 是否出现规则复述、prompt 泄漏、事实新增、格式污染。\n6. 是否可能提高查重风险：太接近原文是改写不足，新增固定表达过多也有风险。\n7. 最终给出能执行的下一步：全文继续、只改高风险、回压、切换基线/2.0 或暂停等 PP 实测；不要输出 increase_strength 作为默认建议。\n\n只返回 JSON 对象，不要 Markdown，不要解释。结构必须是：\n{\n  \"verdict\": \"合格|偏弱|过度|不建议继续|评估失败\",\n  \"recommendedAction\": \"continue_full|rewrite_risky_only|reduce_expansion|switch_to_baseline|switch_to_v2|stop_and_test_pp\",\n  \"recommendedProfile\": \"sample_calibrated_17_v2|sample_calibrated_17_success\",\n  \"recommendedCurrentAiRate\": null,\n  \"recommendedTargetAiRate\": null,\n  \"recommendedParagraphIndices\": [段落index],\n  \"summary\": \"一句中文结论，必须说明是否接近试跑目标，以及是否建议继续叠加全文\",\n  \"risks\": [\"风险1\", \"风险2\"],\n  \"confidence\": 0-100数字\n}"
}

fn parse_json(output: &str) -> anyhow::Result<RawTrialEvaluation> {
    let json_text = extract_json_object(output)?;
    serde_json::from_str(json_text).context("AI 试跑评估返回了非 JSON 内容")
}

fn normalize(raw: RawTrialEvaluation, input: &TrialEvaluationInput) -> TrialEvaluation {
    let fallback = fallback_indices(input);
    TrialEvaluation {
        verdict: clean(raw.verdict).unwrap_or_else(|| "合格".to_string()),
        recommended_action: clean(raw.recommended_action)
            .unwrap_or_else(|| "continue_full".to_string()),
        recommended_profile: clean(raw.recommended_profile)
            .filter(|value| is_supported_profile(value))
            .unwrap_or_else(|| input.prompt_profile.clone()),
        recommended_current_ai_rate: raw
            .recommended_current_ai_rate
            .map(|value| value.clamp(0.0, 100.0)),
        recommended_target_ai_rate: raw
            .recommended_target_ai_rate
            .map(|value| value.clamp(0.0, 100.0)),
        recommended_paragraph_indices: raw
            .recommended_paragraph_indices
            .filter(|items| !items.is_empty())
            .unwrap_or(fallback),
        summary: clean(raw.summary)
            .unwrap_or_else(|| "试跑结果可继续观察，建议按当前策略推进。".to_string()),
        risks: raw
            .risks
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| clean(Some(item)))
            .take(6)
            .collect(),
        confidence: raw.confidence.unwrap_or(62.0).clamp(20.0, 95.0),
    }
}

fn fallback_evaluation(input: &TrialEvaluationInput, error: Option<String>) -> TrialEvaluation {
    let pairs = input
        .results
        .iter()
        .filter(|result| !result.skipped)
        .collect::<Vec<_>>();
    let avg_delta = if pairs.is_empty() {
        0.0
    } else {
        pairs
            .iter()
            .map(|result| cjk_count(&result.rewritten) as f32 - cjk_count(&result.original) as f32)
            .sum::<f32>()
            / pairs.len() as f32
    };
    let (verdict, action, summary) = if avg_delta > 90.0 {
        (
            "过度",
            "reduce_expansion",
            "试跑段落平均扩写较多，建议回压长度或只改高风险段。",
        )
    } else if avg_delta < 18.0 {
        (
            "偏弱",
            "rewrite_risky_only",
            "试跑段落改动偏弱，建议保持60→10强度，叠加全文时优先处理高风险段。",
        )
    } else {
        (
            "合格",
            "continue_full",
            "试跑改动幅度处在可接受范围，可以继续下一步。",
        )
    };
    let mut risks = Vec::new();
    if let Some(error) = error {
        risks.push(format!("AI评估失败，已使用本地指标兜底：{error}"));
    }
    TrialEvaluation {
        verdict: verdict.to_string(),
        recommended_action: action.to_string(),
        recommended_profile: input.prompt_profile.clone(),
        recommended_current_ai_rate: input.current_ai_rate,
        recommended_target_ai_rate: input.target_ai_rate,
        recommended_paragraph_indices: fallback_indices(input),
        summary: summary.to_string(),
        risks,
        confidence: 45.0,
    }
}

fn fallback_indices(input: &TrialEvaluationInput) -> Vec<usize> {
    let mut indices = BTreeSet::new();
    if let Some(analysis) = &input.analysis {
        for risk in analysis.paragraph_risks.iter().take(40) {
            indices.insert(risk.index);
        }
    }
    if indices.is_empty() {
        for paragraph in input.paragraphs.iter().filter(|item| !item.skip).take(60) {
            indices.insert(paragraph.index);
        }
    }
    indices.into_iter().collect()
}

fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn is_supported_profile(value: &str) -> bool {
    matches!(
        value,
        "sample_calibrated_17_v2" | "sample_calibrated_17_success"
    )
}

use crate::models::{
    AigcAnalysis, ApiConfig, ExternalAigcReportEvidence, Paragraph, RewriteOptions, RewriteResult,
    RewriteScopeStats, RewriteSkipCategory,
};
use anyhow::{bail, Context};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use tokio::time::{sleep, timeout, Duration};

const REQUEST_TIMEOUT_SECS: u64 = 45;
const MAX_API_ATTEMPTS: usize = 4;
const RETRY_DELAYS_SECS: [u64; 3] = [2, 4, 8];

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    output_text: Option<String>,
    #[serde(default)]
    response: Option<String>,
    #[serde(default)]
    result: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    #[serde(default)]
    message: Option<ChatResponseMessage>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
}

pub async fn test_connection(config: ApiConfig) -> anyhow::Result<bool> {
    validate_config(&config)?;
    let client = create_client()?;
    let options = RewriteOptions::default();
    let prompt = system_prompt(&config.language, &config.prompt_profile, &options, None);
    rewrite_once(&client, &config, &prompt, "这是一个连通性测试段落。").await?;
    Ok(true)
}

pub fn create_client() -> anyhow::Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .context("unable to create HTTP client")
}

pub async fn complete_once(
    client: &Client,
    config: &ApiConfig,
    system_prompt: &str,
    user_content: &str,
    temperature: f32,
    max_tokens: u32,
) -> anyhow::Result<String> {
    chat_once(
        client,
        config,
        system_prompt,
        user_content,
        temperature,
        max_tokens,
    )
    .await
}

pub fn select_paragraphs(
    paragraphs: Vec<Paragraph>,
    options: Option<RewriteOptions>,
) -> Vec<Paragraph> {
    let mut used_sample = 0usize;
    let options = options.unwrap_or_default();
    let sample_limit = options.sample_limit;
    let exclude_indices: HashSet<usize> = options.exclude_indices.into_iter().collect();
    let include_indices: HashSet<usize> = options.include_indices.into_iter().collect();

    paragraphs
        .into_iter()
        .filter_map(|paragraph| {
            if !include_indices.is_empty() && !include_indices.contains(&paragraph.index) {
                return None;
            }

            if exclude_indices.contains(&paragraph.index) {
                return None;
            }

            if rewrite_skip_reason(&paragraph).is_some() {
                return None;
            }

            if let Some(limit) = sample_limit {
                if used_sample >= limit {
                    return None;
                }
                used_sample += 1;
            }

            Some(paragraph)
        })
        .collect()
}

pub fn select_sample_indices(
    paragraphs: &[Paragraph],
    analysis: Option<&AigcAnalysis>,
    limit: usize,
) -> Vec<usize> {
    let limit = limit.max(1);
    let body: Vec<&Paragraph> = paragraphs
        .iter()
        .filter(|paragraph| rewrite_skip_reason(paragraph).is_none())
        .collect();
    let mut selected = Vec::<usize>::new();
    let mut seen = HashSet::<usize>::new();

    if let Some(analysis) = analysis {
        for risk in analysis.paragraph_risks.iter().take(limit) {
            add_sample_index(&body, risk.index, &mut selected, &mut seen, limit);
        }
    }

    let mut by_len = body.clone();
    by_len.sort_by_key(|paragraph| std::cmp::Reverse(paragraph.text.chars().count()));
    for paragraph in by_len.into_iter().take(limit / 3 + 3) {
        add_sample_index(&body, paragraph.index, &mut selected, &mut seen, limit);
    }

    if !body.is_empty() {
        let last = body.len().saturating_sub(1);
        let positions = [
            0,
            1,
            2,
            body.len() / 4,
            body.len() / 3,
            body.len() / 2,
            body.len() * 2 / 3,
            body.len() * 3 / 4,
            last.saturating_sub(2),
            last.saturating_sub(1),
            last,
        ];
        for position in positions {
            if let Some(paragraph) = body.get(position) {
                add_sample_index(&body, paragraph.index, &mut selected, &mut seen, limit);
            }
        }
    }

    for paragraph in body {
        add_sample_index(&[], paragraph.index, &mut selected, &mut seen, limit);
        if selected.len() >= limit {
            break;
        }
    }

    selected.sort_unstable();
    selected
}

fn add_sample_index(
    body: &[&Paragraph],
    index: usize,
    selected: &mut Vec<usize>,
    seen: &mut HashSet<usize>,
    limit: usize,
) {
    if selected.len() >= limit || seen.contains(&index) {
        return;
    }
    if !body.is_empty() && !body.iter().any(|paragraph| paragraph.index == index) {
        return;
    }
    seen.insert(index);
    selected.push(index);
}

pub fn estimate_rewrite_scope(
    paragraphs: &[Paragraph],
    options: Option<RewriteOptions>,
) -> RewriteScopeStats {
    let options = options.unwrap_or_default();
    let sample_limit = options.sample_limit;
    let exclude_indices: HashSet<usize> = options.exclude_indices.into_iter().collect();
    let include_indices: HashSet<usize> = options.include_indices.into_iter().collect();
    let mut selected = 0usize;
    let mut skipped = 0usize;
    let mut categories = BTreeMap::<String, usize>::new();

    for paragraph in paragraphs {
        if !include_indices.is_empty() && !include_indices.contains(&paragraph.index) {
            skipped += 1;
            *categories.entry("建议范围外".to_string()).or_insert(0) += 1;
            continue;
        }

        if exclude_indices.contains(&paragraph.index) {
            skipped += 1;
            *categories.entry("已改写段落".to_string()).or_insert(0) += 1;
            continue;
        }

        if let Some(reason) = rewrite_skip_reason(paragraph) {
            skipped += 1;
            *categories.entry(reason).or_insert(0) += 1;
            continue;
        }

        if let Some(limit) = sample_limit {
            if selected >= limit {
                skipped += 1;
                *categories.entry("测试上限外".to_string()).or_insert(0) += 1;
                continue;
            }
        }

        selected += 1;
    }

    RewriteScopeStats {
        total: paragraphs.len(),
        selected,
        skipped,
        categories: categories
            .into_iter()
            .map(|(reason, count)| RewriteSkipCategory { reason, count })
            .collect(),
    }
}

pub async fn rewrite_paragraph(
    client: &Client,
    config: &ApiConfig,
    _options: &RewriteOptions,
    paragraph: Paragraph,
    system_prompt: &str,
) -> RewriteResult {
    if paragraph.skip {
        return RewriteResult {
            index: paragraph.index,
            original: paragraph.text.clone(),
            rewritten: paragraph.text,
            accepted: true,
            skipped: true,
            failed: false,
            error: paragraph.skip_reason,
            style: paragraph.style,
        };
    }

    if config.language == "zh" && is_english_heavy(&paragraph.text) {
        return RewriteResult {
            index: paragraph.index,
            original: paragraph.text.clone(),
            rewritten: paragraph.text,
            accepted: true,
            skipped: true,
            failed: false,
            error: Some("英文段落（当前为中文改写模式）".to_string()),
            style: paragraph.style,
        };
    }

    if config.prompt_profile == "doubao_plain_humanize" {
        return rewrite_doubao_two_pass(client, config, paragraph).await;
    }

    match rewrite_once(client, config, system_prompt, &paragraph.text).await {
        Ok(rewritten) => build_success_result(config, paragraph, rewritten),
        Err(error) => RewriteResult {
            index: paragraph.index,
            original: paragraph.text.clone(),
            rewritten: paragraph.text,
            accepted: false,
            skipped: false,
            failed: true,
            error: Some(error.to_string()),
            style: paragraph.style,
        },
    }
}

async fn rewrite_doubao_two_pass(
    client: &Client,
    config: &ApiConfig,
    paragraph: Paragraph,
) -> RewriteResult {
    let default_options = RewriteOptions::default();
    let first_prompt = system_prompt(
        &config.language,
        "doubao_plain_humanize",
        &default_options,
        None,
    );
    let first = match rewrite_once(client, config, &first_prompt, &paragraph.text).await {
        Ok(value) => value,
        Err(error) => {
            return RewriteResult {
                index: paragraph.index,
                original: paragraph.text.clone(),
                rewritten: paragraph.text,
                accepted: false,
                skipped: false,
                failed: true,
                error: Some(error.to_string()),
                style: paragraph.style,
            };
        }
    };

    if invalid_rewrite_output(&first) {
        return RewriteResult {
            index: paragraph.index,
            original: paragraph.text.clone(),
            rewritten: paragraph.text,
            accepted: false,
            skipped: false,
            failed: true,
            error: Some("第一轮返回了提示词规则，已保留原文".to_string()),
            style: paragraph.style,
        };
    }

    let second_input = format!(
        "【原文】\n{}\n\n【第一轮改写】\n{}\n\n请只输出第二轮结果。",
        paragraph.text, first
    );
    let second = rewrite_once(client, config, doubao_second_pass_prompt(), &second_input).await;

    match second {
        Ok(value) if !invalid_rewrite_output(&value) => build_success_result_with_note(
            config,
            paragraph,
            value,
            Some("豆包降AI：已自动二轮降规整度"),
        ),
        Ok(_) => build_success_result_with_note(
            config,
            paragraph,
            first,
            Some("豆包降AI：第二轮异常，已采用第一轮结果"),
        ),
        Err(error) => build_success_result_with_note(
            config,
            paragraph,
            first,
            Some(&format!(
                "豆包降AI：第二轮失败，已采用第一轮结果（{error}）"
            )),
        ),
    }
}

fn build_success_result(
    config: &ApiConfig,
    paragraph: Paragraph,
    rewritten: String,
) -> RewriteResult {
    build_success_result_with_note(config, paragraph, rewritten, None)
}

fn build_success_result_with_note(
    config: &ApiConfig,
    paragraph: Paragraph,
    rewritten: String,
    note: Option<&str>,
) -> RewriteResult {
    if invalid_rewrite_output(&rewritten) {
        return RewriteResult {
            index: paragraph.index,
            original: paragraph.text.clone(),
            rewritten: paragraph.text,
            accepted: false,
            skipped: false,
            failed: true,
            error: Some("模型返回了提示词规则，已保留原文".to_string()),
            style: paragraph.style,
        };
    }

    let change_ratio = rough_change_ratio(&paragraph.text, &rewritten);
    let conservative = config.prompt_profile == "conservative_rewrite";
    let too_much_change = conservative && change_ratio > 0.45;
    let too_long =
        conservative && rewritten.chars().count() > paragraph.text.chars().count() * 13 / 10;

    if too_much_change || too_long {
        return RewriteResult {
            index: paragraph.index,
            original: paragraph.text.clone(),
            rewritten: paragraph.text,
            accepted: true,
            skipped: true,
            failed: false,
            error: Some("保守策略回退：改动过大".to_string()),
            style: paragraph.style,
        };
    }

    RewriteResult {
        index: paragraph.index,
        original: paragraph.text,
        rewritten,
        accepted: true,
        skipped: false,
        failed: false,
        error: note.map(str::to_string),
        style: paragraph.style,
    }
}

async fn rewrite_once(
    client: &Client,
    config: &ApiConfig,
    system_prompt: &str,
    paragraph: &str,
) -> anyhow::Result<String> {
    chat_once(client, config, system_prompt, paragraph, 0.7, 2000).await
}

async fn chat_once(
    client: &Client,
    config: &ApiConfig,
    system_prompt: &str,
    user_content: &str,
    temperature: f32,
    max_tokens: u32,
) -> anyhow::Result<String> {
    let api_base = config.api_base.trim_end_matches('/');
    let url = format!("{api_base}/chat/completions");
    let request = ChatRequest {
        model: config.model.clone(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_content.to_string(),
            },
        ],
        temperature,
        max_tokens,
    };

    let mut last_error = String::new();
    for attempt in 0..MAX_API_ATTEMPTS {
        match chat_attempt(client, &url, config, &request).await {
            Ok(content) => return Ok(content),
            Err(error) if error.retryable && attempt + 1 < MAX_API_ATTEMPTS => {
                last_error = error.message;
                sleep(Duration::from_secs(RETRY_DELAYS_SECS[attempt])).await;
            }
            Err(error) => bail!(error.message),
        }
    }
    bail!(last_error)
}

struct ChatAttemptError {
    message: String,
    retryable: bool,
}

async fn chat_attempt(
    client: &Client,
    url: &str,
    config: &ApiConfig,
    request: &ChatRequest,
) -> Result<String, ChatAttemptError> {
    let response = timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS), async {
        client
            .post(url)
            .bearer_auth(&config.api_key)
            .json(request)
            .send()
            .await
    })
    .await
    .map_err(|_| ChatAttemptError {
        message: "API request timed out".to_string(),
        retryable: true,
    })?
    .map_err(|error| ChatAttemptError {
        message: format!("API request failed: {error}"),
        retryable: true,
    })?;

    let status = response.status();
    let body_text = response.text().await.map_err(|error| ChatAttemptError {
        message: format!("unable to read API response body: {error}"),
        retryable: is_retryable_status(status),
    })?;

    if !status.is_success() {
        return Err(ChatAttemptError {
            message: format!("API returned {status}: {body_text}"),
            retryable: is_retryable_status(status),
        });
    }

    let body: ChatResponse =
        serde_json::from_str(&body_text).map_err(|error| ChatAttemptError {
            message: format!("unable to parse API response: {error}"),
            retryable: false,
        })?;
    extract_chat_content(body)
        .or_else(|| extract_text_from_json(&body_text))
        .ok_or_else(|| ChatAttemptError {
            message: "API response did not include rewritten text".to_string(),
            retryable: false,
        })
}

fn is_retryable_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::TOO_MANY_REQUESTS
            | StatusCode::INTERNAL_SERVER_ERROR
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
    )
}

fn extract_chat_content(body: ChatResponse) -> Option<String> {
    for value in [
        body.output_text,
        body.content,
        body.text,
        body.response,
        body.result,
    ] {
        if let Some(content) = clean_content(value) {
            return Some(content);
        }
    }

    for choice in body.choices {
        if let Some(message) = choice.message {
            for value in [message.content, message.reasoning_content] {
                if let Some(content) = clean_content(value) {
                    return Some(content);
                }
            }
        }
        for value in [choice.text, choice.content] {
            if let Some(content) = clean_content(value) {
                return Some(content);
            }
        }
    }

    None
}

fn extract_text_from_json(body_text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body_text).ok()?;
    for path in [
        &["data", "content"][..],
        &["data", "text"][..],
        &["data", "output_text"][..],
        &["output", "text"][..],
        &["output", "content"][..],
    ] {
        if let Some(content) = get_json_path(&value, path).and_then(|value| value.as_str()) {
            if let Some(content) = clean_content(Some(content.to_string())) {
                return Some(content);
            }
        }
    }
    None
}

fn get_json_path<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn clean_content(value: Option<String>) -> Option<String> {
    value
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
}

pub fn validate_config(config: &ApiConfig) -> anyhow::Result<()> {
    if config.api_base.trim().is_empty() {
        bail!("API 地址不能为空");
    }
    if config.api_key.trim().is_empty() {
        bail!("API Key 不能为空");
    }
    if config.model.trim().is_empty() {
        bail!("模型名不能为空");
    }
    Ok(())
}

pub fn system_prompt(
    language: &str,
    prompt_profile: &str,
    options: &RewriteOptions,
    external_report: Option<&ExternalAigcReportEvidence>,
) -> String {
    match (language, prompt_profile) {
        (_, "sample_calibrated_17_v2") => sample_calibrated_17_v2_prompt(options, external_report),
        (_, "sample_calibrated_17_success") => legacy_directive_prompt(options, external_report),
        (_, "sample_calibrated_17") => legacy_directive_prompt(options, external_report),
        (_, "sample_calibrated_28") => {
            sample_calibrated_28_prompt(options)
        }
        (_, "directive_aigc_reduce") | (_, "directive_aigc_reduce_legacy") => {
            legacy_directive_prompt(options, external_report)
        }
        (_, "doubao_plain_humanize") => {
            "你只改写用户给出的中文论文段落，目标是降低 PaperPass AIGC 痕迹，不是润色得更高级。保留原意、术语、数据、公式、引用和论证顺序，不新增事实、案例、引用或结论。按这些经验处理：多保留或少量加入自然的虚词，如“的、了、到、过、有、能、把、会”；删掉或替换“首先、其次、最后、此外、综上”等机器化连接词，可用“一是、二是、一方面、另一方面、第一点、第二点”；减少短句和连续句号，能合并时用逗号或分号连起来；把偏正式、复杂、生僻的词换成简单常用但仍学术的表达；适当调整前后结构、把字句、被动句和定语位置。不要使用“不仅……更……、展现出、圆满完成、阶梯式增长、显著提升、具有重要意义、提供实践参考、有效路径”等 AI 套话。只输出改写后的段落，不要解释、标题、列表、Markdown 或复述规则。".to_string()
        }
        (_, "plain_spoken_humanize") => {
            "你只改写用户给出的中文论文段落，目标是让文字更普通、更直接、更不像 AI。保留论文语气，但不要写得漂亮、整齐或高级。保留原意、术语、数据、公式、引用和基本论证顺序，不新增事实。尽量使用常见词，减少“促进、推动、优化、完善、显著、重要、有效、路径、现状及对策”等模板词；连接词可以简单一些，句子可以略长一点，保留一点本科论文里常见的不均匀和重复。只输出改写后的段落。".to_string()
        }
        (_, "local_light_rewrite") => {
            "你只做局部轻改，不整段重写。保留原文绝大部分表达，只处理明显 AI 套话、机械连接词和过于工整的句子。保持原意、术语、数据、公式和引用，不新增内容。每段最多改 1-2 处，优先删减或替换“此外、首先、其次、最后、具有重要意义、促进发展、有效路径、不断完善”等模板表达。只输出改写后的段落。".to_string()
        }
        ("en", "local_depattern") => {
            "Rewrite the user's paragraph with small local edits only. Keep the same language, facts, terms, data, formulas, and citations. Change at most two sentences. Keep some awkwardness and repetition. Do not add explanations, headings, lists, summaries, or rules. Return only the rewritten paragraph.".to_string()
        }
        ("en", "paperpass_restructure") => {
            "You are an academic rewriting editor. Restructure the paragraph to reduce detector-friendly AI patterns while preserving meaning.\n\nRules:\n1. Keep the original language; do not translate.\n2. Preserve technical terms, names, data, formulas, and citation markers.\n3. Do not add facts, examples, citations, or new conclusions.\n4. Avoid simple synonym replacement. Change sentence structure and clause order instead.\n5. Keep the output as one academic paragraph.\n6. Output ONLY the rewritten paragraph.".to_string()
        }
        ("en", "conservative_rewrite") => {
            "You are a conservative academic editor. Rewrite only where necessary to reduce repetitive machine-like phrasing.\n\nRules:\n1. Keep the original language; do not translate.\n2. Preserve the original meaning, order of claims, terminology, data, formulas, and citation markers.\n3. Change roughly 20%-35% of the wording. Do not rewrite the whole paragraph.\n4. Do not expand, summarize, add transitions, add examples, add conclusions, or make the paragraph more polished than the source.\n5. Keep acceptable awkwardness and local variation; do not make every sentence uniformly smooth.\n6. Prefer small edits: word order, a few verbs, sentence boundary adjustments.\n7. Output ONLY the rewritten paragraph.".to_string()
        }
        ("en", "light_rewrite") => {
            "You are an academic paper polishing expert. Lightly rewrite the following paragraph:\n\nRequirements:\n1. Replace common expressions with synonyms\n2. Restructure sentences where useful\n3. Preserve the original meaning\n4. Keep all technical terms, names, data, formulas, and citation markers\n5. Keep the original language; do not translate between Chinese and English\n6. Maintain academic tone\n7. Output ONLY the rewritten paragraph, no explanations".to_string()
        }
        ("en", _) => {
            "You are a senior academic editor specializing in removing AI-writing traces while preserving scholarly meaning.\n\nRewrite the paragraph as a natural human academic author would write it.\n\nRules:\n1. Preserve the original claim, logic, terminology, names, data, formulas, and citation markers exactly where they matter.\n2. Keep the original language; do not translate between Chinese and English.\n3. Remove AI-like phrasing: inflated significance, promotional tone, vague authority claims, generic positive conclusions, formulaic transitions, excessive hedging, and mechanical three-part lists.\n4. Avoid stock phrases such as \"crucial\", \"pivotal\", \"landscape\", \"testament to\", \"underscores\", \"not only...but also\", and vague attributions such as \"experts believe\" unless they appear in the source and are necessary.\n5. Vary sentence rhythm. Prefer direct, concrete academic prose over ornate paraphrase.\n6. Do not add facts, examples, citations, claims, explanations, markdown, headings, or bullet points.\n7. Keep the rewrite close enough to the source for academic integrity, but make it less machine-like.\n8. Output ONLY the rewritten paragraph.".to_string()
        }
        (_, "local_depattern") => {
            "只改写用户给出的论文段落。保持中文和原意，不新增事实、案例、引用、数据或结论。每段最多改 1-2 句，其余尽量原样保留；保留一些笨拙、重复和本科论文表达。优先删除或压缩空泛模板句，不要整段重写，不要大量同义词替换。只输出改写后的段落，不要解释、不要标题、不要列表、不要复述规则。".to_string()
        }
        (_, "paperpass_restructure") => {
            "你是中文论文降 AIGC 改写编辑。请参考 PaperPass 降 AIGC 服务常见风格，对段落进行结构性重构，而不是简单同义词替换。\n\n目标：保留原文核心意思和关键词，让表达从“直接生成式文本”变成“人工本科论文式表达”。\n\n规则：\n1. 保持中文，不要翻译，不要新增事实、案例、引用、数据或结论。\n2. 保留专业名词、研究对象、关键概念、数据、公式和引用标记。\n3. 不要只换同义词；重点调整句子骨架、语序、主谓宾关系和并列结构。\n4. 可使用中文论文常见结构：以……为基础、以……为手段、从……角度、在……过程中、进而、从而、由此、在……方面。\n5. 对并列内容可改为“以 A 为……，以 B 为……，以 C 为……”这类人工论文表达。\n6. 删除或弱化空泛的绝对化表达，但不要把段落改得更短到丢失信息。\n7. 改动幅度控制在约 45%-65%，不要整段换主题。\n8. 不输出解释、标题、列表或 Markdown。只输出改写后的段落。".to_string()
        }
        (_, "conservative_rewrite") => {
            "你是保守型中文学术编辑。目标不是把段落润色得更漂亮，而是在尽量保留原文风格的前提下，降低机械重复和明显 AI 腔。\n\n规则：\n1. 保持原文语种，不要翻译。\n2. 保留原文意思、论证顺序、术语、数据、公式和引用标记。\n3. 只改约 20%-35% 的措辞，不要整段重写。\n4. 禁止扩写、总结、加过渡句、加结论、加“研究意义”套话。\n5. 保留原文中可接受的笨拙、重复和个人表达，不要把每句话都修得很顺。\n6. 优先做小改动：调整语序、替换少量动词、拆分或合并一处句子。\n7. 不输出解释、标题、列表或 Markdown。只输出改写后的段落。".to_string()
        }
        (_, "light_rewrite") => {
            "你是学术论文润色专家。请对以下段落进行轻度改写：\n\n要求：\n1. 换用同义词替换常见表述\n2. 适当调整句子结构\n3. 保持段落整体意思不变\n4. 保留所有专业术语、人名、地名、数据、公式、引用标记\n5. 保持原文语种，不要中英互译\n6. 保持学术语气，不要口语化\n7. 不要添加任何解释，只输出改写后的段落".to_string()
        }
        _ => {
            "你是一位资深中文学术编辑，任务是去除论文段落中的 AI 写作痕迹，同时保留学术含义。\n\n请把段落改写得像真实研究者写出来的中文论文，而不是聊天机器人生成的润色稿。\n\n规则：\n1. 保留原文的核心观点、论证关系、专业术语、人名、地名、数据、公式和引用标记。不要新增事实、案例、引用或结论。\n2. 保持原文语种，不要中英互译。\n3. 删除或改写 AI 腔：夸大的意义宣告、宣传式形容词、模糊归因、万能积极结尾、机械连接词、过度限定、三段式排比、否定式排比。\n4. 避免套话和高频 AI 词：此外、至关重要、关键作用、深入探讨、不断演变的格局、彰显、体现、赋能、显著提升、标志着、证明了、不可或缺。\n5. 多用直接、具体、克制的学术表达。能用“是”“有”“包括”说清楚时，不要写成“作为/充当/标志着”。\n6. 调整句子长短和顺序，让节奏更自然，但不要口语化，不要加入第一人称，不要写得像营销文案。\n7. 不输出解释、评分、标题、列表、Markdown 或寒暄。只输出改写后的段落。".to_string()
        }
    }
}

fn sample_calibrated_28_prompt(options: &RewriteOptions) -> String {
    let current = options.current_ai_rate.unwrap_or(60.0).clamp(0.0, 100.0);
    let target = options.target_ai_rate.unwrap_or(10.0).clamp(0.0, 100.0);
    let gap = (current - target).max(0.0);
    let intensity = if current >= 80.0 && target <= 15.0 {
        "强扰动"
    } else if gap >= 40.0 {
        "中高强度"
    } else {
        "中等强度"
    };

    format!(
        "你是中文本科论文降 AIGC 改写助手。用户检测到当前 AI 率约为 {current:.0}%，希望降到 {target:.0}% 左右。本数值只用于决定改写强度，不要在输出中提到。当前强度：{intensity}。\n\n先在内部判断原段落的 AI 痕迹，再只输出改写后的段落。不要输出分析、标题、列表、Markdown、规则复述或“修改后：”。\n\n核心风格：参考低风险成功区间的共同特征，把高 AI 味的浓缩论文句改成更解释性、更像本科生论文的表达。文字可以略有生涩和稚嫩，态度端正，保留学术性，但不要写得太顺、太高级、太像标准润色稿。\n\n必须保留：原文事实、研究对象、术语、数据、公式、引用标记、专有名词和基本论证关系。禁止新增事实、案例、数据、引用、结论或第一人称。禁止“呢、啦、么”等闲聊语气。\n\n改写手法：\n1. 优先把压缩、工整、总结式的长句拆成 2-4 个更朴素的句子；如果原文已经短而清楚，可以只做轻微调整。\n2. 允许比原文长约 20%-35%，用于把过度浓缩的判断解释开；但不能为了凑字加入新信息。\n3. 弱化或替换 AI 和论文模板词，尤其是“显著提升、促进、推动、完善、深度融合、有效路径、提供参考、具有重要意义、赋能、旨在、调研发现、实践参考、理论参考”等。优先改成更普通的说法。\n4. 有控制地加入本科论文常见缓冲表达，如“比较、一些、一定、还、可以、会、里面、方面、情况、来看、过程中、不够、并不、有一部分、相对”等；不要每句话都塞，保持自然分布。\n5. 连接方式要普通，不要统一套“首先、其次、最后、此外、综上”。可以用“从……来看、对……而言、在……中、还有、另外、这样”等较朴素表达。\n6. 可以保留一点不够圆滑的语序和重复，让文字像学生自己整理出来的论文；但不能变成病句、口水话或聊天语气。\n7. 对引用、年份、百分比、观察次数、作者姓名和专业术语必须谨慎保留，不要改错。\n\n输出要求：只输出最终段落纯文本。"
    )
}

fn legacy_directive_prompt(
    options: &RewriteOptions,
    external_report: Option<&ExternalAigcReportEvidence>,
) -> String {
    let current = options.current_ai_rate.unwrap_or(60.0).clamp(0.0, 100.0);
    let target = options.target_ai_rate.unwrap_or(10.0).clamp(0.0, 100.0);
    let gap = (current - target).max(0.0);
    let intensity = if current >= 80.0 && target <= 15.0 {
        "强扰动"
    } else if gap >= 40.0 {
        "中高强度"
    } else {
        "中等强度"
    };

    let guidance = external_rewrite_guidance(external_report);
    format!(
        "你是中文论文降 AIGC 改写助手。用户检测到当前 AI 率约为 {current:.0}%，希望降到 {target:.0}% 左右。本数值只用于决定改写强度，不要在输出中提到。当前强度：{intensity}。\n\n先在内部判断原段落的 AI 痕迹，再只输出改写后的段落。不要输出分析、标题、列表、Markdown、规则复述或“修改后：”。\n\n核心风格：把文本写成略有生涩和稚嫩、像中文并不是很精通但态度端正的人写的论文句子；保留一点学术性，但不要太顺、太完整、太统一，也不要像标准润色稿。\n\n必须保留：原文事实、研究对象、术语、数据、公式、引用标记、专有名词和基本论证关系。禁止新增事实、案例、数据、引用、结论或第一人称。禁止“呢、啦、么”等闲聊语气。\n\n改写手法：\n1. 拆分过长句，或把过于整齐的长并列句改成不完全对称的结构；段落之间不要全都用同一种句式。\n2. 替换或弱化 AI 喜欢的词和大款句式，如“具有重要意义、显著提升、有效路径、促进发展、不断完善、深入探讨、现状及对策、为……提供参考”等。\n3. 可以让语序略别扭、连接词朴素一点，保留少量不够圆滑的表达；但不能变成病句或口水话。\n4. 桌面指令里的词表只作为弱参考：使用/采用 可少量改为 运用/选用；基于 可改为 鉴于/依据；通过 可改为 借助/依靠；不要机械逐词替换。\n5. 允许比原文最多长约 10%，但不要为了凑字扩写新信息。\n\n输出要求：只输出最终段落纯文本。"
    )
    + &guidance
}

fn sample_calibrated_17_v2_prompt(
    options: &RewriteOptions,
    external_report: Option<&ExternalAigcReportEvidence>,
) -> String {
    let base = legacy_directive_prompt(options, external_report);
    format!(
        "{base}\n\n17% 2.0 细调：在不改变上述规则的前提下，比成功链路基线稍微再生涩一点。少用过顺的表达，例如“对于……也有一些作用”“更为紧密一些”“产生作用”；可改成更普通、更笨一点的说法，例如“对……有一点帮助”“结合得更充分一些”“能起到一些帮助”。不要大幅扩写，不要把每段写成解释性长段，不要堆叠“比较、一些、会、情况、过程中”等词。单次长度仍按基线控制，不能为了降低 AIGC 增加新事实、例子、数据或结论。只输出最终段落纯文本。"
    )
}

fn external_rewrite_guidance(report: Option<&ExternalAigcReportEvidence>) -> String {
    let Some(report) = report else {
        return String::new();
    };
    let types = if report.risk_types.is_empty() {
        "摘要式总结、英文摘要、策略清单、阶段推进、结论总结".to_string()
    } else {
        report.risk_types.join("、")
    };
    let summary = report
        .rewrite_guidance
        .as_deref()
        .or(report.analysis_summary.as_deref())
        .unwrap_or("外部报告显示，完整包装感强的摘要、策略和总结段更容易被命中。");
    format!(
        "\n\n外部报告反推规则：本地已有 {} 报告证据，命中 {} 个疑似片段、{} 处标注，主要风险类型为：{}。{} 这类报告抓的不是单个词，而是完整包装结构。\n\n当前成功链路以“测试20段后叠加全文”为主：先让部分正文变成较低AI的底稿，再基于这个底稿跑全文。不要把这些成功样本理解成普通一次性整篇直跑，也不要因为有PP报告就默认切到PP定向。\n\n如果已改写稿 PaperPass 低于20%，视为过线，不要为了追求更低分继续整篇大扩写；PP报告定向只用于原稿未改写先跑PP且高于20%，或17 2.0测试20段后叠加全文仍高于20%/高疑似明显时。参考文献、问卷访谈提纲、原创性声明、版权授权书、致谢和附录类命中不作为正文失败处理。\n\n遇到以下正文段落才重点处理：\n- 摘要或研究概述：一段同时写背景、理论、方法、数据、问题、建议和意义。\n- 文献综述：用“综上所述/现有研究不足/基于此”很顺地推出本研究。\n- 理论定义：写成“概念定义 + 作用意义 + 维度体系”的完整说明。\n- 表格或数据解释：一句话把“表格显示、数据一致、原因、结论”全部收束。\n- 条目解释：第三、第四、首先、其次后面接“原因+例子+作用总结”的完整小作文。\n- 案例描述：把品牌案例、设计细节、用户反应和价值判断一口气讲完。\n- 策略建议：一是二是三是、三个层面、五项策略、六个维度这类整齐清单。\n- 推进计划：第一阶段、第二阶段、第三阶段，最后形成良性循环。\n\n改写时不要只换词；要拆掉“完整方案/研究验证/提供支持/推动转型/分阶段推进/形成良性循环”这类包装句，改成更具体、更散、更像学生自己解释的表达。可以保留事实、数据和顺序，但不要让句子继续保持背景-方法-结论-意义的一条龙结构；不要新增事实，也不要把短段强行扩成长段。对于表格、得分、比例、引用数据，优先保留数值，改解释方式，不改事实。对于案例段，保留案例事实但删掉统一价值总结。",
        report.provider,
        report.suspicious_segment_count,
        report.marked_span_count,
        types,
        summary
    )
}

fn doubao_second_pass_prompt() -> &'static str {
    "你只处理【第一轮改写】里的文字，并参考【原文】校正含义。目标是第二轮降 AIGC：把第一轮里过于工整、过于像提示词执行结果、过于高级润色的地方改得更普通、更直接、更像本科论文。不要继续扩写，不要新增事实、案例、数据、引用或结论；不要把句子改得更漂亮；保留一点重复和不均匀。可以增加少量自然虚词“的、了、到、过、有、能、把、会”，减少句号，用逗号或分号连接相关句子。禁止输出规则、解释、标题、列表、Markdown。只输出最终段落。"
}

fn is_english_heavy(text: &str) -> bool {
    let letters = text.chars().filter(|ch| ch.is_ascii_alphabetic()).count();
    let cjk = text
        .chars()
        .filter(|ch| ('\u{4e00}'..='\u{9fff}').contains(ch))
        .count();
    letters >= 24 && letters > cjk * 3
}

fn should_skip_before_api(text: &str) -> bool {
    let trimmed = text.trim();
    let cjk = trimmed
        .chars()
        .filter(|ch| ('\u{4e00}'..='\u{9fff}').contains(ch))
        .count();
    let total = trimmed.chars().count();
    cjk < 12 || total < 24
}

fn rewrite_skip_reason(paragraph: &Paragraph) -> Option<String> {
    if paragraph.skip {
        return Some(
            paragraph
                .skip_reason
                .clone()
                .unwrap_or_else(|| "解析跳过段落".to_string()),
        );
    }

    if should_skip_before_api(&paragraph.text) {
        return Some("短段或低收益段落".to_string());
    }

    None
}

fn looks_like_instruction_echo(text: &str) -> bool {
    let markers = [
        "用户指令",
        "修改后：",
        "修改后:",
        "规则包括",
        "规则：",
        "目标不是",
        "保持中文",
        "不要翻译",
        "不要新增事实",
        "不要整段重构",
        "每段最多改",
        "只输出改写后的段落",
        "AIGC 编辑",
        "作为中文论文",
    ];
    let hit_count = markers
        .iter()
        .filter(|marker| text.contains(*marker))
        .count();
    hit_count >= 2
}

fn invalid_rewrite_output(text: &str) -> bool {
    looks_like_instruction_echo(text) || looks_like_list_or_explanation(text)
}

fn looks_like_list_or_explanation(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.starts_with("以下是")
        || trimmed.starts_with("改写如下")
        || trimmed.starts_with("优化如下")
        || trimmed.starts_with("第二轮结果")
    {
        return true;
    }

    let list_markers = ["\n1.", "\n1、", "\n一、", "\n- ", "\n* "];
    list_markers.iter().any(|marker| trimmed.contains(marker))
}

fn rough_change_ratio(original: &str, rewritten: &str) -> f32 {
    let original_chars: Vec<char> = original.chars().collect();
    let rewritten_chars: Vec<char> = rewritten.chars().collect();
    let max_len = original_chars.len().max(rewritten_chars.len()).max(1);
    let common = lcs_len(&original_chars, &rewritten_chars);
    1.0 - (common as f32 / max_len as f32)
}

fn lcs_len(left: &[char], right: &[char]) -> usize {
    let mut prev = vec![0usize; right.len() + 1];
    let mut curr = vec![0usize; right.len() + 1];

    for left_char in left {
        for (index, right_char) in right.iter().enumerate() {
            curr[index + 1] = if left_char == right_char {
                prev[index] + 1
            } else {
                prev[index + 1].max(curr[index])
            };
        }
        std::mem::swap(&mut prev, &mut curr);
        curr.fill(0);
    }

    prev[right.len()]
}

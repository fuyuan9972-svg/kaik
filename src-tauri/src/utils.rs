use crate::models::Paragraph;
use anyhow::Context;

pub fn cjk_count(text: &str) -> usize {
    text.chars()
        .filter(|ch| ('\u{4e00}'..='\u{9fff}').contains(ch))
        .count()
}

pub fn truncate_text(text: &str, limit: usize) -> String {
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

pub fn is_body_candidate(paragraph: &Paragraph) -> bool {
    if paragraph.skip {
        return false;
    }
    let compact: String = paragraph
        .text
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    let compact_lower = compact.to_ascii_lowercase();
    if compact.len() < 45 {
        return false;
    }
    if compact.starts_with("关键词")
        || compact.starts_with("关键字")
        || compact_lower.starts_with("keywords")
    {
        return false;
    }
    if compact.contains("参考文献")
        || compact.contains("目录")
        || compact.contains("原创性声明")
        || compact.contains("独创性声明")
        || compact.contains("本人声明")
        || compact.contains("版权使用授权书")
    {
        return false;
    }
    if compact.contains("[J]") || compact.contains("[M]") || compact.contains("[D]") {
        return false;
    }
    cjk_count(&compact) >= 35
}

pub fn is_ai_body_candidate(paragraph: &Paragraph) -> bool {
    if paragraph.skip {
        return false;
    }
    let cjk = cjk_count(&paragraph.text);
    cjk >= 35 && paragraph.text.chars().count() >= 45
}

pub fn extract_json_object(text: &str) -> anyhow::Result<&str> {
    let trimmed = text.trim();
    if let Some(start) = trimmed.find('{') {
        let end = trimmed.rfind('}').context("AI 返回内容不是 JSON 对象")?;
        Ok(&trimmed[start..=end])
    } else {
        Ok(trimmed)
    }
}

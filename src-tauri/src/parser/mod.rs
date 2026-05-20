mod docx;
mod pdf;
mod txt;

use crate::models::{Paragraph, ParagraphStyle};
use anyhow::{bail, Context};
use regex::Regex;
use std::path::Path;

pub fn parse_file(file_path: &str) -> anyhow::Result<Vec<Paragraph>> {
    let path = Path::new(file_path);
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase();

    let raw = match ext.as_str() {
        "docx" => docx::parse(path)?,
        "doc" | "rtf" => parse_with_textutil(path)?,
        "pdf" => pdf::parse(path)?,
        "txt" | "md" => txt::parse(path)?,
        _ => bail!("unsupported file type: .{}", ext),
    };

    Ok(raw
        .into_iter()
        .enumerate()
        .map(|(index, (text, style))| {
            let (skip, skip_reason) = skip_info(&text);
            Paragraph {
                index,
                text,
                style,
                skip,
                skip_reason,
            }
        })
        .collect())
}

pub fn split_text_to_paragraphs(text: &str) -> Vec<(String, ParagraphStyle)> {
    text.replace("\r\n", "\n")
        .split("\n\n")
        .flat_map(|chunk| {
            chunk
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|line| !line.is_empty())
        .map(|line| (line.to_string(), ParagraphStyle::default()))
        .collect()
}

fn parse_with_textutil(path: &Path) -> anyhow::Result<Vec<(String, ParagraphStyle)>> {
    let output = std::process::Command::new("textutil")
        .args(["-convert", "txt", "-stdout", "--"])
        .arg(path)
        .output()
        .with_context(|| format!("unable to run textutil for {}", path.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("unable to convert {} with textutil: {}", path.display(), stderr);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(split_text_to_paragraphs(&text))
}

fn skip_info(text: &str) -> (bool, Option<String>) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return (true, Some("空段落".to_string()));
    }
    if is_cover_or_metadata(trimmed) {
        return (true, Some("封面或元信息".to_string()));
    }
    if is_keyword_line(trimmed) {
        return (true, Some("关键词段落".to_string()));
    }
    if is_legal_statement(trimmed) {
        return (true, Some("声明段落".to_string()));
    }
    if is_reference_like(trimmed)
        || trimmed.contains("[参考文献]")
        || trimmed.contains("[References]")
        || trimmed.eq_ignore_ascii_case("references")
        || trimmed == "参考文献"
    {
        return (true, Some("参考文献段落".to_string()));
    }
    if is_heading_like(trimmed) {
        return (true, Some("标题或小节名".to_string()));
    }
    if trimmed.contains("\\begin{equation}") || trimmed.contains("\\end{equation}") {
        return (true, Some("公式段落".to_string()));
    }
    let latex_inline = Regex::new(r"\$[^$]+\$").expect("valid regex");
    if latex_inline.is_match(trimmed) {
        return (true, Some("公式段落".to_string()));
    }
    let caption_re =
        Regex::new(r"^\s*(图\s*\d+|表\s*\d+|Figure\s+\d+|Table\s+\d+)").expect("valid regex");
    if caption_re.is_match(trimmed) {
        return (true, Some("图表标题".to_string()));
    }
    (false, None)
}

fn is_reference_like(text: &str) -> bool {
    let compact = text.split_whitespace().collect::<String>();
    let reference_re = Regex::new(r"^\s*\[\d+\]").expect("valid regex");
    let marker_re = Regex::new(r"\[(J|M|D|C|N|R|S|P|EB/OL)\]").expect("valid regex");
    let year_re = Regex::new(r"(19|20)\d{2}").expect("valid regex");
    if reference_re.is_match(text) || (marker_re.is_match(&compact) && year_re.is_match(&compact)) {
        return true;
    }

    let english_reference_re =
        Regex::new(r"^[A-Z][A-Za-z\s,.-]+\.\s+.+\[(J|M|D|C|N|R|S|P|EB/OL)\]\.")
            .expect("valid regex");
    english_reference_re.is_match(text)
}

fn is_cover_or_metadata(text: &str) -> bool {
    let compact = text.split_whitespace().collect::<String>();
    let meta_re = Regex::new(
        r"^(本科|专科|硕士|博士)?毕业(设计|论文)(（.*）|\(.*\))?$|^(论文)?题目[:：]?.*|^(姓名|学生姓名|学号|学院|系别|专业|班级|年级|指导教师|导师|完成日期|日期)[:：]?.*|^\d{4}年\d{1,2}月\d{1,2}日$",
    )
    .expect("valid regex");
    meta_re.is_match(&compact)
}

fn is_keyword_line(text: &str) -> bool {
    let keyword_re = Regex::new(r"(?i)^\s*(关键词|关键字|keywords?|key\s+words?)\s*[:：].+")
        .expect("valid regex");
    keyword_re.is_match(text)
}

fn is_legal_statement(text: &str) -> bool {
    let compact = text.split_whitespace().collect::<String>();
    compact.contains("原创性声明")
        || compact.contains("独创性声明")
        || compact.contains("本人声明")
        || compact.contains("版权使用授权书")
        || compact.contains("学位论文使用授权")
        || compact.contains("除文中已注明引用")
}

fn is_heading_like(text: &str) -> bool {
    let compact = text.split_whitespace().collect::<String>();
    let char_count = compact.chars().count();
    if char_count == 0 || char_count > 60 {
        return false;
    }

    let section_re = Regex::new(
        r"^(摘要|关键词|目录|绪论|引言|结论|结语|致谢|附录|Abstract|Keywords?|Contents?|Acknowledgements?)$|^第[一二三四五六七八九十\d]+[章节].*|^\d+(\.\d+)*[\.、．]?\s*.+|^[（(][一二三四五六七八九十\d]+[）)].+",
    )
    .expect("valid regex");
    if section_re.is_match(text.trim()) {
        return true;
    }

    let has_sentence_punctuation = compact.contains('。')
        || compact.contains('；')
        || compact.contains('？')
        || compact.contains('！')
        || compact.contains(". ")
        || compact.contains("; ");
    let title_keywords = [
        "研究", "现状", "对策", "分析", "设计", "论文", "活动", "游戏", "教学", "教育", "声明",
    ];
    !has_sentence_punctuation
        && char_count <= 40
        && title_keywords
            .iter()
            .any(|keyword| compact.contains(keyword))
}

pub fn read_file(path: &Path) -> anyhow::Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("unable to read {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::skip_info;

    #[test]
    fn skips_cover_metadata_and_titles() {
        for text in [
            "本科毕业设计（论文）",
            "民间传统游戏融入幼儿园体育活动的现状及对策研究",
            "姓名：",
            "2026年5月18日",
            "独创性声明",
            "3.文献述评",
            "（一）选题缘由",
        ] {
            let (skip, reason) = skip_info(text);
            assert!(skip, "{text} should be skipped");
            assert!(reason.is_some());
        }
    }

    #[test]
    fn skips_keyword_lines() {
        for text in [
            "关键词：民间传统游戏；幼儿园体育活动；现状调查；优化对策",
            "关键字: 民间传统游戏；幼儿园体育活动",
            "Keywords: folk traditional games; kindergarten physical activities",
            "Key words: folk games; kindergarten sports",
        ] {
            let (skip, reason) = skip_info(text);
            assert!(skip, "{text} should be skipped");
            assert_eq!(reason.as_deref(), Some("关键词段落"));
        }

        let (skip, reason) = skip_info("研究中的关键词选择需要结合幼儿园体育活动的实际情况。");
        assert!(!skip);
        assert!(reason.is_none());
    }

    #[test]
    fn skips_reference_entries() {
        for text in [
            "王艳萍. 传统民间游戏在幼儿园体育活动中的渗透[J]. 教师博览, 2025, (27): 85-87.",
            "Baker A G , Velija P . Families, Pre-School Sport, and Physical Activity: Critical Perspectives[M]. Taylor & Francis: 2025-03-24.",
            "[1] 张三. 幼儿体育活动研究[J]. 教育研究, 2024(1): 1-4.",
        ] {
            let (skip, reason) = skip_info(text);
            assert!(skip, "{text} should be skipped");
            assert_eq!(reason.as_deref(), Some("参考文献段落"));
        }
    }

    #[test]
    fn does_not_skip_real_body_paragraph() {
        let text = "民间传统游戏兼具趣味性与文化内涵，将其纳入幼儿园体育活动，有助于促进幼儿身体发展，也有利于中华优秀传统文化的传承。";
        let (skip, reason) = skip_info(text);
        assert!(!skip);
        assert!(reason.is_none());
    }
}

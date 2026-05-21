use crate::models::{ExternalAigcReportEvidence, ExternalAigcReportSegment};
use crate::utils::{cjk_count, truncate_text};
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

const RISK_TYPES: &[(&str, &[&str])] = &[
    (
        "问卷访谈提纲命中",
        &[
            "请简要介绍",
            "您此次",
            "使用频率如何",
            "哪些体验较差",
            "有哪些具体建议",
            "访谈前",
            "访谈中",
            "追问细节",
            "这份问卷",
            "填写结果",
        ],
    ),
    (
        "参考文献命中",
        &["[J]", "[D]", "[N]", "旅游纵览", "智能城市", "西部旅游", "燕山大学", "吉林大学"],
    ),
    (
        "声明授权命中",
        &["本人声明", "学位论文", "原创性声明", "独创性声明", "法律结果由本人承担", "版权使用授权书"],
    ),
    ("文献综述包装段", &["国内外研究", "研究动态", "文献", "已有研究"]),
    ("理论定义包装段", &["理论", "概念", "模型", "维度", "体系"]),
    ("数据解释包装段", &["数据", "比例", "得分", "评分", "投诉量", "表"]),
    ("条目解释包装段", &["第三", "第四", "首先", "其次", "趣味性", "基本权利"]),
    ("术语例句解释段", &["例句", "相当于", "意思大致", "表目的", "表结果", "用于连接"]),
    ("语体功能解释段", &["语体功能", "程式化", "庄重", "严谨", "公文格式", "语气"]),
    ("案例完整包装段", &["案例", "Hello Kitty", "小小飞行家", "主题", "仪式感", "参与"]),
    ("致谢作文腔", &["致谢", "感谢", "导师", "家人", "室友", "论文也算"]),
    ("策略清单包装段", &["建议", "对策", "策略", "一是", "二是", "第一阶段", "第二阶段"]),
    ("意义闭环包装段", &["提供参考", "推动", "促进", "完善", "形成", "意义"]),
    ("服务质量包装段", &["服务质量", "顾客体验", "满意度", "低成本航空"]),
];

pub fn parse_report(report_path: &str) -> Result<ExternalAigcReportEvidence> {
    let report_path = PathBuf::from(report_path);
    let root = find_report_root(&report_path)?;
    let reduce_source = read_maybe(&root.join("htmls/js/reduceaigcpagedata.js"))?;
    let simple_source = read_maybe(&root.join("htmls/js/simplesentenceresult_ai.js"))?;

    let reduce = extract_js_value(&reduce_source, "var reduceAiDataInfo")
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .unwrap_or(Value::Null);
    let simple = extract_js_value(&simple_source, "var data")
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .ok_or_else(|| anyhow!("未找到 PaperPass AIGC 片段数据，请选择 AIGC检测报告.html 或报告目录"))?;

    let mut segments = parse_segments(&simple);
    segments.sort_by(|a, b| {
        b.suspected_ratio
            .partial_cmp(&a.suspected_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let risk_types = infer_risk_types(&segments);
    let total = number_field(&reduce, "totalSuspectedTextRatio");
    let high = number_field(&reduce, "highSuspectedTextRatio");
    let middle = number_field(&reduce, "middleSuspectedTextRatio");
    let low = number_field(&reduce, "lowSuspectedTextRatio");
    let body_count = segments
        .iter()
        .filter(|segment| segment.segment_kind.as_deref() == Some("body"))
        .count();
    let appendix_count = segments
        .iter()
        .filter(|segment| segment.segment_kind.as_deref() == Some("appendix"))
        .count();
    let reference_count = segments
        .iter()
        .filter(|segment| segment.segment_kind.as_deref() == Some("reference"))
        .count();

    Ok(ExternalAigcReportEvidence {
        provider: "paperpass".to_string(),
        report_file_name: report_path
            .file_name()
            .and_then(|value| value.to_str())
            .map(|value| value.to_string()),
        report_file_path: Some(
            report_path
                .canonicalize()
                .unwrap_or(report_path.clone())
                .to_string_lossy()
                .to_string(),
        ),
        report_score: number_field(&reduce, "score"),
        total_suspected_ratio: total,
        high_and_middle_suspected_ratio: number_field(&reduce, "highAndMiddleSuspectedTextRatio"),
        high_suspected_ratio: high,
        middle_suspected_ratio: middle,
        low_suspected_ratio: low,
        no_ai_suspected_ratio: number_field(&reduce, "noAISuspectedTextRatio"),
        human_written_rate: total.map(|value| round(100.0 - value, 2)),
        suspicious_segment_count: segments.len(),
        body_suspicious_segment_count: Some(body_count),
        appendix_like_segment_count: Some(appendix_count),
        reference_segment_count: Some(reference_count),
        marked_span_count: 0,
        marked_chars: segments.iter().map(|segment| segment.suspected_chars).sum(),
        severe_segment_count: segments
            .iter()
            .filter(|segment| segment.suspected_ratio >= 70.0)
            .count(),
        moderate_segment_count: segments
            .iter()
            .filter(|segment| segment.suspected_ratio >= 60.0 && segment.suspected_ratio < 70.0)
            .count(),
        mild_segment_count: segments
            .iter()
            .filter(|segment| segment.suspected_ratio >= 50.0 && segment.suspected_ratio < 60.0)
            .count(),
        risk_types: risk_types.clone(),
        analysis_summary: Some(build_analysis_summary(total, high, middle, low, &segments, &risk_types)),
        rewrite_guidance: Some(build_rewrite_guidance(total, high, middle, &segments, &risk_types)),
        segments,
    })
}

fn find_report_root(report_path: &Path) -> Result<PathBuf> {
    let meta = fs::metadata(report_path)
        .with_context(|| format!("报告路径不存在：{}", report_path.display()))?;
    if meta.is_dir() {
        return Ok(report_path.to_path_buf());
    }
    let dir = report_path
        .parent()
        .ok_or_else(|| anyhow!("无法识别报告目录：{}", report_path.display()))?;
    if dir.join("htmls").exists() {
        return Ok(dir.to_path_buf());
    }
    if dir.file_name().and_then(|value| value.to_str()) == Some("htmls") {
        if let Some(parent) = dir.parent() {
            return Ok(parent.to_path_buf());
        }
    }
    Ok(dir.to_path_buf())
}

fn read_maybe(path: &Path) -> Result<String> {
    if path.exists() {
        fs::read_to_string(path).with_context(|| format!("无法读取 {}", path.display()))
    } else {
        Ok(String::new())
    }
}

fn extract_js_value<'a>(source: &'a str, marker: &str) -> Option<&'a str> {
    let marker_index = source.find(marker)?;
    let mut start = marker_index + marker.len();
    let bytes = source.as_bytes();
    while bytes.get(start).is_some_and(u8::is_ascii_whitespace) {
        start += 1;
    }
    if bytes.get(start) == Some(&b'=') {
        start += 1;
    }
    while bytes.get(start).is_some_and(u8::is_ascii_whitespace) {
        start += 1;
    }
    let opener = source[start..].chars().next()?;
    let closer = match opener {
        '{' => '}',
        '[' => ']',
        _ => return None,
    };
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (offset, ch) in source[start..].char_indices() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
        } else if ch == opener {
            depth += 1;
        } else if ch == closer {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                let end = start + offset + ch.len_utf8();
                return Some(&source[start..end]);
            }
        }
    }
    None
}

fn parse_segments(simple: &Value) -> Vec<ExternalAigcReportSegment> {
    let Some(map) = simple.as_object() else {
        return Vec::new();
    };
    map.iter()
        .filter_map(|(key, value)| {
            let text = value
                .get("sectionContentList")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default();
            let text = text.trim().to_string();
            if text.is_empty() {
                return None;
            }
            Some(ExternalAigcReportSegment {
                no: key.parse().unwrap_or_default(),
                suspected_chars: cjk_count(&text),
                suspected_ratio: round(
                    value.get("overall").and_then(Value::as_f64).unwrap_or_default() as f32,
                    2,
                ),
                segment_kind: Some(classify_segment_kind(&text).to_string()),
                text,
            })
        })
        .collect()
}

fn infer_risk_types(segments: &[ExternalAigcReportSegment]) -> Vec<String> {
    let full = segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    RISK_TYPES
        .iter()
        .filter_map(|(risk_type, terms)| {
            terms
                .iter()
                .any(|term| full.contains(term))
                .then(|| (*risk_type).to_string())
        })
        .collect()
}

fn classify_segment_kind(text: &str) -> &'static str {
    let compact: String = text.chars().filter(|ch| !ch.is_whitespace()).collect();
    if looks_reference_segment(&compact) {
        "reference"
    } else if looks_appendix_segment(&compact) {
        "appendix"
    } else {
        "body"
    }
}

fn looks_reference_segment(text: &str) -> bool {
    let markers = ["[J]", "[D]", "[M]", "[N]"]
        .iter()
        .filter(|marker| text.contains(**marker))
        .count();
    markers >= 1
        || text.contains("旅游纵览")
        || text.contains("智能城市")
        || text.contains("西部旅游")
        || text.contains("燕山大学")
        || text.contains("吉林大学")
        || text.contains("居舍")
        || text.contains("城市建筑")
}

fn looks_appendix_segment(text: &str) -> bool {
    let signals = [
        "请简要介绍",
        "是否使用",
        "使用频率如何",
        "哪些体验较差",
        "有哪些具体建议",
        "通常咨询哪些",
        "根据您的观察",
        "访谈前",
        "访谈中",
        "结束后及时整理",
        "这份问卷",
        "填写结果",
        "感谢您参加这次访谈",
        "访谈时间大概",
        "再次感谢您的参与",
        "本人声明",
        "原创性声明",
        "独创性声明",
        "法律结果由本人承担",
        "版权使用授权书",
    ];
    let numbered_questions = text.matches('、').count() + text.matches('．').count() + text.matches('.').count();
    signals.iter().any(|term| text.contains(term))
        || (numbered_questions >= 3
            && (text.contains('您') || text.contains("访谈") || text.contains("问卷")))
}

fn build_analysis_summary(
    total: Option<f32>,
    high: Option<f32>,
    middle: Option<f32>,
    low: Option<f32>,
    segments: &[ExternalAigcReportSegment],
    risk_types: &[String],
) -> String {
    format!(
        "PaperPass报告总疑似{}%，高疑似{}%，中疑似{}%，低疑似{}%；共命中{}个片段，主要集中在{}。",
        display_ratio(total),
        display_ratio(high),
        display_ratio(middle),
        display_ratio(low),
        segments.len(),
        if risk_types.is_empty() {
            "摘要/理论/数据/策略包装段".to_string()
        } else {
            risk_types.join("、")
        }
    )
}

fn build_rewrite_guidance(
    total: Option<f32>,
    high: Option<f32>,
    middle: Option<f32>,
    segments: &[ExternalAigcReportSegment],
    risk_types: &[String],
) -> String {
    let total = total.unwrap_or(100.0);
    let high = high.unwrap_or(100.0);
    let middle = middle.unwrap_or(100.0);
    let body_count = segments
        .iter()
        .filter(|segment| segment.segment_kind.as_deref() == Some("body"))
        .count();
    let appendix_count = segments.len().saturating_sub(body_count);
    let top = segments
        .iter()
        .take(3)
        .map(|segment| truncate_text(&segment.text.replace(char::is_whitespace, ""), 70))
        .collect::<Vec<_>>()
        .join(" / ");
    let severity = if total < 20.0 {
        "PP低于20%，按当前目标已经过线；已走17 2.0叠加全文的稿子不建议继续改写"
    } else if high > 0.0 {
        "仍有高疑似片段，优先做命中正文段定向改写"
    } else {
        "高疑似为0，优先处理中低风险正文片段，不要继续全篇大扩写"
    };
    let appendix_guidance = if appendix_count >= 5 && total <= 12.0 {
        format!(
            "本报告有{}个问卷/访谈/参考文献类命中，正文有效命中约{}个；这类附录型命中不应按正文失败处理。",
            appendix_count, body_count
        )
    } else {
        String::new()
    };
    let ratio_guidance = if total <= 10.0 && high <= 2.0 && middle <= 5.0 && body_count <= 6 {
        "这类10%内PP定向改写成功报告说明，按报告命中正文段处理后可以进入强成功区间；残留高/中疑似多集中在文献定义、引用综述和少量参考文献，不需要继续整篇追低。"
    } else if total <= 10.0 && high <= 0.1 && middle <= 4.5 && segments.len() <= 12 {
        "这类10%内强成功报告说明，17 2.0经过测试20段后叠加全文可以进入极低PP区间，少量致谢、问卷说明和定义解释命中不代表失败。"
    } else if total <= 18.0 && high <= 3.0 && middle <= 8.5 && segments.len() <= 14 {
        "这类15%左右成功报告说明，17 2.0叠加全文已达到过线目标；少量高/中/低疑似片段可以接受，不必为了追低分继续改。"
    } else if total <= 18.0 && high <= 0.1 && middle <= 15.0 && body_count <= 12 {
        "这类零高疑似、16%左右报告属于低占比成功边界：正文中疑似仍集中在文献综述、案例观察、理论定义和数据说明；已改写稿可直接停止，未改写原稿才按这些命中段定向处理。"
    } else {
        ""
    };
    format!(
        "{}。{}{}当前成功链路应按“测试20段后叠加全文”理解，不按普通整篇直跑归因。若是未改写原稿先跑PP或PP仍高于20，再重点拆散{}，把表格/数据解释、文献综述、理论定义、条目解释和案例完整包装改得更分散、更具体。典型命中：{}",
        severity,
        ratio_guidance,
        appendix_guidance,
        if risk_types.is_empty() {
            "完整包装段".to_string()
        } else {
            risk_types.join("、")
        },
        top
    )
}

fn number_field(value: &Value, name: &str) -> Option<f32> {
    value.get(name).and_then(Value::as_f64).map(|value| round(value as f32, 2))
}

fn display_ratio(value: Option<f32>) -> String {
    value
        .map(|value| format!("{value:.2}"))
        .unwrap_or_else(|| "未知".to_string())
}

fn round(value: f32, digits: u32) -> f32 {
    let factor = 10f32.powi(digits as i32);
    (value * factor).round() / factor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_report_segments() {
        assert_eq!(classify_segment_kind("本人声明所呈交的学位论文由本人承担"), "appendix");
        assert_eq!(classify_segment_kind("张三. 某某研究[J]. 城市建筑，2025"), "reference");
        assert_eq!(classify_segment_kind("本研究围绕乡村民宿设计展开，结合当地情况提出方案。"), "body");
    }
}

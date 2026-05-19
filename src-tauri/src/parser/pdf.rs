use crate::models::ParagraphStyle;
use crate::parser::split_text_to_paragraphs;
use anyhow::Context;
use std::path::Path;

pub fn parse(path: &Path) -> anyhow::Result<Vec<(String, ParagraphStyle)>> {
    let text = pdf_extract::extract_text(path)
        .with_context(|| format!("unable to extract text from {}", path.display()))?;
    Ok(split_text_to_paragraphs(&text))
}

use crate::models::ParagraphStyle;
use crate::parser::{read_file, split_text_to_paragraphs};
use std::path::Path;

pub fn parse(path: &Path) -> anyhow::Result<Vec<(String, ParagraphStyle)>> {
    let text = read_file(path)?;
    Ok(split_text_to_paragraphs(&text))
}

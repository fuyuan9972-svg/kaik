use crate::models::ParagraphStyle;
use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::{fs::File, io::Read, path::Path};
use zip::ZipArchive;

pub fn parse(path: &Path) -> Result<Vec<(String, ParagraphStyle)>> {
    let file = File::open(path).with_context(|| format!("unable to open {}", path.display()))?;
    let mut archive = ZipArchive::new(file).context("unable to read docx zip archive")?;
    let mut document = archive
        .by_name("word/document.xml")
        .context("docx is missing word/document.xml")?;
    let mut xml = String::new();
    document
        .read_to_string(&mut xml)
        .context("unable to read word/document.xml")?;

    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(true);

    let mut paragraphs = Vec::new();
    let mut current_text = String::new();
    let mut style = ParagraphStyle::default();
    let mut in_paragraph = false;
    let mut in_text = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                let name = event.name();
                if name.as_ref() == b"w:p" {
                    in_paragraph = true;
                    current_text.clear();
                    style = ParagraphStyle::default();
                } else if in_paragraph && name.as_ref() == b"w:t" {
                    in_text = true;
                } else if in_paragraph && name.as_ref() == b"w:b" {
                    style.bold = true;
                } else if in_paragraph && name.as_ref() == b"w:i" {
                    style.italic = true;
                } else if in_paragraph && name.as_ref() == b"w:sz" {
                    for attr in event.attributes().flatten() {
                        if attr.key.as_ref() == b"w:val" {
                            if let Ok(value) = std::str::from_utf8(attr.value.as_ref()) {
                                if let Ok(size) = value.parse::<u32>() {
                                    style.font_size = Some(size / 2);
                                }
                            }
                        }
                    }
                }
            }
            Ok(Event::Empty(event)) => {
                let name = event.name();
                if in_paragraph && name.as_ref() == b"w:tab" {
                    current_text.push('\t');
                } else if in_paragraph && name.as_ref() == b"w:br" {
                    current_text.push('\n');
                } else if in_paragraph && name.as_ref() == b"w:b" {
                    style.bold = true;
                } else if in_paragraph && name.as_ref() == b"w:i" {
                    style.italic = true;
                } else if in_paragraph && name.as_ref() == b"w:sz" {
                    for attr in event.attributes().flatten() {
                        if attr.key.as_ref() == b"w:val" {
                            if let Ok(value) = std::str::from_utf8(attr.value.as_ref()) {
                                if let Ok(size) = value.parse::<u32>() {
                                    style.font_size = Some(size / 2);
                                }
                            }
                        }
                    }
                }
            }
            Ok(Event::Text(text)) if in_text => {
                current_text.push_str(&text.unescape().unwrap_or_default());
            }
            Ok(Event::End(event)) => {
                let name = event.name();
                if name.as_ref() == b"w:t" {
                    in_text = false;
                } else if name.as_ref() == b"w:p" {
                    in_paragraph = false;
                    let trimmed = current_text.trim();
                    if !trimmed.is_empty() {
                        paragraphs.push((trimmed.to_string(), style.clone()));
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(error).context("unable to parse document.xml"),
            _ => {}
        }
    }

    Ok(paragraphs)
}

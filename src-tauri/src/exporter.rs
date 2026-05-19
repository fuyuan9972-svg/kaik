use crate::models::{Paragraph as SourceParagraph, RewriteResult};
use anyhow::Context;
use docx_rs::{Docx, Paragraph, Run};
use std::collections::HashMap;
use std::{fs::File, path::Path};

pub fn export_docx(results: Vec<RewriteResult>, output_path: String) -> anyhow::Result<String> {
    let mut doc = Docx::new();

    for result in results {
        let text = if result.accepted {
            result.rewritten
        } else {
            result.original
        };
        let mut run = Run::new().add_text(text);
        if result.style.bold {
            run = run.bold();
        }
        if result.style.italic {
            run = run.italic();
        }
        if let Some(size) = result.style.font_size {
            run = run.size((size * 2) as usize);
        }
        doc = doc.add_paragraph(Paragraph::new().add_run(run));
    }

    let path = Path::new(&output_path);
    let file =
        File::create(path).with_context(|| format!("unable to create {}", path.display()))?;
    doc.build()
        .pack(file)
        .with_context(|| format!("unable to write {}", path.display()))?;
    Ok(output_path)
}

pub fn export_full_docx(
    paragraphs: Vec<SourceParagraph>,
    results: Vec<RewriteResult>,
    output_path: String,
) -> anyhow::Result<String> {
    let by_index: HashMap<usize, RewriteResult> = results
        .into_iter()
        .map(|result| (result.index, result))
        .collect();
    let mut doc = Docx::new();

    for paragraph in paragraphs {
        let (text, style) = if let Some(result) = by_index.get(&paragraph.index) {
            let text = if result.accepted {
                result.rewritten.clone()
            } else {
                result.original.clone()
            };
            (text, result.style.clone())
        } else {
            (paragraph.text, paragraph.style)
        };

        let mut run = Run::new().add_text(text);
        if style.bold {
            run = run.bold();
        }
        if style.italic {
            run = run.italic();
        }
        if let Some(size) = style.font_size {
            run = run.size((size * 2) as usize);
        }
        doc = doc.add_paragraph(Paragraph::new().add_run(run));
    }

    let path = Path::new(&output_path);
    let file =
        File::create(path).with_context(|| format!("unable to create {}", path.display()))?;
    doc.build()
        .pack(file)
        .with_context(|| format!("unable to write {}", path.display()))?;
    Ok(output_path)
}

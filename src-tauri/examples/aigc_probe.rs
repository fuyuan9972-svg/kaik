use ai_paper_rewriter_lib::aigc_detector;

fn main() -> anyhow::Result<()> {
    for file_path in std::env::args().skip(1) {
        let analysis = aigc_detector::analyze_file(&file_path, &[])?;
        println!(
            "{}\t{:.1}%\t{}-{}\t{}\t{}\tbody={}\tcjk={}\tpara={:.1}\tsent={:.1}\tpunct={:.2}\tai={:.2}\tbuffer={:.2}\tplain={:.2}\tconnect={:.2}",
            analysis.file_name,
            analysis.estimated_aigc,
            analysis.range_low,
            analysis.range_high,
            analysis.profile,
            analysis.risk_level,
            analysis.metrics.body_paragraphs,
            analysis.metrics.cjk_chars,
            analysis.metrics.avg_paragraph_len,
            analysis.metrics.avg_sentence_len,
            analysis.metrics.punctuation_per_100,
            analysis.metrics.ai_terms_per_10k,
            analysis.metrics.buffer_terms_per_10k,
            analysis.metrics.plain_terms_per_10k,
            analysis.metrics.connectors_per_10k
        );
    }
    Ok(())
}

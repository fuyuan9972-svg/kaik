mod ai_aigc_detector;
pub mod aigc_detector;
mod commands;
mod config;
mod exporter;
mod models;
mod parser;
mod rewriter;

pub fn run() {
    tauri::Builder::default()
        .manage(commands::RewriteCancelState::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::parse_file,
            commands::estimate_rewrite_scope,
            commands::rewrite_paragraphs,
            commands::cancel_rewrite,
            commands::test_connection,
            commands::export_docx,
            commands::save_config,
            commands::load_config,
            commands::save_results,
            commands::load_results,
            commands::clear_results,
            commands::load_sessions,
            commands::save_session,
            commands::delete_session,
            commands::export_session_docx,
            commands::analyze_aigc_file,
            commands::analyze_aigc_file_ai,
            commands::load_aigc_calibrations,
            commands::save_aigc_calibration,
            commands::delete_aigc_calibration
        ])
        .run(tauri::generate_context!())
        .expect("failed to run tauri app");
}

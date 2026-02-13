#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod providers;
mod storage;
mod types;
mod util;

use std::sync::Mutex;
use types::DebateLock;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(DebateLock(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            commands::core::resolve_central_home,
            commands::core::list_records,
            commands::core::list_logs,
            commands::core::get_dashboard_stats,
            commands::core::upsert_record,
            commands::core::delete_record,
            commands::search::rebuild_search_index,
            commands::search::search_records,
            commands::ai::run_ai_analysis,
            commands::debate::run_debate_mode,
            commands::debate::replay_debate_mode,
            commands::debate::list_debate_runs,
            commands::export::export_markdown_report,
            commands::health::get_home_fingerprint,
            commands::health::get_health_diagnostics,
            commands::settings::get_app_settings,
            commands::settings::save_app_settings,
            commands::keychain::set_openai_api_key,
            commands::keychain::has_openai_api_key,
            commands::keychain::clear_openai_api_key,
            commands::keychain::set_gemini_api_key,
            commands::keychain::has_gemini_api_key,
            commands::keychain::clear_gemini_api_key,
            commands::keychain::set_claude_api_key,
            commands::keychain::has_claude_api_key,
            commands::keychain::clear_claude_api_key,
            commands::keychain::set_notion_api_key,
            commands::keychain::has_notion_api_key,
            commands::keychain::clear_notion_api_key,
            commands::notion::sync_record_to_notion,
            commands::notion::sync_records_to_notion,
            commands::notion::sync_record_bidirectional,
            commands::notion::sync_records_bidirectional,
            commands::notion::pull_records_from_notion,
            commands::notebooklm::notebooklm_health_check,
            commands::notebooklm::notebooklm_list_notebooks,
            commands::notebooklm::notebooklm_create_notebook,
            commands::notebooklm::notebooklm_add_record_source,
            commands::notebooklm::notebooklm_ask,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

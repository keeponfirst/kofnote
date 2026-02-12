use crate::types::{NotebookLmAskResult, NotebookLmConfig, NotebookSummary};
use serde_json::Value;

#[tauri::command]
pub fn notebooklm_health_check(config: Option<NotebookLmConfig>) -> Result<Value, String> {
    crate::types::notebooklm_health_check(config)
}

#[tauri::command]
pub fn notebooklm_list_notebooks(
    limit: Option<usize>,
    config: Option<NotebookLmConfig>,
) -> Result<Vec<NotebookSummary>, String> {
    crate::types::notebooklm_list_notebooks(limit, config)
}

#[tauri::command]
pub fn notebooklm_create_notebook(
    title: Option<String>,
    config: Option<NotebookLmConfig>,
) -> Result<NotebookSummary, String> {
    crate::types::notebooklm_create_notebook(title, config)
}

#[tauri::command]
pub fn notebooklm_add_record_source(
    central_home: String,
    json_path: String,
    notebook_id: String,
    title: Option<String>,
    config: Option<NotebookLmConfig>,
) -> Result<Value, String> {
    crate::types::notebooklm_add_record_source(central_home, json_path, notebook_id, title, config)
}

#[tauri::command]
pub fn notebooklm_ask(
    notebook_id: String,
    question: String,
    include_citations: Option<bool>,
    config: Option<NotebookLmConfig>,
) -> Result<NotebookLmAskResult, String> {
    crate::types::notebooklm_ask(notebook_id, question, include_citations, config)
}

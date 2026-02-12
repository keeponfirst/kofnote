use crate::types::{NotionBatchSyncResult, NotionSyncResult};

#[tauri::command]
pub fn sync_record_to_notion(
    central_home: String,
    json_path: String,
    database_id: Option<String>,
    conflict_strategy: Option<String>,
) -> Result<NotionSyncResult, String> {
    crate::types::sync_record_to_notion(central_home, json_path, database_id, conflict_strategy)
}

#[tauri::command]
pub fn sync_records_to_notion(
    central_home: String,
    json_paths: Vec<String>,
    database_id: Option<String>,
    conflict_strategy: Option<String>,
) -> Result<NotionBatchSyncResult, String> {
    crate::types::sync_records_to_notion(central_home, json_paths, database_id, conflict_strategy)
}

#[tauri::command]
pub fn sync_record_bidirectional(
    central_home: String,
    json_path: String,
    database_id: Option<String>,
    conflict_strategy: Option<String>,
) -> Result<NotionSyncResult, String> {
    crate::types::sync_record_bidirectional(central_home, json_path, database_id, conflict_strategy)
}

#[tauri::command]
pub fn sync_records_bidirectional(
    central_home: String,
    json_paths: Vec<String>,
    database_id: Option<String>,
    conflict_strategy: Option<String>,
) -> Result<NotionBatchSyncResult, String> {
    crate::types::sync_records_bidirectional(central_home, json_paths, database_id, conflict_strategy)
}

#[tauri::command]
pub fn pull_records_from_notion(
    central_home: String,
    database_id: Option<String>,
    conflict_strategy: Option<String>,
) -> Result<NotionBatchSyncResult, String> {
    crate::types::pull_records_from_notion(central_home, database_id, conflict_strategy)
}

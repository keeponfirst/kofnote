use crate::types::{DashboardStats, LogEntry, Record, RecordPayload, ResolvedHome};

#[tauri::command]
pub fn resolve_central_home(input_path: String) -> Result<ResolvedHome, String> {
    crate::types::resolve_central_home(input_path)
}

#[tauri::command]
pub fn list_records(central_home: String) -> Result<Vec<Record>, String> {
    crate::types::list_records(central_home)
}

#[tauri::command]
pub fn list_logs(central_home: String) -> Result<Vec<LogEntry>, String> {
    crate::types::list_logs(central_home)
}

#[tauri::command]
pub fn get_dashboard_stats(central_home: String) -> Result<DashboardStats, String> {
    crate::types::get_dashboard_stats(central_home)
}

#[tauri::command]
pub fn upsert_record(
    central_home: String,
    payload: RecordPayload,
    previous_json_path: Option<String>,
) -> Result<Record, String> {
    crate::types::upsert_record(central_home, payload, previous_json_path)
}

#[tauri::command]
pub fn delete_record(central_home: String, json_path: String) -> Result<(), String> {
    crate::types::delete_record(central_home, json_path)
}

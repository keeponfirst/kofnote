use crate::commands::keychain::{
    has_claude_api_key_internal, has_gemini_api_key_internal, has_openai_api_key_internal,
};
use crate::storage::index::{get_index_count, index_db_path};
use crate::storage::records::{load_logs, load_records, normalized_home};
use crate::storage::settings_io::load_settings;
use crate::types::{HealthDiagnostics, HomeFingerprint};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[tauri::command]
pub fn get_home_fingerprint(central_home: String) -> Result<HomeFingerprint, String> {
    let home = normalized_home(&central_home)?;
    let records = load_records(&home)?;
    let logs = load_logs(&home)?;

    let latest_record_at = records.first().map(|r| r.created_at.clone()).unwrap_or_default();
    let latest_log_at = logs.first().map(|l| l.timestamp.clone()).unwrap_or_default();

    let mut hasher = DefaultHasher::new();
    home.to_string_lossy().hash(&mut hasher);
    latest_record_at.hash(&mut hasher);
    latest_log_at.hash(&mut hasher);
    records.len().hash(&mut hasher);
    logs.len().hash(&mut hasher);
    for r in records.iter().take(12) {
        r.title.hash(&mut hasher);
        r.created_at.hash(&mut hasher);
        r.record_type.hash(&mut hasher);
    }
    for l in logs.iter().take(12) {
        l.task_intent.hash(&mut hasher);
        l.timestamp.hash(&mut hasher);
    }

    Ok(HomeFingerprint {
        token: format!("{:x}", hasher.finish()),
        records_count: records.len(),
        logs_count: logs.len(),
        latest_record_at,
        latest_log_at,
    })
}

#[tauri::command]
pub fn get_health_diagnostics(central_home: String) -> Result<HealthDiagnostics, String> {
    let home = normalized_home(&central_home)?;
    let records = load_records(&home)?;
    let logs = load_logs(&home)?;

    let index_path = index_db_path(&home);
    let index_exists = index_path.exists();
    let indexed_records = if index_exists {
        get_index_count(&home).unwrap_or(0)
    } else {
        0
    };

    let settings = load_settings();

    Ok(HealthDiagnostics {
        central_home: home.to_string_lossy().to_string(),
        records_count: records.len(),
        logs_count: logs.len(),
        index_path: index_path.to_string_lossy().to_string(),
        index_exists,
        indexed_records,
        latest_record_at: records.first().map(|r| r.created_at.clone()).unwrap_or_default(),
        latest_log_at: logs.first().map(|l| l.timestamp.clone()).unwrap_or_default(),
        has_openai_api_key: has_openai_api_key_internal().unwrap_or(false),
        has_gemini_api_key: has_gemini_api_key_internal().unwrap_or(false),
        has_claude_api_key: has_claude_api_key_internal().unwrap_or(false),
        profile_count: settings.profiles.len(),
    })
}

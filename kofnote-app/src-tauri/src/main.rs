#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use chrono::{DateTime, Duration as ChronoDuration, Local, NaiveDate};
use keyring::{Entry, Error as KeyringError};
use reqwest::blocking::Client;
use rusqlite::{params, params_from_iter, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration as StdDuration, Instant};

const RECORD_TYPE_DIRS: [(&str, &str); 5] = [
    ("decision", "decisions"),
    ("worklog", "worklogs"),
    ("idea", "ideas"),
    ("backlog", "backlogs"),
    ("note", "other"),
];

const OPENAI_SERVICE: &str = "com.keeponfirst.kofnote";
const OPENAI_USERNAME: &str = "openai_api_key";
const GEMINI_USERNAME: &str = "gemini_api_key";
const CLAUDE_USERNAME: &str = "claude_api_key";
const OPENAI_RESPONSES_URL: &str = "https://api.openai.com/v1/responses";
const NOTION_USERNAME: &str = "notion_api_key";
const NOTION_API_VERSION: &str = "2022-06-28";
const NOTION_API_BASE_URL: &str = "https://api.notion.com/v1";
const SETTINGS_DIR_NAME: &str = "kofnote-desktop-tauri";
const SETTINGS_FILE_NAME: &str = "settings.json";
const SEARCH_DB_FILE: &str = "kofnote_search.sqlite";
const DEFAULT_NOTEBOOKLM_COMMAND: &str = "uvx";
const DEFAULT_NOTEBOOKLM_ARGS: [&str; 1] = ["kof-notebooklm-mcp"];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolvedHome {
    central_home: String,
    corrected: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Record {
    record_type: String,
    title: String,
    created_at: String,
    source_text: String,
    final_body: String,
    tags: Vec<String>,
    date: Option<String>,
    notion_page_id: Option<String>,
    notion_url: Option<String>,
    notion_sync_status: String,
    notion_error: Option<String>,
    notion_last_synced_at: Option<String>,
    notion_last_edited_time: Option<String>,
    notion_last_synced_hash: Option<String>,
    json_path: Option<String>,
    md_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordPayload {
    record_type: String,
    title: String,
    created_at: Option<String>,
    source_text: Option<String>,
    final_body: Option<String>,
    tags: Option<Vec<String>>,
    date: Option<String>,
    notion_page_id: Option<String>,
    notion_url: Option<String>,
    notion_sync_status: Option<String>,
    notion_error: Option<String>,
    notion_last_synced_at: Option<String>,
    notion_last_edited_time: Option<String>,
    notion_last_synced_hash: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogEntry {
    timestamp: String,
    event_id: String,
    task_intent: String,
    status: String,
    title: String,
    data: Value,
    raw: Value,
    json_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TagCount {
    tag: String,
    count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DailyCount {
    date: String,
    count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardStats {
    total_records: usize,
    total_logs: usize,
    type_counts: HashMap<String, usize>,
    top_tags: Vec<TagCount>,
    recent_daily_counts: Vec<DailyCount>,
    pending_sync_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchResult {
    records: Vec<Record>,
    total: usize,
    indexed: bool,
    took_ms: u128,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RebuildIndexResult {
    indexed_count: usize,
    index_path: String,
    took_ms: u128,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiAnalysisResponse {
    provider: String,
    model: String,
    content: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct WorkspaceProfile {
    id: String,
    name: String,
    central_home: String,
    default_provider: String,
    default_model: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct NotionSettings {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    database_id: String,
}

impl Default for NotionSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            database_id: String::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct NotebookLmSettings {
    #[serde(default = "default_notebooklm_command")]
    command: String,
    #[serde(default = "default_notebooklm_args")]
    args: Vec<String>,
    #[serde(default)]
    default_notebook_id: Option<String>,
}

impl Default for NotebookLmSettings {
    fn default() -> Self {
        Self {
            command: default_notebooklm_command(),
            args: default_notebooklm_args(),
            default_notebook_id: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct IntegrationsSettings {
    #[serde(default)]
    notion: NotionSettings,
    #[serde(default)]
    notebooklm: NotebookLmSettings,
}

impl Default for IntegrationsSettings {
    fn default() -> Self {
        Self {
            notion: NotionSettings::default(),
            notebooklm: NotebookLmSettings::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    #[serde(default)]
    profiles: Vec<WorkspaceProfile>,
    #[serde(default)]
    active_profile_id: Option<String>,
    #[serde(default = "default_poll_interval")]
    poll_interval_sec: u64,
    #[serde(default)]
    ui_preferences: Value,
    #[serde(default)]
    integrations: IntegrationsSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            profiles: Vec::new(),
            active_profile_id: None,
            poll_interval_sec: default_poll_interval(),
            ui_preferences: json!({}),
            integrations: IntegrationsSettings::default(),
        }
    }
}

fn default_poll_interval() -> u64 {
    8
}

fn default_notebooklm_command() -> String {
    DEFAULT_NOTEBOOKLM_COMMAND.to_string()
}

fn default_notebooklm_args() -> Vec<String> {
    DEFAULT_NOTEBOOKLM_ARGS.iter().map(|item| item.to_string()).collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportReportResult {
    output_path: String,
    title: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthDiagnostics {
    central_home: String,
    records_count: usize,
    logs_count: usize,
    index_path: String,
    index_exists: bool,
    indexed_records: usize,
    latest_record_at: String,
    latest_log_at: String,
    has_openai_api_key: bool,
    has_gemini_api_key: bool,
    has_claude_api_key: bool,
    profile_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HomeFingerprint {
    token: String,
    records_count: usize,
    logs_count: usize,
    latest_record_at: String,
    latest_log_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NotionSyncResult {
    json_path: String,
    notion_page_id: Option<String>,
    notion_url: Option<String>,
    notion_sync_status: String,
    notion_error: Option<String>,
    action: String,
    conflict: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NotionBatchSyncResult {
    total: usize,
    success: usize,
    failed: usize,
    conflicts: usize,
    results: Vec<NotionSyncResult>,
}

#[derive(Debug, Clone)]
struct NotionRemoteRecord {
    page_id: String,
    page_url: Option<String>,
    last_edited_time: Option<String>,
    record_type: String,
    title: String,
    created_at: String,
    date: Option<String>,
    tags: Vec<String>,
    final_body: String,
    source_text: String,
}

#[derive(Debug)]
struct NotionUpsertInfo {
    page_id: String,
    page_url: Option<String>,
    last_edited_time: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NotebookLmConfig {
    command: Option<String>,
    args: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NotebookSummary {
    id: String,
    name: String,
    source_count: Option<usize>,
    updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NotebookLmAskResult {
    answer: String,
    citations: Vec<String>,
}

#[tauri::command]
fn resolve_central_home(input_path: String) -> Result<ResolvedHome, String> {
    if input_path.trim().is_empty() {
        return Err("Central Home path is required".to_string());
    }

    let input = absolute_path(Path::new(input_path.trim()));
    let resolved = detect_central_home_path(&input);
    ensure_structure(&resolved).map_err(|error| error.to_string())?;

    Ok(ResolvedHome {
        central_home: resolved.to_string_lossy().to_string(),
        corrected: resolved != input,
    })
}

#[tauri::command]
fn list_records(central_home: String) -> Result<Vec<Record>, String> {
    let home = normalized_home(&central_home)?;
    load_records(&home)
}

#[tauri::command]
fn list_logs(central_home: String) -> Result<Vec<LogEntry>, String> {
    let home = normalized_home(&central_home)?;
    load_logs(&home)
}

#[tauri::command]
fn get_dashboard_stats(central_home: String) -> Result<DashboardStats, String> {
    let home = normalized_home(&central_home)?;
    let records = load_records(&home)?;
    let logs = load_logs(&home)?;
    Ok(compute_dashboard_stats(&records, &logs))
}

#[tauri::command]
fn upsert_record(
    central_home: String,
    payload: RecordPayload,
    previous_json_path: Option<String>,
) -> Result<Record, String> {
    let home = normalized_home(&central_home)?;
    ensure_structure(&home).map_err(|error| error.to_string())?;

    let record_type = normalize_record_type(&payload.record_type);
    let created_at = payload
        .created_at
        .unwrap_or_else(|| Local::now().to_rfc3339());
    let title = if payload.title.trim().is_empty() {
        "Untitled".to_string()
    } else {
        payload.title.trim().to_string()
    };

    let target_subdir = record_dir_by_type(&record_type);
    let target_dir = home.join("records").join(target_subdir);
    fs::create_dir_all(&target_dir).map_err(|error| error.to_string())?;

    let existing_path = previous_json_path
        .as_ref()
        .map(|path| absolute_path(Path::new(path.trim())));
    let existing_value = existing_path.as_ref().and_then(|path| {
        if !path.exists() {
            return None;
        }
        fs::read_to_string(path)
            .ok()
            .and_then(|content| serde_json::from_str::<Value>(&content).ok())
    });

    let base_name = if let Some(path) = &existing_path {
        if path.exists() {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_else(|| generate_filename(&record_type, &title))
        } else {
            generate_filename(&record_type, &title)
        }
    } else {
        generate_filename(&record_type, &title)
    };

    let json_path = target_dir.join(format!("{base_name}.json"));
    let md_path = target_dir.join(format!("{base_name}.md"));

    let tags = payload.tags.unwrap_or_default();
    let notion_sync_status = payload
        .notion_sync_status
        .unwrap_or_else(|| "SUCCESS".to_string());
    let source_text = payload.source_text.unwrap_or_default();
    let final_body = payload.final_body.unwrap_or_default();
    let notion_last_synced_at = payload
        .notion_last_synced_at
        .or_else(|| existing_value.as_ref().and_then(|value| value_string(value, "notion_last_synced_at")));
    let notion_last_edited_time = payload
        .notion_last_edited_time
        .or_else(|| existing_value.as_ref().and_then(|value| value_string(value, "notion_last_edited_time")));
    let notion_last_synced_hash = payload
        .notion_last_synced_hash
        .or_else(|| existing_value.as_ref().and_then(|value| value_string(value, "notion_last_synced_hash")));

    let persisted = json!({
        "type": record_type,
        "title": title,
        "created_at": created_at,
        "notion_page_id": payload.notion_page_id,
        "notion_url": payload.notion_url,
        "source_text": source_text,
        "final_body": final_body,
        "tags": tags,
        "date": payload.date,
        "notion_sync_status": notion_sync_status,
        "notion_error": payload.notion_error,
        "notion_last_synced_at": notion_last_synced_at,
        "notion_last_edited_time": notion_last_edited_time,
        "notion_last_synced_hash": notion_last_synced_hash,
    });

    let json_bytes = serde_json::to_vec_pretty(&persisted).map_err(|error| error.to_string())?;
    write_atomic(&json_path, &json_bytes).map_err(|error| error.to_string())?;

    let record = record_from_value(&persisted, Some(json_path.clone()), Some(md_path.clone()), None);
    let markdown = render_markdown(&record);
    write_atomic(&md_path, markdown.as_bytes()).map_err(|error| error.to_string())?;

    if let Some(old_path) = existing_path {
        if old_path != json_path {
            let old_md = old_path.with_extension("md");
            let _ = fs::remove_file(&old_path);
            let _ = fs::remove_file(old_md);
            let _ = delete_index_record_if_exists(&home, &old_path.to_string_lossy());
        }
    }

    let _ = upsert_index_record_if_exists(&home, &record);
    Ok(record)
}

#[tauri::command]
fn delete_record(central_home: String, json_path: String) -> Result<(), String> {
    let home = normalized_home(&central_home)?;
    let path = absolute_path(Path::new(json_path.trim()));
    if path.exists() {
        fs::remove_file(&path).map_err(|error| error.to_string())?;
    }
    let md_path = path.with_extension("md");
    if md_path.exists() {
        fs::remove_file(md_path).map_err(|error| error.to_string())?;
    }

    let _ = delete_index_record_if_exists(&home, &path.to_string_lossy());
    Ok(())
}

#[tauri::command]
fn rebuild_search_index(central_home: String) -> Result<RebuildIndexResult, String> {
    let started = Instant::now();
    let home = normalized_home(&central_home)?;
    let records = load_records(&home)?;
    let indexed_count = rebuild_index(&home, &records)?;

    Ok(RebuildIndexResult {
        indexed_count,
        index_path: index_db_path(&home).to_string_lossy().to_string(),
        took_ms: started.elapsed().as_millis(),
    })
}

#[tauri::command]
fn search_records(
    central_home: String,
    query: Option<String>,
    record_type: Option<String>,
    date_from: Option<String>,
    date_to: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<SearchResult, String> {
    let started = Instant::now();
    let home = normalized_home(&central_home)?;
    let limit = limit.unwrap_or(200).clamp(1, 1000);
    let offset = offset.unwrap_or(0);

    let query_text = query.unwrap_or_default().trim().to_string();
    let record_type = record_type
        .map(|item| normalize_record_type(&item))
        .filter(|item| !item.is_empty());
    let date_from = sanitize_date_filter(date_from);
    let date_to = sanitize_date_filter(date_to);

    let use_index = !query_text.is_empty();

    let (records, total, indexed) = if use_index {
        if !index_db_path(&home).exists() {
            let _ = rebuild_index(&home, &load_records(&home)?);
        }

        match search_records_in_index(
            &home,
            &query_text,
            record_type.as_deref(),
            date_from.as_deref(),
            date_to.as_deref(),
            limit,
            offset,
        ) {
            Ok(result) => (result.0, result.1, true),
            Err(_) => {
                let all = load_records(&home)?;
                let records = search_records_in_memory(
                    &all,
                    &query_text,
                    record_type.as_deref(),
                    date_from.as_deref(),
                    date_to.as_deref(),
                    limit,
                    offset,
                );
                let total = count_records_in_memory(
                    &all,
                    &query_text,
                    record_type.as_deref(),
                    date_from.as_deref(),
                    date_to.as_deref(),
                );
                (records, total, false)
            }
        }
    } else {
        let all = load_records(&home)?;
        let filtered = search_records_in_memory(
            &all,
            "",
            record_type.as_deref(),
            date_from.as_deref(),
            date_to.as_deref(),
            limit,
            offset,
        );
        let total = count_records_in_memory(&all, "", record_type.as_deref(), date_from.as_deref(), date_to.as_deref());
        (filtered, total, false)
    };

    Ok(SearchResult {
        records,
        total,
        indexed,
        took_ms: started.elapsed().as_millis(),
    })
}

#[tauri::command]
fn run_ai_analysis(
    central_home: String,
    provider: Option<String>,
    model: Option<String>,
    prompt: String,
    api_key: Option<String>,
    include_logs: Option<bool>,
    max_records: Option<usize>,
) -> Result<AiAnalysisResponse, String> {
    let home = normalized_home(&central_home)?;
    let provider = provider
        .unwrap_or_else(|| "local".to_string())
        .trim()
        .to_lowercase();
    let model = model.unwrap_or_else(|| "gpt-4.1-mini".to_string());

    let records = load_records(&home)?;
    let logs = load_logs(&home)?;
    let include_logs = include_logs.unwrap_or(true);
    let max_records = max_records.unwrap_or(30).clamp(1, 200);

    let content = match provider.as_str() {
        "openai" => run_openai_analysis(&model, &prompt, api_key, &records, &logs, include_logs, max_records)?,
        "local" => run_local_analysis(&prompt, &records, &logs),
        _ => return Err(format!("Unsupported provider: {provider}")),
    };

    Ok(AiAnalysisResponse {
        provider,
        model,
        content,
    })
}

#[tauri::command]
fn export_markdown_report(
    central_home: String,
    output_path: Option<String>,
    title: Option<String>,
    recent_days: Option<i64>,
) -> Result<ExportReportResult, String> {
    let home = normalized_home(&central_home)?;
    let records = load_records(&home)?;
    let logs = load_logs(&home)?;
    let stats = compute_dashboard_stats(&records, &logs);

    let now = Local::now();
    let title = title.unwrap_or_else(|| format!("KOF Report {}", now.format("%Y-%m-%d")));
    let days = recent_days.unwrap_or(7).clamp(1, 365);

    let cutoff = now.date_naive() - ChronoDuration::days(days);

    let recent_records: Vec<&Record> = records
        .iter()
        .filter(|item| {
            extract_day(&item.created_at)
                .and_then(|day| NaiveDate::parse_from_str(&day, "%Y-%m-%d").ok())
                .map(|date| date >= cutoff)
                .unwrap_or(false)
        })
        .take(80)
        .collect();

    let report_md = render_report_markdown(&title, &home, &stats, &recent_records, days);

    let target = if let Some(path) = output_path {
        absolute_path(Path::new(path.trim()))
    } else {
        let report_dir = home.join("assets").join("reports");
        fs::create_dir_all(&report_dir).map_err(|error| error.to_string())?;
        report_dir.join(format!(
            "{}_kof-report.md",
            now.format("%Y%m%d_%H%M%S")
        ))
    };

    write_atomic(&target, report_md.as_bytes()).map_err(|error| error.to_string())?;

    Ok(ExportReportResult {
        output_path: target.to_string_lossy().to_string(),
        title,
    })
}

#[tauri::command]
fn get_home_fingerprint(central_home: String) -> Result<HomeFingerprint, String> {
    let home = normalized_home(&central_home)?;
    let records = load_records(&home)?;
    let logs = load_logs(&home)?;

    let latest_record_at = records.first().map(|item| item.created_at.clone()).unwrap_or_default();
    let latest_log_at = logs.first().map(|item| item.timestamp.clone()).unwrap_or_default();

    let mut hasher = DefaultHasher::new();
    home.to_string_lossy().hash(&mut hasher);
    latest_record_at.hash(&mut hasher);
    latest_log_at.hash(&mut hasher);
    records.len().hash(&mut hasher);
    logs.len().hash(&mut hasher);

    for item in records.iter().take(12) {
        item.title.hash(&mut hasher);
        item.created_at.hash(&mut hasher);
        item.record_type.hash(&mut hasher);
    }

    for item in logs.iter().take(12) {
        item.task_intent.hash(&mut hasher);
        item.timestamp.hash(&mut hasher);
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
fn get_health_diagnostics(central_home: String) -> Result<HealthDiagnostics, String> {
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
        latest_record_at: records.first().map(|item| item.created_at.clone()).unwrap_or_default(),
        latest_log_at: logs.first().map(|item| item.timestamp.clone()).unwrap_or_default(),
        has_openai_api_key: has_openai_api_key_internal().unwrap_or(false),
        has_gemini_api_key: has_gemini_api_key_internal().unwrap_or(false),
        has_claude_api_key: has_claude_api_key_internal().unwrap_or(false),
        profile_count: settings.profiles.len(),
    })
}

#[tauri::command]
fn get_app_settings() -> Result<AppSettings, String> {
    Ok(load_settings())
}

#[tauri::command]
fn save_app_settings(settings: AppSettings) -> Result<AppSettings, String> {
    let normalized = normalize_settings(settings);
    save_settings(&normalized)?;
    Ok(normalized)
}

#[tauri::command]
fn set_openai_api_key(api_key: String) -> Result<bool, String> {
    if api_key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }

    let entry = keyring_entry()?;
    entry
        .set_password(api_key.trim())
        .map_err(|error| error.to_string())?;
    Ok(true)
}

#[tauri::command]
fn has_openai_api_key() -> Result<bool, String> {
    has_openai_api_key_internal()
}

#[tauri::command]
fn clear_openai_api_key() -> Result<bool, String> {
    let entry = keyring_entry()?;
    match entry.delete_password() {
        Ok(_) => Ok(true),
        Err(KeyringError::NoEntry) => Ok(true),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
fn set_gemini_api_key(api_key: String) -> Result<bool, String> {
    if api_key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }

    let entry = gemini_keyring_entry()?;
    entry
        .set_password(api_key.trim())
        .map_err(|error| error.to_string())?;
    Ok(true)
}

#[tauri::command]
fn has_gemini_api_key() -> Result<bool, String> {
    has_gemini_api_key_internal()
}

#[tauri::command]
fn clear_gemini_api_key() -> Result<bool, String> {
    let entry = gemini_keyring_entry()?;
    match entry.delete_password() {
        Ok(_) => Ok(true),
        Err(KeyringError::NoEntry) => Ok(true),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
fn set_claude_api_key(api_key: String) -> Result<bool, String> {
    if api_key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }

    let entry = claude_keyring_entry()?;
    entry
        .set_password(api_key.trim())
        .map_err(|error| error.to_string())?;
    Ok(true)
}

#[tauri::command]
fn has_claude_api_key() -> Result<bool, String> {
    has_claude_api_key_internal()
}

#[tauri::command]
fn clear_claude_api_key() -> Result<bool, String> {
    let entry = claude_keyring_entry()?;
    match entry.delete_password() {
        Ok(_) => Ok(true),
        Err(KeyringError::NoEntry) => Ok(true),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
fn set_notion_api_key(api_key: String) -> Result<bool, String> {
    if api_key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }

    let entry = notion_keyring_entry()?;
    entry
        .set_password(api_key.trim())
        .map_err(|error| error.to_string())?;
    Ok(true)
}

#[tauri::command]
fn has_notion_api_key() -> Result<bool, String> {
    has_notion_api_key_internal()
}

#[tauri::command]
fn clear_notion_api_key() -> Result<bool, String> {
    let entry = notion_keyring_entry()?;
    match entry.delete_password() {
        Ok(_) => Ok(true),
        Err(KeyringError::NoEntry) => Ok(true),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
fn sync_record_to_notion(
    central_home: String,
    json_path: String,
    database_id: Option<String>,
    conflict_strategy: Option<String>,
) -> Result<NotionSyncResult, String> {
    let home = normalized_home(&central_home)?;
    let settings = load_settings();
    let db_id = resolve_notion_database_id(database_id, &settings)?;
    let api_key = resolve_notion_api_key(None)?;
    let strategy = normalize_conflict_strategy(conflict_strategy);
    sync_record_to_notion_internal(&home, &json_path, &db_id, &api_key, &strategy)
}

#[tauri::command]
fn sync_records_to_notion(
    central_home: String,
    json_paths: Vec<String>,
    database_id: Option<String>,
    conflict_strategy: Option<String>,
) -> Result<NotionBatchSyncResult, String> {
    let home = normalized_home(&central_home)?;
    let settings = load_settings();
    let db_id = resolve_notion_database_id(database_id, &settings)?;
    let api_key = resolve_notion_api_key(None)?;
    let strategy = normalize_conflict_strategy(conflict_strategy);

    let mut results: Vec<NotionSyncResult> = Vec::new();
    let mut success = 0usize;
    let mut failed = 0usize;
    let mut conflicts = 0usize;

    for item in json_paths {
        match sync_record_to_notion_internal(&home, &item, &db_id, &api_key, &strategy) {
            Ok(result) => {
                if result.conflict {
                    conflicts += 1;
                    failed += 1;
                } else if result.notion_sync_status == "SUCCESS" {
                    success += 1;
                } else {
                    failed += 1;
                }
                results.push(result);
            }
            Err(error) => {
                failed += 1;
                results.push(NotionSyncResult {
                    json_path: item,
                    notion_page_id: None,
                    notion_url: None,
                    notion_sync_status: "FAILED".to_string(),
                    notion_error: Some(error),
                    action: "error".to_string(),
                    conflict: false,
                });
            }
        }
    }

    Ok(NotionBatchSyncResult {
        total: results.len(),
        success,
        failed,
        conflicts,
        results,
    })
}

#[tauri::command]
fn sync_record_bidirectional(
    central_home: String,
    json_path: String,
    database_id: Option<String>,
    conflict_strategy: Option<String>,
) -> Result<NotionSyncResult, String> {
    let home = normalized_home(&central_home)?;
    let settings = load_settings();
    let db_id = resolve_notion_database_id(database_id, &settings)?;
    let api_key = resolve_notion_api_key(None)?;
    let strategy = normalize_conflict_strategy(conflict_strategy);
    sync_record_bidirectional_internal(&home, &json_path, &db_id, &api_key, &strategy)
}

#[tauri::command]
fn sync_records_bidirectional(
    central_home: String,
    json_paths: Vec<String>,
    database_id: Option<String>,
    conflict_strategy: Option<String>,
) -> Result<NotionBatchSyncResult, String> {
    let home = normalized_home(&central_home)?;
    let settings = load_settings();
    let db_id = resolve_notion_database_id(database_id, &settings)?;
    let api_key = resolve_notion_api_key(None)?;
    let strategy = normalize_conflict_strategy(conflict_strategy);

    let mut results: Vec<NotionSyncResult> = Vec::new();
    let mut success = 0usize;
    let mut failed = 0usize;
    let mut conflicts = 0usize;

    for item in json_paths {
        match sync_record_bidirectional_internal(&home, &item, &db_id, &api_key, &strategy) {
            Ok(result) => {
                if result.conflict {
                    conflicts += 1;
                    failed += 1;
                } else if result.notion_sync_status == "SUCCESS" {
                    success += 1;
                } else {
                    failed += 1;
                }
                results.push(result);
            }
            Err(error) => {
                failed += 1;
                results.push(NotionSyncResult {
                    json_path: item,
                    notion_page_id: None,
                    notion_url: None,
                    notion_sync_status: "FAILED".to_string(),
                    notion_error: Some(error),
                    action: "error".to_string(),
                    conflict: false,
                });
            }
        }
    }

    Ok(NotionBatchSyncResult {
        total: results.len(),
        success,
        failed,
        conflicts,
        results,
    })
}

#[tauri::command]
fn pull_records_from_notion(
    central_home: String,
    database_id: Option<String>,
    conflict_strategy: Option<String>,
) -> Result<NotionBatchSyncResult, String> {
    let home = normalized_home(&central_home)?;
    let settings = load_settings();
    let db_id = resolve_notion_database_id(database_id, &settings)?;
    let api_key = resolve_notion_api_key(None)?;
    let strategy = normalize_conflict_strategy(conflict_strategy);
    pull_records_from_notion_internal(&home, &db_id, &api_key, &strategy)
}

#[tauri::command]
fn notebooklm_health_check(config: Option<NotebookLmConfig>) -> Result<Value, String> {
    notebooklm_call_tool("health_check", json!({}), config)
}

#[tauri::command]
fn notebooklm_list_notebooks(
    limit: Option<usize>,
    config: Option<NotebookLmConfig>,
) -> Result<Vec<NotebookSummary>, String> {
    let payload = notebooklm_call_tool(
        "list_notebooks",
        json!({ "limit": limit.unwrap_or(20).clamp(1, 100) }),
        config,
    )?;
    let notebooks = payload
        .get("notebooks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(notebooks
        .iter()
        .map(parse_notebook_summary)
        .collect::<Vec<_>>())
}

#[tauri::command]
fn notebooklm_create_notebook(
    title: Option<String>,
    config: Option<NotebookLmConfig>,
) -> Result<NotebookSummary, String> {
    let payload = notebooklm_call_tool(
        "create_notebook",
        json!({ "title": title.unwrap_or_else(|| "KOF Note Notebook".to_string()) }),
        config,
    )?;
    let notebook = payload
        .get("notebook")
        .cloned()
        .unwrap_or_else(|| payload.clone());
    Ok(parse_notebook_summary(&notebook))
}

#[tauri::command]
fn notebooklm_add_record_source(
    central_home: String,
    json_path: String,
    notebook_id: String,
    title: Option<String>,
    config: Option<NotebookLmConfig>,
) -> Result<Value, String> {
    let home = normalized_home(&central_home)?;
    let record = load_record_by_json_path(&home, &json_path)?;
    let source_title = title.unwrap_or_else(|| {
        format!(
            "{} | {} | {}",
            record.record_type,
            extract_day(&record.created_at).unwrap_or_else(|| record.created_at.clone()),
            record.title
        )
    });
    let text = render_record_source_text(&record);

    notebooklm_call_tool(
        "add_source",
        json!({
            "notebook_id": notebook_id,
            "source_type": "text",
            "title": source_title,
            "text": text,
        }),
        config,
    )
}

#[tauri::command]
fn notebooklm_ask(
    notebook_id: String,
    question: String,
    include_citations: Option<bool>,
    config: Option<NotebookLmConfig>,
) -> Result<NotebookLmAskResult, String> {
    if question.trim().is_empty() {
        return Err("Question cannot be empty".to_string());
    }

    let payload = notebooklm_call_tool(
        "ask",
        json!({
            "notebook_id": notebook_id,
            "question": question.trim(),
            "include_citations": include_citations.unwrap_or(true),
        }),
        config,
    )?;

    let answer = payload
        .get("answer")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let citations = payload
        .get("citations")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(NotebookLmAskResult { answer, citations })
}

fn normalized_home(input: &str) -> Result<PathBuf, String> {
    if input.trim().is_empty() {
        return Err("Central Home path is required".to_string());
    }

    let home = detect_central_home_path(&absolute_path(Path::new(input.trim())));
    ensure_structure(&home).map_err(|error| error.to_string())?;
    Ok(home)
}

fn compute_dashboard_stats(records: &[Record], logs: &[LogEntry]) -> DashboardStats {
    let mut type_counts: HashMap<String, usize> = HashMap::new();
    let mut tags_counter: HashMap<String, usize> = HashMap::new();
    let mut pending_sync_count: usize = 0;

    for record in records {
        *type_counts.entry(record.record_type.clone()).or_insert(0) += 1;
        for tag in &record.tags {
            let clean = tag.trim();
            if !clean.is_empty() {
                *tags_counter.entry(clean.to_string()).or_insert(0) += 1;
            }
        }
        if matches!(
            record.notion_sync_status.as_str(),
            "PENDING" | "FAILED" | "CONFLICT"
        ) {
            pending_sync_count += 1;
        }
    }

    let mut top_tags: Vec<TagCount> = tags_counter
        .into_iter()
        .map(|(tag, count)| TagCount { tag, count })
        .collect();
    top_tags.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tag.cmp(&b.tag)));
    top_tags.truncate(12);

    let today = Local::now().date_naive();
    let mut daily_map: HashMap<String, usize> = HashMap::new();
    let mut ordered_days: Vec<String> = Vec::new();

    for offset in (0..=6).rev() {
        let day = today - ChronoDuration::days(offset);
        let key = day.format("%Y-%m-%d").to_string();
        daily_map.insert(key.clone(), 0);
        ordered_days.push(key);
    }

    for record in records {
        if let Some(day) = extract_day(&record.created_at) {
            if let Some(value) = daily_map.get_mut(&day) {
                *value += 1;
            }
        }
    }

    for log in logs {
        if let Some(day) = extract_day(&log.timestamp) {
            if let Some(value) = daily_map.get_mut(&day) {
                *value += 1;
            }
        }
    }

    let recent_daily_counts = ordered_days
        .into_iter()
        .map(|date| DailyCount {
            count: *daily_map.get(&date).unwrap_or(&0),
            date,
        })
        .collect::<Vec<_>>();

    DashboardStats {
        total_records: records.len(),
        total_logs: logs.len(),
        type_counts,
        top_tags,
        recent_daily_counts,
        pending_sync_count,
    }
}

fn load_records(central_home: &Path) -> Result<Vec<Record>, String> {
    let mut records: Vec<Record> = Vec::new();
    let records_root = central_home.join("records");
    if !records_root.exists() {
        return Ok(records);
    }

    for (record_type, folder) in RECORD_TYPE_DIRS {
        let dir = records_root.join(folder);
        if !dir.exists() {
            continue;
        }

        let entries = fs::read_dir(&dir).map_err(|error| error.to_string())?;
        for entry in entries {
            let path = match entry {
                Ok(item) => item.path(),
                Err(_) => continue,
            };

            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }

            let content = match fs::read_to_string(&path) {
                Ok(value) => value,
                Err(_) => continue,
            };

            let value: Value = match serde_json::from_str(&content) {
                Ok(parsed) => parsed,
                Err(_) => continue,
            };

            let fallback_type = Some(record_type.to_string());
            let record = record_from_value(
                &value,
                Some(path.clone()),
                Some(path.with_extension("md")),
                fallback_type,
            );
            records.push(record);
        }
    }

    records.sort_by(|a, b| compare_iso_desc(&a.created_at, &b.created_at));
    Ok(records)
}

fn load_logs(central_home: &Path) -> Result<Vec<LogEntry>, String> {
    let logs_root = central_home.join(".agentic").join("logs");
    let mut logs: Vec<LogEntry> = Vec::new();

    if !logs_root.exists() {
        return Ok(logs);
    }

    let entries = fs::read_dir(&logs_root).map_err(|error| error.to_string())?;
    for entry in entries {
        let path = match entry {
            Ok(item) => item.path(),
            Err(_) => continue,
        };

        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(value) => value,
            Err(_) => continue,
        };

        let raw: Value = match serde_json::from_str(&content) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };

        let meta = raw
            .get("meta")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let task = raw
            .get("task")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let data = raw.get("data").cloned().unwrap_or_else(|| json!({}));

        let timestamp = meta
            .get("timestamp")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| file_mtime_iso(&path));
        let event_id = meta
            .get("event_id")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_default();
        let task_intent = task
            .get("intent")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_default();
        let status = task
            .get("status")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_default();

        let title = data
            .get("title")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_default();

        logs.push(LogEntry {
            timestamp,
            event_id,
            task_intent,
            status,
            title,
            data,
            raw,
            json_path: Some(path.to_string_lossy().to_string()),
        });
    }

    logs.sort_by(|a, b| compare_iso_desc(&a.timestamp, &b.timestamp));
    Ok(logs)
}

fn rebuild_index(central_home: &Path, records: &[Record]) -> Result<usize, String> {
    let mut conn = open_index_connection(central_home)?;
    ensure_index_schema(&conn)?;

    let tx = conn.transaction().map_err(|error| error.to_string())?;
    tx.execute("DELETE FROM records_fts", [])
        .map_err(|error| error.to_string())?;

    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO records_fts (
                    json_path,
                    md_path,
                    record_type,
                    title,
                    final_body,
                    source_text,
                    tags,
                    created_at,
                    date,
                    notion_sync_status,
                    notion_page_id,
                    notion_url,
                    notion_error
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .map_err(|error| error.to_string())?;

        for record in records {
            stmt.execute(params![
                record.json_path.clone().unwrap_or_default(),
                record.md_path.clone().unwrap_or_default(),
                record.record_type,
                record.title,
                record.final_body,
                record.source_text,
                record.tags.join(","),
                record.created_at,
                record.date.clone().unwrap_or_default(),
                record.notion_sync_status,
                record.notion_page_id.clone().unwrap_or_default(),
                record.notion_url.clone().unwrap_or_default(),
                record.notion_error.clone().unwrap_or_default(),
            ])
            .map_err(|error| error.to_string())?;
        }
    }

    tx.execute(
        "INSERT INTO records_index_meta (key, value) VALUES ('updatedAt', ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![Local::now().to_rfc3339()],
    )
    .map_err(|error| error.to_string())?;

    tx.execute(
        "INSERT INTO records_index_meta (key, value) VALUES ('recordCount', ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![records.len().to_string()],
    )
    .map_err(|error| error.to_string())?;

    tx.commit().map_err(|error| error.to_string())?;
    Ok(records.len())
}

fn search_records_in_index(
    central_home: &Path,
    query: &str,
    record_type: Option<&str>,
    date_from: Option<&str>,
    date_to: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<(Vec<Record>, usize), String> {
    let conn = open_index_connection(central_home)?;
    ensure_index_schema(&conn)?;

    let mut where_clauses = Vec::new();
    let mut bindings = Vec::new();

    where_clauses.push("records_fts MATCH ?".to_string());
    bindings.push(query.to_string());

    if let Some(record_type) = record_type {
        where_clauses.push("record_type = ?".to_string());
        bindings.push(record_type.to_string());
    }

    if let Some(date_from) = date_from {
        where_clauses.push("substr(created_at, 1, 10) >= ?".to_string());
        bindings.push(date_from.to_string());
    }

    if let Some(date_to) = date_to {
        where_clauses.push("substr(created_at, 1, 10) <= ?".to_string());
        bindings.push(date_to.to_string());
    }

    let where_sql = format!("WHERE {}", where_clauses.join(" AND "));

    let count_sql = format!("SELECT COUNT(*) FROM records_fts {where_sql}");
    let total: usize = conn
        .query_row(&count_sql, params_from_iter(bindings.iter()), |row| row.get(0))
        .map_err(|error| error.to_string())?;

    let select_sql = format!(
        "SELECT
            json_path,
            md_path,
            record_type,
            title,
            final_body,
            source_text,
            tags,
            created_at,
            date,
            notion_sync_status,
            notion_page_id,
            notion_url,
            notion_error
        FROM records_fts
        {where_sql}
        ORDER BY bm25(records_fts), created_at DESC
        LIMIT {limit} OFFSET {offset}"
    );

    let mut stmt = conn.prepare(&select_sql).map_err(|error| error.to_string())?;
    let mut rows = stmt
        .query_map(params_from_iter(bindings.iter()), |row| {
            let tags_raw: String = row.get(6)?;
            Ok(Record {
                json_path: Some(row.get::<_, String>(0)?),
                md_path: Some(row.get::<_, String>(1)?),
                record_type: row.get(2)?,
                title: row.get(3)?,
                final_body: row.get(4)?,
                source_text: row.get(5)?,
                tags: parse_tags(&tags_raw),
                created_at: row.get(7)?,
                date: option_non_empty(row.get::<_, String>(8)?),
                notion_sync_status: row.get(9)?,
                notion_page_id: option_non_empty(row.get::<_, String>(10)?),
                notion_url: option_non_empty(row.get::<_, String>(11)?),
                notion_error: option_non_empty(row.get::<_, String>(12)?),
                notion_last_synced_at: None,
                notion_last_edited_time: None,
                notion_last_synced_hash: None,
            })
        })
        .map_err(|error| error.to_string())?;

    let mut records = Vec::new();
    for row in rows.by_ref() {
        records.push(row.map_err(|error| error.to_string())?);
    }

    Ok((records, total))
}

fn search_records_in_memory(
    records: &[Record],
    query: &str,
    record_type: Option<&str>,
    date_from: Option<&str>,
    date_to: Option<&str>,
    limit: usize,
    offset: usize,
) -> Vec<Record> {
    let lowered = query.trim().to_lowercase();

    records
        .iter()
        .filter(|item| matches_record(item, &lowered, record_type, date_from, date_to))
        .skip(offset)
        .take(limit)
        .cloned()
        .collect()
}

fn count_records_in_memory(
    records: &[Record],
    query: &str,
    record_type: Option<&str>,
    date_from: Option<&str>,
    date_to: Option<&str>,
) -> usize {
    let lowered = query.trim().to_lowercase();

    records
        .iter()
        .filter(|item| matches_record(item, &lowered, record_type, date_from, date_to))
        .count()
}

fn matches_record(
    record: &Record,
    lowered_query: &str,
    record_type: Option<&str>,
    date_from: Option<&str>,
    date_to: Option<&str>,
) -> bool {
    if let Some(record_type) = record_type {
        if record.record_type != record_type {
            return false;
        }
    }

    if let Some(date_from) = date_from {
        let day = extract_day(&record.created_at).unwrap_or_default();
        if day.as_str() < date_from {
            return false;
        }
    }

    if let Some(date_to) = date_to {
        let day = extract_day(&record.created_at).unwrap_or_default();
        if day.as_str() > date_to {
            return false;
        }
    }

    if lowered_query.is_empty() {
        return true;
    }

    let text = format!(
        "{} {} {} {}",
        record.title,
        record.final_body,
        record.source_text,
        record.tags.join(" ")
    )
    .to_lowercase();

    text.contains(lowered_query)
}

fn run_local_analysis(prompt: &str, records: &[Record], logs: &[LogEntry]) -> String {
    let stats = compute_dashboard_stats(records, logs);

    let dominant_type = stats
        .type_counts
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(name, _)| name.as_str())
        .unwrap_or("-");

    let mut lines = vec![
        "# KOF Local Analysis".to_string(),
        String::new(),
        "## Summary".to_string(),
        format!("- Total records: {}", stats.total_records),
        format!("- Total logs: {}", stats.total_logs),
        format!("- Pending sync records: {}", stats.pending_sync_count),
        format!("- Dominant type: {}", dominant_type),
        String::new(),
        "## Top Tags".to_string(),
    ];

    if stats.top_tags.is_empty() {
        lines.push("- (no tags yet)".to_string());
    } else {
        for item in stats.top_tags.iter().take(8) {
            lines.push(format!("- {} ({})", item.tag, item.count));
        }
    }

    lines.push(String::new());
    lines.push("## Recent Focus".to_string());
    for item in records.iter().take(6) {
        lines.push(format!(
            "- [{}] ({}) {}",
            item.created_at,
            item.record_type,
            item.title
        ));
    }

    lines.push(String::new());
    lines.push("## Risks".to_string());
    if stats.pending_sync_count > 0 {
        lines.push("- Pending sync records may diverge from Notion until re-synced.".to_string());
    } else {
        lines.push("- No immediate sync risk detected.".to_string());
    }
    lines.push("- If many backlogs have no date/tag, prioritization quality may drop.".to_string());

    lines.push(String::new());
    lines.push("## Next 7 Days Action Plan".to_string());
    lines.push("1. Consolidate top recurring tags into 2-3 execution themes.".to_string());
    lines.push("2. Convert high-value backlog items to scheduled worklogs.".to_string());
    lines.push("3. Run weekly review and archive stale notes.".to_string());

    if !prompt.trim().is_empty() {
        lines.push(String::new());
        lines.push("## User Prompt Focus".to_string());
        lines.push(prompt.trim().to_string());
    }

    lines.join("\n")
}

fn run_openai_analysis(
    model: &str,
    prompt: &str,
    api_key: Option<String>,
    records: &[Record],
    logs: &[LogEntry],
    include_logs: bool,
    max_records: usize,
) -> Result<String, String> {
    let api_key = resolve_api_key(api_key)?;

    let context = build_context_digest(records, logs, include_logs, max_records);
    let user_prompt = if prompt.trim().is_empty() {
        "Summarize patterns, risks, and a practical 7-day plan.".to_string()
    } else {
        prompt.trim().to_string()
    };

    let merged_prompt = format!(
        "You are analyzing a local-first productivity brain system. \nReturn concise markdown sections: Summary, Patterns, Risks, Action Plan (7 days).\n\nUser request:\n{}\n\nContext:\n{}",
        user_prompt, context
    );

    let payload = json!({
        "model": model,
        "input": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "input_text",
                        "text": merged_prompt
                    }
                ]
            }
        ]
    });

    let client = Client::builder()
        .timeout(StdDuration::from_secs(50))
        .build()
        .map_err(|error| error.to_string())?;

    let response = client
        .post(OPENAI_RESPONSES_URL)
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .map_err(|error| error.to_string())?;

    let status = response.status();
    let body_text = response.text().map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("OpenAI API {}: {}", status.as_u16(), body_text));
    }

    let value: Value = serde_json::from_str(&body_text).map_err(|error| error.to_string())?;
    let output = extract_openai_output_text(&value);

    if output.trim().is_empty() {
        return Err("OpenAI response did not include readable text".to_string());
    }

    Ok(output)
}

fn build_context_digest(
    records: &[Record],
    logs: &[LogEntry],
    include_logs: bool,
    max_records: usize,
) -> String {
    let mut lines = vec!["# Records".to_string()];

    for item in records.iter().take(max_records) {
        lines.push(format!(
            "- [{}] ({}) {} | tags: {}",
            item.created_at,
            item.record_type,
            item.title,
            if item.tags.is_empty() {
                "-".to_string()
            } else {
                item.tags.join(", ")
            }
        ));
    }

    if include_logs {
        lines.push(String::new());
        lines.push("# Logs".to_string());
        for item in logs.iter().take(max_records.min(40)) {
            lines.push(format!(
                "- [{}] {} / {} / {}",
                item.timestamp, item.task_intent, item.status, item.title
            ));
        }
    }

    lines.join("\n")
}

fn render_report_markdown(
    title: &str,
    central_home: &Path,
    stats: &DashboardStats,
    recent_records: &[&Record],
    days: i64,
) -> String {
    let mut lines = vec![
        format!("# {}", title),
        String::new(),
        format!("Generated: {}", Local::now().to_rfc3339()),
        format!("Central Home: {}", central_home.to_string_lossy()),
        String::new(),
        "## KPI".to_string(),
        format!("- Total records: {}", stats.total_records),
        format!("- Total logs: {}", stats.total_logs),
        format!("- Pending sync: {}", stats.pending_sync_count),
        String::new(),
        "## Type Distribution".to_string(),
    ];

    for (record_type, count) in &stats.type_counts {
        lines.push(format!("- {}: {}", record_type, count));
    }

    lines.push(String::new());
    lines.push("## Top Tags".to_string());
    if stats.top_tags.is_empty() {
        lines.push("- (none)".to_string());
    } else {
        for item in &stats.top_tags {
            lines.push(format!("- {} ({})", item.tag, item.count));
        }
    }

    lines.push(String::new());
    lines.push(format!("## Recent Records (last {} days)", days));
    for item in recent_records {
        lines.push(format!(
            "- [{}] ({}) {}",
            item.created_at, item.record_type, item.title
        ));
    }

    lines.join("\n")
}

fn parse_tags(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn option_non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn compare_iso_desc(a: &str, b: &str) -> std::cmp::Ordering {
    b.cmp(a)
}

fn sanitize_date_filter(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| item.len() == 10)
        .filter(|item| NaiveDate::parse_from_str(item, "%Y-%m-%d").is_ok())
}

fn resolve_api_key(api_key: Option<String>) -> Result<String, String> {
    if let Some(provided) = api_key {
        if !provided.trim().is_empty() {
            return Ok(provided.trim().to_string());
        }
    }

    let entry = keyring_entry()?;
    entry
        .get_password()
        .map_err(|_| "Missing OpenAI API key. Set it in Settings first.".to_string())
}

fn extract_openai_output_text(value: &Value) -> String {
    if let Some(text) = value.get("output_text").and_then(Value::as_str) {
        if !text.trim().is_empty() {
            return text.trim().to_string();
        }
    }

    let mut chunks = Vec::new();

    if let Some(output) = value.get("output").and_then(Value::as_array) {
        for item in output {
            if let Some(content) = item.get("content").and_then(Value::as_array) {
                for block in content {
                    let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
                    if matches!(block_type, "output_text" | "text") {
                        if let Some(text) = block.get("text").and_then(Value::as_str) {
                            if !text.trim().is_empty() {
                                chunks.push(text.trim().to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    chunks.join("\n")
}

fn open_index_connection(central_home: &Path) -> Result<Connection, String> {
    let path = index_db_path(central_home);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    Connection::open(path).map_err(|error| error.to_string())
}

fn ensure_index_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE VIRTUAL TABLE IF NOT EXISTS records_fts USING fts5(
            json_path UNINDEXED,
            md_path UNINDEXED,
            record_type,
            title,
            final_body,
            source_text,
            tags,
            created_at,
            date,
            notion_sync_status,
            notion_page_id UNINDEXED,
            notion_url UNINDEXED,
            notion_error UNINDEXED
         );
         CREATE TABLE IF NOT EXISTS records_index_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
         );",
    )
    .map_err(|error| error.to_string())
}

fn get_index_count(central_home: &Path) -> Result<usize, String> {
    let conn = open_index_connection(central_home)?;
    ensure_index_schema(&conn)?;
    conn.query_row("SELECT COUNT(*) FROM records_fts", [], |row| row.get(0))
        .map_err(|error| error.to_string())
}

fn upsert_index_record_if_exists(central_home: &Path, record: &Record) -> Result<(), String> {
    let index_path = index_db_path(central_home);
    if !index_path.exists() {
        return Ok(());
    }

    let conn = open_index_connection(central_home)?;
    ensure_index_schema(&conn)?;

    if let Some(json_path) = &record.json_path {
        conn.execute("DELETE FROM records_fts WHERE json_path = ?", params![json_path])
            .map_err(|error| error.to_string())?;
    }

    conn.execute(
        "INSERT INTO records_fts (
            json_path,
            md_path,
            record_type,
            title,
            final_body,
            source_text,
            tags,
            created_at,
            date,
            notion_sync_status,
            notion_page_id,
            notion_url,
            notion_error
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            record.json_path.clone().unwrap_or_default(),
            record.md_path.clone().unwrap_or_default(),
            record.record_type,
            record.title,
            record.final_body,
            record.source_text,
            record.tags.join(","),
            record.created_at,
            record.date.clone().unwrap_or_default(),
            record.notion_sync_status,
            record.notion_page_id.clone().unwrap_or_default(),
            record.notion_url.clone().unwrap_or_default(),
            record.notion_error.clone().unwrap_or_default(),
        ],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn delete_index_record_if_exists(central_home: &Path, json_path: &str) -> Result<(), String> {
    let index_path = index_db_path(central_home);
    if !index_path.exists() {
        return Ok(());
    }

    let conn = open_index_connection(central_home)?;
    ensure_index_schema(&conn)?;

    conn.execute("DELETE FROM records_fts WHERE json_path = ?", params![json_path])
        .map_err(|error| error.to_string())?;

    Ok(())
}

fn index_db_path(central_home: &Path) -> PathBuf {
    central_home.join(".agentic").join(SEARCH_DB_FILE)
}

fn ensure_structure(central_home: &Path) -> std::io::Result<()> {
    let records_root = central_home.join("records");
    fs::create_dir_all(&records_root)?;
    for (_, folder) in RECORD_TYPE_DIRS {
        fs::create_dir_all(records_root.join(folder))?;
    }
    fs::create_dir_all(central_home.join(".agentic").join("logs"))?;
    Ok(())
}

fn detect_central_home_path(candidate: &Path) -> PathBuf {
    let mut path = candidate.to_path_buf();

    if path.is_file() {
        if let Some(parent) = path.parent() {
            path = parent.to_path_buf();
        }
    }

    let name = path.file_name().and_then(|value| value.to_str()).unwrap_or_default();
    let parent_name = path
        .parent()
        .and_then(|value| value.file_name())
        .and_then(|value| value.to_str())
        .unwrap_or_default();

    let record_folders: HashSet<&str> = RECORD_TYPE_DIRS.iter().map(|(_, folder)| *folder).collect();

    if record_folders.contains(name) && parent_name == "records" {
        if let Some(home) = path.parent().and_then(Path::parent) {
            return home.to_path_buf();
        }
    }

    if name == "records" {
        if let Some(home) = path.parent() {
            return home.to_path_buf();
        }
    }

    if name == "logs" && parent_name == ".agentic" {
        if let Some(home) = path.parent().and_then(Path::parent) {
            return home.to_path_buf();
        }
    }

    if name == ".agentic" {
        if let Some(home) = path.parent() {
            return home.to_path_buf();
        }
    }

    if is_central_home(&path) {
        return path;
    }

    for ancestor in path.ancestors() {
        if is_central_home(ancestor) {
            return ancestor.to_path_buf();
        }
    }

    path
}

fn is_central_home(path: &Path) -> bool {
    path.join(".agentic").join("CENTRAL_LOG_MARKER").exists()
        || path.join("records").exists()
        || path.join(".agentic").join("logs").exists()
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn normalize_record_type(record_type: &str) -> String {
    let lower = record_type.trim().to_lowercase();
    if RECORD_TYPE_DIRS.iter().any(|(item, _)| *item == lower) {
        lower
    } else {
        "note".to_string()
    }
}

fn record_dir_by_type(record_type: &str) -> &'static str {
    for (item, dir) in RECORD_TYPE_DIRS {
        if item == record_type {
            return dir;
        }
    }
    "other"
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    for ch in value.to_lowercase().chars() {
        if ch.is_alphanumeric() || ch == '-' || ch == '_' {
            slug.push(ch);
        } else {
            slug.push('-');
        }
    }

    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }

    let trimmed = slug.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "untitled".to_string()
    } else if trimmed.chars().count() > 48 {
        trimmed.chars().take(48).collect()
    } else {
        trimmed
    }
}

fn generate_filename(record_type: &str, title: &str) -> String {
    let stamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let slug = slugify(title);
    format!("{stamp}_{record_type}_{slug}")
}

fn file_mtime_iso(path: &Path) -> String {
    let metadata = match fs::metadata(path) {
        Ok(item) => item,
        Err(_) => return String::new(),
    };

    let modified = match metadata.modified() {
        Ok(item) => item,
        Err(_) => return String::new(),
    };

    let datetime: DateTime<Local> = modified.into();
    datetime.to_rfc3339()
}

fn extract_day(value: &str) -> Option<String> {
    if value.len() < 10 {
        return None;
    }
    let day = &value[0..10];
    if NaiveDate::parse_from_str(day, "%Y-%m-%d").is_ok() {
        Some(day.to_string())
    } else {
        None
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn value_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn value_string_array(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn record_from_value(
    value: &Value,
    json_path: Option<PathBuf>,
    md_path: Option<PathBuf>,
    fallback_type: Option<String>,
) -> Record {
    let record_type = value_string(value, "type")
        .or(fallback_type)
        .map(|item| normalize_record_type(&item))
        .unwrap_or_else(|| "note".to_string());

    let created_at = value_string(value, "created_at").unwrap_or_else(|| {
        json_path
            .as_ref()
            .map(|path| file_mtime_iso(path))
            .unwrap_or_default()
    });

    Record {
        record_type,
        title: value_string(value, "title").unwrap_or_else(|| "Untitled".to_string()),
        created_at,
        source_text: value_string(value, "source_text").unwrap_or_default(),
        final_body: value_string(value, "final_body").unwrap_or_default(),
        tags: value_string_array(value, "tags"),
        date: value_string(value, "date"),
        notion_page_id: value_string(value, "notion_page_id"),
        notion_url: value_string(value, "notion_url"),
        notion_sync_status: value_string(value, "notion_sync_status")
            .unwrap_or_else(|| "SUCCESS".to_string()),
        notion_error: value_string(value, "notion_error"),
        notion_last_synced_at: value_string(value, "notion_last_synced_at"),
        notion_last_edited_time: value_string(value, "notion_last_edited_time"),
        notion_last_synced_hash: value_string(value, "notion_last_synced_hash"),
        json_path: json_path.map(|path| path.to_string_lossy().to_string()),
        md_path: md_path.map(|path| path.to_string_lossy().to_string()),
    }
}

fn render_markdown(record: &Record) -> String {
    let emoji = match record.record_type.as_str() {
        "decision" => "⚖️",
        "worklog" => "📝",
        "idea" => "💡",
        "backlog" => "📋",
        _ => "📄",
    };

    let mut lines = vec![
        format!("# {} {}", emoji, record.title),
        String::new(),
        format!("**Type:** {}", record.record_type.to_uppercase()),
        format!("**Created:** {}", record.created_at),
    ];

    if let Some(date) = &record.date {
        lines.push(format!("**Date:** {}", date));
    }

    if !record.tags.is_empty() {
        lines.push(format!("**Tags:** {}", record.tags.join(", ")));
    }

    if let Some(url) = &record.notion_url {
        lines.push(format!("**Notion:** {}", url));
    }

    lines.extend(vec![
        String::new(),
        "---".to_string(),
        String::new(),
        record.final_body.clone(),
        String::new(),
        "---".to_string(),
        String::new(),
        "## Original Input".to_string(),
        String::new(),
        format!("> {}", record.source_text),
    ]);

    lines.join("\n")
}

fn app_settings_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join(SETTINGS_DIR_NAME).join(SETTINGS_FILE_NAME)
}

fn load_settings() -> AppSettings {
    let path = app_settings_path();
    if !path.exists() {
        return AppSettings::default();
    }

    match fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<AppSettings>(&content) {
            Ok(settings) => normalize_settings(settings),
            Err(_) => AppSettings::default(),
        },
        Err(_) => AppSettings::default(),
    }
}

fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let path = app_settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(settings).map_err(|error| error.to_string())?;
    write_atomic(&path, &bytes).map_err(|error| error.to_string())
}

fn normalize_settings(mut settings: AppSettings) -> AppSettings {
    if settings.poll_interval_sec == 0 {
        settings.poll_interval_sec = default_poll_interval();
    }

    let mut seen_ids = HashSet::new();
    for profile in &mut settings.profiles {
        profile.id = if profile.id.trim().is_empty() {
            format!("profile-{}", slugify(&profile.name))
        } else {
            slugify(&profile.id)
        };
        if profile.id.is_empty() {
            profile.id = format!("profile-{}", Local::now().timestamp_millis());
        }

        if seen_ids.contains(&profile.id) {
            profile.id = format!("{}-{}", profile.id, Local::now().timestamp_millis());
        }
        seen_ids.insert(profile.id.clone());

        if profile.name.trim().is_empty() {
            profile.name = "Untitled Profile".to_string();
        }
        profile.central_home = profile.central_home.trim().to_string();

        let provider = profile.default_provider.trim().to_lowercase();
        profile.default_provider = if provider.is_empty() {
            "local".to_string()
        } else {
            provider
        };

        if profile.default_model.trim().is_empty() {
            profile.default_model = "gpt-4.1-mini".to_string();
        }
    }

    if settings.profiles.is_empty() {
        settings.active_profile_id = None;
    } else {
        let active_missing = settings
            .active_profile_id
            .as_ref()
            .map(|active| !settings.profiles.iter().any(|profile| &profile.id == active))
            .unwrap_or(true);

        if active_missing {
            settings.active_profile_id = settings.profiles.first().map(|profile| profile.id.clone());
        }
    }

    settings.integrations.notion.database_id = settings
        .integrations
        .notion
        .database_id
        .trim()
        .to_string();

    let notebook_command = settings.integrations.notebooklm.command.trim().to_string();
    settings.integrations.notebooklm.command = if notebook_command.is_empty() {
        default_notebooklm_command()
    } else {
        notebook_command
    };

    if settings.integrations.notebooklm.args.is_empty() {
        settings.integrations.notebooklm.args = default_notebooklm_args();
    } else {
        settings.integrations.notebooklm.args = settings
            .integrations
            .notebooklm
            .args
            .iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>();
        if settings.integrations.notebooklm.args.is_empty() {
            settings.integrations.notebooklm.args = default_notebooklm_args();
        }
    }

    settings
}

fn keyring_entry() -> Result<Entry, String> {
    Entry::new(OPENAI_SERVICE, OPENAI_USERNAME).map_err(|error| error.to_string())
}

fn has_keyring_entry_value(entry: Entry) -> Result<bool, String> {
    match entry.get_password() {
        Ok(value) => Ok(!value.trim().is_empty()),
        Err(KeyringError::NoEntry) => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

fn has_openai_api_key_internal() -> Result<bool, String> {
    has_keyring_entry_value(keyring_entry()?)
}

fn gemini_keyring_entry() -> Result<Entry, String> {
    Entry::new(OPENAI_SERVICE, GEMINI_USERNAME).map_err(|error| error.to_string())
}

fn has_gemini_api_key_internal() -> Result<bool, String> {
    has_keyring_entry_value(gemini_keyring_entry()?)
}

fn claude_keyring_entry() -> Result<Entry, String> {
    Entry::new(OPENAI_SERVICE, CLAUDE_USERNAME).map_err(|error| error.to_string())
}

fn has_claude_api_key_internal() -> Result<bool, String> {
    has_keyring_entry_value(claude_keyring_entry()?)
}

fn notion_keyring_entry() -> Result<Entry, String> {
    Entry::new(OPENAI_SERVICE, NOTION_USERNAME).map_err(|error| error.to_string())
}

fn has_notion_api_key_internal() -> Result<bool, String> {
    has_keyring_entry_value(notion_keyring_entry()?)
}

fn resolve_notion_api_key(api_key: Option<String>) -> Result<String, String> {
    if let Some(value) = api_key {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let entry = notion_keyring_entry()?;
    match entry.get_password() {
        Ok(value) if !value.trim().is_empty() => Ok(value.trim().to_string()),
        Ok(_) => Err("Missing Notion API key. Add it in Settings > Integrations.".to_string()),
        Err(KeyringError::NoEntry) => {
            Err("Missing Notion API key. Add it in Settings > Integrations.".to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

fn resolve_notion_database_id(database_id: Option<String>, settings: &AppSettings) -> Result<String, String> {
    let from_arg = database_id
        .unwrap_or_default()
        .trim()
        .to_string();
    if !from_arg.is_empty() {
        return Ok(from_arg);
    }

    let from_settings = settings.integrations.notion.database_id.trim().to_string();
    if !from_settings.is_empty() {
        return Ok(from_settings);
    }

    Err("Notion database ID is required. Set it in Settings > Integrations.".to_string())
}

fn load_record_by_json_path(central_home: &Path, json_path: &str) -> Result<Record, String> {
    let path = absolute_path(Path::new(json_path.trim()));
    if !path.exists() {
        return Err(format!("Record json not found: {}", path.to_string_lossy()));
    }

    let content = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let value: Value = serde_json::from_str(&content).map_err(|error| error.to_string())?;

    let inferred_type = infer_record_type_from_path(central_home, &path);
    Ok(record_from_value(
        &value,
        Some(path.clone()),
        Some(path.with_extension("md")),
        inferred_type,
    ))
}

fn infer_record_type_from_path(central_home: &Path, path: &Path) -> Option<String> {
    let root = central_home.join("records");
    let relative = path.strip_prefix(&root).ok()?;
    let folder = relative
        .components()
        .next()
        .and_then(|part| part.as_os_str().to_str())?;
    for (record_type, dir) in RECORD_TYPE_DIRS {
        if dir == folder {
            return Some(record_type.to_string());
        }
    }
    None
}

fn persist_record_to_files(record: &Record, json_path: &Path, md_path: &Path) -> Result<(), String> {
    let persisted = json!({
        "type": record.record_type,
        "title": record.title,
        "created_at": record.created_at,
        "notion_page_id": record.notion_page_id,
        "notion_url": record.notion_url,
        "source_text": record.source_text,
        "final_body": record.final_body,
        "tags": record.tags,
        "date": record.date,
        "notion_sync_status": record.notion_sync_status,
        "notion_error": record.notion_error,
        "notion_last_synced_at": record.notion_last_synced_at,
        "notion_last_edited_time": record.notion_last_edited_time,
        "notion_last_synced_hash": record.notion_last_synced_hash,
    });
    let json_bytes = serde_json::to_vec_pretty(&persisted).map_err(|error| error.to_string())?;
    write_atomic(json_path, &json_bytes).map_err(|error| error.to_string())?;
    let markdown = render_markdown(record);
    write_atomic(md_path, markdown.as_bytes()).map_err(|error| error.to_string())?;
    Ok(())
}

fn normalize_conflict_strategy(value: Option<String>) -> String {
    match value
        .unwrap_or_else(|| "manual".to_string())
        .trim()
        .to_lowercase()
        .as_str()
    {
        "local" | "local_wins" => "local_wins".to_string(),
        "notion" | "notion_wins" | "remote_wins" => "notion_wins".to_string(),
        _ => "manual".to_string(),
    }
}

fn record_sync_hash(record: &Record) -> String {
    let mut hasher = DefaultHasher::new();
    record.record_type.hash(&mut hasher);
    record.title.hash(&mut hasher);
    record.created_at.hash(&mut hasher);
    record.source_text.hash(&mut hasher);
    record.final_body.hash(&mut hasher);
    record.date.hash(&mut hasher);
    for tag in &record.tags {
        tag.hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}

fn local_has_changed_since_sync(record: &Record) -> bool {
    let base = record
        .notion_last_synced_hash
        .clone()
        .unwrap_or_default()
        .trim()
        .to_string();
    if base.is_empty() {
        return true;
    }
    record_sync_hash(record) != base
}

fn remote_has_changed(record: &Record, remote: &NotionRemoteRecord) -> bool {
    let current = remote
        .last_edited_time
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_string();
    if current.is_empty() {
        return false;
    }
    let previous = record
        .notion_last_edited_time
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_string();
    previous != current
}

fn mark_record_synced(record: &mut Record, remote_last_edited_time: Option<String>) {
    record.notion_sync_status = "SUCCESS".to_string();
    record.notion_error = None;
    record.notion_last_synced_at = Some(Local::now().to_rfc3339());
    if let Some(value) = remote_last_edited_time {
        if !value.trim().is_empty() {
            record.notion_last_edited_time = Some(value);
        }
    }
    record.notion_last_synced_hash = Some(record_sync_hash(record));
}

fn build_sync_result(
    json_path: &Path,
    record: &Record,
    action: &str,
    conflict: bool,
) -> NotionSyncResult {
    NotionSyncResult {
        json_path: json_path.to_string_lossy().to_string(),
        notion_page_id: record.notion_page_id.clone(),
        notion_url: record.notion_url.clone(),
        notion_sync_status: record.notion_sync_status.clone(),
        notion_error: record.notion_error.clone(),
        action: action.to_string(),
        conflict,
    }
}

fn sync_record_to_notion_internal(
    central_home: &Path,
    json_path: &str,
    database_id: &str,
    notion_api_key: &str,
    conflict_strategy: &str,
) -> Result<NotionSyncResult, String> {
    let path = absolute_path(Path::new(json_path.trim()));
    let md_path = path.with_extension("md");
    let mut record = load_record_by_json_path(central_home, &path.to_string_lossy())?;
    let client = notion_client()?;

    if let Some(page_id) = record
        .notion_page_id
        .as_ref()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
    {
        if let Ok(remote_meta) = notion_fetch_remote_record(&page_id, notion_api_key, &client, false) {
            let local_changed = local_has_changed_since_sync(&record);
            let notion_changed = remote_has_changed(&record, &remote_meta);
            if local_changed && notion_changed {
                match conflict_strategy {
                    "manual" => {
                        record.notion_sync_status = "CONFLICT".to_string();
                        record.notion_error = Some(
                            "Conflict detected: local and Notion both changed since last sync."
                                .to_string(),
                        );
                        persist_record_to_files(&record, &path, &md_path)?;
                        let _ = upsert_index_record_if_exists(central_home, &record);
                        return Ok(build_sync_result(
                            &path,
                            &record,
                            "conflict_manual",
                            true,
                        ));
                    }
                    "notion_wins" => {
                        let remote_full =
                            notion_fetch_remote_record(&page_id, notion_api_key, &client, true)?;
                        let mut next = apply_remote_to_local_record(&record, &remote_full);
                        mark_record_synced(&mut next, remote_full.last_edited_time.clone());
                        persist_record_to_files(&next, &path, &md_path)?;
                        let _ = upsert_index_record_if_exists(central_home, &next);
                        return Ok(build_sync_result(
                            &path,
                            &next,
                            "pulled_notion_conflict_notion_wins",
                            false,
                        ));
                    }
                    _ => {}
                }
            }
        }
    }

    push_local_record_to_notion(
        central_home,
        &client,
        &mut record,
        &path,
        &md_path,
        database_id,
        notion_api_key,
        "pushed_local",
    )
}

fn sync_record_bidirectional_internal(
    central_home: &Path,
    json_path: &str,
    database_id: &str,
    notion_api_key: &str,
    conflict_strategy: &str,
) -> Result<NotionSyncResult, String> {
    let path = absolute_path(Path::new(json_path.trim()));
    let md_path = path.with_extension("md");
    let mut record = load_record_by_json_path(central_home, &path.to_string_lossy())?;
    let client = notion_client()?;

    let page_id = record
        .notion_page_id
        .as_ref()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty());

    let remote = if let Some(id) = page_id.as_ref() {
        notion_fetch_remote_record(id, notion_api_key, &client, true).ok()
    } else {
        None
    };

    if let Some(remote_record) = remote {
        let local_changed = local_has_changed_since_sync(&record);
        let notion_changed = remote_has_changed(&record, &remote_record);

        if local_changed && notion_changed {
            match conflict_strategy {
                "manual" => {
                    record.notion_sync_status = "CONFLICT".to_string();
                    record.notion_error = Some(
                        "Conflict detected: local and Notion both changed since last sync."
                            .to_string(),
                    );
                    persist_record_to_files(&record, &path, &md_path)?;
                    let _ = upsert_index_record_if_exists(central_home, &record);
                    return Ok(build_sync_result(
                        &path,
                        &record,
                        "conflict_manual",
                        true,
                    ));
                }
                "notion_wins" => {
                    let mut next = apply_remote_to_local_record(&record, &remote_record);
                    mark_record_synced(&mut next, remote_record.last_edited_time.clone());
                    persist_record_to_files(&next, &path, &md_path)?;
                    let _ = upsert_index_record_if_exists(central_home, &next);
                    return Ok(build_sync_result(
                        &path,
                        &next,
                        "pulled_notion_conflict_notion_wins",
                        false,
                    ));
                }
                _ => {
                    return push_local_record_to_notion(
                        central_home,
                        &client,
                        &mut record,
                        &path,
                        &md_path,
                        database_id,
                        notion_api_key,
                        "pushed_local_conflict_local_wins",
                    )
                }
            }
        }

        if local_changed {
            return push_local_record_to_notion(
                central_home,
                &client,
                &mut record,
                &path,
                &md_path,
                database_id,
                notion_api_key,
                "pushed_local",
            );
        }

        if notion_changed {
            let mut next = apply_remote_to_local_record(&record, &remote_record);
            mark_record_synced(&mut next, remote_record.last_edited_time.clone());
            persist_record_to_files(&next, &path, &md_path)?;
            let _ = upsert_index_record_if_exists(central_home, &next);
            return Ok(build_sync_result(&path, &next, "pulled_notion", false));
        }

        mark_record_synced(&mut record, remote_record.last_edited_time.clone());
        persist_record_to_files(&record, &path, &md_path)?;
        let _ = upsert_index_record_if_exists(central_home, &record);
        return Ok(build_sync_result(&path, &record, "noop", false));
    }

    push_local_record_to_notion(
        central_home,
        &client,
        &mut record,
        &path,
        &md_path,
        database_id,
        notion_api_key,
        "pushed_local",
    )
}

fn pull_records_from_notion_internal(
    central_home: &Path,
    database_id: &str,
    notion_api_key: &str,
    conflict_strategy: &str,
) -> Result<NotionBatchSyncResult, String> {
    let client = notion_client()?;
    let pages = notion_query_database_pages(database_id, notion_api_key, &client)?;
    let locals = load_records(central_home)?;
    let mut by_page_id: HashMap<String, Record> = HashMap::new();
    for record in locals {
        if let Some(page_id) = record
            .notion_page_id
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            by_page_id.insert(page_id, record);
        }
    }

    let mut results = Vec::new();
    let mut success = 0usize;
    let mut failed = 0usize;
    let mut conflicts = 0usize;

    for page in pages {
        let remote = match notion_remote_record_from_page(&page, notion_api_key, &client, true) {
            Ok(item) => item,
            Err(error) => {
                failed += 1;
                results.push(NotionSyncResult {
                    json_path: String::new(),
                    notion_page_id: page
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    notion_url: page
                        .get("url")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    notion_sync_status: "FAILED".to_string(),
                    notion_error: Some(error),
                    action: "pull_failed".to_string(),
                    conflict: false,
                });
                continue;
            }
        };

        if let Some(existing) = by_page_id.get(&remote.page_id).cloned() {
            let (json_path, md_path) = resolve_record_paths(central_home, &existing)?;
            let local_changed = local_has_changed_since_sync(&existing);
            let notion_changed = remote_has_changed(&existing, &remote);

            let result = if local_changed && notion_changed {
                match conflict_strategy {
                    "manual" => {
                        let mut conflict_record = existing.clone();
                        conflict_record.notion_sync_status = "CONFLICT".to_string();
                        conflict_record.notion_error = Some(
                            "Conflict detected while pulling from Notion (manual strategy)."
                                .to_string(),
                        );
                        persist_record_to_files(&conflict_record, &json_path, &md_path)?;
                        let _ = upsert_index_record_if_exists(central_home, &conflict_record);
                        build_sync_result(&json_path, &conflict_record, "conflict_manual", true)
                    }
                    "local_wins" => {
                        let mut local_record = existing.clone();
                        push_local_record_to_notion(
                            central_home,
                            &client,
                            &mut local_record,
                            &json_path,
                            &md_path,
                            database_id,
                            notion_api_key,
                            "pushed_local_conflict_local_wins",
                        )?
                    }
                    _ => {
                        let mut next = apply_remote_to_local_record(&existing, &remote);
                        mark_record_synced(&mut next, remote.last_edited_time.clone());
                        persist_record_to_files(&next, &json_path, &md_path)?;
                        let _ = upsert_index_record_if_exists(central_home, &next);
                        build_sync_result(
                            &json_path,
                            &next,
                            "pulled_notion_conflict_notion_wins",
                            false,
                        )
                    }
                }
            } else if notion_changed {
                let mut next = apply_remote_to_local_record(&existing, &remote);
                mark_record_synced(&mut next, remote.last_edited_time.clone());
                persist_record_to_files(&next, &json_path, &md_path)?;
                let _ = upsert_index_record_if_exists(central_home, &next);
                build_sync_result(&json_path, &next, "pulled_notion", false)
            } else if local_changed {
                match conflict_strategy {
                    "local_wins" => {
                        let mut local_record = existing.clone();
                        push_local_record_to_notion(
                            central_home,
                            &client,
                            &mut local_record,
                            &json_path,
                            &md_path,
                            database_id,
                            notion_api_key,
                            "pushed_local_local_only_change",
                        )?
                    }
                    "notion_wins" => {
                        let mut next = apply_remote_to_local_record(&existing, &remote);
                        mark_record_synced(&mut next, remote.last_edited_time.clone());
                        persist_record_to_files(&next, &json_path, &md_path)?;
                        let _ = upsert_index_record_if_exists(central_home, &next);
                        build_sync_result(&json_path, &next, "pulled_notion_local_only_change", false)
                    }
                    _ => {
                        let mut pending = existing.clone();
                        pending.notion_sync_status = "PENDING".to_string();
                        pending.notion_error = Some(
                            "Local-only changes detected. Pull skipped by manual strategy."
                                .to_string(),
                        );
                        persist_record_to_files(&pending, &json_path, &md_path)?;
                        let _ = upsert_index_record_if_exists(central_home, &pending);
                        build_sync_result(&json_path, &pending, "local_only_pending", false)
                    }
                }
            } else {
                let mut stable = existing.clone();
                mark_record_synced(&mut stable, remote.last_edited_time.clone());
                persist_record_to_files(&stable, &json_path, &md_path)?;
                let _ = upsert_index_record_if_exists(central_home, &stable);
                build_sync_result(&json_path, &stable, "noop", false)
            };

            if result.conflict {
                conflicts += 1;
                failed += 1;
            } else if result.notion_sync_status == "SUCCESS" {
                success += 1;
            } else {
                failed += 1;
            }
            results.push(result);
        } else {
            let mut next = record_from_remote(&remote);
            mark_record_synced(&mut next, remote.last_edited_time.clone());
            let (json_path, md_path) =
                generate_unique_record_paths(central_home, &next.record_type, &next.title)?;
            persist_record_to_files(&next, &json_path, &md_path)?;
            let _ = upsert_index_record_if_exists(central_home, &next);
            success += 1;
            results.push(build_sync_result(
                &json_path,
                &next,
                "created_local_from_notion",
                false,
            ));
        }
    }

    Ok(NotionBatchSyncResult {
        total: results.len(),
        success,
        failed,
        conflicts,
        results,
    })
}

fn resolve_record_paths(central_home: &Path, record: &Record) -> Result<(PathBuf, PathBuf), String> {
    if let Some(path) = record
        .json_path
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        let json = absolute_path(Path::new(&path));
        return Ok((json.clone(), json.with_extension("md")));
    }
    generate_unique_record_paths(central_home, &record.record_type, &record.title)
}

fn generate_unique_record_paths(
    central_home: &Path,
    record_type: &str,
    title: &str,
) -> Result<(PathBuf, PathBuf), String> {
    let dir = central_home.join("records").join(record_dir_by_type(record_type));
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let base = generate_filename(record_type, title);
    let mut json_path = dir.join(format!("{base}.json"));
    let mut idx = 1usize;
    while json_path.exists() {
        json_path = dir.join(format!("{base}_{idx}.json"));
        idx += 1;
    }
    Ok((json_path.clone(), json_path.with_extension("md")))
}

fn record_from_remote(remote: &NotionRemoteRecord) -> Record {
    Record {
        record_type: normalize_record_type(&remote.record_type),
        title: if remote.title.trim().is_empty() {
            "Untitled".to_string()
        } else {
            remote.title.clone()
        },
        created_at: if remote.created_at.trim().is_empty() {
            Local::now().to_rfc3339()
        } else {
            remote.created_at.clone()
        },
        source_text: remote.source_text.clone(),
        final_body: remote.final_body.clone(),
        tags: remote.tags.clone(),
        date: remote.date.clone(),
        notion_page_id: Some(remote.page_id.clone()),
        notion_url: remote.page_url.clone(),
        notion_sync_status: "SUCCESS".to_string(),
        notion_error: None,
        notion_last_synced_at: None,
        notion_last_edited_time: remote.last_edited_time.clone(),
        notion_last_synced_hash: None,
        json_path: None,
        md_path: None,
    }
}

fn apply_remote_to_local_record(local: &Record, remote: &NotionRemoteRecord) -> Record {
    let mut next = local.clone();
    next.record_type = normalize_record_type(&remote.record_type);
    next.title = if remote.title.trim().is_empty() {
        local.title.clone()
    } else {
        remote.title.clone()
    };
    next.created_at = if remote.created_at.trim().is_empty() {
        local.created_at.clone()
    } else {
        remote.created_at.clone()
    };
    next.source_text = remote.source_text.clone();
    next.final_body = remote.final_body.clone();
    next.tags = remote.tags.clone();
    next.date = remote.date.clone();
    next.notion_page_id = Some(remote.page_id.clone());
    next.notion_url = remote.page_url.clone();
    next.notion_error = None;
    next
}

fn push_local_record_to_notion(
    central_home: &Path,
    client: &Client,
    record: &mut Record,
    json_path: &Path,
    md_path: &Path,
    database_id: &str,
    notion_api_key: &str,
    action: &str,
) -> Result<NotionSyncResult, String> {
    match notion_upsert_record(database_id, notion_api_key, record, client) {
        Ok(info) => {
            record.notion_page_id = Some(info.page_id);
            record.notion_url = info.page_url;
            mark_record_synced(record, info.last_edited_time);
            persist_record_to_files(record, json_path, md_path)?;
            let _ = upsert_index_record_if_exists(central_home, record);
            Ok(build_sync_result(json_path, record, action, false))
        }
        Err(error) => {
            record.notion_sync_status = "FAILED".to_string();
            record.notion_error = Some(error);
            let _ = persist_record_to_files(record, json_path, md_path);
            let _ = upsert_index_record_if_exists(central_home, record);
            Ok(build_sync_result(json_path, record, "push_failed", false))
        }
    }
}

fn notion_client() -> Result<Client, String> {
    Client::builder()
        .timeout(StdDuration::from_secs(50))
        .build()
        .map_err(|error| error.to_string())
}

fn notion_upsert_record(
    database_id: &str,
    notion_api_key: &str,
    record: &Record,
    client: &Client,
) -> Result<NotionUpsertInfo, String> {
    let database_value = notion_fetch_database(database_id, notion_api_key, client)?;

    let properties_schema = database_value
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| "Notion database properties not found".to_string())?;
    let title_property_name =
        notion_find_title_property_name(properties_schema).ok_or_else(|| {
            "Could not find title property in target Notion database".to_string()
        })?;

    let properties = notion_build_properties(properties_schema, &title_property_name, record);

    let patch_response = if let Some(page_id) = record
        .notion_page_id
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        Some(
            client
                .patch(format!("{NOTION_API_BASE_URL}/pages/{page_id}"))
                .header("Authorization", format!("Bearer {notion_api_key}"))
                .header("Notion-Version", NOTION_API_VERSION)
                .header("Content-Type", "application/json")
                .json(&json!({ "properties": properties }))
                .send()
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };

    let create_page = if let Some(response) = patch_response {
        let status = response.status();
        let body_text = response.text().map_err(|error| error.to_string())?;
        if status.is_success() {
            Some(serde_json::from_str::<Value>(&body_text).map_err(|error| error.to_string())?)
        } else {
            let code = notion_error_code_from_body(&body_text);
            if status.as_u16() == 404 || code.as_deref() == Some("object_not_found") {
                None
            } else {
                return Err(format!("Notion API {}: {}", status.as_u16(), body_text));
            }
        }
    } else {
        None
    };

    let value = if let Some(patched) = create_page {
        patched
    } else {
        let response = client
            .post(format!("{NOTION_API_BASE_URL}/pages"))
            .header("Authorization", format!("Bearer {notion_api_key}"))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .json(&json!({
                "parent": { "database_id": database_id },
                "properties": properties,
                "children": notion_build_children(record),
            }))
            .send()
            .map_err(|error| error.to_string())?;

        let status = response.status();
        let body_text = response.text().map_err(|error| error.to_string())?;
        if !status.is_success() {
            return Err(format!("Notion API {}: {}", status.as_u16(), body_text));
        }
        serde_json::from_str::<Value>(&body_text).map_err(|error| error.to_string())?
    };

    let page_id = value
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "Notion response missing page id".to_string())?;
    let page_url = value.get("url").and_then(Value::as_str).map(str::to_string);
    let last_edited_time = value
        .get("last_edited_time")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(NotionUpsertInfo {
        page_id,
        page_url,
        last_edited_time,
    })
}

fn notion_error_code_from_body(body_text: &str) -> Option<String> {
    serde_json::from_str::<Value>(body_text)
        .ok()
        .and_then(|value| value.get("code").and_then(Value::as_str).map(str::to_string))
}

fn notion_fetch_database(database_id: &str, notion_api_key: &str, client: &Client) -> Result<Value, String> {
    let response = client
        .get(format!("{NOTION_API_BASE_URL}/databases/{database_id}"))
        .header("Authorization", format!("Bearer {notion_api_key}"))
        .header("Notion-Version", NOTION_API_VERSION)
        .send()
        .map_err(|error| error.to_string())?;
    let status = response.status();
    let body = response.text().map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("Notion database API {}: {}", status.as_u16(), body));
    }
    serde_json::from_str(&body).map_err(|error| error.to_string())
}

fn notion_query_database_pages(
    database_id: &str,
    notion_api_key: &str,
    client: &Client,
) -> Result<Vec<Value>, String> {
    let mut pages: Vec<Value> = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let body = if let Some(next_cursor) = cursor.clone() {
            json!({ "page_size": 100, "start_cursor": next_cursor })
        } else {
            json!({ "page_size": 100 })
        };

        let response = client
            .post(format!("{NOTION_API_BASE_URL}/databases/{database_id}/query"))
            .header("Authorization", format!("Bearer {notion_api_key}"))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .map_err(|error| error.to_string())?;

        let status = response.status();
        let text = response.text().map_err(|error| error.to_string())?;
        if !status.is_success() {
            return Err(format!("Notion query API {}: {}", status.as_u16(), text));
        }

        let value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
        let batch = value
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        pages.extend(batch);

        let has_more = value
            .get("has_more")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if has_more {
            cursor = value
                .get("next_cursor")
                .and_then(Value::as_str)
                .map(str::to_string);
            if cursor.is_none() {
                break;
            }
        } else {
            break;
        }
    }

    Ok(pages)
}

fn notion_fetch_remote_record(
    page_id: &str,
    notion_api_key: &str,
    client: &Client,
    include_content: bool,
) -> Result<NotionRemoteRecord, String> {
    let response = client
        .get(format!("{NOTION_API_BASE_URL}/pages/{page_id}"))
        .header("Authorization", format!("Bearer {notion_api_key}"))
        .header("Notion-Version", NOTION_API_VERSION)
        .send()
        .map_err(|error| error.to_string())?;
    let status = response.status();
    let body = response.text().map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("Notion page API {}: {}", status.as_u16(), body));
    }
    let page: Value = serde_json::from_str(&body).map_err(|error| error.to_string())?;
    notion_remote_record_from_page(&page, notion_api_key, client, include_content)
}

fn notion_remote_record_from_page(
    page: &Value,
    notion_api_key: &str,
    client: &Client,
    include_content: bool,
) -> Result<NotionRemoteRecord, String> {
    let page_id = page
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "Notion page missing id".to_string())?;
    let page_url = page.get("url").and_then(Value::as_str).map(str::to_string);
    let last_edited_time = page
        .get("last_edited_time")
        .and_then(Value::as_str)
        .map(str::to_string);

    let properties = page
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| "Notion page missing properties".to_string())?;

    let title = notion_extract_title_from_properties(properties);
    let record_type = notion_extract_record_type_from_properties(properties);
    let tags = notion_extract_tags_from_properties(properties);
    let date = notion_extract_date_from_properties(properties);
    let created_at = notion_extract_created_at_from_properties(page, properties);

    let (final_body, source_text) = if include_content {
        notion_fetch_page_content(&page_id, notion_api_key, client)?
    } else {
        (String::new(), String::new())
    };

    Ok(NotionRemoteRecord {
        page_id,
        page_url,
        last_edited_time,
        record_type,
        title,
        created_at,
        date,
        tags,
        final_body,
        source_text,
    })
}

fn notion_extract_title_from_properties(properties: &serde_json::Map<String, Value>) -> String {
    for (_, property) in properties {
        let kind = property.get("type").and_then(Value::as_str).unwrap_or_default();
        if kind == "title" {
            let text = notion_plain_text_from_rich_text(
                property.get("title").unwrap_or(&Value::Null),
            );
            if !text.trim().is_empty() {
                return text;
            }
        }
    }
    "Untitled".to_string()
}

fn notion_extract_record_type_from_properties(properties: &serde_json::Map<String, Value>) -> String {
    if let Some(property) = notion_find_page_property_by_candidates(properties, &["Type", "Record Type"]) {
        let kind = property.get("type").and_then(Value::as_str).unwrap_or_default();
        let value = match kind {
            "select" => property
                .get("select")
                .and_then(|item| item.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            "rich_text" => notion_plain_text_from_rich_text(
                property.get("rich_text").unwrap_or(&Value::Null),
            ),
            "title" => notion_plain_text_from_rich_text(
                property.get("title").unwrap_or(&Value::Null),
            ),
            _ => String::new(),
        };
        if !value.trim().is_empty() {
            return normalize_record_type(&value);
        }
    }
    "note".to_string()
}

fn notion_extract_tags_from_properties(properties: &serde_json::Map<String, Value>) -> Vec<String> {
    if let Some(property) = notion_find_page_property_by_candidates(properties, &["Tags", "Tag"]) {
        let kind = property.get("type").and_then(Value::as_str).unwrap_or_default();
        match kind {
            "multi_select" => {
                return property
                    .get("multi_select")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.get("name").and_then(Value::as_str))
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            }
            "rich_text" => {
                let text = notion_plain_text_from_rich_text(
                    property.get("rich_text").unwrap_or(&Value::Null),
                );
                return text
                    .split(',')
                    .map(|item| item.trim().to_string())
                    .filter(|item| !item.is_empty())
                    .collect::<Vec<_>>();
            }
            "select" => {
                if let Some(value) = property
                    .get("select")
                    .and_then(|item| item.get("name"))
                    .and_then(Value::as_str)
                {
                    if !value.trim().is_empty() {
                        return vec![value.trim().to_string()];
                    }
                }
            }
            _ => {}
        }
    }
    Vec::new()
}

fn notion_extract_date_from_properties(properties: &serde_json::Map<String, Value>) -> Option<String> {
    let property = notion_find_page_property_by_candidates(properties, &["Date"])?;
    let kind = property.get("type").and_then(Value::as_str).unwrap_or_default();
    match kind {
        "date" => property
            .get("date")
            .and_then(|item| item.get("start"))
            .and_then(Value::as_str)
            .map(str::to_string),
        "rich_text" => {
            let text = notion_plain_text_from_rich_text(property.get("rich_text").unwrap_or(&Value::Null));
            if text.trim().is_empty() {
                None
            } else {
                Some(text.trim().to_string())
            }
        }
        _ => None,
    }
}

fn notion_extract_created_at_from_properties(
    page: &Value,
    properties: &serde_json::Map<String, Value>,
) -> String {
    if let Some(property) =
        notion_find_page_property_by_candidates(properties, &["Created At", "Created", "Timestamp"])
    {
        let kind = property.get("type").and_then(Value::as_str).unwrap_or_default();
        let from_property = match kind {
            "date" => property
                .get("date")
                .and_then(|item| item.get("start"))
                .and_then(Value::as_str)
                .map(str::to_string),
            "rich_text" => {
                let text = notion_plain_text_from_rich_text(
                    property.get("rich_text").unwrap_or(&Value::Null),
                );
                if text.trim().is_empty() {
                    None
                } else {
                    Some(text.trim().to_string())
                }
            }
            _ => None,
        };
        if let Some(value) = from_property {
            return value;
        }
    }

    page.get("created_time")
        .and_then(Value::as_str)
        .or_else(|| page.get("last_edited_time").and_then(Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| Local::now().to_rfc3339())
}

fn notion_find_page_property_by_candidates<'a>(
    properties: &'a serde_json::Map<String, Value>,
    candidates: &[&str],
) -> Option<&'a Value> {
    for candidate in candidates {
        for (name, property) in properties {
            if name.eq_ignore_ascii_case(candidate) {
                return Some(property);
            }
        }
    }
    None
}

fn notion_plain_text_from_rich_text(value: &Value) -> String {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    item.get("plain_text")
                        .and_then(Value::as_str)
                        .or_else(|| {
                            item.get("text")
                                .and_then(|text| text.get("content"))
                                .and_then(Value::as_str)
                        })
                        .unwrap_or_default()
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

fn notion_fetch_page_content(
    page_id: &str,
    notion_api_key: &str,
    client: &Client,
) -> Result<(String, String), String> {
    let mut blocks: Vec<Value> = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let mut request = client
            .get(format!("{NOTION_API_BASE_URL}/blocks/{page_id}/children"))
            .header("Authorization", format!("Bearer {notion_api_key}"))
            .header("Notion-Version", NOTION_API_VERSION)
            .query(&[("page_size", "100")]);
        if let Some(next_cursor) = cursor.as_ref() {
            request = request.query(&[("start_cursor", next_cursor)]);
        }

        let response = request.send().map_err(|error| error.to_string())?;
        let status = response.status();
        let text = response.text().map_err(|error| error.to_string())?;
        if !status.is_success() {
            return Err(format!(
                "Notion block children API {}: {}",
                status.as_u16(),
                text
            ));
        }

        let value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
        if let Some(items) = value.get("results").and_then(Value::as_array) {
            blocks.extend(items.iter().cloned());
        }

        let has_more = value
            .get("has_more")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if has_more {
            cursor = value
                .get("next_cursor")
                .and_then(Value::as_str)
                .map(str::to_string);
            if cursor.is_none() {
                break;
            }
        } else {
            break;
        }
    }

    Ok(notion_extract_content_sections(&blocks))
}

fn notion_extract_content_sections(blocks: &[Value]) -> (String, String) {
    let mut final_lines: Vec<String> = Vec::new();
    let mut source_lines: Vec<String> = Vec::new();
    let mut fallback_lines: Vec<String> = Vec::new();
    let mut section = "";

    for block in blocks {
        let block_type = block.get("type").and_then(Value::as_str).unwrap_or_default();
        let text = notion_extract_block_text(block, block_type);
        let clean = text.trim();
        if clean.is_empty() {
            continue;
        }

        if block_type.starts_with("heading_") {
            if clean.eq_ignore_ascii_case("Final Body") {
                section = "final";
                continue;
            }
            if clean.eq_ignore_ascii_case("Source Text") {
                section = "source";
                continue;
            }
        }

        match section {
            "final" => final_lines.push(clean.to_string()),
            "source" => source_lines.push(clean.to_string()),
            _ => fallback_lines.push(clean.to_string()),
        }
    }

    let final_body = if !final_lines.is_empty() {
        final_lines.join("\n\n")
    } else {
        fallback_lines.join("\n\n")
    };
    let source_text = source_lines.join("\n\n");
    (final_body, source_text)
}

fn notion_extract_block_text(block: &Value, block_type: &str) -> String {
    let section = block.get(block_type).unwrap_or(&Value::Null);
    if let Some(rich_text) = section.get("rich_text") {
        return notion_plain_text_from_rich_text(rich_text);
    }
    String::new()
}

fn notion_find_title_property_name(properties: &serde_json::Map<String, Value>) -> Option<String> {
    for (name, schema) in properties {
        if schema.get("type").and_then(Value::as_str) == Some("title") {
            return Some(name.to_string());
        }
    }
    None
}

fn notion_find_property_by_candidates(
    properties: &serde_json::Map<String, Value>,
    candidates: &[&str],
) -> Option<(String, String)> {
    for candidate in candidates {
        for (name, schema) in properties {
            if name.eq_ignore_ascii_case(candidate) {
                let prop_type = schema
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                return Some((name.to_string(), prop_type));
            }
        }
    }
    None
}

fn notion_build_properties(
    properties_schema: &serde_json::Map<String, Value>,
    title_property_name: &str,
    record: &Record,
) -> Value {
    let mut properties = serde_json::Map::<String, Value>::new();
    properties.insert(
        title_property_name.to_string(),
        json!({
            "title": [{
                "type": "text",
                "text": { "content": record.title.chars().take(1800).collect::<String>() }
            }]
        }),
    );

    if let Some((name, kind)) = notion_find_property_by_candidates(properties_schema, &["Type", "Record Type"]) {
        match kind.as_str() {
            "select" => {
                properties.insert(name, json!({ "select": { "name": record.record_type } }));
            }
            "rich_text" => {
                properties.insert(name, json!({ "rich_text": [{ "type": "text", "text": { "content": record.record_type } }] }));
            }
            _ => {}
        }
    }

    if let Some((name, kind)) = notion_find_property_by_candidates(properties_schema, &["Tags", "Tag"]) {
        match kind.as_str() {
            "multi_select" => {
                properties.insert(
                    name,
                    json!({
                        "multi_select": record
                            .tags
                            .iter()
                            .filter(|item| !item.trim().is_empty())
                            .map(|item| json!({ "name": item.trim() }))
                            .collect::<Vec<_>>()
                    }),
                );
            }
            "rich_text" => {
                properties.insert(
                    name,
                    json!({ "rich_text": [{ "type": "text", "text": { "content": record.tags.join(", ") } }] }),
                );
            }
            _ => {}
        }
    }

    if let Some((name, kind)) = notion_find_property_by_candidates(properties_schema, &["Date"]) {
        if kind == "date" {
            let start = record
                .date
                .clone()
                .or_else(|| extract_day(&record.created_at))
                .unwrap_or_else(|| Local::now().format("%Y-%m-%d").to_string());
            properties.insert(name, json!({ "date": { "start": start } }));
        }
    }

    if let Some((name, kind)) =
        notion_find_property_by_candidates(properties_schema, &["Created At", "Created", "Timestamp"])
    {
        match kind.as_str() {
            "date" => {
                properties.insert(name, json!({ "date": { "start": record.created_at } }));
            }
            "rich_text" => {
                properties.insert(
                    name,
                    json!({ "rich_text": [{ "type": "text", "text": { "content": record.created_at } }] }),
                );
            }
            _ => {}
        }
    }

    Value::Object(properties)
}

fn notion_build_children(record: &Record) -> Vec<Value> {
    let final_body = if record.final_body.trim().is_empty() {
        "(empty)".to_string()
    } else {
        record.final_body.clone()
    };
    let source_text = if record.source_text.trim().is_empty() {
        "(empty)".to_string()
    } else {
        record.source_text.clone()
    };

    vec![
        json!({
            "object": "block",
            "type": "heading_2",
            "heading_2": {
                "rich_text": [{ "type": "text", "text": { "content": "Final Body" } }]
            }
        }),
        json!({
            "object": "block",
            "type": "paragraph",
            "paragraph": {
                "rich_text": [{ "type": "text", "text": { "content": final_body.chars().take(1800).collect::<String>() } }]
            }
        }),
        json!({
            "object": "block",
            "type": "heading_2",
            "heading_2": {
                "rich_text": [{ "type": "text", "text": { "content": "Source Text" } }]
            }
        }),
        json!({
            "object": "block",
            "type": "paragraph",
            "paragraph": {
                "rich_text": [{ "type": "text", "text": { "content": source_text.chars().take(1800).collect::<String>() } }]
            }
        }),
    ]
}

fn parse_notebook_summary(value: &Value) -> NotebookSummary {
    let id = value
        .get("id")
        .or_else(|| value.get("notebook_id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let name = value
        .get("name")
        .or_else(|| value.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("Untitled Notebook")
        .to_string();
    let source_count = value
        .get("source_count")
        .and_then(Value::as_u64)
        .map(|item| item as usize);
    let updated_at = value
        .get("updated_at")
        .and_then(Value::as_str)
        .map(str::to_string);

    NotebookSummary {
        id,
        name,
        source_count,
        updated_at,
    }
}

fn render_record_source_text(record: &Record) -> String {
    let mut lines = vec![
        format!("# {}", record.title),
        String::new(),
        format!("- Type: {}", record.record_type),
        format!("- Created At: {}", record.created_at),
        format!("- Date: {}", record.date.clone().unwrap_or_default()),
        format!("- Tags: {}", record.tags.join(", ")),
        String::new(),
        "## Final Body".to_string(),
        record.final_body.clone(),
        String::new(),
        "## Source Text".to_string(),
        record.source_text.clone(),
    ];
    lines.retain(|line| !(line.starts_with("- Date: ") && line == "- Date: "));
    lines.join("\n")
}

fn resolve_notebooklm_runtime(config: Option<NotebookLmConfig>) -> (String, Vec<String>) {
    let settings = load_settings();
    let default_command = settings.integrations.notebooklm.command.trim().to_string();
    let default_args = settings.integrations.notebooklm.args.clone();

    let command = config
        .as_ref()
        .and_then(|item| item.command.as_ref())
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .or_else(|| {
            if default_command.is_empty() {
                None
            } else {
                Some(default_command)
            }
        })
        .unwrap_or_else(default_notebooklm_command);

    let args = config
        .and_then(|item| item.args)
        .filter(|items| !items.is_empty())
        .unwrap_or_else(|| {
            if default_args.is_empty() {
                default_notebooklm_args()
            } else {
                default_args
            }
        });

    (command, args)
}

fn notebooklm_call_tool(
    tool_name: &str,
    arguments: Value,
    config: Option<NotebookLmConfig>,
) -> Result<Value, String> {
    let (command, args) = resolve_notebooklm_runtime(config);

    let mut child = Command::new(&command)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Failed to start NotebookLM MCP command `{command}`: {error}"))?;

    let result = (|| -> Result<Value, String> {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "NotebookLM MCP stdin unavailable".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "NotebookLM MCP stdout unavailable".to_string())?;

        let (tx, rx) = mpsc::channel::<Value>();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.starts_with('{') {
                            if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
                                let _ = tx.send(value);
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        write_jsonrpc_line(
            &mut stdin,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "clientInfo": { "name": "kofnote-app", "version": "0.1.0" },
                    "capabilities": {}
                }
            }),
        )?;
        wait_jsonrpc_result(&rx, 1, StdDuration::from_secs(25))?;

        write_jsonrpc_line(
            &mut stdin,
            &json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
                "params": {}
            }),
        )?;

        write_jsonrpc_line(
            &mut stdin,
            &json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": { "name": tool_name, "arguments": arguments }
            }),
        )?;

        let call_response = wait_jsonrpc_result(&rx, 2, StdDuration::from_secs(90))?;
        parse_mcp_tool_payload(&call_response)
    })();

    let _ = child.kill();
    let _ = child.wait();
    result
}

fn write_jsonrpc_line(stdin: &mut std::process::ChildStdin, value: &Value) -> Result<(), String> {
    let text = serde_json::to_string(value).map_err(|error| error.to_string())?;
    stdin
        .write_all(format!("{text}\n").as_bytes())
        .map_err(|error| error.to_string())
}

fn wait_jsonrpc_result(
    rx: &mpsc::Receiver<Value>,
    expected_id: u64,
    timeout: StdDuration,
) -> Result<Value, String> {
    let deadline = Instant::now() + timeout;
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err("NotebookLM MCP response timeout".to_string());
        }
        let wait_for = deadline.saturating_duration_since(now);
        match rx.recv_timeout(wait_for) {
            Ok(message) => {
                let id = message.get("id").and_then(Value::as_u64).unwrap_or(0);
                if id != expected_id {
                    continue;
                }

                if let Some(error) = message.get("error") {
                    return Err(format!("NotebookLM MCP error: {error}"));
                }
                return Ok(message);
            }
            Err(_) => return Err("NotebookLM MCP response timeout".to_string()),
        }
    }
}

fn parse_mcp_tool_payload(response: &Value) -> Result<Value, String> {
    let result = response
        .get("result")
        .ok_or_else(|| "NotebookLM MCP missing result".to_string())?;

    if result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(format!("NotebookLM MCP tool error: {result}"));
    }

    let text = result
        .get("content")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find_map(|item| {
                let item_type = item.get("type").and_then(Value::as_str).unwrap_or_default();
                if item_type == "text" {
                    item.get("text").and_then(Value::as_str).map(str::to_string)
                } else {
                    None
                }
            })
        })
        .unwrap_or_default();

    if text.trim().is_empty() {
        return Ok(json!({}));
    }

    let parsed = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({ "text": text }));
    if let Some(error) = parsed.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_else(|| text.as_str());
        return Err(format!("NotebookLM MCP tool error: {message}"));
    }
    Ok(parsed)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            resolve_central_home,
            list_records,
            list_logs,
            get_dashboard_stats,
            upsert_record,
            delete_record,
            rebuild_search_index,
            search_records,
            run_ai_analysis,
            export_markdown_report,
            get_home_fingerprint,
            get_health_diagnostics,
            get_app_settings,
            save_app_settings,
            set_openai_api_key,
            has_openai_api_key,
            clear_openai_api_key,
            set_gemini_api_key,
            has_gemini_api_key,
            clear_gemini_api_key,
            set_claude_api_key,
            has_claude_api_key,
            clear_claude_api_key,
            set_notion_api_key,
            has_notion_api_key,
            clear_notion_api_key,
            sync_record_to_notion,
            sync_records_to_notion,
            sync_record_bidirectional,
            sync_records_bidirectional,
            pull_records_from_notion,
            notebooklm_health_check,
            notebooklm_list_notebooks,
            notebooklm_create_notebook,
            notebooklm_add_record_source,
            notebooklm_ask,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

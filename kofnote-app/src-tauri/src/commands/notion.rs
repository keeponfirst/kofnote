// Notion sync: commands and internal helpers (moved from types.rs)

use crate::commands::keychain::resolve_notion_api_key;
use crate::storage::index::upsert_index_record_if_exists;
use crate::storage::records::{
    normalized_home, load_records, persist_record_to_files, record_from_value,
};
use crate::storage::settings_io::load_settings;
use crate::types::{
    AppSettings, NotionBatchSyncResult, NotionRemoteRecord, NotionSyncResult, NotionUpsertInfo,
    Record, RECORD_TYPE_DIRS, NOTION_API_BASE_URL, NOTION_API_VERSION,
};
use crate::util::{
    absolute_path, extract_day, generate_filename, normalize_record_type, record_dir_by_type,
};
use chrono::Local;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Duration as StdDuration;

// ─────────────────────────────────────────────────────────────────────────────
// Public Tauri commands
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn sync_record_to_notion(
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
pub fn sync_records_to_notion(
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
pub fn sync_record_bidirectional(
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
pub fn sync_records_bidirectional(
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
pub fn pull_records_from_notion(
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

// ─────────────────────────────────────────────────────────────────────────────
// Internal: config & record loading
// ─────────────────────────────────────────────────────────────────────────────

fn resolve_notion_database_id(
    database_id: Option<String>,
    settings: &AppSettings,
) -> Result<String, String> {
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

pub(crate) fn load_record_by_json_path(central_home: &Path, json_path: &str) -> Result<Record, String> {
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

fn notion_fetch_database(
    database_id: &str,
    notion_api_key: &str,
    client: &Client,
) -> Result<Value, String> {
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

fn notion_extract_record_type_from_properties(
    properties: &serde_json::Map<String, Value>,
) -> String {
    if let Some(property) =
        notion_find_page_property_by_candidates(properties, &["Type", "Record Type"])
    {
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

fn notion_extract_tags_from_properties(
    properties: &serde_json::Map<String, Value>,
) -> Vec<String> {
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

fn notion_extract_date_from_properties(
    properties: &serde_json::Map<String, Value>,
) -> Option<String> {
    let property = notion_find_page_property_by_candidates(properties, &["Date"])?;
    let kind = property.get("type").and_then(Value::as_str).unwrap_or_default();
    match kind {
        "date" => property
            .get("date")
            .and_then(|item| item.get("start"))
            .and_then(Value::as_str)
            .map(str::to_string),
        "rich_text" => {
            let text =
                notion_plain_text_from_rich_text(property.get("rich_text").unwrap_or(&Value::Null));
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

fn notion_find_title_property_name(
    properties: &serde_json::Map<String, Value>,
) -> Option<String> {
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

    if let Some((name, kind)) =
        notion_find_property_by_candidates(properties_schema, &["Type", "Record Type"])
    {
        match kind.as_str() {
            "select" => {
                properties.insert(name, json!({ "select": { "name": record.record_type } }));
            }
            "rich_text" => {
                properties.insert(
                    name,
                    json!({ "rich_text": [{ "type": "text", "text": { "content": record.record_type } }] }),
                );
            }
            _ => {}
        }
    }

    if let Some((name, kind)) =
        notion_find_property_by_candidates(properties_schema, &["Tags", "Tag"])
    {
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

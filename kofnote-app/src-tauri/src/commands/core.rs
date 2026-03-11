use crate::storage::index::{
    delete_index_record_if_exists, index_db_path, upsert_index_record_if_exists,
};
use crate::storage::records::{
    detect_central_home_path, ensure_structure, load_logs, load_records, normalized_home,
    record_from_value, render_markdown,
};
use crate::types::{
    DashboardStats, DailyCount, LogEntry, Record, RecordPayload, ResolvedHome, TagCount,
};
use crate::util::{
    absolute_path, extract_day, generate_filename, normalize_record_type, record_dir_by_type,
    sanitize_date_filter, value_string, write_atomic,
};
use chrono::{Duration as ChronoDuration, Local};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub(crate) fn compute_dashboard_stats(records: &[Record], logs: &[LogEntry]) -> DashboardStats {
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

pub(crate) fn search_records_in_memory(
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

pub(crate) fn count_records_in_memory(
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
    if let Some(rt) = record_type {
        if record.record_type != rt {
            return false;
        }
    }
    if let Some(df) = date_from {
        let day = extract_day(&record.created_at).unwrap_or_default();
        if day.as_str() < df {
            return false;
        }
    }
    if let Some(dt) = date_to {
        let day = extract_day(&record.created_at).unwrap_or_default();
        if day.as_str() > dt {
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

#[tauri::command]
pub fn resolve_central_home(input_path: String) -> Result<ResolvedHome, String> {
    if input_path.trim().is_empty() {
        return Err("Central Home path is required".to_string());
    }
    let input = absolute_path(Path::new(input_path.trim()));
    let resolved = detect_central_home_path(&input);
    ensure_structure(&resolved).map_err(|e| e.to_string())?;
    Ok(ResolvedHome {
        central_home: resolved.to_string_lossy().to_string(),
        corrected: resolved != input,
    })
}

#[tauri::command]
pub fn list_records(central_home: String) -> Result<Vec<Record>, String> {
    let home = normalized_home(&central_home)?;
    load_records(&home)
}

#[tauri::command]
pub fn list_logs(central_home: String) -> Result<Vec<LogEntry>, String> {
    let home = normalized_home(&central_home)?;
    load_logs(&home)
}

#[tauri::command]
pub fn get_dashboard_stats(central_home: String) -> Result<DashboardStats, String> {
    let home = normalized_home(&central_home)?;
    let records = load_records(&home)?;
    let logs = load_logs(&home)?;
    Ok(compute_dashboard_stats(&records, &logs))
}

#[tauri::command]
pub fn upsert_record(
    central_home: String,
    payload: RecordPayload,
    previous_json_path: Option<String>,
) -> Result<Record, String> {
    let home = normalized_home(&central_home)?;
    ensure_structure(&home).map_err(|e| e.to_string())?;

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
    fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;

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
    let notion_last_synced_at = payload.notion_last_synced_at.or_else(|| {
        existing_value
            .as_ref()
            .and_then(|v| value_string(v, "notion_last_synced_at"))
    });
    let notion_last_edited_time = payload.notion_last_edited_time.or_else(|| {
        existing_value
            .as_ref()
            .and_then(|v| value_string(v, "notion_last_edited_time"))
    });
    let notion_last_synced_hash = payload.notion_last_synced_hash.or_else(|| {
        existing_value
            .as_ref()
            .and_then(|v| value_string(v, "notion_last_synced_hash"))
    });

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

    let json_bytes = serde_json::to_vec_pretty(&persisted).map_err(|e| e.to_string())?;
    write_atomic(&json_path, &json_bytes).map_err(|e| e.to_string())?;

    let record = record_from_value(&persisted, Some(json_path.clone()), Some(md_path.clone()), None);
    let markdown = render_markdown(&record);
    write_atomic(&md_path, markdown.as_bytes()).map_err(|e| e.to_string())?;

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
pub fn delete_record(central_home: String, json_path: String) -> Result<(), String> {
    let home = normalized_home(&central_home)?;
    let path = absolute_path(Path::new(json_path.trim()));
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    let md_path = path.with_extension("md");
    if md_path.exists() {
        fs::remove_file(md_path).map_err(|e| e.to_string())?;
    }
    let _ = delete_index_record_if_exists(&home, &path.to_string_lossy());
    Ok(())
}

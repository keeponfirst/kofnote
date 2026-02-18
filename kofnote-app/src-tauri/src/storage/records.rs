use crate::{types::{LogEntry, Record, RECORD_TYPE_DIRS}, util::*};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn load_records(central_home: &Path) -> Result<Vec<Record>, String> {
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

pub(crate) fn load_logs(central_home: &Path) -> Result<Vec<LogEntry>, String> {
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

pub(crate) fn record_from_value(
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

pub(crate) fn persist_record_to_files(
    record: &Record,
    json_path: &Path,
    md_path: &Path,
) -> Result<(), String> {
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

pub(crate) fn render_markdown(record: &Record) -> String {
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

pub(crate) fn detect_central_home_path(candidate: &Path) -> PathBuf {
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

pub(crate) fn is_central_home(path: &Path) -> bool {
    path.join(".agentic").join("CENTRAL_LOG_MARKER").exists()
        || path.join("records").exists()
        || path.join(".agentic").join("logs").exists()
}

pub(crate) fn ensure_structure(central_home: &Path) -> std::io::Result<()> {
    let records_root = central_home.join("records");
    fs::create_dir_all(&records_root)?;
    for (_, folder) in RECORD_TYPE_DIRS {
        fs::create_dir_all(records_root.join(folder))?;
    }
    fs::create_dir_all(records_root.join("debates"))?;
    fs::create_dir_all(central_home.join(".agentic").join("logs"))?;
    fs::create_dir_all(central_home.join("prompts").join("profiles"))?;
    fs::create_dir_all(central_home.join("prompts").join("templates"))?;
    Ok(())
}

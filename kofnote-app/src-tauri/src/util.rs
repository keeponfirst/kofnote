use crate::types::RECORD_TYPE_DIRS;
use chrono::{DateTime, Local, NaiveDate};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn parse_tags(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

pub(crate) fn option_non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(crate) fn compare_iso_desc(a: &str, b: &str) -> std::cmp::Ordering {
    b.cmp(a)
}

pub(crate) fn sanitize_date_filter(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| item.len() == 10)
        .filter(|item| NaiveDate::parse_from_str(item, "%Y-%m-%d").is_ok())
}

pub(crate) fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

pub(crate) fn normalize_record_type(record_type: &str) -> String {
    let lower = record_type.trim().to_lowercase();
    if RECORD_TYPE_DIRS.iter().any(|(item, _)| *item == lower) {
        lower
    } else {
        "note".to_string()
    }
}

pub(crate) fn record_dir_by_type(record_type: &str) -> &'static str {
    for (item, dir) in RECORD_TYPE_DIRS {
        if item == record_type {
            return dir;
        }
    }
    "other"
}

pub(crate) fn slugify(value: &str) -> String {
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

pub(crate) fn generate_filename(record_type: &str, title: &str) -> String {
    let stamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let slug = slugify(title);
    format!("{stamp}_{record_type}_{slug}")
}

pub(crate) fn file_mtime_iso(path: &Path) -> String {
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

pub(crate) fn extract_day(value: &str) -> Option<String> {
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

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)?;
    Ok(())
}

pub(crate) fn value_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

pub(crate) fn value_string_array(value: &Value, key: &str) -> Vec<String> {
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

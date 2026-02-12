#![allow(dead_code)]

use crate::types::{LogEntry, Record};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn load_records(central_home: &Path) -> Result<Vec<Record>, String> {
    crate::types::load_records(central_home)
}

pub fn load_logs(central_home: &Path) -> Result<Vec<LogEntry>, String> {
    crate::types::load_logs(central_home)
}

pub fn record_from_value(
    value: &Value,
    json_path: Option<PathBuf>,
    md_path: Option<PathBuf>,
    record_type_override: Option<String>,
) -> Record {
    crate::types::record_from_value(value, json_path, md_path, record_type_override)
}

pub fn persist_record_to_files(record: &Record, json_path: &Path, md_path: &Path) -> Result<(), String> {
    crate::types::persist_record_to_files(record, json_path, md_path)
}

pub fn render_markdown(record: &Record) -> String {
    crate::types::render_markdown(record)
}

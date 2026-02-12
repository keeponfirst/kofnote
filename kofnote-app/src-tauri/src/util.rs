#![allow(dead_code)]

use std::cmp::Ordering;
use std::path::Path;

pub fn slugify(value: &str) -> String {
    crate::types::slugify(value)
}

pub fn generate_filename(record_type: &str, title: &str) -> String {
    crate::types::generate_filename(record_type, title)
}

pub fn file_mtime_iso(path: &Path) -> String {
    crate::types::file_mtime_iso(path)
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    crate::types::write_atomic(path, bytes)
}

pub fn compare_iso_desc(a: &str, b: &str) -> Ordering {
    crate::types::compare_iso_desc(a, b)
}

pub fn parse_tags(raw: &str) -> Vec<String> {
    crate::types::parse_tags(raw)
}

pub fn option_non_empty(value: String) -> Option<String> {
    crate::types::option_non_empty(value)
}

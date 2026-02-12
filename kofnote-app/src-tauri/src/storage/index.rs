#![allow(dead_code)]

use crate::types::Record;
use rusqlite::Connection;
use std::path::Path;

pub fn open_index_connection(central_home: &Path) -> Result<Connection, String> {
    crate::types::open_index_connection(central_home)
}

pub fn ensure_index_schema(conn: &Connection) -> Result<(), String> {
    crate::types::ensure_index_schema(conn)
}

pub fn rebuild_index(central_home: &Path, records: &[Record]) -> Result<usize, String> {
    crate::types::rebuild_index(central_home, records)
}

pub fn search_records_in_index(
    central_home: &Path,
    query: &str,
    record_type: Option<&str>,
    date_from: Option<&str>,
    date_to: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<(Vec<Record>, usize), String> {
    crate::types::search_records_in_index(
        central_home,
        query,
        record_type,
        date_from,
        date_to,
        limit,
        offset,
    )
}

pub fn upsert_index_record_if_exists(central_home: &Path, record: &Record) -> Result<(), String> {
    crate::types::upsert_index_record_if_exists(central_home, record)
}

pub fn delete_index_record_if_exists(central_home: &Path, json_path: &str) -> Result<(), String> {
    crate::types::delete_index_record_if_exists(central_home, json_path)
}

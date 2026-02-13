use crate::{types::{Record, SEARCH_DB_FILE}, util::*};
use chrono::Local;
use rusqlite::{params, params_from_iter, Connection};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn open_index_connection(central_home: &Path) -> Result<Connection, String> {
    let path = index_db_path(central_home);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    Connection::open(path).map_err(|error| error.to_string())
}

pub(crate) fn ensure_index_schema(conn: &Connection) -> Result<(), String> {
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
         );
         CREATE TABLE IF NOT EXISTS debate_runs (
            run_id TEXT PRIMARY KEY,
            output_type TEXT NOT NULL,
            problem TEXT NOT NULL,
            consensus_score REAL NOT NULL,
            confidence_score REAL NOT NULL,
            selected_option TEXT NOT NULL,
            degraded INTEGER NOT NULL DEFAULT 0,
            started_at TEXT NOT NULL,
            finished_at TEXT NOT NULL,
            artifacts_root TEXT NOT NULL,
            final_packet_path TEXT NOT NULL,
            writeback_json_path TEXT
         );
         CREATE TABLE IF NOT EXISTS debate_turns (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id TEXT NOT NULL,
            round_number INTEGER NOT NULL,
            role TEXT NOT NULL,
            provider TEXT NOT NULL,
            model_name TEXT NOT NULL,
            status TEXT NOT NULL,
            claim TEXT NOT NULL,
            rationale TEXT NOT NULL,
            challenges_json TEXT NOT NULL,
            revisions_json TEXT NOT NULL,
            error_code TEXT,
            error_message TEXT,
            duration_ms INTEGER NOT NULL DEFAULT 0,
            started_at TEXT NOT NULL,
            finished_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_debate_turns_run_id ON debate_turns(run_id);
         CREATE TABLE IF NOT EXISTS debate_actions (
            run_id TEXT NOT NULL,
            action_id TEXT NOT NULL,
            action TEXT NOT NULL,
            owner TEXT NOT NULL,
            due TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'OPEN',
            PRIMARY KEY (run_id, action_id)
         );",
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn rebuild_index(central_home: &Path, records: &[Record]) -> Result<usize, String> {
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

pub(crate) fn search_records_in_index(
    central_home: &Path,
    query: &str,
    record_type: Option<&str>,
    date_from: Option<&str>,
    date_to: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<(Vec<Record>, usize, HashMap<String, String>), String> {
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
            notion_error,
            snippet(records_fts, 2, '<mark>', '</mark>', '...', 32) AS snippet
        FROM records_fts
        {where_sql}
        ORDER BY bm25(records_fts), created_at DESC
        LIMIT {limit} OFFSET {offset}"
    );

    let mut stmt = conn.prepare(&select_sql).map_err(|error| error.to_string())?;
    let mut rows = stmt
        .query_map(params_from_iter(bindings.iter()), |row| {
            let tags_raw: String = row.get(6)?;
            let record = Record {
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
            };
            let snippet: String = row.get::<_, String>(13).unwrap_or_default();
            Ok((record, snippet))
        })
        .map_err(|error| error.to_string())?;

    let mut records = Vec::new();
    let mut snippets = HashMap::new();
    for row in rows.by_ref() {
        let (record, snippet) = row.map_err(|error| error.to_string())?;
        if let Some(json_path) = &record.json_path {
            if !snippet.trim().is_empty() {
                snippets.insert(json_path.clone(), snippet);
            }
        }
        records.push(record);
    }

    Ok((records, total, snippets))
}

pub(crate) fn get_index_count(central_home: &Path) -> Result<usize, String> {
    let conn = open_index_connection(central_home)?;
    ensure_index_schema(&conn)?;
    conn.query_row("SELECT COUNT(*) FROM records_fts", [], |row| row.get(0))
        .map_err(|error| error.to_string())
}

pub(crate) fn upsert_index_record_if_exists(central_home: &Path, record: &Record) -> Result<(), String> {
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

pub(crate) fn delete_index_record_if_exists(central_home: &Path, json_path: &str) -> Result<(), String> {
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

pub(crate) fn index_db_path(central_home: &Path) -> PathBuf {
    central_home.join(".agentic").join(SEARCH_DB_FILE)
}

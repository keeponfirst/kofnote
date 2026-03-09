use crate::storage::index::{
    load_all_memory_items, resolve_workspace_from_central_home, search_memory_in_index,
    search_records_in_index,
};
use crate::storage::records::load_records;
use crate::types::{
    normalized_home, RebuildIndexResult, Record, SearchResult, TimelineGroup, TimelineResponse,
    UnifiedMemoryItem, UnifiedSearchResult,
};
use std::collections::HashMap;
use std::time::Instant;
use chrono::Datelike;


#[tauri::command]
pub fn rebuild_search_index(central_home: String) -> Result<RebuildIndexResult, String> {
    crate::types::rebuild_search_index(central_home)
}

#[tauri::command]
pub fn search_records(
    central_home: String,
    query: Option<String>,
    record_type: Option<String>,
    date_from: Option<String>,
    date_to: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<SearchResult, String> {
    crate::types::search_records(
        central_home,
        query,
        record_type,
        date_from,
        date_to,
        limit,
        offset,
    )
}

// --- Second Brain P0: Unified Search + Timeline ---

#[tauri::command]
pub fn unified_search(
    central_home: String,
    query: String,
    sources: Option<Vec<String>>,
    date_from: Option<String>,
    date_to: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<UnifiedSearchResult, String> {
    let started = Instant::now();
    let home = normalized_home(&central_home)?;
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let offset = offset.unwrap_or(0);
    let query = query.trim().to_string();

    if query.is_empty() {
        return Err("Query cannot be empty".to_string());
    }

    let sources = sources.unwrap_or_else(|| vec!["records".to_string(), "memory".to_string()]);
    let search_records = sources.iter().any(|s| s == "records");
    let search_memory = sources.iter().any(|s| s == "memory");

    let mut all_items: Vec<UnifiedMemoryItem> = Vec::new();
    let mut source_counts: HashMap<String, usize> = HashMap::new();

    // Search records
    if search_records {
        if let Ok((records, _total, snippets)) = search_records_in_index(
            &home,
            &query,
            None,
            date_from.as_deref(),
            date_to.as_deref(),
            limit,
            0,
        ) {
            let count = records.len();
            for record in records {
                let snippet = record
                    .json_path
                    .as_ref()
                    .and_then(|jp| snippets.get(jp))
                    .cloned()
                    .unwrap_or_default();
                all_items.push(record_to_unified_item(&record, &snippet));
            }
            source_counts.insert("records".to_string(), count);
        }
    }

    // Search memory
    if search_memory {
        if let Some(workspace) = resolve_workspace_from_central_home(&home) {
            if let Ok((memory_items, _total)) = search_memory_in_index(
                &home,
                &query,
                date_from.as_deref(),
                date_to.as_deref(),
                limit,
                0,
            ) {
                let count = memory_items.len();
                all_items.extend(memory_items);
                source_counts.insert("memory".to_string(), count);
            }
        }
    }

    // Sort by created_at descending
    all_items.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let total = all_items.len();
    // Apply offset + limit on merged results
    let items: Vec<UnifiedMemoryItem> = all_items.into_iter().skip(offset).take(limit).collect();

    Ok(UnifiedSearchResult {
        items,
        total,
        took_ms: started.elapsed().as_millis(),
        source_counts,
    })
}

#[tauri::command]
pub fn get_timeline(
    central_home: String,
    group_by: String,
    sources: Option<Vec<String>>,
    date_from: Option<String>,
    date_to: Option<String>,
    limit: Option<usize>,
) -> Result<TimelineResponse, String> {
    let home = normalized_home(&central_home)?;
    let limit = limit.unwrap_or(30);
    let sources = sources.unwrap_or_else(|| vec!["records".to_string(), "memory".to_string()]);

    let mut all_items: Vec<UnifiedMemoryItem> = Vec::new();

    // Load records
    if sources.iter().any(|s| s == "records") {
        if let Ok(records) = load_records(&home) {
            for record in &records {
                all_items.push(record_to_unified_item(record, ""));
            }
        }
    }

    // Load memory
    if sources.iter().any(|s| s == "memory") {
        if let Some(workspace) = resolve_workspace_from_central_home(&home) {
            all_items.extend(load_all_memory_items(&workspace));
        }
    }

    // Filter by date range
    if let Some(ref df) = date_from {
        all_items.retain(|item| &item.created_at[..10.min(item.created_at.len())] >= df.as_str());
    }
    if let Some(ref dt) = date_to {
        all_items.retain(|item| &item.created_at[..10.min(item.created_at.len())] <= dt.as_str());
    }

    // Sort by created_at descending
    all_items.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // Group
    let groups = group_items(all_items, &group_by, limit);
    let total_items: usize = groups.iter().map(|g| g.count).sum();

    Ok(TimelineResponse {
        total_groups: groups.len(),
        total_items,
        groups,
    })
}

fn record_to_unified_item(record: &Record, snippet: &str) -> UnifiedMemoryItem {
    let mut metadata = serde_json::Map::new();
    if let Some(ref url) = record.notion_url {
        metadata.insert("notionUrl".to_string(), serde_json::json!(url));
    }
    if let Some(ref jp) = record.json_path {
        metadata.insert("jsonPath".to_string(), serde_json::json!(jp));
    }

    let display_snippet = if snippet.is_empty() {
        // Truncate final_body for display
        let chars: String = record.final_body.chars().take(200).collect();
        if record.final_body.chars().count() > 200 {
            format!("{chars}...")
        } else {
            chars
        }
    } else {
        snippet.to_string()
    };

    UnifiedMemoryItem {
        id: record
            .json_path
            .clone()
            .unwrap_or_else(|| record.title.clone()),
        source: "record".to_string(),
        source_type: record.record_type.clone(),
        title: record.title.clone(),
        snippet: display_snippet,
        body: record.final_body.clone(),
        created_at: record.created_at.clone(),
        tags: record.tags.clone(),
        relevance_score: None,
        metadata: if metadata.is_empty() {
            None
        } else {
            Some(serde_json::Value::Object(metadata))
        },
    }
}

fn group_items(
    items: Vec<UnifiedMemoryItem>,
    group_by: &str,
    limit: usize,
) -> Vec<TimelineGroup> {
    let mut groups_map: std::collections::BTreeMap<String, Vec<UnifiedMemoryItem>> =
        std::collections::BTreeMap::new();

    for item in items {
        let key = extract_group_key(&item.created_at, group_by);
        groups_map.entry(key).or_default().push(item);
    }

    // BTreeMap is sorted ascending; reverse for newest-first
    let mut groups: Vec<TimelineGroup> = groups_map
        .into_iter()
        .rev()
        .take(limit)
        .map(|(key, items)| {
            let mut source_counts = HashMap::new();
            for item in &items {
                *source_counts
                    .entry(item.source.clone())
                    .or_insert(0usize) += 1;
            }
            TimelineGroup {
                label: key.clone(),
                date: key,
                count: items.len(),
                items,
                source_counts,
            }
        })
        .collect();

    groups
}

fn extract_group_key(created_at: &str, group_by: &str) -> String {
    // created_at is ISO 8601: "2026-02-28T10:30:00Z" or "2026-02-28T10:30:00+08:00"
    let date_part = &created_at[..10.min(created_at.len())];

    match group_by {
        "week" => {
            // Parse date and get ISO week
            if let Ok(date) = chrono::NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
                let iso_week = date.iso_week();
                format!("{}-W{:02}", iso_week.year(), iso_week.week())
            } else {
                date_part.to_string()
            }
        }
        "month" => {
            // "2026-02-28" → "2026-02"
            date_part[..7.min(date_part.len())].to_string()
        }
        _ => {
            // "day" (default)
            date_part.to_string()
        }
    }
}

use crate::types::{RebuildIndexResult, SearchResult};

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

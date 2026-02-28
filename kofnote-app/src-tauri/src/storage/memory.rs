use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

use crate::types::UnifiedMemoryItem;

/// Parsed representation of a memory/*.md file.
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub file_path: PathBuf,
    pub title: String,
    pub body: String,
    pub created_at: String,
    pub source_type: String, // "session" | "daily"
    pub openclaw_source: Option<String>, // "telegram" | "webchat" | etc.
    pub session_id: Option<String>,
}

/// Parse a single memory markdown file into a MemoryEntry.
/// Returns None if the file cannot be read or is empty.
pub fn parse_memory_file(path: &Path) -> Option<MemoryEntry> {
    let content = fs::read_to_string(path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }

    let file_name = path.file_stem()?.to_str()?;

    // Try session format: "# Session: 2026-02-21 09:37:59 UTC"
    if let Some(entry) = try_parse_session(path, file_name, trimmed) {
        return Some(entry);
    }

    // Fallback: daily summary format (no Session header)
    Some(parse_daily(path, file_name, trimmed))
}

/// Parse session format:
/// ```
/// # Session: 2026-02-21 09:37:59 UTC
///
/// - **Session Key**: agent:main:main
/// - **Session ID**: uuid
/// - **Source**: telegram
///
/// ## Conversation Summary
/// ...
/// ```
fn try_parse_session(path: &Path, file_name: &str, content: &str) -> Option<MemoryEntry> {
    let first_line = content.lines().next()?;

    // Match "# Session: YYYY-MM-DD HH:MM:SS UTC"
    let timestamp_str = first_line.strip_prefix("# Session:")?;
    let timestamp_str = timestamp_str.trim();

    // Convert "2026-02-21 09:37:59 UTC" → "2026-02-21T09:37:59Z"
    let created_at = parse_session_timestamp(timestamp_str)?;

    // Extract metadata from bullet lines
    let mut openclaw_source: Option<String> = None;
    let mut session_id: Option<String> = None;
    let mut body_start = 0;

    for (i, line) in content.lines().enumerate() {
        if line.starts_with("- **Source**:") {
            if let Some(val) = line.split(':').nth(1) {
                let val = val.trim().trim_matches('*').trim();
                if !val.is_empty() {
                    openclaw_source = Some(val.to_string());
                }
            }
        } else if line.starts_with("- **Session ID**:") {
            if let Some(val) = line.split("**:").nth(1) {
                let val = val.trim();
                if !val.is_empty() {
                    session_id = Some(val.to_string());
                }
            }
        } else if line.starts_with("## ") {
            // Body starts at the first ## heading (e.g. "## Conversation Summary")
            body_start = i;
            break;
        }
    }

    let body = if body_start > 0 {
        content
            .lines()
            .skip(body_start)
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    } else {
        // No ## section — use everything after header block
        content
            .lines()
            .skip_while(|l| l.starts_with('#') || l.starts_with("- **") || l.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    };

    let title = format!("Session {}", &created_at[..10]);

    Some(MemoryEntry {
        file_path: path.to_path_buf(),
        title,
        body,
        created_at,
        source_type: "session".to_string(),
        openclaw_source,
        session_id,
    })
}

/// Parse daily summary format (no # Session header).
/// Filename: 2026-02-18.md or 2026-02-20.md
fn parse_daily(path: &Path, file_name: &str, content: &str) -> MemoryEntry {
    // Extract date from filename: "2026-02-18" → "2026-02-18T00:00:00Z"
    let date_part = &file_name[..10.min(file_name.len())];
    let created_at = format!("{date_part}T00:00:00Z");

    // Try to extract title from first heading
    let title = content
        .lines()
        .find(|l| l.starts_with('#'))
        .map(|l| l.trim_start_matches('#').trim().to_string())
        .unwrap_or_else(|| format!("Memory {date_part}"));

    MemoryEntry {
        file_path: path.to_path_buf(),
        title,
        body: content.to_string(),
        created_at,
        source_type: "daily".to_string(),
        openclaw_source: None,
        session_id: None,
    }
}

/// Convert "2026-02-21 09:37:59 UTC" → "2026-02-21T09:37:59Z"
fn parse_session_timestamp(raw: &str) -> Option<String> {
    let raw = raw.trim().trim_end_matches("UTC").trim();
    // Expect "YYYY-MM-DD HH:MM:SS"
    if raw.len() >= 19 && raw.chars().nth(4) == Some('-') && raw.chars().nth(10) == Some(' ') {
        let iso = format!("{}T{}Z", &raw[..10], &raw[11..19]);
        Some(iso)
    } else {
        None
    }
}

/// Load all memory/*.md files from the workspace directory.
pub fn load_all_memory_files(workspace: &Path) -> Vec<MemoryEntry> {
    let memory_dir = workspace.join("memory");
    if !memory_dir.is_dir() {
        return Vec::new();
    }

    let mut entries = Vec::new();
    if let Ok(read_dir) = fs::read_dir(&memory_dir) {
        for dir_entry in read_dir.flatten() {
            let path = dir_entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                if let Some(entry) = parse_memory_file(&path) {
                    entries.push(entry);
                }
            }
        }
    }

    // Sort by created_at descending (newest first)
    entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    entries
}

/// Convert a MemoryEntry to a UnifiedMemoryItem for search/timeline use.
pub fn memory_entry_to_unified_item(entry: &MemoryEntry, snippet_len: usize) -> UnifiedMemoryItem {
    let snippet = truncate_body(&entry.body, snippet_len);

    let mut metadata = serde_json::Map::new();
    if let Some(ref src) = entry.openclaw_source {
        metadata.insert("openclawSource".to_string(), json!(src));
    }
    if let Some(ref sid) = entry.session_id {
        metadata.insert("sessionId".to_string(), json!(sid));
    }

    let meta_val = if metadata.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(metadata))
    };

    UnifiedMemoryItem {
        id: entry.file_path.to_string_lossy().to_string(),
        source: "memory".to_string(),
        source_type: entry.source_type.clone(),
        title: entry.title.clone(),
        snippet,
        body: entry.body.clone(),
        created_at: entry.created_at.clone(),
        tags: Vec::new(),
        relevance_score: None,
        metadata: meta_val,
    }
}

fn truncate_body(body: &str, max_chars: usize) -> String {
    // Strip markdown formatting noise for snippet
    let clean: String = body
        .lines()
        .filter(|l| !l.starts_with("```") && !l.starts_with("## "))
        .take(10)
        .collect::<Vec<_>>()
        .join(" ");

    if clean.chars().count() <= max_chars {
        clean
    } else {
        let truncated: String = clean.chars().take(max_chars).collect();
        format!("{truncated}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_temp_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn test_parse_session_format() {
        let dir = TempDir::new().unwrap();
        let content = r#"# Session: 2026-02-21 09:37:59 UTC

- **Session Key**: agent:main:main
- **Session ID**: 5fc99979-f954-4d1d-b705-372bdb6bf81d
- **Source**: telegram

## Conversation Summary

user: 給我現在用量
assistant: 目前用量 15k in / 33 out
"#;
        let path = write_temp_file(dir.path(), "2026-02-21-0937.md", content);
        let entry = parse_memory_file(&path).unwrap();

        assert_eq!(entry.created_at, "2026-02-21T09:37:59Z");
        assert_eq!(entry.source_type, "session");
        assert_eq!(entry.openclaw_source.as_deref(), Some("telegram"));
        assert_eq!(entry.session_id.as_deref(), Some("5fc99979-f954-4d1d-b705-372bdb6bf81d"));
        assert!(entry.body.contains("給我現在用量"));
        assert_eq!(entry.title, "Session 2026-02-21");
    }

    #[test]
    fn test_parse_daily_format() {
        let dir = TempDir::new().unwrap();
        let content = r#"
## 2026-02-18 任務摘要 (傳圖實驗)
- Telegram 傳圖問題排查
- 設定檔修正
"#;
        let path = write_temp_file(dir.path(), "2026-02-18.md", content);
        let entry = parse_memory_file(&path).unwrap();

        assert_eq!(entry.created_at, "2026-02-18T00:00:00Z");
        assert_eq!(entry.source_type, "daily");
        assert!(entry.openclaw_source.is_none());
        assert!(entry.body.contains("Telegram 傳圖問題排查"));
    }

    #[test]
    fn test_parse_minimal_session() {
        let dir = TempDir::new().unwrap();
        let content = r#"# Session: 2026-02-15 08:24:48 UTC

- **Session Key**: agent:main:main
- **Session ID**: 12b5cfd3-6f7b-4435-9956-a4474722ab26
- **Source**: webchat
"#;
        let path = write_temp_file(dir.path(), "2026-02-15-0824.md", content);
        let entry = parse_memory_file(&path).unwrap();

        assert_eq!(entry.created_at, "2026-02-15T08:24:48Z");
        assert_eq!(entry.openclaw_source.as_deref(), Some("webchat"));
        assert!(entry.body.is_empty());
    }

    #[test]
    fn test_empty_file_returns_none() {
        let dir = TempDir::new().unwrap();
        let path = write_temp_file(dir.path(), "empty.md", "   ");
        assert!(parse_memory_file(&path).is_none());
    }

    #[test]
    fn test_load_all_memory_files() {
        let dir = TempDir::new().unwrap();
        let mem_dir = dir.path().join("memory");
        fs::create_dir_all(&mem_dir).unwrap();

        write_temp_file(&mem_dir, "2026-02-20.md", "# 2026-02-20\n- item 1\n");
        write_temp_file(&mem_dir, "2026-02-21-0937.md", "# Session: 2026-02-21 09:37:59 UTC\n\n- **Source**: telegram\n\n## Summary\nHello\n");
        write_temp_file(&mem_dir, "not-markdown.txt", "skip me");

        let entries = load_all_memory_files(dir.path());
        assert_eq!(entries.len(), 2);
        // Newest first
        assert_eq!(entries[0].created_at, "2026-02-21T09:37:59Z");
        assert_eq!(entries[1].created_at, "2026-02-20T00:00:00Z");
    }

    #[test]
    fn test_memory_entry_to_unified_item() {
        let entry = MemoryEntry {
            file_path: PathBuf::from("memory/2026-02-20.md"),
            title: "2026-02-20".to_string(),
            body: "Some content here".to_string(),
            created_at: "2026-02-20T00:00:00Z".to_string(),
            source_type: "daily".to_string(),
            openclaw_source: None,
            session_id: None,
        };
        let item = memory_entry_to_unified_item(&entry, 200);
        assert_eq!(item.source, "memory");
        assert_eq!(item.source_type, "daily");
        assert_eq!(item.id, "memory/2026-02-20.md");
        assert!(item.metadata.is_none());
    }
}

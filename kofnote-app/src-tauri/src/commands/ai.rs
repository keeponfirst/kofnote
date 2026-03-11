use crate::commands::core::compute_dashboard_stats;
use crate::storage::records::{load_logs, load_records, normalized_home};
use crate::types::{AiAnalysisResponse, LogEntry, Record, OPENAI_RESPONSES_URL};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::time::Duration as StdDuration;

fn build_context_digest(
    records: &[Record],
    logs: &[LogEntry],
    include_logs: bool,
    max_records: usize,
) -> String {
    let mut lines = vec!["# Records".to_string()];
    for item in records.iter().take(max_records) {
        lines.push(format!(
            "- [{}] ({}) {} | tags: {}",
            item.created_at,
            item.record_type,
            item.title,
            if item.tags.is_empty() {
                "-".to_string()
            } else {
                item.tags.join(", ")
            }
        ));
    }
    if include_logs {
        lines.push(String::new());
        lines.push("# Logs".to_string());
        for item in logs.iter().take(max_records.min(40)) {
            lines.push(format!(
                "- [{}] {} / {} / {}",
                item.timestamp, item.task_intent, item.status, item.title
            ));
        }
    }
    lines.join("\n")
}

fn run_local_analysis(prompt: &str, records: &[Record], logs: &[LogEntry]) -> String {
    let stats = compute_dashboard_stats(records, logs);
    let dominant_type = stats
        .type_counts
        .iter()
        .max_by_key(|(_, c)| *c)
        .map(|(name, _)| name.as_str())
        .unwrap_or("-");

    let mut lines = vec![
        "# KOF Local Analysis".to_string(),
        String::new(),
        "## Summary".to_string(),
        format!("- Total records: {}", stats.total_records),
        format!("- Total logs: {}", stats.total_logs),
        format!("- Pending sync records: {}", stats.pending_sync_count),
        format!("- Dominant type: {}", dominant_type),
        String::new(),
        "## Top Tags".to_string(),
    ];
    if stats.top_tags.is_empty() {
        lines.push("- (no tags yet)".to_string());
    } else {
        for item in stats.top_tags.iter().take(8) {
            lines.push(format!("- {} ({})", item.tag, item.count));
        }
    }
    lines.push(String::new());
    lines.push("## Recent Focus".to_string());
    for item in records.iter().take(6) {
        lines.push(format!(
            "- [{}] ({}) {}",
            item.created_at, item.record_type, item.title
        ));
    }
    lines.push(String::new());
    lines.push("## Risks".to_string());
    if stats.pending_sync_count > 0 {
        lines.push("- Pending sync records may diverge from Notion until re-synced.".to_string());
    } else {
        lines.push("- No immediate sync risk detected.".to_string());
    }
    lines.push("- If many backlogs have no date/tag, prioritization quality may drop.".to_string());
    lines.push(String::new());
    lines.push("## Next 7 Days Action Plan".to_string());
    lines.push("1. Consolidate top recurring tags into 2-3 execution themes.".to_string());
    lines.push("2. Convert high-value backlog items to scheduled worklogs.".to_string());
    lines.push("3. Run weekly review and archive stale notes.".to_string());
    if !prompt.trim().is_empty() {
        lines.push(String::new());
        lines.push("## User Prompt Focus".to_string());
        lines.push(prompt.trim().to_string());
    }
    lines.join("\n")
}

fn run_openai_analysis(
    model: &str,
    prompt: &str,
    api_key: Option<String>,
    records: &[Record],
    logs: &[LogEntry],
    include_logs: bool,
    max_records: usize,
) -> Result<String, String> {
    let api_key = crate::providers::openai::resolve_api_key(api_key)?;
    let context = build_context_digest(records, logs, include_logs, max_records);
    let user_prompt = if prompt.trim().is_empty() {
        "Summarize patterns, risks, and a practical 7-day plan.".to_string()
    } else {
        prompt.trim().to_string()
    };
    let merged_prompt = format!(
        "You are analyzing a local-first productivity brain system. \nReturn concise markdown sections: Summary, Patterns, Risks, Action Plan (7 days).\n\nUser request:\n{}\n\nContext:\n{}",
        user_prompt, context
    );
    let payload = json!({
        "model": model,
        "input": [
            {
                "role": "user",
                "content": [
                    { "type": "input_text", "text": merged_prompt }
                ]
            }
        ]
    });
    let client = Client::builder()
        .timeout(StdDuration::from_secs(50))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .post(OPENAI_RESPONSES_URL)
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .map_err(|e| e.to_string())?;
    let status = response.status();
    let body_text = response.text().map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("OpenAI API {}: {}", status.as_u16(), body_text));
    }
    let value: Value = serde_json::from_str(&body_text).map_err(|e| e.to_string())?;
    let output = crate::providers::openai::extract_openai_output_text(&value);
    if output.trim().is_empty() {
        return Err("OpenAI response did not include readable text".to_string());
    }
    Ok(output)
}

fn run_gemini_analysis(
    model: &str,
    prompt: &str,
    records: &[Record],
    logs: &[LogEntry],
    include_logs: bool,
    max_records: usize,
) -> Result<String, String> {
    let context = build_context_digest(records, logs, include_logs, max_records);
    let user_prompt = if prompt.trim().is_empty() {
        "Summarize patterns, risks, and a practical 7-day plan.".to_string()
    } else {
        prompt.trim().to_string()
    };
    let merged = format!(
        "You are analyzing a local-first productivity brain system.\nReturn concise markdown sections: Summary, Patterns, Risks, Action Plan (7 days).\n\nUser request:\n{}\n\nContext:\n{}",
        user_prompt, context
    );
    let model = if model.trim().is_empty() {
        "gemini-2.0-flash"
    } else {
        model.trim()
    };
    crate::providers::gemini::run_gemini_text_completion(model, &merged, 60, 4096)
}

fn run_claude_analysis(
    model: &str,
    prompt: &str,
    records: &[Record],
    logs: &[LogEntry],
    include_logs: bool,
    max_records: usize,
) -> Result<String, String> {
    let context = build_context_digest(records, logs, include_logs, max_records);
    let user_prompt = if prompt.trim().is_empty() {
        "Summarize patterns, risks, and a practical 7-day plan.".to_string()
    } else {
        prompt.trim().to_string()
    };
    let merged = format!(
        "You are analyzing a local-first productivity brain system.\nReturn concise markdown sections: Summary, Patterns, Risks, Action Plan (7 days).\n\nUser request:\n{}\n\nContext:\n{}",
        user_prompt, context
    );
    let model = if model.trim().is_empty() {
        "claude-3-5-sonnet-latest"
    } else {
        model.trim()
    };
    crate::providers::claude::run_claude_text_completion(model, &merged, 60, 4096)
}

#[tauri::command]
pub fn run_ai_analysis(
    central_home: String,
    provider: Option<String>,
    model: Option<String>,
    prompt: String,
    api_key: Option<String>,
    include_logs: Option<bool>,
    max_records: Option<usize>,
) -> Result<AiAnalysisResponse, String> {
    let home = normalized_home(&central_home)?;
    let provider = provider
        .unwrap_or_else(|| "local".to_string())
        .trim()
        .to_lowercase();
    let model = model.unwrap_or_else(|| "gpt-4.1-mini".to_string());
    let records = load_records(&home)?;
    let logs = load_logs(&home)?;
    let include_logs = include_logs.unwrap_or(true);
    let max_records = max_records.unwrap_or(30).clamp(1, 200);

    let content = match provider.as_str() {
        "openai" => run_openai_analysis(&model, &prompt, api_key, &records, &logs, include_logs, max_records)?,
        "gemini" => run_gemini_analysis(&model, &prompt, &records, &logs, include_logs, max_records)?,
        "claude" => run_claude_analysis(&model, &prompt, &records, &logs, include_logs, max_records)?,
        "local" => run_local_analysis(&prompt, &records, &logs),
        _ => return Err(format!("Unsupported provider: {provider}")),
    };

    Ok(AiAnalysisResponse {
        provider,
        model,
        content,
    })
}

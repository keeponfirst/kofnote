use crate::storage::records::normalized_home;
use crate::storage::settings_io::load_settings;
use crate::commands::notion::load_record_by_json_path;
use crate::types::{
    NotebookLmAskResult, NotebookLmConfig, NotebookSummary, Record,
    DEFAULT_NOTEBOOKLM_ARGS, DEFAULT_NOTEBOOKLM_COMMAND,
};
use crate::util::extract_day;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration as StdDuration, Instant};
use std::process::{Command, Stdio};

fn parse_notebook_summary(value: &Value) -> NotebookSummary {
    let id = value
        .get("id")
        .or_else(|| value.get("notebook_id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let name = value
        .get("name")
        .or_else(|| value.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("Untitled Notebook")
        .to_string();
    let source_count = value
        .get("source_count")
        .and_then(Value::as_u64)
        .map(|c| c as usize);
    let updated_at = value
        .get("updated_at")
        .and_then(Value::as_str)
        .map(str::to_string);
    NotebookSummary {
        id,
        name,
        source_count,
        updated_at,
    }
}

fn render_record_source_text(record: &Record) -> String {
    let mut lines = vec![
        format!("# {}", record.title),
        String::new(),
        format!("- Type: {}", record.record_type),
        format!("- Created At: {}", record.created_at),
        format!("- Date: {}", record.date.clone().unwrap_or_default()),
        format!("- Tags: {}", record.tags.join(", ")),
        String::new(),
        "## Final Body".to_string(),
        record.final_body.clone(),
        String::new(),
        "## Source Text".to_string(),
        record.source_text.clone(),
    ];
    lines.retain(|line| !(line.starts_with("- Date: ") && line == "- Date: "));
    lines.join("\n")
}

fn resolve_notebooklm_runtime(config: Option<NotebookLmConfig>) -> (String, Vec<String>) {
    let settings = load_settings();
    let default_command = settings.integrations.notebooklm.command.trim().to_string();
    let default_args = settings.integrations.notebooklm.args.clone();

    let command = config
        .as_ref()
        .and_then(|c| c.command.as_ref())
        .map(|s: &String| s.trim().to_string())
        .filter(|s: &String| !s.is_empty())
        .or_else(|| if default_command.is_empty() { None } else { Some(default_command) })
        .unwrap_or_else(|| DEFAULT_NOTEBOOKLM_COMMAND.to_string());

    let args = config
        .and_then(|c| c.args)
        .filter(|a: &Vec<String>| !a.is_empty())
        .unwrap_or_else(|| {
            if default_args.is_empty() {
                DEFAULT_NOTEBOOKLM_ARGS.iter().map(|s| s.to_string()).collect()
            } else {
                default_args
            }
        });

    (command, args)
}

fn write_jsonrpc_line(stdin: &mut std::process::ChildStdin, value: &Value) -> Result<(), String> {
    let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
    stdin.write_all(format!("{text}\n").as_bytes()).map_err(|e| e.to_string())
}

fn wait_jsonrpc_result(
    rx: &mpsc::Receiver<Value>,
    expected_id: u64,
    timeout: StdDuration,
) -> Result<Value, String> {
    let deadline = Instant::now() + timeout;
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err("NotebookLM MCP response timeout".to_string());
        }
        let wait_for = deadline.saturating_duration_since(now);
        match rx.recv_timeout(wait_for) {
            Ok(message) => {
                let id = message.get("id").and_then(Value::as_u64).unwrap_or(0);
                if id != expected_id {
                    continue;
                }
                if let Some(error) = message.get("error") {
                    return Err(format!("NotebookLM MCP error: {error}"));
                }
                return Ok(message);
            }
            Err(_) => return Err("NotebookLM MCP response timeout".to_string()),
        }
    }
}

fn parse_mcp_tool_payload(response: &Value) -> Result<Value, String> {
    let result = response
        .get("result")
        .ok_or_else(|| "NotebookLM MCP missing result".to_string())?;
    if result.get("isError").and_then(Value::as_bool).unwrap_or(false) {
        return Err(format!("NotebookLM MCP tool error: {result}"));
    }
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find_map(|item| {
                let t = item.get("type").and_then(Value::as_str).unwrap_or_default();
                if t == "text" {
                    item.get("text").and_then(Value::as_str).map(str::to_string)
                } else {
                    None
                }
            })
        })
        .unwrap_or_default();
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    let parsed = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({ "text": text }));
    if let Some(error) = parsed.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_else(|| text.as_str());
        return Err(format!("NotebookLM MCP tool error: {message}"));
    }
    Ok(parsed)
}

fn notebooklm_call_tool(
    tool_name: &str,
    arguments: Value,
    config: Option<NotebookLmConfig>,
) -> Result<Value, String> {
    let (command, args) = resolve_notebooklm_runtime(config);

    let mut child = Command::new(&command)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to start NotebookLM MCP command `{command}`: {e}"))?;

    let result = (|| -> Result<Value, String> {
        let mut stdin = child.stdin.take().ok_or_else(|| "NotebookLM MCP stdin unavailable".to_string())?;
        let stdout = child.stdout.take().ok_or_else(|| "NotebookLM MCP stdout unavailable".to_string())?;

        let (tx, rx) = mpsc::channel::<Value>();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.starts_with('{') {
                            if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
                                let _ = tx.send(value);
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        write_jsonrpc_line(
            &mut stdin,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "clientInfo": { "name": "kofnote-app", "version": "0.1.0" },
                    "capabilities": {}
                }
            }),
        )?;
        wait_jsonrpc_result(&rx, 1, StdDuration::from_secs(25))?;

        write_jsonrpc_line(
            &mut stdin,
            &json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }),
        )?;

        write_jsonrpc_line(
            &mut stdin,
            &json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": { "name": tool_name, "arguments": arguments }
            }),
        )?;

        let call_response = wait_jsonrpc_result(&rx, 2, StdDuration::from_secs(90))?;
        parse_mcp_tool_payload(&call_response)
    })();

    let _ = child.kill();
    let _ = child.wait();
    result
}

#[tauri::command]
pub fn notebooklm_health_check(config: Option<NotebookLmConfig>) -> Result<Value, String> {
    notebooklm_call_tool("health_check", json!({}), config)
}

#[tauri::command]
pub fn notebooklm_list_notebooks(
    limit: Option<usize>,
    config: Option<NotebookLmConfig>,
) -> Result<Vec<NotebookSummary>, String> {
    let payload = notebooklm_call_tool(
        "list_notebooks",
        json!({ "limit": limit.unwrap_or(20).clamp(1, 100) }),
        config,
    )?;
    let notebooks = payload
        .get("notebooks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(notebooks.iter().map(parse_notebook_summary).collect())
}

#[tauri::command]
pub fn notebooklm_create_notebook(
    title: Option<String>,
    config: Option<NotebookLmConfig>,
) -> Result<NotebookSummary, String> {
    let payload = notebooklm_call_tool(
        "create_notebook",
        json!({ "title": title.unwrap_or_else(|| "KOF Note Notebook".to_string()) }),
        config,
    )?;
    let notebook = payload.get("notebook").cloned().unwrap_or_else(|| payload.clone());
    Ok(parse_notebook_summary(&notebook))
}

#[tauri::command]
pub fn notebooklm_add_record_source(
    central_home: String,
    json_path: String,
    notebook_id: String,
    title: Option<String>,
    config: Option<NotebookLmConfig>,
) -> Result<Value, String> {
    let home = normalized_home(&central_home)?;
    let record = load_record_by_json_path(&home, &json_path)?;
    let source_title = title.unwrap_or_else(|| {
        format!(
            "{} | {} | {}",
            record.record_type,
            extract_day(&record.created_at).unwrap_or_else(|| record.created_at.clone()),
            record.title
        )
    });
    let text = render_record_source_text(&record);
    notebooklm_call_tool(
        "add_source",
        json!({
            "notebook_id": notebook_id,
            "source_type": "text",
            "title": source_title,
            "text": text,
        }),
        config,
    )
}

#[tauri::command]
pub fn notebooklm_ask(
    notebook_id: String,
    question: String,
    include_citations: Option<bool>,
    config: Option<NotebookLmConfig>,
) -> Result<NotebookLmAskResult, String> {
    if question.trim().is_empty() {
        return Err("Question cannot be empty".to_string());
    }
    let payload = notebooklm_call_tool(
        "ask",
        json!({
            "notebook_id": notebook_id,
            "question": question.trim(),
            "include_citations": include_citations.unwrap_or(true),
        }),
        config,
    )?;
    let answer = payload
        .get("answer")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let citations = payload
        .get("citations")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(NotebookLmAskResult { answer, citations })
}

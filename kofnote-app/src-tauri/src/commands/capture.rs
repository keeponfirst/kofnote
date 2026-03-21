use crate::providers::claude::{resolve_claude_api_key, run_claude_text_completion};
use crate::providers::gemini::{resolve_gemini_api_key, run_gemini_text_completion};
use crate::providers::openai::{resolve_api_key as resolve_openai_api_key, run_openai_text_completion};
use crate::storage::index::upsert_index_record_if_exists;
use crate::storage::records::{
    detect_central_home_path, ensure_structure, persist_record_to_files, record_from_value,
};
use crate::util::{absolute_path, write_atomic};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use tauri::{AppHandle, Emitter};

const CAPTURE_SYSTEM_PROMPT: &str = r#"你是 KOF Note 的智慧知識管理助理。使用者剛剛捕捉了一段文字或 URL。

請深度分析這段內容，然後以 JSON 格式回傳以下欄位：

- "type"：分類，只能是 "decision" / "idea" / "backlog" / "note" / "worklog" 其中之一
  - decision：包含選擇、判斷、決定、取捨的內容
  - idea：靈感、可能性、創意方向、假設
  - backlog：待辦、任務、需要執行的事項
  - worklog：工作記錄、進度、已完成的事
  - note：其他知識、參考資料、學習筆記
- "title"：簡潔有意義的標題（繁體中文，最多 80 字）
- "summary"：2～4 句話的深度分析（繁體中文）
  - 說明這段內容的核心含意
  - 為什麼值得記錄
  - 有什麼潛在行動或洞察
- "tags"：2～5 個相關標籤（英文小寫 kebab-case）

僅回傳合法 JSON，不要加其他文字。"#;

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CaptureCompletePayload {
    pub json_path: String,
    pub record_type: String,
    pub title: String,
    pub tags: Vec<String>,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CaptureFailedPayload {
    pub json_path: String,
    pub error: String,
}

struct AiAnalysis {
    record_type: String,
    title: String,
    summary: String,
    tags: Vec<String>,
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}...")
    }
}

fn build_prompt(content: &str, source_hint: &Option<String>) -> String {
    let source_part = source_hint
        .as_ref()
        .map(|hint| format!("\n來源：{hint}"))
        .unwrap_or_default();
    format!(
        "{CAPTURE_SYSTEM_PROMPT}\n\n---\n\n以下是使用者捕捉的內容：{source_part}\n\n{content}"
    )
}

fn call_ai(
    content: &str,
    source_hint: &Option<String>,
    provider: Option<&str>,
    model: Option<&str>,
    openai_key: Option<String>,
    gemini_key: Option<String>,
    claude_key: Option<String>,
) -> Result<String, String> {
    let prompt = build_prompt(content, source_hint);
    let max_seconds = 60;
    let max_tokens = 1024;

    // If an explicit provider is requested
    if let Some(p) = provider {
        let m = model.unwrap_or("");
        match p {
            "codex-cli" => {
                return crate::providers::cli::run_codex_cli_completion(
                    m,
                    &prompt,
                    max_seconds,
                    max_tokens,
                );
            }
            "gemini-cli" => {
                return crate::providers::cli::run_gemini_cli_completion(
                    m,
                    &prompt,
                    max_seconds,
                    max_tokens,
                );
            }
            "claude-cli" => {
                return crate::providers::cli::run_claude_cli_completion(
                    m,
                    &prompt,
                    max_seconds,
                    max_tokens,
                );
            }
            "openai" | "openai-api" => {
                let actual_model = if m.is_empty() { "gpt-4o-mini" } else { m };
                return run_openai_text_completion(actual_model, &prompt, max_seconds, max_tokens, openai_key);
            }
            "gemini" | "gemini-api" => {
                let actual_model = if m.is_empty() { "gemini-2.0-flash" } else { m };
                return run_gemini_text_completion(actual_model, &prompt, max_seconds, max_tokens, gemini_key);
            }
            "claude" | "claude-api" => {
                let actual_model = if m.is_empty() { "claude-sonnet-4-6" } else { m };
                return run_claude_text_completion(actual_model, &prompt, max_seconds, max_tokens, claude_key);
            }
            _ => {
                // Fallthrough to default behavior if unknown provider
            }
        }
    }

    // Default fallback priority: Claude → OpenAI → Gemini
    if let Some(key) = claude_key {
        return run_claude_text_completion("claude-sonnet-4-6", &prompt, max_seconds, max_tokens, Some(key));
    }
    if let Some(key) = openai_key {
        return run_openai_text_completion("gpt-4o-mini", &prompt, max_seconds, max_tokens, Some(key));
    }
    if let Some(key) = gemini_key {
        return run_gemini_text_completion("gemini-2.0-flash", &prompt, max_seconds, max_tokens, Some(key));
    }
    Err("NO_AI_KEY".to_string())
}

fn parse_ai_response(raw: &str) -> Result<AiAnalysis, String> {
    // Strip markdown code fences if present
    let trimmed = raw.trim();
    let json_str = if trimmed.starts_with("```") {
        let without_start = trimmed
            .strip_prefix("```json")
            .or_else(|| trimmed.strip_prefix("```"))
            .unwrap_or(trimmed);
        without_start
            .strip_suffix("```")
            .unwrap_or(without_start)
            .trim()
    } else {
        trimmed
    };

    let value: Value = serde_json::from_str(json_str)
        .map_err(|e| format!("AI JSON parse error: {e}"))?;

    let record_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("note")
        .to_string();
    let title = value
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Quick Capture")
        .to_string();
    let summary = value
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let tags = value
        .get("tags")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Ok(AiAnalysis {
        record_type,
        title,
        summary,
        tags,
    })
}

fn update_record_on_disk(
    central_home: &Path,
    json_path: &str,
    analysis: &AiAnalysis,
) -> Result<(), String> {
    let path = Path::new(json_path);
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut value: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    // Update fields with AI analysis
    if let Some(obj) = value.as_object_mut() {
        obj.insert("type".to_string(), json!(analysis.record_type));
        obj.insert("title".to_string(), json!(analysis.title));
        obj.insert("final_body".to_string(), json!(analysis.summary));

        // Merge tags: keep "quick-capture" and add AI tags
        let mut tags: Vec<String> = vec!["quick-capture".to_string()];
        for tag in &analysis.tags {
            if !tags.contains(tag) {
                tags.push(tag.clone());
            }
        }
        obj.insert("tags".to_string(), json!(tags));
    }

    let json_bytes = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
    write_atomic(path, &json_bytes).map_err(|e| e.to_string())?;

    // Rebuild record and write markdown + update search index
    let record = record_from_value(
        &value,
        Some(path.to_path_buf()),
        Some(path.with_extension("md")),
        None,
    );
    let md_path = path.with_extension("md");
    persist_record_to_files(&record, path, &md_path)?;
    let _ = upsert_index_record_if_exists(central_home, &record);

    Ok(())
}

#[tauri::command]
pub fn quick_capture(
    app: AppHandle,
    central_home: String,
    content: String,
    source_hint: Option<String>,
    provider: Option<String>,
    model: Option<String>,
) -> Result<String, String> {
    if content.trim().is_empty() {
        return Err("Content is empty".to_string());
    }

    let home = detect_central_home_path(&absolute_path(Path::new(central_home.trim())));
    ensure_structure(&home).map_err(|e| e.to_string())?;

    // Step 1: Immediately save as provisional note via upsert_record
    let json_path = crate::commands::core::upsert_record(
        central_home.clone(),
        serde_json::from_value(json!({
            "recordType": "note",
            "title": format!("Quick Capture: {}", truncate(&content, 60)),
            "sourceText": content,
            "finalBody": "⏳ AI 分析中...",
            "tags": ["quick-capture"],
        }))
        .map_err(|e| e.to_string())?,
        None,
    )?
    .json_path
    .clone()
    .unwrap_or_default();

    // Resolve API keys on the main thread (macOS Keychain access might fail in background threads without prompt)
    let claude_key = resolve_claude_api_key(None).ok();
    let openai_key = resolve_openai_api_key(None).ok();
    let gemini_key = resolve_gemini_api_key(None).ok();

    // Step 2: Background thread for AI analysis
    let json_path_clone = json_path.clone();
    let home_clone = home.clone();
    std::thread::spawn(move || {
        println!("[QuickCapture] Starting AI analysis...");
        
        // Capture explicitly requested provider handling OR fallbacks
        let ai_result = call_ai(
            &content,
            &source_hint,
            provider.as_deref(),
            model.as_deref(),
            openai_key,
            gemini_key,
            claude_key,
        );

        match ai_result {
            Ok(raw_response) => {
                println!("[QuickCapture] Raw AI response:\n{}", raw_response);
                match parse_ai_response(&raw_response) {
                    Ok(analysis) => {
                        println!("[QuickCapture] Parsed successfully: {:?}", analysis.title);
                        if let Err(e) =
                            update_record_on_disk(&home_clone, &json_path_clone, &analysis)
                        {
                            println!("[QuickCapture] Disk update failed: {}", e);
                            let _ = app.emit(
                                "capture_failed",
                                CaptureFailedPayload {
                                    json_path: json_path_clone,
                                    error: e,
                                },
                            );
                            return;
                        }
                        println!("[QuickCapture] Emit capture_complete");
                        let _ = app.emit(
                            "capture_complete",
                            CaptureCompletePayload {
                                json_path: json_path_clone,
                                record_type: analysis.record_type,
                                title: analysis.title,
                                tags: analysis.tags,
                            },
                        );
                    }
                    Err(e) => {
                        println!("[QuickCapture] Parse error: {}", e);
                        let _ = app.emit(
                            "capture_failed",
                            CaptureFailedPayload {
                                json_path: json_path_clone,
                                error: e,
                            },
                        );
                    }
                }
            }
            Err(e) if e == "NO_AI_KEY" => {
                println!("[QuickCapture] Error: NO_AI_KEY");
                // No AI key available — keep note as-is, notify frontend
                let _ = app.emit(
                    "capture_failed",
                    CaptureFailedPayload {
                        json_path: json_path_clone,
                        error: "NO_AI_KEY".to_string(),
                    },
                );
            }
            Err(e) => {
                let _ = app.emit(
                    "capture_failed",
                    CaptureFailedPayload {
                        json_path: json_path_clone,
                        error: e,
                    },
                );
            }
        }
    });

    Ok(json_path)
}

use crate::types::{OPENAI_RESPONSES_URL, OPENAI_SERVICE, OPENAI_USERNAME};
use keyring::Entry;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::time::Duration as StdDuration;

pub(crate) fn resolve_api_key(api_key: Option<String>) -> Result<String, String> {
    if let Some(provided) = api_key {
        if !provided.trim().is_empty() {
            return Ok(provided.trim().to_string());
        }
    }

    let entry = Entry::new(OPENAI_SERVICE, OPENAI_USERNAME).map_err(|error| error.to_string())?;
    entry
        .get_password()
        .map_err(|_| "Missing OpenAI API key. Set it in Settings first.".to_string())
}

pub(crate) fn extract_openai_output_text(value: &Value) -> String {
    if let Some(text) = value.get("output_text").and_then(Value::as_str) {
        if !text.trim().is_empty() {
            return text.trim().to_string();
        }
    }

    let mut chunks = Vec::new();

    if let Some(output) = value.get("output").and_then(Value::as_array) {
        for item in output {
            if let Some(content) = item.get("content").and_then(Value::as_array) {
                for block in content {
                    let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
                    if matches!(block_type, "output_text" | "text") {
                        if let Some(text) = block.get("text").and_then(Value::as_str) {
                            if !text.trim().is_empty() {
                                chunks.push(text.trim().to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    chunks.join("\n")
}

pub(crate) fn run_openai_text_completion(
    model: &str,
    prompt: &str,
    max_turn_seconds: u64,
    max_turn_tokens: u32,
) -> Result<String, String> {
    let api_key = resolve_api_key(None)?;
    let payload = json!({
        "model": model,
        "input": [{
            "role": "user",
            "content": [{
                "type": "input_text",
                "text": prompt,
            }]
        }],
        "max_output_tokens": max_turn_tokens,
    });

    let client = Client::builder()
        .timeout(StdDuration::from_secs(max_turn_seconds))
        .build()
        .map_err(|error| error.to_string())?;

    let response = client
        .post(OPENAI_RESPONSES_URL)
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .map_err(|error| error.to_string())?;

    let status = response.status();
    let body = response.text().map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("OpenAI API {}: {body}", status.as_u16()));
    }

    let value: Value = serde_json::from_str(&body).map_err(|error| error.to_string())?;
    let text = extract_openai_output_text(&value);
    if text.trim().is_empty() {
        return Err("OpenAI response is empty".to_string());
    }
    Ok(text)
}

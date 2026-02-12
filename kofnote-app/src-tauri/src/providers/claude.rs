use crate::types::{ANTHROPIC_API_VERSION, CLAUDE_API_URL, CLAUDE_USERNAME, OPENAI_SERVICE};
use keyring::{Entry, Error as KeyringError};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::time::Duration as StdDuration;

pub(crate) fn resolve_claude_api_key(api_key: Option<String>) -> Result<String, String> {
    if let Some(provided) = api_key {
        if !provided.trim().is_empty() {
            return Ok(provided.trim().to_string());
        }
    }

    let entry = Entry::new(OPENAI_SERVICE, CLAUDE_USERNAME).map_err(|error| error.to_string())?;
    match entry.get_password() {
        Ok(value) if !value.trim().is_empty() => Ok(value.trim().to_string()),
        Ok(_) => Err("Missing Claude API key. Set it in Settings first.".to_string()),
        Err(KeyringError::NoEntry) => {
            Err("Missing Claude API key. Set it in Settings first.".to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn run_claude_text_completion(
    model: &str,
    prompt: &str,
    max_turn_seconds: u64,
    max_turn_tokens: u32,
) -> Result<String, String> {
    let api_key = resolve_claude_api_key(None)?;
    let payload = json!({
        "model": model,
        "max_tokens": max_turn_tokens,
        "messages": [{
            "role": "user",
            "content": prompt
        }]
    });

    let client = Client::builder()
        .timeout(StdDuration::from_secs(max_turn_seconds))
        .build()
        .map_err(|error| error.to_string())?;

    let response = client
        .post(CLAUDE_API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_API_VERSION)
        .header("content-type", "application/json")
        .json(&payload)
        .send()
        .map_err(|error| error.to_string())?;

    let status = response.status();
    let body = response.text().map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("Claude API {}: {body}", status.as_u16()));
    }

    let value: Value = serde_json::from_str(&body).map_err(|error| error.to_string())?;
    let text = value
        .get("content")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| item.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();

    if text.trim().is_empty() {
        return Err("Claude response is empty".to_string());
    }
    Ok(text)
}

use crate::types::{GEMINI_API_BASE_URL, OPENAI_SERVICE, GEMINI_USERNAME};
use keyring::{Entry, Error as KeyringError};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::time::Duration as StdDuration;

pub(crate) fn resolve_gemini_api_key(api_key: Option<String>) -> Result<String, String> {
    if let Some(provided) = api_key {
        if !provided.trim().is_empty() {
            return Ok(provided.trim().to_string());
        }
    }

    let entry = Entry::new(OPENAI_SERVICE, GEMINI_USERNAME).map_err(|error| error.to_string())?;
    match entry.get_password() {
        Ok(value) if !value.trim().is_empty() => Ok(value.trim().to_string()),
        Ok(_) => Err("Missing Gemini API key. Set it in Settings first.".to_string()),
        Err(KeyringError::NoEntry) => {
            Err("Missing Gemini API key. Set it in Settings first.".to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn run_gemini_text_completion(
    model: &str,
    prompt: &str,
    max_turn_seconds: u64,
    max_turn_tokens: u32,
) -> Result<String, String> {
    let api_key = resolve_gemini_api_key(None)?;
    let url = format!("{GEMINI_API_BASE_URL}/{model}:generateContent?key={api_key}");
    let payload = json!({
        "contents": [{
            "parts": [{
                "text": prompt
            }]
        }],
        "generationConfig": {
            "maxOutputTokens": max_turn_tokens
        }
    });

    let client = Client::builder()
        .timeout(StdDuration::from_secs(max_turn_seconds))
        .build()
        .map_err(|error| error.to_string())?;

    let response = client
        .post(url)
        .json(&payload)
        .send()
        .map_err(|error| error.to_string())?;

    let status = response.status();
    let body = response.text().map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("Gemini API {}: {body}", status.as_u16()));
    }

    let value: Value = serde_json::from_str(&body).map_err(|error| error.to_string())?;
    let text = value
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();

    if text.trim().is_empty() {
        return Err("Gemini response is empty".to_string());
    }
    Ok(text)
}

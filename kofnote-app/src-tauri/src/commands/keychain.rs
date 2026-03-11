use crate::types::{
    CLAUDE_USERNAME, GEMINI_USERNAME, NOTION_USERNAME, OPENAI_SERVICE, OPENAI_USERNAME,
};
use keyring::{Entry, Error as KeyringError};

fn keyring_entry() -> Result<Entry, String> {
    Entry::new(OPENAI_SERVICE, OPENAI_USERNAME).map_err(|e| e.to_string())
}

fn has_keyring_entry_value(entry: Entry) -> Result<bool, String> {
    match entry.get_password() {
        Ok(value) => Ok(!value.trim().is_empty()),
        Err(KeyringError::NoEntry) => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}

pub(crate) fn has_openai_api_key_internal() -> Result<bool, String> {
    has_keyring_entry_value(keyring_entry()?)
}

fn gemini_keyring_entry() -> Result<Entry, String> {
    Entry::new(OPENAI_SERVICE, GEMINI_USERNAME).map_err(|e| e.to_string())
}

pub(crate) fn has_gemini_api_key_internal() -> Result<bool, String> {
    has_keyring_entry_value(gemini_keyring_entry()?)
}

fn claude_keyring_entry() -> Result<Entry, String> {
    Entry::new(OPENAI_SERVICE, CLAUDE_USERNAME).map_err(|e| e.to_string())
}

pub(crate) fn has_claude_api_key_internal() -> Result<bool, String> {
    has_keyring_entry_value(claude_keyring_entry()?)
}

fn notion_keyring_entry() -> Result<Entry, String> {
    Entry::new(OPENAI_SERVICE, NOTION_USERNAME).map_err(|e| e.to_string())
}

pub(crate) fn has_notion_api_key_internal() -> Result<bool, String> {
    has_keyring_entry_value(notion_keyring_entry()?)
}

pub(crate) fn resolve_notion_api_key(api_key: Option<String>) -> Result<String, String> {
    if let Some(value) = api_key {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    let entry = notion_keyring_entry()?;
    match entry.get_password() {
        Ok(value) if !value.trim().is_empty() => Ok(value.trim().to_string()),
        Ok(_) => Err("Missing Notion API key. Add it in Settings > Integrations.".to_string()),
        Err(KeyringError::NoEntry) => {
            Err("Missing Notion API key. Add it in Settings > Integrations.".to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn set_openai_api_key(api_key: String) -> Result<bool, String> {
    if api_key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }
    keyring_entry()?
        .set_password(api_key.trim())
        .map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
pub fn has_openai_api_key() -> Result<bool, String> {
    has_openai_api_key_internal()
}

#[tauri::command]
pub fn clear_openai_api_key() -> Result<bool, String> {
    match keyring_entry()?.delete_password() {
        Ok(_) => Ok(true),
        Err(KeyringError::NoEntry) => Ok(true),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn set_gemini_api_key(api_key: String) -> Result<bool, String> {
    if api_key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }
    gemini_keyring_entry()?
        .set_password(api_key.trim())
        .map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
pub fn has_gemini_api_key() -> Result<bool, String> {
    has_gemini_api_key_internal()
}

#[tauri::command]
pub fn clear_gemini_api_key() -> Result<bool, String> {
    match gemini_keyring_entry()?.delete_password() {
        Ok(_) => Ok(true),
        Err(KeyringError::NoEntry) => Ok(true),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn set_claude_api_key(api_key: String) -> Result<bool, String> {
    if api_key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }
    claude_keyring_entry()?
        .set_password(api_key.trim())
        .map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
pub fn has_claude_api_key() -> Result<bool, String> {
    has_claude_api_key_internal()
}

#[tauri::command]
pub fn clear_claude_api_key() -> Result<bool, String> {
    match claude_keyring_entry()?.delete_password() {
        Ok(_) => Ok(true),
        Err(KeyringError::NoEntry) => Ok(true),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn set_notion_api_key(api_key: String) -> Result<bool, String> {
    if api_key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }
    notion_keyring_entry()?
        .set_password(api_key.trim())
        .map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
pub fn has_notion_api_key() -> Result<bool, String> {
    has_notion_api_key_internal()
}

#[tauri::command]
pub fn clear_notion_api_key() -> Result<bool, String> {
    match notion_keyring_entry()?.delete_password() {
        Ok(_) => Ok(true),
        Err(KeyringError::NoEntry) => Ok(true),
        Err(e) => Err(e.to_string()),
    }
}

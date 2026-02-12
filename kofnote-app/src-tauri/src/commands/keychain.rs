#[tauri::command]
pub fn set_openai_api_key(api_key: String) -> Result<bool, String> {
    crate::types::set_openai_api_key(api_key)
}

#[tauri::command]
pub fn has_openai_api_key() -> Result<bool, String> {
    crate::types::has_openai_api_key()
}

#[tauri::command]
pub fn clear_openai_api_key() -> Result<bool, String> {
    crate::types::clear_openai_api_key()
}

#[tauri::command]
pub fn set_gemini_api_key(api_key: String) -> Result<bool, String> {
    crate::types::set_gemini_api_key(api_key)
}

#[tauri::command]
pub fn has_gemini_api_key() -> Result<bool, String> {
    crate::types::has_gemini_api_key()
}

#[tauri::command]
pub fn clear_gemini_api_key() -> Result<bool, String> {
    crate::types::clear_gemini_api_key()
}

#[tauri::command]
pub fn set_claude_api_key(api_key: String) -> Result<bool, String> {
    crate::types::set_claude_api_key(api_key)
}

#[tauri::command]
pub fn has_claude_api_key() -> Result<bool, String> {
    crate::types::has_claude_api_key()
}

#[tauri::command]
pub fn clear_claude_api_key() -> Result<bool, String> {
    crate::types::clear_claude_api_key()
}

#[tauri::command]
pub fn set_notion_api_key(api_key: String) -> Result<bool, String> {
    crate::types::set_notion_api_key(api_key)
}

#[tauri::command]
pub fn has_notion_api_key() -> Result<bool, String> {
    crate::types::has_notion_api_key()
}

#[tauri::command]
pub fn clear_notion_api_key() -> Result<bool, String> {
    crate::types::clear_notion_api_key()
}

use crate::types::{PromptProfile, PromptRunRequest, PromptRunResponse, PromptTemplate};

#[tauri::command]
pub fn list_prompt_profiles(central_home: String) -> Result<Vec<PromptProfile>, String> {
    crate::types::list_prompt_profiles(central_home)
}

#[tauri::command]
pub fn upsert_prompt_profile(central_home: String, profile: PromptProfile) -> Result<PromptProfile, String> {
    crate::types::upsert_prompt_profile(central_home, profile)
}

#[tauri::command]
pub fn delete_prompt_profile(central_home: String, id: String) -> Result<(), String> {
    crate::types::delete_prompt_profile(central_home, id)
}

#[tauri::command]
pub fn list_prompt_templates(central_home: String) -> Result<Vec<PromptTemplate>, String> {
    crate::types::list_prompt_templates(central_home)
}

#[tauri::command]
pub fn upsert_prompt_template(central_home: String, template: PromptTemplate) -> Result<PromptTemplate, String> {
    crate::types::upsert_prompt_template(central_home, template)
}

#[tauri::command]
pub fn delete_prompt_template(central_home: String, id: String) -> Result<(), String> {
    crate::types::delete_prompt_template(central_home, id)
}

#[tauri::command]
pub fn run_prompt_service(central_home: String, request: PromptRunRequest) -> Result<PromptRunResponse, String> {
    crate::types::run_prompt_service(central_home, request)
}

//! Prompt profiles and templates: list, upsert, delete, run. Implementation moved from types.

use crate::storage::records::normalized_home;
use crate::types::{
    PromptProfile, PromptRunRequest, PromptRunResponse, PromptTemplate, OPENAI_RESPONSES_URL,
};
use crate::util::write_atomic;
use chrono::Local;
use reqwest::blocking::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Duration as StdDuration;

fn prompt_profiles_dir(central_home: &Path) -> std::path::PathBuf {
    central_home.join("prompts").join("profiles")
}

fn prompt_templates_dir(central_home: &Path) -> std::path::PathBuf {
    central_home.join("prompts").join("templates")
}

#[tauri::command]
pub fn list_prompt_profiles(central_home: String) -> Result<Vec<PromptProfile>, String> {
    let home = normalized_home(&central_home)?;
    let dir = prompt_profiles_dir(&home);
    let mut profiles: Vec<PromptProfile> = Vec::new();
    if !dir.exists() {
        return Ok(profiles);
    }
    let entries = fs::read_dir(&dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(profile) = serde_json::from_str::<PromptProfile>(&content) {
                profiles.push(profile);
            }
        }
    }
    profiles.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(profiles)
}

#[tauri::command]
pub fn upsert_prompt_profile(central_home: String, profile: PromptProfile) -> Result<PromptProfile, String> {
    let home = normalized_home(&central_home)?;
    let dir = prompt_profiles_dir(&home);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let id = if profile.id.trim().is_empty() {
        format!("profile_{}", Local::now().format("%Y%m%d%H%M%S%3f"))
    } else {
        profile.id.trim().to_string()
    };
    let now = Local::now().to_rfc3339();
    let created_at = if profile.created_at.is_empty() { now.clone() } else { profile.created_at.clone() };
    let saved = PromptProfile {
        id: id.clone(),
        name: profile.name.clone(),
        display_name: profile.display_name.clone(),
        role: profile.role.clone(),
        company: profile.company.clone(),
        department: profile.department.clone(),
        bio: profile.bio.clone(),
        created_at,
        updated_at: now,
    };
    let bytes = serde_json::to_vec_pretty(&saved).map_err(|e| e.to_string())?;
    write_atomic(&dir.join(format!("{id}.json")), &bytes).map_err(|e| e.to_string())?;
    Ok(saved)
}

#[tauri::command]
pub fn delete_prompt_profile(central_home: String, id: String) -> Result<(), String> {
    let home = normalized_home(&central_home)?;
    let path = prompt_profiles_dir(&home).join(format!("{id}.json"));
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn list_prompt_templates(central_home: String) -> Result<Vec<PromptTemplate>, String> {
    let home = normalized_home(&central_home)?;
    let dir = prompt_templates_dir(&home);
    let mut templates: Vec<PromptTemplate> = Vec::new();
    if !dir.exists() {
        return Ok(templates);
    }
    let entries = fs::read_dir(&dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(tmpl) = serde_json::from_str::<PromptTemplate>(&content) {
                templates.push(tmpl);
            }
        }
    }
    templates.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(templates)
}

#[tauri::command]
pub fn upsert_prompt_template(central_home: String, template: PromptTemplate) -> Result<PromptTemplate, String> {
    let home = normalized_home(&central_home)?;
    let dir = prompt_templates_dir(&home);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let id = if template.id.trim().is_empty() {
        format!("tmpl_{}", Local::now().format("%Y%m%d%H%M%S%3f"))
    } else {
        template.id.trim().to_string()
    };
    let now = Local::now().to_rfc3339();
    let created_at = if template.created_at.is_empty() { now.clone() } else { template.created_at.clone() };
    let saved = PromptTemplate {
        id: id.clone(),
        name: template.name.clone(),
        description: template.description.clone(),
        content: template.content.clone(),
        variables: template.variables.clone(),
        created_at,
        updated_at: now,
    };
    let bytes = serde_json::to_vec_pretty(&saved).map_err(|e| e.to_string())?;
    write_atomic(&dir.join(format!("{id}.json")), &bytes).map_err(|e| e.to_string())?;
    Ok(saved)
}

#[tauri::command]
pub fn delete_prompt_template(central_home: String, id: String) -> Result<(), String> {
    let home = normalized_home(&central_home)?;
    let path = prompt_templates_dir(&home).join(format!("{id}.json"));
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn resolve_prompt_content(content: &str, profile: &PromptProfile, variable_values: &HashMap<String, String>) -> String {
    let mut result = content.to_string();
    result = result.replace("{{display_name}}", &profile.display_name);
    result = result.replace("{{role}}", &profile.role);
    result = result.replace("{{company}}", &profile.company);
    result = result.replace("{{department}}", &profile.department);
    result = result.replace("{{bio}}", &profile.bio);
    for (key, value) in variable_values {
        result = result.replace(&format!("{{{{{key}}}}}"), value);
    }
    result
}

#[tauri::command]
pub fn run_prompt_service(central_home: String, request: PromptRunRequest) -> Result<PromptRunResponse, String> {
    let home = normalized_home(&central_home)?;

    let profile_path = prompt_profiles_dir(&home).join(format!("{}.json", request.profile_id));
    let profile_content = fs::read_to_string(&profile_path)
        .map_err(|_| format!("Profile not found: {}", request.profile_id))?;
    let profile: PromptProfile = serde_json::from_str(&profile_content)
        .map_err(|e| format!("Failed to parse profile: {e}"))?;

    let template_path = prompt_templates_dir(&home).join(format!("{}.json", request.template_id));
    let template_content = fs::read_to_string(&template_path)
        .map_err(|_| format!("Template not found: {}", request.template_id))?;
    let template: PromptTemplate = serde_json::from_str(&template_content)
        .map_err(|e| format!("Failed to parse template: {e}"))?;

    let resolved = resolve_prompt_content(&template.content, &profile, &request.variable_values);
    let openai_key = crate::providers::openai::resolve_api_key(None).ok();
    let gemini_key = crate::providers::gemini::resolve_gemini_api_key(None).ok();
    let claude_key = crate::providers::claude::resolve_claude_api_key(None).ok();

    let provider = request.provider.unwrap_or_else(|| "local".to_string());
    let model = request.model.unwrap_or_default();

    let result = match provider.as_str() {
        "openai" => {
            let api_key = openai_key.ok_or_else(|| "Missing OpenAI API key. Set it in Settings first.".to_string())?;
            let model = if model.trim().is_empty() { "gpt-4.1-mini".to_string() } else { model.clone() };
            let payload = serde_json::json!({
                "model": model,
                "input": [{ "role": "user", "content": [{ "type": "input_text", "text": resolved }] }]
            });
            let client = Client::builder().timeout(StdDuration::from_secs(60)).build().map_err(|e| e.to_string())?;
            let response = client.post(OPENAI_RESPONSES_URL).bearer_auth(api_key).json(&payload).send().map_err(|e| e.to_string())?;
            let status = response.status();
            let body = response.text().map_err(|e| e.to_string())?;
            if !status.is_success() {
                return Err(format!("OpenAI API {}: {}", status.as_u16(), body));
            }
            let val: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
            crate::providers::openai::extract_openai_output_text(&val)
        }
        "gemini" => {
            let model = if model.trim().is_empty() { "gemini-2.0-flash".to_string() } else { model.clone() };
            crate::providers::gemini::run_gemini_text_completion(&model, &resolved, 60, 4096, gemini_key)?
        }
        "claude" => {
            let model = if model.trim().is_empty() { "claude-3-5-sonnet-latest".to_string() } else { model.clone() };
            crate::providers::claude::run_claude_text_completion(&model, &resolved, 60, 4096, claude_key)?
        }
        _ => resolved.clone(),
    };

    Ok(PromptRunResponse {
        result,
        resolved_prompt: resolved,
        provider,
    })
}

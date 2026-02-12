use crate::types::AppSettings;

#[tauri::command]
pub fn get_app_settings() -> Result<AppSettings, String> {
    crate::types::get_app_settings()
}

#[tauri::command]
pub fn save_app_settings(settings: AppSettings) -> Result<AppSettings, String> {
    crate::types::save_app_settings(settings)
}

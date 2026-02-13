use crate::{types::{AppSettings, SETTINGS_DIR_NAME, SETTINGS_FILE_NAME}, util::write_atomic};
use std::fs;
use std::path::PathBuf;

pub(crate) fn app_settings_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join(SETTINGS_DIR_NAME).join(SETTINGS_FILE_NAME)
}

pub(crate) fn load_settings() -> AppSettings {
    let path = app_settings_path();
    if !path.exists() {
        return AppSettings::default();
    }

    match fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<AppSettings>(&content) {
            Ok(settings) => crate::types::normalize_settings(settings),
            Err(_) => AppSettings::default(),
        },
        Err(_) => AppSettings::default(),
    }
}

pub(crate) fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let path = app_settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(settings).map_err(|error| error.to_string())?;
    write_atomic(&path, &bytes).map_err(|error| error.to_string())
}

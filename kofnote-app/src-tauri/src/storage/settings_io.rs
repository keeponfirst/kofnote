#![allow(dead_code)]

use crate::types::AppSettings;
use std::path::PathBuf;

pub fn app_settings_path() -> PathBuf {
    crate::types::app_settings_path()
}

pub fn load_settings() -> AppSettings {
    crate::types::load_settings()
}

pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    crate::types::save_settings(settings)
}

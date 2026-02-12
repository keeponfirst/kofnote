use crate::types::{HealthDiagnostics, HomeFingerprint};

#[tauri::command]
pub fn get_home_fingerprint(central_home: String) -> Result<HomeFingerprint, String> {
    crate::types::get_home_fingerprint(central_home)
}

#[tauri::command]
pub fn get_health_diagnostics(central_home: String) -> Result<HealthDiagnostics, String> {
    crate::types::get_health_diagnostics(central_home)
}

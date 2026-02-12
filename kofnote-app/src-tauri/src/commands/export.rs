use crate::types::ExportReportResult;

#[tauri::command]
pub fn export_markdown_report(
    central_home: String,
    output_path: Option<String>,
    title: Option<String>,
    recent_days: Option<i64>,
) -> Result<ExportReportResult, String> {
    crate::types::export_markdown_report(central_home, output_path, title, recent_days)
}

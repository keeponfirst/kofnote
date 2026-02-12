use crate::types::AiAnalysisResponse;

#[tauri::command]
pub fn run_ai_analysis(
    central_home: String,
    provider: Option<String>,
    model: Option<String>,
    prompt: String,
    api_key: Option<String>,
    include_logs: Option<bool>,
    max_records: Option<usize>,
) -> Result<AiAnalysisResponse, String> {
    crate::types::run_ai_analysis(
        central_home,
        provider,
        model,
        prompt,
        api_key,
        include_logs,
        max_records,
    )
}

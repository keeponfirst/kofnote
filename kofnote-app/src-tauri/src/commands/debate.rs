use crate::types::{DebateLock, DebateModeRequest, DebateModeResponse, DebateReplayResponse};
use tauri::State;

#[tauri::command]
pub async fn run_debate_mode(
    lock: State<'_, DebateLock>,
    central_home: String,
    request: DebateModeRequest,
) -> Result<DebateModeResponse, String> {
    crate::types::run_debate_mode(lock, central_home, request).await
}

#[tauri::command]
pub async fn replay_debate_mode(
    central_home: String,
    run_id: String,
) -> Result<DebateReplayResponse, String> {
    crate::types::replay_debate_mode(central_home, run_id).await
}

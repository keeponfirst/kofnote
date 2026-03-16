//! Debate mode commands: run, replay, list. Implementation moved from types.

use crate::commands::core::upsert_record;
use crate::commands::notion::load_record_by_json_path;
use crate::storage::index::{ensure_index_schema, open_index_connection};
use crate::storage::records::{ensure_structure, normalized_home};
use crate::storage::settings_io::load_settings;
use crate::types::{
    DebateAction, DebateChallenge, DebateDecision, DebateFinalPacket, DebateLock, DebateModeRequest,
    DebateModeResponse, DebateNormalizedRequest, DebatePacketConsensus, DebatePacketParticipant,
    DebatePacketTimestamps, DebateProgress, DebateProviderRegistry, DebateReplayConsistency,
    DebateReplayResponse, DebateRejectedOption, DebateRisk, DebateRole, DebateRound, DebateRoundArtifact,
    DebateRunSummary, DebateState, DebateTrace, DebateTurn, Record, RecordPayload,
};
use crate::util::{file_mtime_iso, normalize_record_type, write_atomic};
use crate::providers::openai::resolve_api_key as resolve_openai_api_key;
use crate::providers::gemini::resolve_gemini_api_key;
use crate::providers::claude::resolve_claude_api_key;
use crate::providers::openai::run_openai_text_completion;
use crate::providers::gemini::run_gemini_text_completion;
use crate::providers::claude::run_claude_text_completion;
use chrono::{Duration as ChronoDuration, Local, NaiveDate};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::Instant;
use tauri::{Emitter, State};
use rusqlite::params;

#[tauri::command]
pub async fn run_debate_mode(
    app: tauri::AppHandle,
    lock: State<'_, DebateLock>,
    central_home: String,
    request: DebateModeRequest,
) -> Result<DebateModeResponse, String> {
    // Atomic check-and-set in a single lock scope
    let current_run_id = {
        let mut guard = lock.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(run_id) = guard.as_ref() {
            return Err(format!("Another debate is already running: {run_id}"));
        }
        let run_id = generate_debate_run_id();
        *guard = Some(run_id.clone());
        run_id
    };

    let home = normalized_home(&central_home)?;
    let app_for_worker = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        run_debate_mode_internal_with_app(Some(app_for_worker), &home, request)
    })
    .await
    .map_err(|error| format!("Debate worker join error: {error}"));

    // ALWAYS clear lock, even on error.
    match lock.0.lock() {
        Ok(mut guard) => {
            *guard = None;
        }
        Err(e) => {
            let mut guard = e.into_inner();
            *guard = None;
        }
    }

    // Flatten: Result<Result<T, E>, E> -> Result<T, E>
    result?
}

#[tauri::command]
pub async fn replay_debate_mode(
    central_home: String,
    run_id: String,
) -> Result<DebateReplayResponse, String> {
    let home = normalized_home(&central_home)?;
    let run_id = run_id.trim().to_string();
    tauri::async_runtime::spawn_blocking(move || replay_debate_mode_internal(&home, &run_id))
        .await
        .map_err(|error| format!("Debate replay worker join error: {error}"))?
}

#[tauri::command]
pub fn list_debate_runs(central_home: String) -> Result<Vec<DebateRunSummary>, String> {
    let home = normalized_home(&central_home)?;
    list_debate_runs_internal(&home)
}

fn list_debate_runs_internal(central_home: &Path) -> Result<Vec<DebateRunSummary>, String> {
    let debates_dir = central_home.join("records").join("debates");
    if !debates_dir.exists() {
        return Ok(vec![]);
    }

    let mut runs = Vec::new();
    let entries = fs::read_dir(&debates_dir).map_err(|error| error.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let run_id = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let request_path = path.join("request.json");
        if !request_path.exists() {
            continue;
        }

        let request_text = fs::read_to_string(&request_path).unwrap_or_default();
        let request_value: Value = serde_json::from_str(&request_text).unwrap_or_default();

        let problem = request_value
            .get("problem")
            .and_then(Value::as_str)
            .unwrap_or("")
            .chars()
            .take(120)
            .collect::<String>();
        let provider = request_value
            .get("participants")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("modelProvider"))
            .and_then(Value::as_str)
            .unwrap_or("local")
            .to_string();
        let output_type = request_value
            .get("outputType")
            .and_then(Value::as_str)
            .unwrap_or("decision")
            .to_string();

        let consensus_path = path.join("consensus.json");
        let degraded = if consensus_path.exists() {
            let text = fs::read_to_string(&consensus_path).unwrap_or_default();
            let value: Value = serde_json::from_str(&text).unwrap_or_default();
            value.get("degraded").and_then(Value::as_bool).unwrap_or(false)
        } else {
            false
        };

        let created_at = file_mtime_iso(&request_path);
        runs.push(DebateRunSummary {
            run_id,
            problem,
            provider,
            output_type,
            degraded,
            created_at,
            artifacts_root: path.to_string_lossy().to_string(),
        });
    }

    runs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(runs)
}

/// Exposed for tests (e.g. from types::tests).
pub(crate) fn run_debate_mode_internal(central_home: &Path, request: DebateModeRequest) -> Result<DebateModeResponse, String> {
    run_debate_mode_internal_with_app(None, central_home, request)
}

fn run_debate_mode_internal_with_app(
    app: Option<tauri::AppHandle>,
    central_home: &Path,
    request: DebateModeRequest,
) -> Result<DebateModeResponse, String> {
    let settings = load_settings();
    let provider_registry = DebateProviderRegistry::from_settings(&settings);
    let normalized = normalize_debate_request(request, &provider_registry)?;
    ensure_structure(central_home).map_err(|error| error.to_string())?;

    let openai_key = resolve_openai_api_key(None).ok();
    let gemini_key = resolve_gemini_api_key(None).ok();
    let claude_key = resolve_claude_api_key(None).ok();

    let run_id = generate_debate_run_id();
    let run_root = central_home.join("records").join("debates").join(&run_id);
    let rounds_root = run_root.join("rounds");
    fs::create_dir_all(&rounds_root).map_err(|error| error.to_string())?;

    let started_at = Local::now().to_rfc3339();
    let mut state: Option<DebateState> = None;
    let mut error_codes = normalized.warning_codes.clone();
    let mut degraded = false;

    advance_debate_state(&mut state, DebateState::Intake)?;

    let request_json_path = run_root.join("request.json");
    write_json_artifact(
        &request_json_path,
        &json!({
            "runId": run_id,
            "problem": normalized.problem,
            "constraints": normalized.constraints,
            "outputType": normalized.output_type,
            "participants": normalized
                .participants
                .iter()
                .map(|item| {
                    json!({
                        "role": item.role.as_str(),
                        "modelProvider": item.model_provider,
                        "providerType": item.provider_type,
                        "modelName": item.model_name,
                    })
                })
                .collect::<Vec<_>>(),
            "maxTurnSeconds": normalized.max_turn_seconds,
            "maxTurnTokens": normalized.max_turn_tokens,
            "warnings": normalized.warning_codes,
            "startedAt": started_at,
        }),
    )?;

    let total_turns = normalized.participants.len() * DebateRound::all().len();
    let mut turn_index = 0usize;
    let mut rounds: Vec<DebateRoundArtifact> = Vec::new();
    for round in DebateRound::all() {
        let next_state = match round {
            DebateRound::Round1 => DebateState::Round1,
            DebateRound::Round2 => DebateState::Round2,
            DebateRound::Round3 => DebateState::Round3,
        };
        advance_debate_state(&mut state, next_state)?;

        let round_started = Local::now().to_rfc3339();
        let mut turns = Vec::new();

        for participant in &normalized.participants {
            let target_role = if round == DebateRound::Round2 {
                Some(debate_round2_target(participant.role))
            } else {
                None
            };
            turn_index += 1;
            if let Some(app_handle) = app.as_ref() {
                let _ = app_handle.emit(
                    "debate-progress",
                    DebateProgress {
                        run_id: run_id.clone(),
                        round: round.as_str().to_string(),
                        role: participant.role.as_str().to_string(),
                        turn_index,
                        total_turns,
                        status: "started".to_string(),
                    },
                );
            }
            let turn = execute_debate_turn(
                participant,
                round,
                target_role,
                &normalized,
                &rounds,
                normalized.max_turn_seconds,
                normalized.max_turn_tokens,
                openai_key.clone(),
                gemini_key.clone(),
                claude_key.clone(),
            );
            if let Some(app_handle) = app.as_ref() {
                let _ = app_handle.emit(
                    "debate-progress",
                    DebateProgress {
                        run_id: run_id.clone(),
                        round: round.as_str().to_string(),
                        role: participant.role.as_str().to_string(),
                        turn_index,
                        total_turns,
                        status: if turn.status == "ok" {
                            "completed".to_string()
                        } else {
                            "failed".to_string()
                        },
                    },
                );
            }

            if turn.status != "ok" {
                degraded = true;
                if let Some(code) = &turn.error_code {
                    error_codes.push(code.clone());
                }
            }
            turns.push(turn);
        }

        let round_artifact = DebateRoundArtifact {
            round: round.as_str().to_string(),
            turns,
            started_at: round_started,
            finished_at: Local::now().to_rfc3339(),
        };
        write_json_artifact(
            &rounds_root.join(format!("{}.json", round.as_str())),
            &round_artifact,
        )?;
        rounds.push(round_artifact);
    }

    let total_turns = rounds.iter().map(|round| round.turns.len()).sum::<usize>();
    let ok_turns = rounds
        .iter()
        .flat_map(|round| round.turns.iter())
        .filter(|turn| turn.status == "ok")
        .count();
    if total_turns > 0 && ok_turns == 0 {
        let unique_codes = dedup_non_empty(error_codes.clone());
        let failure_path = run_root.join("failure.json");
        write_json_artifact(
            &failure_path,
            &json!({
                "runId": run_id,
                "artifactsRoot": run_root.to_string_lossy(),
                "totalTurns": total_turns,
                "okTurns": ok_turns,
                "errorCodes": unique_codes,
            }),
        )?;
        return Err(debate_error(
            "DEBATE_ERR_ALL_TURNS_FAILED",
            &format!(
                "All debate turns failed. run={run_id}. See artifacts at {}",
                run_root.to_string_lossy()
            ),
        ));
    }

    advance_debate_state(&mut state, DebateState::Consensus)?;
    let consensus = build_debate_consensus(&rounds, &error_codes);
    write_json_artifact(&run_root.join("consensus.json"), &consensus)?;

    advance_debate_state(&mut state, DebateState::Judge)?;
    let decision = build_debate_decision(&normalized, &rounds);
    let risks = build_debate_risks(&rounds);
    let next_actions = build_debate_actions(&normalized.output_type, &decision, &risks);

    advance_debate_state(&mut state, DebateState::Packetize)?;
    let participants = normalized
        .participants
        .iter()
        .map(|item| DebatePacketParticipant {
            role: item.role.as_str().to_string(),
            model_provider: item.model_provider.clone(),
            model_name: item.model_name.clone(),
        })
        .collect::<Vec<_>>();

    let mut final_packet = DebateFinalPacket {
        run_id: run_id.clone(),
        mode: "debate-v0.1".to_string(),
        problem: normalized.problem.clone(),
        constraints: normalized.constraints.clone(),
        output_type: normalized.output_type.clone(),
        participants,
        consensus,
        decision,
        risks,
        next_actions,
        trace: DebateTrace {
            round_refs: vec![
                "round-1".to_string(),
                "round-2".to_string(),
                "round-3".to_string(),
            ],
            evidence_refs: vec![
                request_json_path.to_string_lossy().to_string(),
                run_root.join("consensus.json").to_string_lossy().to_string(),
            ],
        },
        timestamps: DebatePacketTimestamps {
            started_at: started_at.clone(),
            finished_at: Local::now().to_rfc3339(),
        },
    };
    validate_final_packet(&final_packet)?;

    let final_packet_json_path = run_root.join("final-packet.json");
    let final_packet_md_path = run_root.join("final-packet.md");
    write_json_artifact(&final_packet_json_path, &final_packet)?;
    write_atomic(&final_packet_md_path, render_debate_packet_markdown(&final_packet).as_bytes())
        .map_err(|error| error.to_string())?;

    advance_debate_state(&mut state, DebateState::Writeback)?;
    let writeback_record = writeback_debate_result(central_home, &normalized, &final_packet)?;
    let writeback_json_path = writeback_record.json_path.clone();

    if let Some(path) = &writeback_json_path {
        final_packet
            .trace
            .evidence_refs
            .push(format!("writeback:{path}"));
    }
    final_packet.timestamps.finished_at = Local::now().to_rfc3339();
    validate_final_packet(&final_packet)?;

    write_json_artifact(&final_packet_json_path, &final_packet)?;
    write_atomic(&final_packet_md_path, render_debate_packet_markdown(&final_packet).as_bytes())
        .map_err(|error| error.to_string())?;

    upsert_debate_index(
        central_home,
        &final_packet,
        &rounds,
        degraded,
        &run_root,
        writeback_json_path.clone(),
    )?;

    Ok(DebateModeResponse {
        run_id,
        mode: "debate-v0.1".to_string(),
        state: state.unwrap_or(DebateState::Intake).as_str().to_string(),
        degraded,
        final_packet,
        artifacts_root: run_root.to_string_lossy().to_string(),
        writeback_json_path,
        error_codes: dedup_non_empty(error_codes),
    })
}

/// Exposed for tests (e.g. from types::tests).
pub(crate) fn replay_debate_mode_internal(central_home: &Path, run_id: &str) -> Result<DebateReplayResponse, String> {
    if run_id.trim().is_empty() {
        return Err(debate_error("DEBATE_ERR_INPUT", "run_id is required"));
    }

    let run_root = central_home.join("records").join("debates").join(run_id.trim());
    if !run_root.exists() {
        return Err(debate_error(
            "DEBATE_ERR_NOT_FOUND",
            &format!("Debate run not found: {}", run_root.to_string_lossy()),
        ));
    }

    let request_path = run_root.join("request.json");
    let consensus_path = run_root.join("consensus.json");
    let final_path = run_root.join("final-packet.json");
    let rounds_root = run_root.join("rounds");

    let request_value = read_json_value(&request_path)?;
    let consensus_value = read_json_value(&consensus_path)?;
    let final_value = read_json_value(&final_path)?;
    let final_packet: DebateFinalPacket =
        serde_json::from_value(final_value.clone()).map_err(|error| error.to_string())?;

    let mut rounds = Vec::new();
    let mut issues = Vec::new();

    for round in DebateRound::all() {
        let path = rounds_root.join(format!("{}.json", round.as_str()));
        if !path.exists() {
            issues.push(format!("Missing round artifact: {}", path.to_string_lossy()));
            continue;
        }
        rounds.push(read_json_value(&path)?);
    }

    let mut writeback_record: Option<Record> = None;
    for evidence in &final_packet.trace.evidence_refs {
        if let Some(path) = evidence.strip_prefix("writeback:") {
            if let Ok(record) = load_record_by_json_path(central_home, path) {
                writeback_record = Some(record);
            } else {
                issues.push(format!("Writeback reference missing: {path}"));
            }
        }
    }

    let indexed_turns = count_debate_turns(central_home, run_id)?;
    let indexed_actions = count_debate_actions(central_home, run_id)?;
    let expected_turns = rounds
        .iter()
        .filter_map(|item| item.get("turns").and_then(Value::as_array))
        .map(|items| items.len())
        .sum::<usize>();
    let expected_actions = final_packet.next_actions.len();

    if indexed_turns != expected_turns {
        issues.push(format!(
            "Turn count mismatch: file={expected_turns}, sqlite={indexed_turns}"
        ));
    }
    if indexed_actions != expected_actions {
        issues.push(format!(
            "Action count mismatch: file={expected_actions}, sqlite={indexed_actions}"
        ));
    }

    let files_complete = request_path.exists()
        && consensus_path.exists()
        && final_path.exists()
        && rounds.len() == DebateRound::all().len();
    let sql_indexed = indexed_turns > 0 || indexed_actions > 0;

    Ok(DebateReplayResponse {
        run_id: run_id.trim().to_string(),
        request: request_value,
        rounds,
        consensus: consensus_value,
        final_packet,
        writeback_record,
        consistency: DebateReplayConsistency {
            files_complete,
            sql_indexed,
            issues,
        },
    })
}

pub(crate) fn normalize_debate_request(
    request: DebateModeRequest,
    provider_registry: &DebateProviderRegistry,
) -> Result<DebateNormalizedRequest, String> {
    let problem = request.problem.trim().to_string();
    if problem.is_empty() {
        return Err(debate_error("DEBATE_ERR_INPUT", "Problem cannot be empty"));
    }

    let output_type = normalize_debate_output_type(&request.output_type)?;
    let max_turn_seconds = request.max_turn_seconds.unwrap_or(35).clamp(5, 120);
    let max_turn_tokens = request.max_turn_tokens.unwrap_or(900).clamp(128, 4096);

    let mut warning_codes = Vec::new();
    let mut provided = HashMap::new();
    for config in request.participants {
        let role_name = config.role.unwrap_or_default();
        let provider_input = config
            .model_provider
            .unwrap_or_else(|| "local".to_string());
        let model_name_input = config.model_name.unwrap_or_default();
        if let Some(role) = parse_debate_role(&role_name) {
            let input_normalized = provider_input.trim().to_lowercase();
            let (provider, provider_warning) =
                normalize_debate_provider(provider_input.trim(), provider_registry);
            if provider != input_normalized && !input_normalized.is_empty() {
                warning_codes.push("DEBATE_WARN_PROVIDER_NORMALIZED".to_string());
            }
            if let Some(code) = provider_warning {
                warning_codes.push(code);
            }
            provided.insert(
                role,
                crate::types::DebateRuntimeParticipant {
                    role,
                    model_provider: provider.clone(),
                    provider_type: resolve_provider_type(&provider, provider_registry),
                    model_name: normalize_debate_model_name(&provider, &model_name_input),
                },
            );
        } else if !role_name.trim().is_empty() {
            warning_codes.push("DEBATE_WARN_UNKNOWN_ROLE_IGNORED".to_string());
        }
    }

    let mut participants = Vec::new();
    for role in DebateRole::all() {
        if let Some(item) = provided.remove(&role) {
            participants.push(item);
        } else {
            participants.push(crate::types::DebateRuntimeParticipant {
                role,
                model_provider: "local".to_string(),
                provider_type: "builtin".to_string(),
                model_name: normalize_debate_model_name("local", ""),
            });
        }
    }

    let constraints = request
        .constraints
        .iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();

    Ok(DebateNormalizedRequest {
        problem,
        constraints,
        output_type,
        participants,
        max_turn_seconds,
        max_turn_tokens,
        writeback_record_type: request
            .writeback_record_type
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty()),
        warning_codes: dedup_non_empty(warning_codes),
    })
}

fn normalize_debate_output_type(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_lowercase();
    if matches!(
        normalized.as_str(),
        "decision" | "writing" | "architecture" | "planning" | "evaluation"
    ) {
        Ok(normalized)
    } else {
        Err(debate_error(
            "DEBATE_ERR_INPUT",
            "output_type must be one of: decision|writing|architecture|planning|evaluation",
        ))
    }
}

fn normalize_provider_alias(value: &str) -> String {
    match value.trim().to_lowercase().as_str() {
        "codex" => "codex-cli".to_string(),
        "chatgpt" => "chatgpt-web".to_string(),
        other => other.to_string(),
    }
}

fn resolve_provider_type(provider_id: &str, provider_registry: &DebateProviderRegistry) -> String {
    if matches!(provider_id, "openai" | "gemini" | "claude" | "local") {
        "builtin".to_string()
    } else if let Some(provider) = provider_registry.get(provider_id) {
        provider.provider_type.clone()
    } else {
        "builtin".to_string()
    }
}

fn normalize_debate_provider(
    value: &str,
    provider_registry: &DebateProviderRegistry,
) -> (String, Option<String>) {
    let normalized = value.trim().to_lowercase();
    if normalized.is_empty() {
        return ("local".to_string(), None);
    }

    if matches!(normalized.as_str(), "openai" | "gemini" | "claude" | "local") {
        return (normalized, None);
    }

    let canonical = normalize_provider_alias(&normalized);
    if provider_registry.is_enabled(&canonical) {
        return (canonical, None);
    }

    if provider_registry.get(&canonical).is_some() {
        return (
            "local".to_string(),
            Some("DEBATE_WARN_PROVIDER_DISABLED_FALLBACK_LOCAL".to_string()),
        );
    }

    (
        "local".to_string(),
        Some("DEBATE_WARN_PROVIDER_UNKNOWN_FALLBACK_LOCAL".to_string()),
    )
}

fn normalize_debate_model_name(provider: &str, model_name: &str) -> String {
    let trimmed = model_name.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }

    match provider {
        "openai" => "gpt-4.1-mini".to_string(),
        "gemini" => "gemini-2.0-flash".to_string(),
        "claude" => "claude-3-5-sonnet-latest".to_string(),
        "codex-cli" => "codex".to_string(),
        "gemini-cli" => "gemini".to_string(),
        "claude-cli" => "claude".to_string(),
        "chatgpt-web" => "chatgpt-web".to_string(),
        "gemini-web" => "gemini-web".to_string(),
        "claude-web" => "claude-web".to_string(),
        _ => "local-heuristic-v1".to_string(),
    }
}

fn parse_debate_role(value: &str) -> Option<DebateRole> {
    match value.trim().to_lowercase().as_str() {
        "proponent" => Some(DebateRole::Proponent),
        "critic" => Some(DebateRole::Critic),
        "analyst" => Some(DebateRole::Analyst),
        "synthesizer" => Some(DebateRole::Synthesizer),
        "judge" => Some(DebateRole::Judge),
        _ => None,
    }
}

pub(crate) fn validate_debate_transition(current: Option<DebateState>, next: DebateState) -> bool {
    matches!(
        (current, next),
        (None, DebateState::Intake)
            | (Some(DebateState::Intake), DebateState::Round1)
            | (Some(DebateState::Round1), DebateState::Round2)
            | (Some(DebateState::Round2), DebateState::Round3)
            | (Some(DebateState::Round3), DebateState::Consensus)
            | (Some(DebateState::Consensus), DebateState::Judge)
            | (Some(DebateState::Judge), DebateState::Packetize)
            | (Some(DebateState::Packetize), DebateState::Writeback)
    )
}

fn advance_debate_state(current: &mut Option<DebateState>, next: DebateState) -> Result<(), String> {
    if validate_debate_transition(*current, next) {
        *current = Some(next);
        Ok(())
    } else {
        Err(debate_error(
            "DEBATE_ERR_STATE",
            &format!(
                "Invalid transition: {:?} -> {}",
                current,
                next.as_str()
            ),
        ))
    }
}

fn debate_round2_target(role: DebateRole) -> DebateRole {
    match role {
        DebateRole::Proponent => DebateRole::Critic,
        DebateRole::Critic => DebateRole::Proponent,
        DebateRole::Analyst => DebateRole::Proponent,
        DebateRole::Synthesizer => DebateRole::Critic,
        DebateRole::Judge => DebateRole::Synthesizer,
    }
}

fn execute_debate_turn(
    participant: &crate::types::DebateRuntimeParticipant,
    round: DebateRound,
    target_role: Option<DebateRole>,
    request: &DebateNormalizedRequest,
    previous_rounds: &[DebateRoundArtifact],
    max_turn_seconds: u64,
    max_turn_tokens: u32,
    openai_key: Option<String>,
    gemini_key: Option<String>,
    claude_key: Option<String>,
) -> DebateTurn {
    let started_at = Local::now().to_rfc3339();
    let timer = Instant::now();

    let result = if participant.model_provider == "local"
        || provider_uses_local_stub(&participant.model_provider, &participant.provider_type)
    {
        Ok(generate_local_debate_text(
            participant.role,
            round,
            target_role,
            request,
            previous_rounds,
        ))
    } else {
        let prompt = build_debate_provider_prompt(participant.role, round, target_role, request, previous_rounds);
        run_debate_provider_text(
            &participant.model_provider,
            &participant.model_name,
            &prompt,
            max_turn_seconds,
            max_turn_tokens,
            openai_key,
            gemini_key,
            claude_key,
        )
    };

    match result {
        Ok(text) => {
            let claim = extract_claim_text(&text).unwrap_or_else(|| extract_first_non_empty_line(&text));
            let mut rationale = text.trim().to_string();
            if rationale.is_empty() {
                rationale = "No rationale returned.".to_string();
            }

            let risks = extract_risk_lines(&text);
            let challenges = if round == DebateRound::Round2 {
                build_round2_challenges(participant.role, target_role, &text, previous_rounds)
            } else {
                Vec::new()
            };
            let revisions = if round == DebateRound::Round3 {
                build_round3_revisions(participant.role, &text, previous_rounds)
            } else {
                Vec::new()
            };

            DebateTurn {
                role: participant.role.as_str().to_string(),
                round: round.as_str().to_string(),
                model_provider: participant.model_provider.clone(),
                model_name: participant.model_name.clone(),
                status: "ok".to_string(),
                claim,
                rationale,
                risks,
                challenges,
                revisions,
                target_role: target_role.map(|item| item.as_str().to_string()),
                duration_ms: timer.elapsed().as_millis(),
                error_code: None,
                error_message: None,
                started_at,
                finished_at: Local::now().to_rfc3339(),
            }
        }
        Err(error) => {
            let (code, message) = parse_debate_error(&error);
            DebateTurn {
                role: participant.role.as_str().to_string(),
                round: round.as_str().to_string(),
                model_provider: participant.model_provider.clone(),
                model_name: participant.model_name.clone(),
                status: "failed".to_string(),
                claim: String::new(),
                rationale: String::new(),
                risks: Vec::new(),
                challenges: Vec::new(),
                revisions: Vec::new(),
                target_role: target_role.map(|item| item.as_str().to_string()),
                duration_ms: timer.elapsed().as_millis(),
                error_code: code,
                error_message: Some(message),
                started_at,
                finished_at: Local::now().to_rfc3339(),
            }
        }
    }
}

pub(crate) fn provider_uses_local_stub(provider_id: &str, provider_type: &str) -> bool {
    provider_type == "web" || matches!(provider_id, "chatgpt-web" | "gemini-web" | "claude-web")
}

fn build_debate_provider_prompt(
    role: DebateRole,
    round: DebateRound,
    target_role: Option<DebateRole>,
    request: &DebateNormalizedRequest,
    previous_rounds: &[DebateRoundArtifact],
) -> String {
    let constraints = if request.constraints.is_empty() {
        "- none".to_string()
    } else {
        request
            .constraints
            .iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let mut context = Vec::new();
    for artifact in previous_rounds {
        for turn in &artifact.turns {
            if turn.status == "ok" {
                context.push(format!(
                    "{} / {}: {}",
                    artifact.round,
                    turn.role,
                    summarize_text_line(&turn.claim, 120)
                ));
            }
        }
    }
    let prior_context = if context.is_empty() {
        "none".to_string()
    } else {
        context.join("\n")
    };

    let round_instruction = match round {
        DebateRound::Round1 => "Provide opening position with claim, rationale, and key risks.",
        DebateRound::Round2 => "Challenge another role's position with concrete questions and weak points.",
        DebateRound::Round3 => "Revise your position based on cross-examination and provide final stance.",
    };

    let target = target_role
        .map(|item| item.as_str().to_string())
        .unwrap_or_else(|| "-".to_string());

    format!(
        "You are role {role}. Problem: {problem}\nOutput type: {output_type}\nConstraints:\n{constraints}\nTarget role: {target}\nRound instruction: {round_instruction}\nPrior context:\n{prior_context}\n\nReturn concise markdown in this shape:\nClaim: ...\nRationale: ...\nRisks: ...",
        role = role.as_str(),
        problem = request.problem,
        output_type = request.output_type,
    )
}

fn run_debate_provider_text(
    provider: &str,
    model: &str,
    prompt: &str,
    max_turn_seconds: u64,
    max_turn_tokens: u32,
    openai_key: Option<String>,
    gemini_key: Option<String>,
    claude_key: Option<String>,
) -> Result<String, String> {
    match provider {
        "codex-cli" => crate::providers::cli::run_codex_cli_completion(
            model,
            prompt,
            max_turn_seconds,
            max_turn_tokens,
        )
            .map_err(|error| debate_error("DEBATE_ERR_PROVIDER_CODEX_CLI", &error)),
        "gemini-cli" => crate::providers::cli::run_gemini_cli_completion(
            model,
            prompt,
            max_turn_seconds,
            max_turn_tokens,
        )
            .map_err(|error| debate_error("DEBATE_ERR_PROVIDER_GEMINI_CLI", &error)),
        "claude-cli" => crate::providers::cli::run_claude_cli_completion(
            model,
            prompt,
            max_turn_seconds,
            max_turn_tokens,
        )
            .map_err(|error| debate_error("DEBATE_ERR_PROVIDER_CLAUDE_CLI", &error)),
        "openai" => run_openai_text_completion(
            model,
            prompt,
            max_turn_seconds,
            max_turn_tokens,
            openai_key,
        )
        .map_err(|error| debate_error("DEBATE_ERR_PROVIDER_OPENAI", &error)),
        "gemini" => run_gemini_text_completion(
            model,
            prompt,
            max_turn_seconds,
            max_turn_tokens,
            gemini_key,
        )
        .map_err(|error| debate_error("DEBATE_ERR_PROVIDER_GEMINI", &error)),
        "claude" => run_claude_text_completion(
            model,
            prompt,
            max_turn_seconds,
            max_turn_tokens,
            claude_key,
        )
        .map_err(|error| debate_error("DEBATE_ERR_PROVIDER_CLAUDE", &error)),
        "local" => Ok(prompt.to_string()),
        _ => Err(debate_error(
            "DEBATE_ERR_PROVIDER_UNSUPPORTED",
            &format!("Unsupported provider: {provider}"),
        )),
    }
}

fn generate_local_debate_text(
    role: DebateRole,
    round: DebateRound,
    target_role: Option<DebateRole>,
    request: &DebateNormalizedRequest,
    previous_rounds: &[DebateRoundArtifact],
) -> String {
    let focus = summarize_text_line(&request.problem, 80);
    let constraints = if request.constraints.is_empty() {
        "no explicit constraints".to_string()
    } else {
        request.constraints.join("; ")
    };

    match round {
        DebateRound::Round1 => format!(
            "Claim: {} perspective recommends a practical path for {}.\nRationale: Prioritize local-first traceability and fast operator control under {}.\nRisks: hidden assumptions may survive without explicit cross-check.",
            role.as_str(),
            focus,
            constraints
        ),
        DebateRound::Round2 => format!(
            "Claim: {} challenges {} on evidence depth.\nRationale: Ask for concrete trade-offs, not generic statements.\nRisks: without challenge quality, consensus may converge too early.",
            role.as_str(),
            target_role.map(|item| item.as_str()).unwrap_or("peer")
        ),
        DebateRound::Round3 => {
            let prior = find_turn(previous_rounds, DebateRound::Round2, role)
                .map(|item| summarize_text_line(&item.claim, 120))
                .unwrap_or_else(|| "cross-examination feedback".to_string());
            format!(
                "Claim: {} revised position keeps local-first execution and adds guardrails.\nRationale: Revision incorporates '{}'.\nRisks: operational overhead increases if writeback contracts are not automated.",
                role.as_str(),
                prior
            )
        }
    }
}

fn build_round2_challenges(
    role: DebateRole,
    target_role: Option<DebateRole>,
    text: &str,
    previous_rounds: &[DebateRoundArtifact],
) -> Vec<DebateChallenge> {
    let Some(target) = target_role else {
        return Vec::new();
    };

    let target_claim = find_turn(previous_rounds, DebateRound::Round1, target)
        .map(|item| summarize_text_line(&item.claim, 140))
        .unwrap_or_else(|| "missing target claim".to_string());
    let question = format!(
        "How does {} defend this claim under failure conditions: {}",
        target.as_str(),
        target_claim
    );

    vec![DebateChallenge {
        source_role: role.as_str().to_string(),
        target_role: target.as_str().to_string(),
        question,
        response: summarize_text_line(text, 180),
    }]
}

fn build_round3_revisions(role: DebateRole, text: &str, previous_rounds: &[DebateRoundArtifact]) -> Vec<String> {
    let challenge_ref = find_turn(previous_rounds, DebateRound::Round2, role)
        .and_then(|item| item.challenges.first())
        .map(|item| format!("Addressed challenge to {}", item.target_role))
        .unwrap_or_else(|| "No challenge data available".to_string());

    dedup_non_empty(vec![
        challenge_ref,
        summarize_text_line(text, 180),
        "Added explicit risk mitigation and execution checkpoints.".to_string(),
    ])
}

fn build_debate_consensus(rounds: &[DebateRoundArtifact], error_codes: &[String]) -> DebatePacketConsensus {
    let total_turns = rounds.iter().map(|round| round.turns.len()).sum::<usize>();
    let ok_turns = rounds
        .iter()
        .flat_map(|round| round.turns.iter())
        .filter(|turn| turn.status == "ok")
        .count();
    let failure_count = total_turns.saturating_sub(ok_turns);

    let base_score = if total_turns == 0 {
        0.0
    } else {
        ok_turns as f64 / total_turns as f64
    };
    let confidence = (base_score - (failure_count as f64 * 0.03)).clamp(0.0, 1.0);

    let agreements = dedup_non_empty(
        rounds
            .iter()
            .flat_map(|round| round.turns.iter())
            .filter(|turn| turn.status == "ok")
            .map(|turn| summarize_text_line(&turn.claim, 120))
            .take(6)
            .collect::<Vec<_>>(),
    );

    let mut disagreements = rounds
        .iter()
        .flat_map(|round| round.turns.iter())
        .filter_map(|turn| turn.error_message.as_ref())
        .map(|item| summarize_text_line(item, 120))
        .collect::<Vec<_>>();

    if disagreements.is_empty() {
        disagreements = rounds
            .iter()
            .flat_map(|round| round.turns.iter())
            .filter(|turn| turn.role == "Critic")
            .flat_map(|turn| turn.risks.clone())
            .take(4)
            .collect::<Vec<_>>();
    }

    for code in error_codes {
        disagreements.push(format!("Observed warning/error code: {code}"));
    }

    DebatePacketConsensus {
        consensus_score: round_score(base_score),
        confidence_score: round_score(confidence),
        key_agreements: if agreements.is_empty() {
            vec!["Participants aligned on delivering an executable local-first packet.".to_string()]
        } else {
            agreements
        },
        key_disagreements: if disagreements.is_empty() {
            vec!["No major disagreement captured.".to_string()]
        } else {
            dedup_non_empty(disagreements)
        },
    }
}

fn build_debate_decision(request: &DebateNormalizedRequest, rounds: &[DebateRoundArtifact]) -> DebateDecision {
    let selected_option = find_turn(rounds, DebateRound::Round3, DebateRole::Synthesizer)
        .or_else(|| find_turn(rounds, DebateRound::Round3, DebateRole::Proponent))
        .map(|turn| turn.claim.trim().to_string())
        .filter(|item| !item.is_empty())
        .unwrap_or_else(|| format!("Adopt a constrained {} execution path.", request.output_type));

    let why_selected = dedup_non_empty(vec![
        find_turn(rounds, DebateRound::Round3, DebateRole::Synthesizer)
            .map(|turn| summarize_text_line(&turn.rationale, 180))
            .unwrap_or_default(),
        find_turn(rounds, DebateRound::Round3, DebateRole::Analyst)
            .map(|turn| summarize_text_line(&turn.rationale, 180))
            .unwrap_or_default(),
        "Chosen for replayability, explicit risk handling, and direct actionability.".to_string(),
    ]);

    let rejected_options = dedup_non_empty(
        rounds
            .iter()
            .flat_map(|round| round.turns.iter())
            .filter(|turn| turn.role == DebateRole::Critic.as_str())
            .map(|turn| summarize_text_line(&turn.claim, 120))
            .take(2)
            .collect::<Vec<_>>(),
    )
    .into_iter()
    .enumerate()
    .map(|(index, option)| DebateRejectedOption {
        option,
        reason: format!("Rejected by judge due to unresolved trade-offs (#{}).", index + 1),
    })
    .collect::<Vec<_>>();

    DebateDecision {
        selected_option,
        why_selected: if why_selected.is_empty() {
            vec!["No explicit rationale captured.".to_string()]
        } else {
            why_selected
        },
        rejected_options,
    }
}

fn build_debate_risks(rounds: &[DebateRoundArtifact]) -> Vec<DebateRisk> {
    let mut raw_risks = rounds
        .iter()
        .flat_map(|round| round.turns.iter())
        .flat_map(|turn| turn.risks.clone())
        .collect::<Vec<_>>();
    if raw_risks.is_empty() {
        raw_risks = vec![
            "Consensus quality may drop when provider failures cluster.".to_string(),
            "Writeback trace could break if local storage is unavailable.".to_string(),
        ];
    }

    dedup_non_empty(raw_risks)
        .into_iter()
        .take(5)
        .map(|risk| DebateRisk {
            severity: classify_risk_severity(&risk).to_string(),
            mitigation: format!("Track via run replay and add explicit check for: {}", summarize_text_line(&risk, 80)),
            risk,
        })
        .collect()
}

fn build_debate_actions(
    output_type: &str,
    decision: &DebateDecision,
    risks: &[DebateRisk],
) -> Vec<DebateAction> {
    let risk_focus = risks
        .first()
        .map(|item| summarize_text_line(&item.risk, 100))
        .unwrap_or_else(|| "No critical risk recorded".to_string());

    vec![
        DebateAction {
            id: "A1".to_string(),
            action: format!(
                "Execute selected option for {}: {}",
                output_type,
                summarize_text_line(&decision.selected_option, 110)
            ),
            owner: "me".to_string(),
            due: due_after_days(1),
        },
        DebateAction {
            id: "A2".to_string(),
            action: format!("Mitigate primary risk: {risk_focus}"),
            owner: "me".to_string(),
            due: due_after_days(3),
        },
        DebateAction {
            id: "A3".to_string(),
            action: "Review execution result and run replay audit.".to_string(),
            owner: "me".to_string(),
            due: due_after_days(7),
        },
    ]
}

pub(crate) fn validate_final_packet(packet: &DebateFinalPacket) -> Result<(), String> {
    if packet.run_id.trim().is_empty() {
        return Err(debate_error("DEBATE_ERR_PACKET", "run_id is required"));
    }
    if packet.problem.trim().is_empty() {
        return Err(debate_error("DEBATE_ERR_PACKET", "problem is required"));
    }
    normalize_debate_output_type(&packet.output_type)?;

    if packet.participants.len() != DebateRole::all().len() {
        return Err(debate_error(
            "DEBATE_ERR_PACKET",
            "participants must contain exactly 5 fixed roles",
        ));
    }

    let mut role_seen = HashSet::new();
    for participant in &packet.participants {
        let Some(role) = parse_debate_role(&participant.role) else {
            return Err(debate_error(
                "DEBATE_ERR_PACKET",
                "participant role contains invalid value",
            ));
        };
        role_seen.insert(role);
        if participant.model_provider.trim().is_empty() || participant.model_name.trim().is_empty() {
            return Err(debate_error(
                "DEBATE_ERR_PACKET",
                "participant provider/model cannot be empty",
            ));
        }
    }
    if role_seen.len() != DebateRole::all().len() {
        return Err(debate_error(
            "DEBATE_ERR_PACKET",
            "participant roles must be unique and complete",
        ));
    }

    if !(0.0..=1.0).contains(&packet.consensus.consensus_score)
        || !(0.0..=1.0).contains(&packet.consensus.confidence_score)
    {
        return Err(debate_error(
            "DEBATE_ERR_PACKET",
            "consensus/confidence score must be in [0,1]",
        ));
    }

    if packet.next_actions.is_empty() {
        return Err(debate_error(
            "DEBATE_ERR_PACKET",
            "next_actions cannot be empty",
        ));
    }
    for action in &packet.next_actions {
        if action.id.trim().is_empty()
            || action.action.trim().is_empty()
            || action.owner.trim().is_empty()
            || NaiveDate::parse_from_str(action.due.trim(), "%Y-%m-%d").is_err()
        {
            return Err(debate_error(
                "DEBATE_ERR_PACKET",
                "next_actions contain invalid fields",
            ));
        }
    }

    if packet.timestamps.started_at.trim().is_empty() || packet.timestamps.finished_at.trim().is_empty() {
        return Err(debate_error(
            "DEBATE_ERR_PACKET",
            "timestamps.started_at/finished_at are required",
        ));
    }

    Ok(())
}

fn write_json_artifact<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    write_atomic(path, &bytes).map_err(|error| error.to_string())
}

pub(crate) fn render_debate_packet_markdown(packet: &DebateFinalPacket) -> String {
    let mut lines = vec![
        format!("# Debate Final Packet - {}", packet.run_id),
        String::new(),
        format!("**Mode:** {}", packet.mode),
        format!("**Output Type:** {}", packet.output_type),
        format!("**Started:** {}", packet.timestamps.started_at),
        format!("**Finished:** {}", packet.timestamps.finished_at),
        String::new(),
        "## Problem".to_string(),
        packet.problem.clone(),
        String::new(),
        "## Constraints".to_string(),
    ];

    if packet.constraints.is_empty() {
        lines.push("- none".to_string());
    } else {
        for item in &packet.constraints {
            lines.push(format!("- {item}"));
        }
    }

    lines.push(String::new());
    lines.push("## Conclusion".to_string());
    lines.push(format!(
        "- TL;DR: {}",
        summarize_text_line(&packet.decision.selected_option, 110)
    ));
    lines.push(format!("- Selected: {}", packet.decision.selected_option));
    lines.push(String::new());
    lines.push("## Why Selected".to_string());
    for item in &packet.decision.why_selected {
        lines.push(format!("- {item}"));
    }
    if !packet.decision.rejected_options.is_empty() {
        lines.push("- Rejected options:".to_string());
        for item in &packet.decision.rejected_options {
            lines.push(format!("  - {} ({})", item.option, item.reason));
        }
    }

    lines.push(String::new());
    lines.push("## Consensus".to_string());
    lines.push(format!(
        "- consensus_score: {:.3}",
        packet.consensus.consensus_score
    ));
    lines.push(format!(
        "- confidence_score: {:.3}",
        packet.consensus.confidence_score
    ));
    lines.push("- agreements:".to_string());
    for item in &packet.consensus.key_agreements {
        lines.push(format!("  - {item}"));
    }
    lines.push("- disagreements:".to_string());
    for item in &packet.consensus.key_disagreements {
        lines.push(format!("  - {item}"));
    }

    lines.push(String::new());
    lines.push("## Risks".to_string());
    for item in &packet.risks {
        lines.push(format!(
            "- [{}] {} -> mitigation: {}",
            item.severity, item.risk, item.mitigation
        ));
    }

    lines.push(String::new());
    lines.push("## Next Actions".to_string());
    for item in &packet.next_actions {
        lines.push(format!(
            "- {} | {} | owner={} | due={}",
            item.id, item.action, item.owner, item.due
        ));
    }

    lines.push(String::new());
    lines.push("## Trace".to_string());
    lines.push(format!("- round refs: {}", packet.trace.round_refs.join(", ")));
    for evidence in &packet.trace.evidence_refs {
        lines.push(format!("- evidence: {evidence}"));
    }

    lines.join("\n")
}

fn writeback_debate_result(
    central_home: &Path,
    request: &DebateNormalizedRequest,
    final_packet: &DebateFinalPacket,
) -> Result<Record, String> {
    let target_type = select_writeback_record_type(
        request.writeback_record_type.as_deref(),
        &final_packet.output_type,
    );
    let title = format!(
        "Debate {} - {}",
        final_packet.output_type.to_uppercase(),
        summarize_text_line(&final_packet.problem, 56)
    );

    let mut body_lines = vec![
        format!("Run ID: `{}`", final_packet.run_id),
        String::new(),
        "## Conclusion".to_string(),
        format!(
            "TL;DR: {}",
            summarize_text_line(&final_packet.decision.selected_option, 110)
        ),
        String::new(),
        "## Selected Option".to_string(),
        final_packet.decision.selected_option.clone(),
        String::new(),
        "## Why Selected".to_string(),
    ];
    for item in &final_packet.decision.why_selected {
        body_lines.push(format!("- {item}"));
    }
    body_lines.push(String::new());
    body_lines.push("## Risks".to_string());
    for item in &final_packet.risks {
        body_lines.push(format!("- [{}] {}", item.severity, item.risk));
    }
    body_lines.push(String::new());
    body_lines.push("## Next Actions".to_string());
    for item in &final_packet.next_actions {
        body_lines.push(format!("- {} ({}) due {}", item.action, item.id, item.due));
    }

    let now = Local::now();
    let payload = RecordPayload {
        record_type: target_type,
        title,
        created_at: Some(final_packet.timestamps.finished_at.clone()),
        source_text: Some(final_packet.problem.clone()),
        final_body: Some(body_lines.join("\n")),
        tags: Some(vec![
            "debate".to_string(),
            "debate-v0.1".to_string(),
            final_packet.output_type.clone(),
            format!("run:{}", final_packet.run_id),
        ]),
        date: Some(now.format("%Y-%m-%d").to_string()),
        notion_page_id: None,
        notion_url: None,
        notion_sync_status: Some("SUCCESS".to_string()),
        notion_error: None,
        notion_last_synced_at: None,
        notion_last_edited_time: None,
        notion_last_synced_hash: None,
    };

    upsert_record(central_home.to_string_lossy().to_string(), payload, None)
}

fn select_writeback_record_type(requested: Option<&str>, output_type: &str) -> String {
    if let Some(value) = requested {
        let normalized = normalize_record_type(value);
        if normalized == "decision" || normalized == "worklog" {
            return normalized;
        }
    }

    if output_type == "decision" {
        "decision".to_string()
    } else {
        "worklog".to_string()
    }
}

fn upsert_debate_index(
    central_home: &Path,
    final_packet: &DebateFinalPacket,
    rounds: &[DebateRoundArtifact],
    degraded: bool,
    artifacts_root: &Path,
    writeback_json_path: Option<String>,
) -> Result<(), String> {
    let mut conn = open_index_connection(central_home)?;
    ensure_index_schema(&conn)?;
    let tx = conn.transaction().map_err(|error| error.to_string())?;

    tx.execute(
        "INSERT INTO debate_runs (
            run_id,
            output_type,
            problem,
            consensus_score,
            confidence_score,
            selected_option,
            degraded,
            started_at,
            finished_at,
            artifacts_root,
            final_packet_path,
            writeback_json_path
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(run_id) DO UPDATE SET
            output_type=excluded.output_type,
            problem=excluded.problem,
            consensus_score=excluded.consensus_score,
            confidence_score=excluded.confidence_score,
            selected_option=excluded.selected_option,
            degraded=excluded.degraded,
            started_at=excluded.started_at,
            finished_at=excluded.finished_at,
            artifacts_root=excluded.artifacts_root,
            final_packet_path=excluded.final_packet_path,
            writeback_json_path=excluded.writeback_json_path",
        params![
            final_packet.run_id,
            final_packet.output_type,
            final_packet.problem,
            final_packet.consensus.consensus_score,
            final_packet.consensus.confidence_score,
            final_packet.decision.selected_option,
            if degraded { 1 } else { 0 },
            final_packet.timestamps.started_at,
            final_packet.timestamps.finished_at,
            artifacts_root.to_string_lossy().to_string(),
            artifacts_root.join("final-packet.json").to_string_lossy().to_string(),
            writeback_json_path.unwrap_or_default(),
        ],
    )
    .map_err(|error| error.to_string())?;

    tx.execute(
        "DELETE FROM debate_turns WHERE run_id = ?",
        params![final_packet.run_id],
    )
    .map_err(|error| error.to_string())?;
    tx.execute(
        "DELETE FROM debate_actions WHERE run_id = ?",
        params![final_packet.run_id],
    )
    .map_err(|error| error.to_string())?;

    for round in rounds {
        for turn in &round.turns {
            let challenges = serde_json::to_string(&turn.challenges).unwrap_or_else(|_| "[]".to_string());
            let revisions = serde_json::to_string(&turn.revisions).unwrap_or_else(|_| "[]".to_string());
            tx.execute(
                "INSERT INTO debate_turns (
                    run_id,
                    round_number,
                    role,
                    provider,
                    model_name,
                    status,
                    claim,
                    rationale,
                    challenges_json,
                    revisions_json,
                    error_code,
                    error_message,
                    duration_ms,
                    started_at,
                    finished_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    final_packet.run_id,
                    round_number_from_str(&round.round),
                    turn.role,
                    turn.model_provider,
                    turn.model_name,
                    turn.status,
                    turn.claim,
                    turn.rationale,
                    challenges,
                    revisions,
                    turn.error_code.clone().unwrap_or_default(),
                    turn.error_message.clone().unwrap_or_default(),
                    i64::try_from(turn.duration_ms).unwrap_or(i64::MAX),
                    turn.started_at,
                    turn.finished_at,
                ],
            )
            .map_err(|error| error.to_string())?;
        }
    }

    for action in &final_packet.next_actions {
        tx.execute(
            "INSERT INTO debate_actions (
                run_id,
                action_id,
                action,
                owner,
                due,
                status
            ) VALUES (?, ?, ?, ?, ?, ?)",
            params![
                final_packet.run_id,
                action.id,
                action.action,
                action.owner,
                action.due,
                "OPEN",
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    tx.commit().map_err(|error| error.to_string())
}

fn count_debate_turns(central_home: &Path, run_id: &str) -> Result<usize, String> {
    let conn = open_index_connection(central_home)?;
    ensure_index_schema(&conn)?;
    conn.query_row(
        "SELECT COUNT(*) FROM debate_turns WHERE run_id = ?",
        params![run_id],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

fn count_debate_actions(central_home: &Path, run_id: &str) -> Result<usize, String> {
    let conn = open_index_connection(central_home)?;
    ensure_index_schema(&conn)?;
    conn.query_row(
        "SELECT COUNT(*) FROM debate_actions WHERE run_id = ?",
        params![run_id],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

fn read_json_value(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Err(debate_error(
            "DEBATE_ERR_NOT_FOUND",
            &format!("Artifact missing: {}", path.to_string_lossy()),
        ));
    }
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str::<Value>(&content).map_err(|error| error.to_string())
}

fn debate_error(code: &str, message: &str) -> String {
    format!("{code}: {message}")
}

fn parse_debate_error(error: &str) -> (Option<String>, String) {
    if let Some((code, rest)) = error.split_once(':') {
        let trimmed_code = code.trim().to_string();
        let trimmed_rest = rest.trim().to_string();
        if trimmed_code.starts_with("DEBATE_") {
            return (Some(trimmed_code), trimmed_rest);
        }
    }
    (None, error.to_string())
}

fn generate_debate_run_id() -> String {
    let now = Local::now();
    let millis = now.timestamp_millis().abs();
    format!(
        "debate_{}_{}",
        now.format("%Y%m%d_%H%M%S"),
        millis % 100000
    )
}

fn dedup_non_empty(items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in items {
        let clean = item.trim().to_string();
        if clean.is_empty() || !seen.insert(clean.clone()) {
            continue;
        }
        out.push(clean);
    }
    out
}

fn summarize_text_line(value: &str, max_chars: usize) -> String {
    let line = value
        .lines()
        .map(|item| item.trim())
        .find(|item| !item.is_empty())
        .unwrap_or("")
        .to_string();

    if line.chars().count() <= max_chars {
        line
    } else {
        let truncated = line.chars().take(max_chars.saturating_sub(1)).collect::<String>();
        format!("{}…", truncated.trim())
    }
}

fn trim_bullet_prefix(value: &str) -> &str {
    value.trim_start_matches(['-', '*', '•', ' '])
}

fn strip_claim_label(value: &str) -> &str {
    let labels = ["claim:", "claim：", "主張:", "主張：", "結論:", "結論："];
    let normalized = trim_bullet_prefix(value);
    let lower = normalized.to_lowercase();
    for label in labels {
        if lower.starts_with(label) {
            return normalized[label.len()..].trim();
        }
    }
    normalized
}

fn extract_first_non_empty_line(value: &str) -> String {
    value
        .lines()
        .map(|line| strip_claim_label(line.trim()))
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string()
}

pub(crate) fn extract_claim_text(value: &str) -> Option<String> {
    let claim_labels = ["claim:", "claim：", "主張:", "主張：", "結論:", "結論："];
    let stop_labels = [
        "rationale:",
        "rationale：",
        "reason:",
        "reason：",
        "why:",
        "why：",
        "risks:",
        "risks：",
        "risk:",
        "risk：",
        "理由:",
        "理由：",
        "風險:",
        "風險：",
    ];

    let lines = value.lines().map(|line| line.trim()).collect::<Vec<_>>();
    let mut collecting = false;
    let mut parts = Vec::new();

    for line in lines {
        if line.is_empty() {
            if collecting {
                break;
            }
            continue;
        }

        let normalized = trim_bullet_prefix(line);
        let lower = normalized.to_lowercase();

        if !collecting {
            if let Some(label) = claim_labels.iter().find(|label| lower.starts_with(*label)) {
                collecting = true;
                let tail = normalized[label.len()..].trim();
                if !tail.is_empty() {
                    parts.push(tail.to_string());
                }
            }
            continue;
        }

        if stop_labels.iter().any(|label| lower.starts_with(label)) {
            break;
        }
        parts.push(normalized.to_string());
    }

    let claim = parts.join(" ").trim().to_string();
    if claim.is_empty() {
        None
    } else {
        Some(claim)
    }
}

fn extract_risk_lines(value: &str) -> Vec<String> {
    let mut risks = value
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .filter(|line| {
            let lower = line.to_lowercase();
            lower.contains("risk")
                || lower.contains("blocker")
                || lower.contains("issue")
                || lower.contains("failure")
                || lower.contains("風險")
                || lower.contains("阻塞")
                || lower.contains("問題")
        })
        .map(|line| line.trim_start_matches(['-', '*', '•', ' ']).trim().to_string())
        .collect::<Vec<_>>();

    if risks.is_empty() {
        let fallback = summarize_text_line(value, 130);
        if !fallback.is_empty() {
            risks.push(format!("Potential risk: {fallback}"));
        }
    }

    dedup_non_empty(risks)
}

fn find_turn<'a>(
    rounds: &'a [DebateRoundArtifact],
    round: DebateRound,
    role: DebateRole,
) -> Option<&'a DebateTurn> {
    rounds
        .iter()
        .find(|artifact| artifact.round == round.as_str())
        .and_then(|artifact| {
            artifact
                .turns
                .iter()
                .find(|turn| turn.role == role.as_str() && turn.status == "ok")
        })
}

fn round_score(value: f64) -> f64 {
    (value.clamp(0.0, 1.0) * 1000.0).round() / 1000.0
}

fn classify_risk_severity(risk: &str) -> &'static str {
    let lower = risk.to_lowercase();
    if lower.contains("security")
        || lower.contains("data loss")
        || lower.contains("outage")
        || lower.contains("blocking")
        || lower.contains("critical")
    {
        "high"
    } else if lower.contains("latency")
        || lower.contains("cost")
        || lower.contains("quality")
        || lower.contains("stability")
    {
        "medium"
    } else {
        "low"
    }
}

fn due_after_days(days: i64) -> String {
    (Local::now().date_naive() + ChronoDuration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

fn round_number_from_str(value: &str) -> i64 {
    match value {
        "round-1" => 1,
        "round-2" => 2,
        "round-3" => 3,
        _ => 0,
    }
}

use crate::storage::index::*;
use crate::storage::records::*;
use crate::storage::settings_io::*;
use crate::util::*;
use chrono::{Duration as ChronoDuration, Local, NaiveDate};
use keyring::{Entry, Error as KeyringError};
use reqwest::blocking::Client;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::{Duration as StdDuration, Instant};

pub(crate) const RECORD_TYPE_DIRS: [(&str, &str); 5] = [
    ("decision", "decisions"),
    ("worklog", "worklogs"),
    ("idea", "ideas"),
    ("backlog", "backlogs"),
    ("note", "other"),
];

pub(crate) const OPENAI_SERVICE: &str = "com.keeponfirst.kofnote";
pub(crate) const OPENAI_USERNAME: &str = "openai_api_key";
pub(crate) const GEMINI_USERNAME: &str = "gemini_api_key";
pub(crate) const CLAUDE_USERNAME: &str = "claude_api_key";
pub(crate) const OPENAI_RESPONSES_URL: &str = "https://api.openai.com/v1/responses";
pub(crate) const NOTION_USERNAME: &str = "notion_api_key";
pub(crate) const NOTION_API_VERSION: &str = "2022-06-28";
pub(crate) const NOTION_API_BASE_URL: &str = "https://api.notion.com/v1";
pub(crate) const SETTINGS_DIR_NAME: &str = "kofnote-desktop-tauri";
pub(crate) const SETTINGS_FILE_NAME: &str = "settings.json";
pub(crate) const SEARCH_DB_FILE: &str = "kofnote_search.sqlite";
pub(crate) const DEFAULT_NOTEBOOKLM_COMMAND: &str = "uvx";
pub(crate) const DEFAULT_NOTEBOOKLM_ARGS: [&str; 1] = ["kof-notebooklm-mcp"];
pub(crate) const GEMINI_API_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";
pub(crate) const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";
pub(crate) const ANTHROPIC_API_VERSION: &str = "2023-06-01";
pub(crate) const CODEX_MODEL_FALLBACKS: [&str; 3] = ["gpt-5-codex", "o3", "o4-mini"];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedHome {
    central_home: String,
    corrected: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub(crate) record_type: String,
    pub(crate) title: String,
    pub(crate) created_at: String,
    pub(crate) source_text: String,
    pub(crate) final_body: String,
    pub(crate) tags: Vec<String>,
    pub(crate) date: Option<String>,
    pub(crate) notion_page_id: Option<String>,
    pub(crate) notion_url: Option<String>,
    pub(crate) notion_sync_status: String,
    pub(crate) notion_error: Option<String>,
    pub(crate) notion_last_synced_at: Option<String>,
    pub(crate) notion_last_edited_time: Option<String>,
    pub(crate) notion_last_synced_hash: Option<String>,
    pub(crate) json_path: Option<String>,
    pub(crate) md_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordPayload {
    record_type: String,
    title: String,
    created_at: Option<String>,
    source_text: Option<String>,
    final_body: Option<String>,
    tags: Option<Vec<String>>,
    date: Option<String>,
    notion_page_id: Option<String>,
    notion_url: Option<String>,
    notion_sync_status: Option<String>,
    notion_error: Option<String>,
    notion_last_synced_at: Option<String>,
    notion_last_edited_time: Option<String>,
    notion_last_synced_hash: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub(crate) timestamp: String,
    pub(crate) event_id: String,
    pub(crate) task_intent: String,
    pub(crate) status: String,
    pub(crate) title: String,
    pub(crate) data: Value,
    pub(crate) raw: Value,
    pub(crate) json_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCount {
    tag: String,
    count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyCount {
    date: String,
    count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    total_records: usize,
    total_logs: usize,
    type_counts: HashMap<String, usize>,
    top_tags: Vec<TagCount>,
    recent_daily_counts: Vec<DailyCount>,
    pending_sync_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    records: Vec<Record>,
    total: usize,
    indexed: bool,
    took_ms: u128,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebuildIndexResult {
    indexed_count: usize,
    index_path: String,
    took_ms: u128,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAnalysisResponse {
    provider: String,
    model: String,
    content: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProfile {
    id: String,
    name: String,
    central_home: String,
    default_provider: String,
    default_model: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NotionSettings {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    database_id: String,
}

impl Default for NotionSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            database_id: String::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NotebookLmSettings {
    #[serde(default = "default_notebooklm_command")]
    command: String,
    #[serde(default = "default_notebooklm_args")]
    args: Vec<String>,
    #[serde(default)]
    default_notebook_id: Option<String>,
}

impl Default for NotebookLmSettings {
    fn default() -> Self {
        Self {
            command: default_notebooklm_command(),
            args: default_notebooklm_args(),
            default_notebook_id: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationsSettings {
    #[serde(default)]
    notion: NotionSettings,
    #[serde(default)]
    notebooklm: NotebookLmSettings,
}

impl Default for IntegrationsSettings {
    fn default() -> Self {
        Self {
            notion: NotionSettings::default(),
            notebooklm: NotebookLmSettings::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateProviderConfig {
    id: String,
    #[serde(rename = "type")]
    provider_type: String,
    #[serde(default = "default_enabled_true")]
    enabled: bool,
    #[serde(default)]
    capabilities: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRegistrySettings {
    #[serde(default = "default_debate_provider_configs")]
    providers: Vec<DebateProviderConfig>,
}

impl Default for ProviderRegistrySettings {
    fn default() -> Self {
        Self {
            providers: default_debate_provider_configs(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    profiles: Vec<WorkspaceProfile>,
    #[serde(default)]
    active_profile_id: Option<String>,
    #[serde(default = "default_poll_interval")]
    poll_interval_sec: u64,
    #[serde(default)]
    ui_preferences: Value,
    #[serde(default)]
    integrations: IntegrationsSettings,
    #[serde(default)]
    provider_registry: ProviderRegistrySettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            profiles: Vec::new(),
            active_profile_id: None,
            poll_interval_sec: default_poll_interval(),
            ui_preferences: json!({}),
            integrations: IntegrationsSettings::default(),
            provider_registry: ProviderRegistrySettings::default(),
        }
    }
}

fn default_poll_interval() -> u64 {
    8
}

fn default_enabled_true() -> bool {
    true
}

fn default_debate_provider_configs() -> Vec<DebateProviderConfig> {
    vec![
        DebateProviderConfig {
            id: "codex-cli".to_string(),
            provider_type: "cli".to_string(),
            enabled: true,
            capabilities: vec![
                "debate".to_string(),
                "cli-execution".to_string(),
                "structured-output".to_string(),
            ],
        },
        DebateProviderConfig {
            id: "gemini-cli".to_string(),
            provider_type: "cli".to_string(),
            enabled: true,
            capabilities: vec![
                "debate".to_string(),
                "cli-execution".to_string(),
                "structured-output".to_string(),
            ],
        },
        DebateProviderConfig {
            id: "claude-cli".to_string(),
            provider_type: "cli".to_string(),
            enabled: true,
            capabilities: vec![
                "debate".to_string(),
                "cli-execution".to_string(),
                "structured-output".to_string(),
            ],
        },
        DebateProviderConfig {
            id: "chatgpt-web".to_string(),
            provider_type: "web".to_string(),
            enabled: true,
            capabilities: vec![
                "debate".to_string(),
                "web-automation".to_string(),
                "structured-output".to_string(),
            ],
        },
        DebateProviderConfig {
            id: "gemini-web".to_string(),
            provider_type: "web".to_string(),
            enabled: true,
            capabilities: vec![
                "debate".to_string(),
                "web-automation".to_string(),
                "structured-output".to_string(),
            ],
        },
        DebateProviderConfig {
            id: "claude-web".to_string(),
            provider_type: "web".to_string(),
            enabled: true,
            capabilities: vec![
                "debate".to_string(),
                "web-automation".to_string(),
                "structured-output".to_string(),
            ],
        },
    ]
}

fn default_notebooklm_command() -> String {
    DEFAULT_NOTEBOOKLM_COMMAND.to_string()
}

fn default_notebooklm_args() -> Vec<String> {
    DEFAULT_NOTEBOOKLM_ARGS.iter().map(|item| item.to_string()).collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReportResult {
    output_path: String,
    title: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthDiagnostics {
    central_home: String,
    records_count: usize,
    logs_count: usize,
    index_path: String,
    index_exists: bool,
    indexed_records: usize,
    latest_record_at: String,
    latest_log_at: String,
    has_openai_api_key: bool,
    has_gemini_api_key: bool,
    has_claude_api_key: bool,
    profile_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeFingerprint {
    token: String,
    records_count: usize,
    logs_count: usize,
    latest_record_at: String,
    latest_log_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotionSyncResult {
    json_path: String,
    notion_page_id: Option<String>,
    notion_url: Option<String>,
    notion_sync_status: String,
    notion_error: Option<String>,
    action: String,
    conflict: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotionBatchSyncResult {
    total: usize,
    success: usize,
    failed: usize,
    conflicts: usize,
    results: Vec<NotionSyncResult>,
}

#[derive(Debug, Clone)]
pub struct NotionRemoteRecord {
    page_id: String,
    page_url: Option<String>,
    last_edited_time: Option<String>,
    record_type: String,
    title: String,
    created_at: String,
    date: Option<String>,
    tags: Vec<String>,
    final_body: String,
    source_text: String,
}

#[derive(Debug)]
pub struct NotionUpsertInfo {
    page_id: String,
    page_url: Option<String>,
    last_edited_time: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookLmConfig {
    command: Option<String>,
    args: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookSummary {
    id: String,
    name: String,
    source_count: Option<usize>,
    updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookLmAskResult {
    answer: String,
    citations: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateParticipantConfig {
    role: Option<String>,
    model_provider: Option<String>,
    model_name: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateModeRequest {
    problem: String,
    #[serde(default)]
    constraints: Vec<String>,
    output_type: String,
    #[serde(default)]
    participants: Vec<DebateParticipantConfig>,
    max_turn_seconds: Option<u64>,
    max_turn_tokens: Option<u32>,
    writeback_record_type: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebateModeResponse {
    run_id: String,
    mode: String,
    state: String,
    degraded: bool,
    final_packet: DebateFinalPacket,
    artifacts_root: String,
    writeback_json_path: Option<String>,
    error_codes: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebateReplayConsistency {
    files_complete: bool,
    sql_indexed: bool,
    issues: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebateReplayResponse {
    run_id: String,
    request: Value,
    rounds: Vec<Value>,
    consensus: Value,
    final_packet: DebateFinalPacket,
    writeback_record: Option<Record>,
    consistency: DebateReplayConsistency,
}

#[derive(Debug, Clone)]
pub struct DebateRuntimeParticipant {
    role: DebateRole,
    model_provider: String,
    provider_type: String,
    model_name: String,
}

#[derive(Debug, Clone)]
pub struct DebateNormalizedRequest {
    problem: String,
    constraints: Vec<String>,
    output_type: String,
    participants: Vec<DebateRuntimeParticipant>,
    max_turn_seconds: u64,
    max_turn_tokens: u32,
    writeback_record_type: Option<String>,
    warning_codes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DebateProviderRegistry {
    providers: HashMap<String, DebateProviderConfig>,
}

impl DebateProviderRegistry {
    fn from_settings(settings: &AppSettings) -> Self {
        let mut providers = HashMap::new();
        for item in &settings.provider_registry.providers {
            let id = item.id.trim().to_lowercase();
            if id.is_empty() {
                continue;
            }
            providers.insert(
                id.clone(),
                DebateProviderConfig {
                    id,
                    provider_type: normalize_provider_type(&item.provider_type),
                    enabled: item.enabled,
                    capabilities: normalize_provider_capabilities(&item.capabilities),
                },
            );
        }
        Self { providers }
    }

    fn get(&self, provider_id: &str) -> Option<&DebateProviderConfig> {
        self.providers.get(&provider_id.trim().to_lowercase())
    }

    fn is_enabled(&self, provider_id: &str) -> bool {
        self.get(provider_id).map(|item| item.enabled).unwrap_or(false)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DebateRole {
    Proponent,
    Critic,
    Analyst,
    Synthesizer,
    Judge,
}

impl DebateRole {
    fn all() -> [Self; 5] {
        [
            Self::Proponent,
            Self::Critic,
            Self::Analyst,
            Self::Synthesizer,
            Self::Judge,
        ]
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Proponent => "Proponent",
            Self::Critic => "Critic",
            Self::Analyst => "Analyst",
            Self::Synthesizer => "Synthesizer",
            Self::Judge => "Judge",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DebateRound {
    Round1,
    Round2,
    Round3,
}

impl DebateRound {
    fn all() -> [Self; 3] {
        [Self::Round1, Self::Round2, Self::Round3]
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Round1 => "round-1",
            Self::Round2 => "round-2",
            Self::Round3 => "round-3",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum DebateState {
    Intake,
    Round1,
    Round2,
    Round3,
    Consensus,
    Judge,
    Packetize,
    Writeback,
}

impl DebateState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Intake => "Intake",
            Self::Round1 => "Round1",
            Self::Round2 => "Round2",
            Self::Round3 => "Round3",
            Self::Consensus => "Consensus",
            Self::Judge => "Judge",
            Self::Packetize => "Packetize",
            Self::Writeback => "Writeback",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateChallenge {
    source_role: String,
    target_role: String,
    question: String,
    response: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateTurn {
    role: String,
    round: String,
    model_provider: String,
    model_name: String,
    status: String,
    claim: String,
    rationale: String,
    risks: Vec<String>,
    challenges: Vec<DebateChallenge>,
    revisions: Vec<String>,
    target_role: Option<String>,
    duration_ms: u128,
    error_code: Option<String>,
    error_message: Option<String>,
    started_at: String,
    finished_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateRoundArtifact {
    round: String,
    turns: Vec<DebateTurn>,
    started_at: String,
    finished_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebatePacketParticipant {
    role: String,
    model_provider: String,
    model_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebatePacketConsensus {
    consensus_score: f64,
    confidence_score: f64,
    key_agreements: Vec<String>,
    key_disagreements: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateRejectedOption {
    option: String,
    reason: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateDecision {
    selected_option: String,
    why_selected: Vec<String>,
    rejected_options: Vec<DebateRejectedOption>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateRisk {
    risk: String,
    severity: String,
    mitigation: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateAction {
    id: String,
    action: String,
    owner: String,
    due: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateTrace {
    round_refs: Vec<String>,
    evidence_refs: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebatePacketTimestamps {
    started_at: String,
    finished_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateFinalPacket {
    run_id: String,
    mode: String,
    problem: String,
    constraints: Vec<String>,
    output_type: String,
    participants: Vec<DebatePacketParticipant>,
    consensus: DebatePacketConsensus,
    decision: DebateDecision,
    risks: Vec<DebateRisk>,
    next_actions: Vec<DebateAction>,
    trace: DebateTrace,
    timestamps: DebatePacketTimestamps,
}


pub struct DebateLock(pub Mutex<Option<String>>);

pub(crate) fn resolve_central_home(input_path: String) -> Result<ResolvedHome, String> {
    if input_path.trim().is_empty() {
        return Err("Central Home path is required".to_string());
    }

    let input = absolute_path(Path::new(input_path.trim()));
    let resolved = detect_central_home_path(&input);
    ensure_structure(&resolved).map_err(|error| error.to_string())?;

    Ok(ResolvedHome {
        central_home: resolved.to_string_lossy().to_string(),
        corrected: resolved != input,
    })
}

pub(crate) fn list_records(central_home: String) -> Result<Vec<Record>, String> {
    let home = normalized_home(&central_home)?;
    load_records(&home)
}

pub(crate) fn list_logs(central_home: String) -> Result<Vec<LogEntry>, String> {
    let home = normalized_home(&central_home)?;
    load_logs(&home)
}

pub(crate) fn get_dashboard_stats(central_home: String) -> Result<DashboardStats, String> {
    let home = normalized_home(&central_home)?;
    let records = load_records(&home)?;
    let logs = load_logs(&home)?;
    Ok(compute_dashboard_stats(&records, &logs))
}

pub(crate) fn upsert_record(
    central_home: String,
    payload: RecordPayload,
    previous_json_path: Option<String>,
) -> Result<Record, String> {
    let home = normalized_home(&central_home)?;
    ensure_structure(&home).map_err(|error| error.to_string())?;

    let record_type = normalize_record_type(&payload.record_type);
    let created_at = payload
        .created_at
        .unwrap_or_else(|| Local::now().to_rfc3339());
    let title = if payload.title.trim().is_empty() {
        "Untitled".to_string()
    } else {
        payload.title.trim().to_string()
    };

    let target_subdir = record_dir_by_type(&record_type);
    let target_dir = home.join("records").join(target_subdir);
    fs::create_dir_all(&target_dir).map_err(|error| error.to_string())?;

    let existing_path = previous_json_path
        .as_ref()
        .map(|path| absolute_path(Path::new(path.trim())));
    let existing_value = existing_path.as_ref().and_then(|path| {
        if !path.exists() {
            return None;
        }
        fs::read_to_string(path)
            .ok()
            .and_then(|content| serde_json::from_str::<Value>(&content).ok())
    });

    let base_name = if let Some(path) = &existing_path {
        if path.exists() {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_else(|| generate_filename(&record_type, &title))
        } else {
            generate_filename(&record_type, &title)
        }
    } else {
        generate_filename(&record_type, &title)
    };

    let json_path = target_dir.join(format!("{base_name}.json"));
    let md_path = target_dir.join(format!("{base_name}.md"));

    let tags = payload.tags.unwrap_or_default();
    let notion_sync_status = payload
        .notion_sync_status
        .unwrap_or_else(|| "SUCCESS".to_string());
    let source_text = payload.source_text.unwrap_or_default();
    let final_body = payload.final_body.unwrap_or_default();
    let notion_last_synced_at = payload
        .notion_last_synced_at
        .or_else(|| existing_value.as_ref().and_then(|value| value_string(value, "notion_last_synced_at")));
    let notion_last_edited_time = payload
        .notion_last_edited_time
        .or_else(|| existing_value.as_ref().and_then(|value| value_string(value, "notion_last_edited_time")));
    let notion_last_synced_hash = payload
        .notion_last_synced_hash
        .or_else(|| existing_value.as_ref().and_then(|value| value_string(value, "notion_last_synced_hash")));

    let persisted = json!({
        "type": record_type,
        "title": title,
        "created_at": created_at,
        "notion_page_id": payload.notion_page_id,
        "notion_url": payload.notion_url,
        "source_text": source_text,
        "final_body": final_body,
        "tags": tags,
        "date": payload.date,
        "notion_sync_status": notion_sync_status,
        "notion_error": payload.notion_error,
        "notion_last_synced_at": notion_last_synced_at,
        "notion_last_edited_time": notion_last_edited_time,
        "notion_last_synced_hash": notion_last_synced_hash,
    });

    let json_bytes = serde_json::to_vec_pretty(&persisted).map_err(|error| error.to_string())?;
    write_atomic(&json_path, &json_bytes).map_err(|error| error.to_string())?;

    let record = record_from_value(&persisted, Some(json_path.clone()), Some(md_path.clone()), None);
    let markdown = render_markdown(&record);
    write_atomic(&md_path, markdown.as_bytes()).map_err(|error| error.to_string())?;

    if let Some(old_path) = existing_path {
        if old_path != json_path {
            let old_md = old_path.with_extension("md");
            let _ = fs::remove_file(&old_path);
            let _ = fs::remove_file(old_md);
            let _ = delete_index_record_if_exists(&home, &old_path.to_string_lossy());
        }
    }

    let _ = upsert_index_record_if_exists(&home, &record);
    Ok(record)
}

pub(crate) fn delete_record(central_home: String, json_path: String) -> Result<(), String> {
    let home = normalized_home(&central_home)?;
    let path = absolute_path(Path::new(json_path.trim()));
    if path.exists() {
        fs::remove_file(&path).map_err(|error| error.to_string())?;
    }
    let md_path = path.with_extension("md");
    if md_path.exists() {
        fs::remove_file(md_path).map_err(|error| error.to_string())?;
    }

    let _ = delete_index_record_if_exists(&home, &path.to_string_lossy());
    Ok(())
}

pub(crate) fn rebuild_search_index(central_home: String) -> Result<RebuildIndexResult, String> {
    let started = Instant::now();
    let home = normalized_home(&central_home)?;
    let records = load_records(&home)?;
    let indexed_count = rebuild_index(&home, &records)?;

    Ok(RebuildIndexResult {
        indexed_count,
        index_path: index_db_path(&home).to_string_lossy().to_string(),
        took_ms: started.elapsed().as_millis(),
    })
}

pub(crate) fn search_records(
    central_home: String,
    query: Option<String>,
    record_type: Option<String>,
    date_from: Option<String>,
    date_to: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<SearchResult, String> {
    let started = Instant::now();
    let home = normalized_home(&central_home)?;
    let limit = limit.unwrap_or(200).clamp(1, 1000);
    let offset = offset.unwrap_or(0);

    let query_text = query.unwrap_or_default().trim().to_string();
    let record_type = record_type
        .map(|item| normalize_record_type(&item))
        .filter(|item| !item.is_empty());
    let date_from = sanitize_date_filter(date_from);
    let date_to = sanitize_date_filter(date_to);

    let use_index = !query_text.is_empty();

    let (records, total, indexed) = if use_index {
        if !index_db_path(&home).exists() {
            let _ = rebuild_index(&home, &load_records(&home)?);
        }

        match search_records_in_index(
            &home,
            &query_text,
            record_type.as_deref(),
            date_from.as_deref(),
            date_to.as_deref(),
            limit,
            offset,
        ) {
            Ok(result) => (result.0, result.1, true),
            Err(_) => {
                let all = load_records(&home)?;
                let records = search_records_in_memory(
                    &all,
                    &query_text,
                    record_type.as_deref(),
                    date_from.as_deref(),
                    date_to.as_deref(),
                    limit,
                    offset,
                );
                let total = count_records_in_memory(
                    &all,
                    &query_text,
                    record_type.as_deref(),
                    date_from.as_deref(),
                    date_to.as_deref(),
                );
                (records, total, false)
            }
        }
    } else {
        let all = load_records(&home)?;
        let filtered = search_records_in_memory(
            &all,
            "",
            record_type.as_deref(),
            date_from.as_deref(),
            date_to.as_deref(),
            limit,
            offset,
        );
        let total = count_records_in_memory(&all, "", record_type.as_deref(), date_from.as_deref(), date_to.as_deref());
        (filtered, total, false)
    };

    Ok(SearchResult {
        records,
        total,
        indexed,
        took_ms: started.elapsed().as_millis(),
    })
}

pub(crate) fn run_ai_analysis(
    central_home: String,
    provider: Option<String>,
    model: Option<String>,
    prompt: String,
    api_key: Option<String>,
    include_logs: Option<bool>,
    max_records: Option<usize>,
) -> Result<AiAnalysisResponse, String> {
    let home = normalized_home(&central_home)?;
    let provider = provider
        .unwrap_or_else(|| "local".to_string())
        .trim()
        .to_lowercase();
    let model = model.unwrap_or_else(|| "gpt-4.1-mini".to_string());

    let records = load_records(&home)?;
    let logs = load_logs(&home)?;
    let include_logs = include_logs.unwrap_or(true);
    let max_records = max_records.unwrap_or(30).clamp(1, 200);

    let content = match provider.as_str() {
        "openai" => run_openai_analysis(&model, &prompt, api_key, &records, &logs, include_logs, max_records)?,
        "gemini" => run_gemini_analysis(&model, &prompt, &records, &logs, include_logs, max_records)?,
        "claude" => run_claude_analysis(&model, &prompt, &records, &logs, include_logs, max_records)?,
        "local" => run_local_analysis(&prompt, &records, &logs),
        _ => return Err(format!("Unsupported provider: {provider}")),
    };

    Ok(AiAnalysisResponse {
        provider,
        model,
        content,
    })
}

pub(crate) fn export_markdown_report(
    central_home: String,
    output_path: Option<String>,
    title: Option<String>,
    recent_days: Option<i64>,
) -> Result<ExportReportResult, String> {
    let home = normalized_home(&central_home)?;
    let records = load_records(&home)?;
    let logs = load_logs(&home)?;
    let stats = compute_dashboard_stats(&records, &logs);

    let now = Local::now();
    let title = title.unwrap_or_else(|| format!("KOF Report {}", now.format("%Y-%m-%d")));
    let days = recent_days.unwrap_or(7).clamp(1, 365);

    let cutoff = now.date_naive() - ChronoDuration::days(days);

    let recent_records: Vec<&Record> = records
        .iter()
        .filter(|item| {
            extract_day(&item.created_at)
                .and_then(|day| NaiveDate::parse_from_str(&day, "%Y-%m-%d").ok())
                .map(|date| date >= cutoff)
                .unwrap_or(false)
        })
        .take(80)
        .collect();

    let report_md = render_report_markdown(&title, &home, &stats, &recent_records, days);

    let target = if let Some(path) = output_path {
        absolute_path(Path::new(path.trim()))
    } else {
        let report_dir = home.join("assets").join("reports");
        fs::create_dir_all(&report_dir).map_err(|error| error.to_string())?;
        report_dir.join(format!(
            "{}_kof-report.md",
            now.format("%Y%m%d_%H%M%S")
        ))
    };

    write_atomic(&target, report_md.as_bytes()).map_err(|error| error.to_string())?;

    Ok(ExportReportResult {
        output_path: target.to_string_lossy().to_string(),
        title,
    })
}

pub(crate) fn get_home_fingerprint(central_home: String) -> Result<HomeFingerprint, String> {
    let home = normalized_home(&central_home)?;
    let records = load_records(&home)?;
    let logs = load_logs(&home)?;

    let latest_record_at = records.first().map(|item| item.created_at.clone()).unwrap_or_default();
    let latest_log_at = logs.first().map(|item| item.timestamp.clone()).unwrap_or_default();

    let mut hasher = DefaultHasher::new();
    home.to_string_lossy().hash(&mut hasher);
    latest_record_at.hash(&mut hasher);
    latest_log_at.hash(&mut hasher);
    records.len().hash(&mut hasher);
    logs.len().hash(&mut hasher);

    for item in records.iter().take(12) {
        item.title.hash(&mut hasher);
        item.created_at.hash(&mut hasher);
        item.record_type.hash(&mut hasher);
    }

    for item in logs.iter().take(12) {
        item.task_intent.hash(&mut hasher);
        item.timestamp.hash(&mut hasher);
    }

    Ok(HomeFingerprint {
        token: format!("{:x}", hasher.finish()),
        records_count: records.len(),
        logs_count: logs.len(),
        latest_record_at,
        latest_log_at,
    })
}

pub(crate) fn get_health_diagnostics(central_home: String) -> Result<HealthDiagnostics, String> {
    let home = normalized_home(&central_home)?;
    let records = load_records(&home)?;
    let logs = load_logs(&home)?;

    let index_path = index_db_path(&home);
    let index_exists = index_path.exists();
    let indexed_records = if index_exists {
        get_index_count(&home).unwrap_or(0)
    } else {
        0
    };

    let settings = load_settings();

    Ok(HealthDiagnostics {
        central_home: home.to_string_lossy().to_string(),
        records_count: records.len(),
        logs_count: logs.len(),
        index_path: index_path.to_string_lossy().to_string(),
        index_exists,
        indexed_records,
        latest_record_at: records.first().map(|item| item.created_at.clone()).unwrap_or_default(),
        latest_log_at: logs.first().map(|item| item.timestamp.clone()).unwrap_or_default(),
        has_openai_api_key: has_openai_api_key_internal().unwrap_or(false),
        has_gemini_api_key: has_gemini_api_key_internal().unwrap_or(false),
        has_claude_api_key: has_claude_api_key_internal().unwrap_or(false),
        profile_count: settings.profiles.len(),
    })
}

pub(crate) fn get_app_settings() -> Result<AppSettings, String> {
    Ok(load_settings())
}

pub(crate) fn save_app_settings(settings: AppSettings) -> Result<AppSettings, String> {
    let normalized = normalize_settings(settings);
    save_settings(&normalized)?;
    Ok(normalized)
}

pub(crate) fn set_openai_api_key(api_key: String) -> Result<bool, String> {
    if api_key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }

    let entry = keyring_entry()?;
    entry
        .set_password(api_key.trim())
        .map_err(|error| error.to_string())?;
    Ok(true)
}

pub(crate) fn has_openai_api_key() -> Result<bool, String> {
    has_openai_api_key_internal()
}

pub(crate) fn clear_openai_api_key() -> Result<bool, String> {
    let entry = keyring_entry()?;
    match entry.delete_password() {
        Ok(_) => Ok(true),
        Err(KeyringError::NoEntry) => Ok(true),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn set_gemini_api_key(api_key: String) -> Result<bool, String> {
    if api_key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }

    let entry = gemini_keyring_entry()?;
    entry
        .set_password(api_key.trim())
        .map_err(|error| error.to_string())?;
    Ok(true)
}

pub(crate) fn has_gemini_api_key() -> Result<bool, String> {
    has_gemini_api_key_internal()
}

pub(crate) fn clear_gemini_api_key() -> Result<bool, String> {
    let entry = gemini_keyring_entry()?;
    match entry.delete_password() {
        Ok(_) => Ok(true),
        Err(KeyringError::NoEntry) => Ok(true),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn set_claude_api_key(api_key: String) -> Result<bool, String> {
    if api_key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }

    let entry = claude_keyring_entry()?;
    entry
        .set_password(api_key.trim())
        .map_err(|error| error.to_string())?;
    Ok(true)
}

pub(crate) fn has_claude_api_key() -> Result<bool, String> {
    has_claude_api_key_internal()
}

pub(crate) fn clear_claude_api_key() -> Result<bool, String> {
    let entry = claude_keyring_entry()?;
    match entry.delete_password() {
        Ok(_) => Ok(true),
        Err(KeyringError::NoEntry) => Ok(true),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn set_notion_api_key(api_key: String) -> Result<bool, String> {
    if api_key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }

    let entry = notion_keyring_entry()?;
    entry
        .set_password(api_key.trim())
        .map_err(|error| error.to_string())?;
    Ok(true)
}

pub(crate) fn has_notion_api_key() -> Result<bool, String> {
    has_notion_api_key_internal()
}

pub(crate) fn clear_notion_api_key() -> Result<bool, String> {
    let entry = notion_keyring_entry()?;
    match entry.delete_password() {
        Ok(_) => Ok(true),
        Err(KeyringError::NoEntry) => Ok(true),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn sync_record_to_notion(
    central_home: String,
    json_path: String,
    database_id: Option<String>,
    conflict_strategy: Option<String>,
) -> Result<NotionSyncResult, String> {
    let home = normalized_home(&central_home)?;
    let settings = load_settings();
    let db_id = resolve_notion_database_id(database_id, &settings)?;
    let api_key = resolve_notion_api_key(None)?;
    let strategy = normalize_conflict_strategy(conflict_strategy);
    sync_record_to_notion_internal(&home, &json_path, &db_id, &api_key, &strategy)
}

pub(crate) fn sync_records_to_notion(
    central_home: String,
    json_paths: Vec<String>,
    database_id: Option<String>,
    conflict_strategy: Option<String>,
) -> Result<NotionBatchSyncResult, String> {
    let home = normalized_home(&central_home)?;
    let settings = load_settings();
    let db_id = resolve_notion_database_id(database_id, &settings)?;
    let api_key = resolve_notion_api_key(None)?;
    let strategy = normalize_conflict_strategy(conflict_strategy);

    let mut results: Vec<NotionSyncResult> = Vec::new();
    let mut success = 0usize;
    let mut failed = 0usize;
    let mut conflicts = 0usize;

    for item in json_paths {
        match sync_record_to_notion_internal(&home, &item, &db_id, &api_key, &strategy) {
            Ok(result) => {
                if result.conflict {
                    conflicts += 1;
                    failed += 1;
                } else if result.notion_sync_status == "SUCCESS" {
                    success += 1;
                } else {
                    failed += 1;
                }
                results.push(result);
            }
            Err(error) => {
                failed += 1;
                results.push(NotionSyncResult {
                    json_path: item,
                    notion_page_id: None,
                    notion_url: None,
                    notion_sync_status: "FAILED".to_string(),
                    notion_error: Some(error),
                    action: "error".to_string(),
                    conflict: false,
                });
            }
        }
    }

    Ok(NotionBatchSyncResult {
        total: results.len(),
        success,
        failed,
        conflicts,
        results,
    })
}

pub(crate) fn sync_record_bidirectional(
    central_home: String,
    json_path: String,
    database_id: Option<String>,
    conflict_strategy: Option<String>,
) -> Result<NotionSyncResult, String> {
    let home = normalized_home(&central_home)?;
    let settings = load_settings();
    let db_id = resolve_notion_database_id(database_id, &settings)?;
    let api_key = resolve_notion_api_key(None)?;
    let strategy = normalize_conflict_strategy(conflict_strategy);
    sync_record_bidirectional_internal(&home, &json_path, &db_id, &api_key, &strategy)
}

pub(crate) fn sync_records_bidirectional(
    central_home: String,
    json_paths: Vec<String>,
    database_id: Option<String>,
    conflict_strategy: Option<String>,
) -> Result<NotionBatchSyncResult, String> {
    let home = normalized_home(&central_home)?;
    let settings = load_settings();
    let db_id = resolve_notion_database_id(database_id, &settings)?;
    let api_key = resolve_notion_api_key(None)?;
    let strategy = normalize_conflict_strategy(conflict_strategy);

    let mut results: Vec<NotionSyncResult> = Vec::new();
    let mut success = 0usize;
    let mut failed = 0usize;
    let mut conflicts = 0usize;

    for item in json_paths {
        match sync_record_bidirectional_internal(&home, &item, &db_id, &api_key, &strategy) {
            Ok(result) => {
                if result.conflict {
                    conflicts += 1;
                    failed += 1;
                } else if result.notion_sync_status == "SUCCESS" {
                    success += 1;
                } else {
                    failed += 1;
                }
                results.push(result);
            }
            Err(error) => {
                failed += 1;
                results.push(NotionSyncResult {
                    json_path: item,
                    notion_page_id: None,
                    notion_url: None,
                    notion_sync_status: "FAILED".to_string(),
                    notion_error: Some(error),
                    action: "error".to_string(),
                    conflict: false,
                });
            }
        }
    }

    Ok(NotionBatchSyncResult {
        total: results.len(),
        success,
        failed,
        conflicts,
        results,
    })
}

pub(crate) fn pull_records_from_notion(
    central_home: String,
    database_id: Option<String>,
    conflict_strategy: Option<String>,
) -> Result<NotionBatchSyncResult, String> {
    let home = normalized_home(&central_home)?;
    let settings = load_settings();
    let db_id = resolve_notion_database_id(database_id, &settings)?;
    let api_key = resolve_notion_api_key(None)?;
    let strategy = normalize_conflict_strategy(conflict_strategy);
    pull_records_from_notion_internal(&home, &db_id, &api_key, &strategy)
}

pub(crate) fn notebooklm_health_check(config: Option<NotebookLmConfig>) -> Result<Value, String> {
    notebooklm_call_tool("health_check", json!({}), config)
}

pub(crate) fn notebooklm_list_notebooks(
    limit: Option<usize>,
    config: Option<NotebookLmConfig>,
) -> Result<Vec<NotebookSummary>, String> {
    let payload = notebooklm_call_tool(
        "list_notebooks",
        json!({ "limit": limit.unwrap_or(20).clamp(1, 100) }),
        config,
    )?;
    let notebooks = payload
        .get("notebooks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(notebooks
        .iter()
        .map(parse_notebook_summary)
        .collect::<Vec<_>>())
}

pub(crate) fn notebooklm_create_notebook(
    title: Option<String>,
    config: Option<NotebookLmConfig>,
) -> Result<NotebookSummary, String> {
    let payload = notebooklm_call_tool(
        "create_notebook",
        json!({ "title": title.unwrap_or_else(|| "KOF Note Notebook".to_string()) }),
        config,
    )?;
    let notebook = payload
        .get("notebook")
        .cloned()
        .unwrap_or_else(|| payload.clone());
    Ok(parse_notebook_summary(&notebook))
}

pub(crate) fn notebooklm_add_record_source(
    central_home: String,
    json_path: String,
    notebook_id: String,
    title: Option<String>,
    config: Option<NotebookLmConfig>,
) -> Result<Value, String> {
    let home = normalized_home(&central_home)?;
    let record = load_record_by_json_path(&home, &json_path)?;
    let source_title = title.unwrap_or_else(|| {
        format!(
            "{} | {} | {}",
            record.record_type,
            extract_day(&record.created_at).unwrap_or_else(|| record.created_at.clone()),
            record.title
        )
    });
    let text = render_record_source_text(&record);

    notebooklm_call_tool(
        "add_source",
        json!({
            "notebook_id": notebook_id,
            "source_type": "text",
            "title": source_title,
            "text": text,
        }),
        config,
    )
}

pub(crate) fn notebooklm_ask(
    notebook_id: String,
    question: String,
    include_citations: Option<bool>,
    config: Option<NotebookLmConfig>,
) -> Result<NotebookLmAskResult, String> {
    if question.trim().is_empty() {
        return Err("Question cannot be empty".to_string());
    }

    let payload = notebooklm_call_tool(
        "ask",
        json!({
            "notebook_id": notebook_id,
            "question": question.trim(),
            "include_citations": include_citations.unwrap_or(true),
        }),
        config,
    )?;

    let answer = payload
        .get("answer")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let citations = payload
        .get("citations")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(NotebookLmAskResult { answer, citations })
}

pub(crate) async fn run_debate_mode(
    lock: tauri::State<'_, DebateLock>,
    central_home: String,
    request: DebateModeRequest,
) -> Result<DebateModeResponse, String> {
    // Check lock
    {
        let guard = lock
            .0
            .lock()
            .map_err(|error| format!("Lock poisoned: {error}"))?;
        if let Some(run_id) = guard.as_ref() {
            return Err(format!("Another debate is already running: {run_id}"));
        }
    }
    // Set lock
    {
        let mut guard = lock
            .0
            .lock()
            .map_err(|error| format!("Lock poisoned: {error}"))?;
        *guard = Some(generate_debate_run_id());
    }

    let home = normalized_home(&central_home)?;
    let result = tauri::async_runtime::spawn_blocking(move || run_debate_mode_internal(&home, request))
        .await
        .map_err(|error| format!("Debate worker join error: {error}"));

    // ALWAYS clear lock, even on error.
    if let Ok(mut guard) = lock.0.lock() {
        *guard = None;
    }

    // Flatten: Result<Result<T, E>, E> -> Result<T, E>
    result?
}

pub(crate) async fn replay_debate_mode(
    central_home: String,
    run_id: String,
) -> Result<DebateReplayResponse, String> {
    let home = normalized_home(&central_home)?;
    let run_id = run_id.trim().to_string();
    tauri::async_runtime::spawn_blocking(move || replay_debate_mode_internal(&home, &run_id))
        .await
        .map_err(|error| format!("Debate replay worker join error: {error}"))?
}

fn normalized_home(input: &str) -> Result<PathBuf, String> {
    if input.trim().is_empty() {
        return Err("Central Home path is required".to_string());
    }

    let home = detect_central_home_path(&absolute_path(Path::new(input.trim())));
    ensure_structure(&home).map_err(|error| error.to_string())?;
    Ok(home)
}

fn compute_dashboard_stats(records: &[Record], logs: &[LogEntry]) -> DashboardStats {
    let mut type_counts: HashMap<String, usize> = HashMap::new();
    let mut tags_counter: HashMap<String, usize> = HashMap::new();
    let mut pending_sync_count: usize = 0;

    for record in records {
        *type_counts.entry(record.record_type.clone()).or_insert(0) += 1;
        for tag in &record.tags {
            let clean = tag.trim();
            if !clean.is_empty() {
                *tags_counter.entry(clean.to_string()).or_insert(0) += 1;
            }
        }
        if matches!(
            record.notion_sync_status.as_str(),
            "PENDING" | "FAILED" | "CONFLICT"
        ) {
            pending_sync_count += 1;
        }
    }

    let mut top_tags: Vec<TagCount> = tags_counter
        .into_iter()
        .map(|(tag, count)| TagCount { tag, count })
        .collect();
    top_tags.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tag.cmp(&b.tag)));
    top_tags.truncate(12);

    let today = Local::now().date_naive();
    let mut daily_map: HashMap<String, usize> = HashMap::new();
    let mut ordered_days: Vec<String> = Vec::new();

    for offset in (0..=6).rev() {
        let day = today - ChronoDuration::days(offset);
        let key = day.format("%Y-%m-%d").to_string();
        daily_map.insert(key.clone(), 0);
        ordered_days.push(key);
    }

    for record in records {
        if let Some(day) = extract_day(&record.created_at) {
            if let Some(value) = daily_map.get_mut(&day) {
                *value += 1;
            }
        }
    }

    for log in logs {
        if let Some(day) = extract_day(&log.timestamp) {
            if let Some(value) = daily_map.get_mut(&day) {
                *value += 1;
            }
        }
    }

    let recent_daily_counts = ordered_days
        .into_iter()
        .map(|date| DailyCount {
            count: *daily_map.get(&date).unwrap_or(&0),
            date,
        })
        .collect::<Vec<_>>();

    DashboardStats {
        total_records: records.len(),
        total_logs: logs.len(),
        type_counts,
        top_tags,
        recent_daily_counts,
        pending_sync_count,
    }
}

fn search_records_in_memory(
    records: &[Record],
    query: &str,
    record_type: Option<&str>,
    date_from: Option<&str>,
    date_to: Option<&str>,
    limit: usize,
    offset: usize,
) -> Vec<Record> {
    let lowered = query.trim().to_lowercase();

    records
        .iter()
        .filter(|item| matches_record(item, &lowered, record_type, date_from, date_to))
        .skip(offset)
        .take(limit)
        .cloned()
        .collect()
}

fn count_records_in_memory(
    records: &[Record],
    query: &str,
    record_type: Option<&str>,
    date_from: Option<&str>,
    date_to: Option<&str>,
) -> usize {
    let lowered = query.trim().to_lowercase();

    records
        .iter()
        .filter(|item| matches_record(item, &lowered, record_type, date_from, date_to))
        .count()
}

fn matches_record(
    record: &Record,
    lowered_query: &str,
    record_type: Option<&str>,
    date_from: Option<&str>,
    date_to: Option<&str>,
) -> bool {
    if let Some(record_type) = record_type {
        if record.record_type != record_type {
            return false;
        }
    }

    if let Some(date_from) = date_from {
        let day = extract_day(&record.created_at).unwrap_or_default();
        if day.as_str() < date_from {
            return false;
        }
    }

    if let Some(date_to) = date_to {
        let day = extract_day(&record.created_at).unwrap_or_default();
        if day.as_str() > date_to {
            return false;
        }
    }

    if lowered_query.is_empty() {
        return true;
    }

    let text = format!(
        "{} {} {} {}",
        record.title,
        record.final_body,
        record.source_text,
        record.tags.join(" ")
    )
    .to_lowercase();

    text.contains(lowered_query)
}

fn run_local_analysis(prompt: &str, records: &[Record], logs: &[LogEntry]) -> String {
    let stats = compute_dashboard_stats(records, logs);

    let dominant_type = stats
        .type_counts
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(name, _)| name.as_str())
        .unwrap_or("-");

    let mut lines = vec![
        "# KOF Local Analysis".to_string(),
        String::new(),
        "## Summary".to_string(),
        format!("- Total records: {}", stats.total_records),
        format!("- Total logs: {}", stats.total_logs),
        format!("- Pending sync records: {}", stats.pending_sync_count),
        format!("- Dominant type: {}", dominant_type),
        String::new(),
        "## Top Tags".to_string(),
    ];

    if stats.top_tags.is_empty() {
        lines.push("- (no tags yet)".to_string());
    } else {
        for item in stats.top_tags.iter().take(8) {
            lines.push(format!("- {} ({})", item.tag, item.count));
        }
    }

    lines.push(String::new());
    lines.push("## Recent Focus".to_string());
    for item in records.iter().take(6) {
        lines.push(format!(
            "- [{}] ({}) {}",
            item.created_at,
            item.record_type,
            item.title
        ));
    }

    lines.push(String::new());
    lines.push("## Risks".to_string());
    if stats.pending_sync_count > 0 {
        lines.push("- Pending sync records may diverge from Notion until re-synced.".to_string());
    } else {
        lines.push("- No immediate sync risk detected.".to_string());
    }
    lines.push("- If many backlogs have no date/tag, prioritization quality may drop.".to_string());

    lines.push(String::new());
    lines.push("## Next 7 Days Action Plan".to_string());
    lines.push("1. Consolidate top recurring tags into 2-3 execution themes.".to_string());
    lines.push("2. Convert high-value backlog items to scheduled worklogs.".to_string());
    lines.push("3. Run weekly review and archive stale notes.".to_string());

    if !prompt.trim().is_empty() {
        lines.push(String::new());
        lines.push("## User Prompt Focus".to_string());
        lines.push(prompt.trim().to_string());
    }

    lines.join("\n")
}

fn run_openai_analysis(
    model: &str,
    prompt: &str,
    api_key: Option<String>,
    records: &[Record],
    logs: &[LogEntry],
    include_logs: bool,
    max_records: usize,
) -> Result<String, String> {
    let api_key = crate::providers::openai::resolve_api_key(api_key)?;

    let context = build_context_digest(records, logs, include_logs, max_records);
    let user_prompt = if prompt.trim().is_empty() {
        "Summarize patterns, risks, and a practical 7-day plan.".to_string()
    } else {
        prompt.trim().to_string()
    };

    let merged_prompt = format!(
        "You are analyzing a local-first productivity brain system. \nReturn concise markdown sections: Summary, Patterns, Risks, Action Plan (7 days).\n\nUser request:\n{}\n\nContext:\n{}",
        user_prompt, context
    );

    let payload = json!({
        "model": model,
        "input": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "input_text",
                        "text": merged_prompt
                    }
                ]
            }
        ]
    });

    let client = Client::builder()
        .timeout(StdDuration::from_secs(50))
        .build()
        .map_err(|error| error.to_string())?;

    let response = client
        .post(OPENAI_RESPONSES_URL)
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .map_err(|error| error.to_string())?;

    let status = response.status();
    let body_text = response.text().map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("OpenAI API {}: {}", status.as_u16(), body_text));
    }

    let value: Value = serde_json::from_str(&body_text).map_err(|error| error.to_string())?;
    let output = crate::providers::openai::extract_openai_output_text(&value);

    if output.trim().is_empty() {
        return Err("OpenAI response did not include readable text".to_string());
    }

    Ok(output)
}

fn run_gemini_analysis(
    model: &str,
    prompt: &str,
    records: &[Record],
    logs: &[LogEntry],
    include_logs: bool,
    max_records: usize,
) -> Result<String, String> {
    let context = build_context_digest(records, logs, include_logs, max_records);
    let user_prompt = if prompt.trim().is_empty() {
        "Summarize patterns, risks, and a practical 7-day plan.".to_string()
    } else {
        prompt.trim().to_string()
    };
    let merged = format!(
        "You are analyzing a local-first productivity brain system.\nReturn concise markdown sections: Summary, Patterns, Risks, Action Plan (7 days).\n\nUser request:\n{}\n\nContext:\n{}",
        user_prompt, context
    );
    let model = if model.trim().is_empty() { "gemini-2.0-flash" } else { model.trim() };
    crate::providers::gemini::run_gemini_text_completion(model, &merged, 60, 4096)
}

fn run_claude_analysis(
    model: &str,
    prompt: &str,
    records: &[Record],
    logs: &[LogEntry],
    include_logs: bool,
    max_records: usize,
) -> Result<String, String> {
    let context = build_context_digest(records, logs, include_logs, max_records);
    let user_prompt = if prompt.trim().is_empty() {
        "Summarize patterns, risks, and a practical 7-day plan.".to_string()
    } else {
        prompt.trim().to_string()
    };
    let merged = format!(
        "You are analyzing a local-first productivity brain system.\nReturn concise markdown sections: Summary, Patterns, Risks, Action Plan (7 days).\n\nUser request:\n{}\n\nContext:\n{}",
        user_prompt, context
    );
    let model = if model.trim().is_empty() {
        "claude-3-5-sonnet-latest"
    } else {
        model.trim()
    };
    crate::providers::claude::run_claude_text_completion(model, &merged, 60, 4096)
}

fn run_debate_mode_internal(central_home: &Path, request: DebateModeRequest) -> Result<DebateModeResponse, String> {
    let settings = load_settings();
    let provider_registry = DebateProviderRegistry::from_settings(&settings);
    let normalized = normalize_debate_request(request, &provider_registry)?;
    ensure_structure(central_home).map_err(|error| error.to_string())?;

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
            let turn = execute_debate_turn(
                participant,
                round,
                target_role,
                &normalized,
                &rounds,
                normalized.max_turn_seconds,
                normalized.max_turn_tokens,
            );

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

fn replay_debate_mode_internal(central_home: &Path, run_id: &str) -> Result<DebateReplayResponse, String> {
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

fn normalize_debate_request(
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
                DebateRuntimeParticipant {
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
            participants.push(DebateRuntimeParticipant {
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

fn validate_debate_transition(current: Option<DebateState>, next: DebateState) -> bool {
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
    participant: &DebateRuntimeParticipant,
    round: DebateRound,
    target_role: Option<DebateRole>,
    request: &DebateNormalizedRequest,
    previous_rounds: &[DebateRoundArtifact],
    max_turn_seconds: u64,
    max_turn_tokens: u32,
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

fn provider_uses_local_stub(provider_id: &str, provider_type: &str) -> bool {
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
        "openai" => crate::providers::openai::run_openai_text_completion(
            model,
            prompt,
            max_turn_seconds,
            max_turn_tokens,
        )
        .map_err(|error| debate_error("DEBATE_ERR_PROVIDER_OPENAI", &error)),
        "gemini" => crate::providers::gemini::run_gemini_text_completion(
            model,
            prompt,
            max_turn_seconds,
            max_turn_tokens,
        )
        .map_err(|error| debate_error("DEBATE_ERR_PROVIDER_GEMINI", &error)),
        "claude" => crate::providers::claude::run_claude_text_completion(
            model,
            prompt,
            max_turn_seconds,
            max_turn_tokens,
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

fn validate_final_packet(packet: &DebateFinalPacket) -> Result<(), String> {
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

fn render_debate_packet_markdown(packet: &DebateFinalPacket) -> String {
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

fn extract_claim_text(value: &str) -> Option<String> {
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

fn build_context_digest(
    records: &[Record],
    logs: &[LogEntry],
    include_logs: bool,
    max_records: usize,
) -> String {
    let mut lines = vec!["# Records".to_string()];

    for item in records.iter().take(max_records) {
        lines.push(format!(
            "- [{}] ({}) {} | tags: {}",
            item.created_at,
            item.record_type,
            item.title,
            if item.tags.is_empty() {
                "-".to_string()
            } else {
                item.tags.join(", ")
            }
        ));
    }

    if include_logs {
        lines.push(String::new());
        lines.push("# Logs".to_string());
        for item in logs.iter().take(max_records.min(40)) {
            lines.push(format!(
                "- [{}] {} / {} / {}",
                item.timestamp, item.task_intent, item.status, item.title
            ));
        }
    }

    lines.join("\n")
}

fn render_report_markdown(
    title: &str,
    central_home: &Path,
    stats: &DashboardStats,
    recent_records: &[&Record],
    days: i64,
) -> String {
    let mut lines = vec![
        format!("# {}", title),
        String::new(),
        format!("Generated: {}", Local::now().to_rfc3339()),
        format!("Central Home: {}", central_home.to_string_lossy()),
        String::new(),
        "## KPI".to_string(),
        format!("- Total records: {}", stats.total_records),
        format!("- Total logs: {}", stats.total_logs),
        format!("- Pending sync: {}", stats.pending_sync_count),
        String::new(),
        "## Type Distribution".to_string(),
    ];

    for (record_type, count) in &stats.type_counts {
        lines.push(format!("- {}: {}", record_type, count));
    }

    lines.push(String::new());
    lines.push("## Top Tags".to_string());
    if stats.top_tags.is_empty() {
        lines.push("- (none)".to_string());
    } else {
        for item in &stats.top_tags {
            lines.push(format!("- {} ({})", item.tag, item.count));
        }
    }

    lines.push(String::new());
    lines.push(format!("## Recent Records (last {} days)", days));
    for item in recent_records {
        lines.push(format!(
            "- [{}] ({}) {}",
            item.created_at, item.record_type, item.title
        ));
    }

    lines.join("\n")
}

fn normalize_provider_type(value: &str) -> String {
    if value.trim().eq_ignore_ascii_case("web") {
        "web".to_string()
    } else {
        "cli".to_string()
    }
}

fn normalize_provider_capabilities(input: &[String]) -> Vec<String> {
    let normalized = input
        .iter()
        .map(|item| item.trim().to_lowercase())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    let deduped = dedup_non_empty(normalized);
    if deduped.is_empty() {
        vec!["debate".to_string()]
    } else {
        deduped
    }
}

fn normalize_provider_registry_settings(
    registry: ProviderRegistrySettings,
) -> ProviderRegistrySettings {
    let mut by_id = HashMap::new();
    for item in default_debate_provider_configs() {
        by_id.insert(item.id.clone(), item);
    }

    for mut item in registry.providers {
        let id = item.id.trim().to_lowercase();
        if id.is_empty() {
            continue;
        }
        let defaults = by_id.get(&id).cloned();
        item.id = id.clone();
        item.provider_type = normalize_provider_type(&item.provider_type);
        item.capabilities = normalize_provider_capabilities(
            &if item.capabilities.is_empty() {
                defaults
                    .as_ref()
                    .map(|base| base.capabilities.clone())
                    .unwrap_or_else(|| vec!["debate".to_string()])
            } else {
                item.capabilities.clone()
            },
        );
        by_id.insert(id, item);
    }

    let mut providers = by_id.into_values().collect::<Vec<_>>();
    providers.sort_by(|a, b| a.id.cmp(&b.id));
    ProviderRegistrySettings { providers }
}

pub(crate) fn normalize_settings(mut settings: AppSettings) -> AppSettings {
    if settings.poll_interval_sec == 0 {
        settings.poll_interval_sec = default_poll_interval();
    }

    let mut seen_ids = HashSet::new();
    for profile in &mut settings.profiles {
        profile.id = if profile.id.trim().is_empty() {
            format!("profile-{}", slugify(&profile.name))
        } else {
            slugify(&profile.id)
        };
        if profile.id.is_empty() {
            profile.id = format!("profile-{}", Local::now().timestamp_millis());
        }

        if seen_ids.contains(&profile.id) {
            profile.id = format!("{}-{}", profile.id, Local::now().timestamp_millis());
        }
        seen_ids.insert(profile.id.clone());

        if profile.name.trim().is_empty() {
            profile.name = "Untitled Profile".to_string();
        }
        profile.central_home = profile.central_home.trim().to_string();

        let provider = profile.default_provider.trim().to_lowercase();
        profile.default_provider = if provider.is_empty() {
            "local".to_string()
        } else {
            provider
        };

        if profile.default_model.trim().is_empty() {
            profile.default_model = "gpt-4.1-mini".to_string();
        }
    }

    if settings.profiles.is_empty() {
        settings.active_profile_id = None;
    } else {
        let active_missing = settings
            .active_profile_id
            .as_ref()
            .map(|active| !settings.profiles.iter().any(|profile| &profile.id == active))
            .unwrap_or(true);

        if active_missing {
            settings.active_profile_id = settings.profiles.first().map(|profile| profile.id.clone());
        }
    }

    settings.integrations.notion.database_id = settings
        .integrations
        .notion
        .database_id
        .trim()
        .to_string();

    let notebook_command = settings.integrations.notebooklm.command.trim().to_string();
    settings.integrations.notebooklm.command = if notebook_command.is_empty() {
        default_notebooklm_command()
    } else {
        notebook_command
    };

    if settings.integrations.notebooklm.args.is_empty() {
        settings.integrations.notebooklm.args = default_notebooklm_args();
    } else {
        settings.integrations.notebooklm.args = settings
            .integrations
            .notebooklm
            .args
            .iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>();
        if settings.integrations.notebooklm.args.is_empty() {
            settings.integrations.notebooklm.args = default_notebooklm_args();
        }
    }

    settings.provider_registry =
        normalize_provider_registry_settings(settings.provider_registry.clone());

    settings
}

fn keyring_entry() -> Result<Entry, String> {
    Entry::new(OPENAI_SERVICE, OPENAI_USERNAME).map_err(|error| error.to_string())
}

fn has_keyring_entry_value(entry: Entry) -> Result<bool, String> {
    match entry.get_password() {
        Ok(value) => Ok(!value.trim().is_empty()),
        Err(KeyringError::NoEntry) => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

fn has_openai_api_key_internal() -> Result<bool, String> {
    has_keyring_entry_value(keyring_entry()?)
}

fn gemini_keyring_entry() -> Result<Entry, String> {
    Entry::new(OPENAI_SERVICE, GEMINI_USERNAME).map_err(|error| error.to_string())
}

fn has_gemini_api_key_internal() -> Result<bool, String> {
    has_keyring_entry_value(gemini_keyring_entry()?)
}

fn claude_keyring_entry() -> Result<Entry, String> {
    Entry::new(OPENAI_SERVICE, CLAUDE_USERNAME).map_err(|error| error.to_string())
}

fn has_claude_api_key_internal() -> Result<bool, String> {
    has_keyring_entry_value(claude_keyring_entry()?)
}

fn notion_keyring_entry() -> Result<Entry, String> {
    Entry::new(OPENAI_SERVICE, NOTION_USERNAME).map_err(|error| error.to_string())
}

fn has_notion_api_key_internal() -> Result<bool, String> {
    has_keyring_entry_value(notion_keyring_entry()?)
}

fn resolve_notion_api_key(api_key: Option<String>) -> Result<String, String> {
    if let Some(value) = api_key {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let entry = notion_keyring_entry()?;
    match entry.get_password() {
        Ok(value) if !value.trim().is_empty() => Ok(value.trim().to_string()),
        Ok(_) => Err("Missing Notion API key. Add it in Settings > Integrations.".to_string()),
        Err(KeyringError::NoEntry) => {
            Err("Missing Notion API key. Add it in Settings > Integrations.".to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

fn resolve_notion_database_id(database_id: Option<String>, settings: &AppSettings) -> Result<String, String> {
    let from_arg = database_id
        .unwrap_or_default()
        .trim()
        .to_string();
    if !from_arg.is_empty() {
        return Ok(from_arg);
    }

    let from_settings = settings.integrations.notion.database_id.trim().to_string();
    if !from_settings.is_empty() {
        return Ok(from_settings);
    }

    Err("Notion database ID is required. Set it in Settings > Integrations.".to_string())
}

fn load_record_by_json_path(central_home: &Path, json_path: &str) -> Result<Record, String> {
    let path = absolute_path(Path::new(json_path.trim()));
    if !path.exists() {
        return Err(format!("Record json not found: {}", path.to_string_lossy()));
    }

    let content = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let value: Value = serde_json::from_str(&content).map_err(|error| error.to_string())?;

    let inferred_type = infer_record_type_from_path(central_home, &path);
    Ok(record_from_value(
        &value,
        Some(path.clone()),
        Some(path.with_extension("md")),
        inferred_type,
    ))
}

fn infer_record_type_from_path(central_home: &Path, path: &Path) -> Option<String> {
    let root = central_home.join("records");
    let relative = path.strip_prefix(&root).ok()?;
    let folder = relative
        .components()
        .next()
        .and_then(|part| part.as_os_str().to_str())?;
    for (record_type, dir) in RECORD_TYPE_DIRS {
        if dir == folder {
            return Some(record_type.to_string());
        }
    }
    None
}

fn normalize_conflict_strategy(value: Option<String>) -> String {
    match value
        .unwrap_or_else(|| "manual".to_string())
        .trim()
        .to_lowercase()
        .as_str()
    {
        "local" | "local_wins" => "local_wins".to_string(),
        "notion" | "notion_wins" | "remote_wins" => "notion_wins".to_string(),
        _ => "manual".to_string(),
    }
}

fn record_sync_hash(record: &Record) -> String {
    let mut hasher = DefaultHasher::new();
    record.record_type.hash(&mut hasher);
    record.title.hash(&mut hasher);
    record.created_at.hash(&mut hasher);
    record.source_text.hash(&mut hasher);
    record.final_body.hash(&mut hasher);
    record.date.hash(&mut hasher);
    for tag in &record.tags {
        tag.hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}

fn local_has_changed_since_sync(record: &Record) -> bool {
    let base = record
        .notion_last_synced_hash
        .clone()
        .unwrap_or_default()
        .trim()
        .to_string();
    if base.is_empty() {
        return true;
    }
    record_sync_hash(record) != base
}

fn remote_has_changed(record: &Record, remote: &NotionRemoteRecord) -> bool {
    let current = remote
        .last_edited_time
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_string();
    if current.is_empty() {
        return false;
    }
    let previous = record
        .notion_last_edited_time
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_string();
    previous != current
}

fn mark_record_synced(record: &mut Record, remote_last_edited_time: Option<String>) {
    record.notion_sync_status = "SUCCESS".to_string();
    record.notion_error = None;
    record.notion_last_synced_at = Some(Local::now().to_rfc3339());
    if let Some(value) = remote_last_edited_time {
        if !value.trim().is_empty() {
            record.notion_last_edited_time = Some(value);
        }
    }
    record.notion_last_synced_hash = Some(record_sync_hash(record));
}

fn build_sync_result(
    json_path: &Path,
    record: &Record,
    action: &str,
    conflict: bool,
) -> NotionSyncResult {
    NotionSyncResult {
        json_path: json_path.to_string_lossy().to_string(),
        notion_page_id: record.notion_page_id.clone(),
        notion_url: record.notion_url.clone(),
        notion_sync_status: record.notion_sync_status.clone(),
        notion_error: record.notion_error.clone(),
        action: action.to_string(),
        conflict,
    }
}

fn sync_record_to_notion_internal(
    central_home: &Path,
    json_path: &str,
    database_id: &str,
    notion_api_key: &str,
    conflict_strategy: &str,
) -> Result<NotionSyncResult, String> {
    let path = absolute_path(Path::new(json_path.trim()));
    let md_path = path.with_extension("md");
    let mut record = load_record_by_json_path(central_home, &path.to_string_lossy())?;
    let client = notion_client()?;

    if let Some(page_id) = record
        .notion_page_id
        .as_ref()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
    {
        if let Ok(remote_meta) = notion_fetch_remote_record(&page_id, notion_api_key, &client, false) {
            let local_changed = local_has_changed_since_sync(&record);
            let notion_changed = remote_has_changed(&record, &remote_meta);
            if local_changed && notion_changed {
                match conflict_strategy {
                    "manual" => {
                        record.notion_sync_status = "CONFLICT".to_string();
                        record.notion_error = Some(
                            "Conflict detected: local and Notion both changed since last sync."
                                .to_string(),
                        );
                        persist_record_to_files(&record, &path, &md_path)?;
                        let _ = upsert_index_record_if_exists(central_home, &record);
                        return Ok(build_sync_result(
                            &path,
                            &record,
                            "conflict_manual",
                            true,
                        ));
                    }
                    "notion_wins" => {
                        let remote_full =
                            notion_fetch_remote_record(&page_id, notion_api_key, &client, true)?;
                        let mut next = apply_remote_to_local_record(&record, &remote_full);
                        mark_record_synced(&mut next, remote_full.last_edited_time.clone());
                        persist_record_to_files(&next, &path, &md_path)?;
                        let _ = upsert_index_record_if_exists(central_home, &next);
                        return Ok(build_sync_result(
                            &path,
                            &next,
                            "pulled_notion_conflict_notion_wins",
                            false,
                        ));
                    }
                    _ => {}
                }
            }
        }
    }

    push_local_record_to_notion(
        central_home,
        &client,
        &mut record,
        &path,
        &md_path,
        database_id,
        notion_api_key,
        "pushed_local",
    )
}

fn sync_record_bidirectional_internal(
    central_home: &Path,
    json_path: &str,
    database_id: &str,
    notion_api_key: &str,
    conflict_strategy: &str,
) -> Result<NotionSyncResult, String> {
    let path = absolute_path(Path::new(json_path.trim()));
    let md_path = path.with_extension("md");
    let mut record = load_record_by_json_path(central_home, &path.to_string_lossy())?;
    let client = notion_client()?;

    let page_id = record
        .notion_page_id
        .as_ref()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty());

    let remote = if let Some(id) = page_id.as_ref() {
        notion_fetch_remote_record(id, notion_api_key, &client, true).ok()
    } else {
        None
    };

    if let Some(remote_record) = remote {
        let local_changed = local_has_changed_since_sync(&record);
        let notion_changed = remote_has_changed(&record, &remote_record);

        if local_changed && notion_changed {
            match conflict_strategy {
                "manual" => {
                    record.notion_sync_status = "CONFLICT".to_string();
                    record.notion_error = Some(
                        "Conflict detected: local and Notion both changed since last sync."
                            .to_string(),
                    );
                    persist_record_to_files(&record, &path, &md_path)?;
                    let _ = upsert_index_record_if_exists(central_home, &record);
                    return Ok(build_sync_result(
                        &path,
                        &record,
                        "conflict_manual",
                        true,
                    ));
                }
                "notion_wins" => {
                    let mut next = apply_remote_to_local_record(&record, &remote_record);
                    mark_record_synced(&mut next, remote_record.last_edited_time.clone());
                    persist_record_to_files(&next, &path, &md_path)?;
                    let _ = upsert_index_record_if_exists(central_home, &next);
                    return Ok(build_sync_result(
                        &path,
                        &next,
                        "pulled_notion_conflict_notion_wins",
                        false,
                    ));
                }
                _ => {
                    return push_local_record_to_notion(
                        central_home,
                        &client,
                        &mut record,
                        &path,
                        &md_path,
                        database_id,
                        notion_api_key,
                        "pushed_local_conflict_local_wins",
                    )
                }
            }
        }

        if local_changed {
            return push_local_record_to_notion(
                central_home,
                &client,
                &mut record,
                &path,
                &md_path,
                database_id,
                notion_api_key,
                "pushed_local",
            );
        }

        if notion_changed {
            let mut next = apply_remote_to_local_record(&record, &remote_record);
            mark_record_synced(&mut next, remote_record.last_edited_time.clone());
            persist_record_to_files(&next, &path, &md_path)?;
            let _ = upsert_index_record_if_exists(central_home, &next);
            return Ok(build_sync_result(&path, &next, "pulled_notion", false));
        }

        mark_record_synced(&mut record, remote_record.last_edited_time.clone());
        persist_record_to_files(&record, &path, &md_path)?;
        let _ = upsert_index_record_if_exists(central_home, &record);
        return Ok(build_sync_result(&path, &record, "noop", false));
    }

    push_local_record_to_notion(
        central_home,
        &client,
        &mut record,
        &path,
        &md_path,
        database_id,
        notion_api_key,
        "pushed_local",
    )
}

fn pull_records_from_notion_internal(
    central_home: &Path,
    database_id: &str,
    notion_api_key: &str,
    conflict_strategy: &str,
) -> Result<NotionBatchSyncResult, String> {
    let client = notion_client()?;
    let pages = notion_query_database_pages(database_id, notion_api_key, &client)?;
    let locals = load_records(central_home)?;
    let mut by_page_id: HashMap<String, Record> = HashMap::new();
    for record in locals {
        if let Some(page_id) = record
            .notion_page_id
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            by_page_id.insert(page_id, record);
        }
    }

    let mut results = Vec::new();
    let mut success = 0usize;
    let mut failed = 0usize;
    let mut conflicts = 0usize;

    for page in pages {
        let remote = match notion_remote_record_from_page(&page, notion_api_key, &client, true) {
            Ok(item) => item,
            Err(error) => {
                failed += 1;
                results.push(NotionSyncResult {
                    json_path: String::new(),
                    notion_page_id: page
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    notion_url: page
                        .get("url")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    notion_sync_status: "FAILED".to_string(),
                    notion_error: Some(error),
                    action: "pull_failed".to_string(),
                    conflict: false,
                });
                continue;
            }
        };

        if let Some(existing) = by_page_id.get(&remote.page_id).cloned() {
            let (json_path, md_path) = resolve_record_paths(central_home, &existing)?;
            let local_changed = local_has_changed_since_sync(&existing);
            let notion_changed = remote_has_changed(&existing, &remote);

            let result = if local_changed && notion_changed {
                match conflict_strategy {
                    "manual" => {
                        let mut conflict_record = existing.clone();
                        conflict_record.notion_sync_status = "CONFLICT".to_string();
                        conflict_record.notion_error = Some(
                            "Conflict detected while pulling from Notion (manual strategy)."
                                .to_string(),
                        );
                        persist_record_to_files(&conflict_record, &json_path, &md_path)?;
                        let _ = upsert_index_record_if_exists(central_home, &conflict_record);
                        build_sync_result(&json_path, &conflict_record, "conflict_manual", true)
                    }
                    "local_wins" => {
                        let mut local_record = existing.clone();
                        push_local_record_to_notion(
                            central_home,
                            &client,
                            &mut local_record,
                            &json_path,
                            &md_path,
                            database_id,
                            notion_api_key,
                            "pushed_local_conflict_local_wins",
                        )?
                    }
                    _ => {
                        let mut next = apply_remote_to_local_record(&existing, &remote);
                        mark_record_synced(&mut next, remote.last_edited_time.clone());
                        persist_record_to_files(&next, &json_path, &md_path)?;
                        let _ = upsert_index_record_if_exists(central_home, &next);
                        build_sync_result(
                            &json_path,
                            &next,
                            "pulled_notion_conflict_notion_wins",
                            false,
                        )
                    }
                }
            } else if notion_changed {
                let mut next = apply_remote_to_local_record(&existing, &remote);
                mark_record_synced(&mut next, remote.last_edited_time.clone());
                persist_record_to_files(&next, &json_path, &md_path)?;
                let _ = upsert_index_record_if_exists(central_home, &next);
                build_sync_result(&json_path, &next, "pulled_notion", false)
            } else if local_changed {
                match conflict_strategy {
                    "local_wins" => {
                        let mut local_record = existing.clone();
                        push_local_record_to_notion(
                            central_home,
                            &client,
                            &mut local_record,
                            &json_path,
                            &md_path,
                            database_id,
                            notion_api_key,
                            "pushed_local_local_only_change",
                        )?
                    }
                    "notion_wins" => {
                        let mut next = apply_remote_to_local_record(&existing, &remote);
                        mark_record_synced(&mut next, remote.last_edited_time.clone());
                        persist_record_to_files(&next, &json_path, &md_path)?;
                        let _ = upsert_index_record_if_exists(central_home, &next);
                        build_sync_result(&json_path, &next, "pulled_notion_local_only_change", false)
                    }
                    _ => {
                        let mut pending = existing.clone();
                        pending.notion_sync_status = "PENDING".to_string();
                        pending.notion_error = Some(
                            "Local-only changes detected. Pull skipped by manual strategy."
                                .to_string(),
                        );
                        persist_record_to_files(&pending, &json_path, &md_path)?;
                        let _ = upsert_index_record_if_exists(central_home, &pending);
                        build_sync_result(&json_path, &pending, "local_only_pending", false)
                    }
                }
            } else {
                let mut stable = existing.clone();
                mark_record_synced(&mut stable, remote.last_edited_time.clone());
                persist_record_to_files(&stable, &json_path, &md_path)?;
                let _ = upsert_index_record_if_exists(central_home, &stable);
                build_sync_result(&json_path, &stable, "noop", false)
            };

            if result.conflict {
                conflicts += 1;
                failed += 1;
            } else if result.notion_sync_status == "SUCCESS" {
                success += 1;
            } else {
                failed += 1;
            }
            results.push(result);
        } else {
            let mut next = record_from_remote(&remote);
            mark_record_synced(&mut next, remote.last_edited_time.clone());
            let (json_path, md_path) =
                generate_unique_record_paths(central_home, &next.record_type, &next.title)?;
            persist_record_to_files(&next, &json_path, &md_path)?;
            let _ = upsert_index_record_if_exists(central_home, &next);
            success += 1;
            results.push(build_sync_result(
                &json_path,
                &next,
                "created_local_from_notion",
                false,
            ));
        }
    }

    Ok(NotionBatchSyncResult {
        total: results.len(),
        success,
        failed,
        conflicts,
        results,
    })
}

fn resolve_record_paths(central_home: &Path, record: &Record) -> Result<(PathBuf, PathBuf), String> {
    if let Some(path) = record
        .json_path
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        let json = absolute_path(Path::new(&path));
        return Ok((json.clone(), json.with_extension("md")));
    }
    generate_unique_record_paths(central_home, &record.record_type, &record.title)
}

fn generate_unique_record_paths(
    central_home: &Path,
    record_type: &str,
    title: &str,
) -> Result<(PathBuf, PathBuf), String> {
    let dir = central_home.join("records").join(record_dir_by_type(record_type));
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let base = generate_filename(record_type, title);
    let mut json_path = dir.join(format!("{base}.json"));
    let mut idx = 1usize;
    while json_path.exists() {
        json_path = dir.join(format!("{base}_{idx}.json"));
        idx += 1;
    }
    Ok((json_path.clone(), json_path.with_extension("md")))
}

fn record_from_remote(remote: &NotionRemoteRecord) -> Record {
    Record {
        record_type: normalize_record_type(&remote.record_type),
        title: if remote.title.trim().is_empty() {
            "Untitled".to_string()
        } else {
            remote.title.clone()
        },
        created_at: if remote.created_at.trim().is_empty() {
            Local::now().to_rfc3339()
        } else {
            remote.created_at.clone()
        },
        source_text: remote.source_text.clone(),
        final_body: remote.final_body.clone(),
        tags: remote.tags.clone(),
        date: remote.date.clone(),
        notion_page_id: Some(remote.page_id.clone()),
        notion_url: remote.page_url.clone(),
        notion_sync_status: "SUCCESS".to_string(),
        notion_error: None,
        notion_last_synced_at: None,
        notion_last_edited_time: remote.last_edited_time.clone(),
        notion_last_synced_hash: None,
        json_path: None,
        md_path: None,
    }
}

fn apply_remote_to_local_record(local: &Record, remote: &NotionRemoteRecord) -> Record {
    let mut next = local.clone();
    next.record_type = normalize_record_type(&remote.record_type);
    next.title = if remote.title.trim().is_empty() {
        local.title.clone()
    } else {
        remote.title.clone()
    };
    next.created_at = if remote.created_at.trim().is_empty() {
        local.created_at.clone()
    } else {
        remote.created_at.clone()
    };
    next.source_text = remote.source_text.clone();
    next.final_body = remote.final_body.clone();
    next.tags = remote.tags.clone();
    next.date = remote.date.clone();
    next.notion_page_id = Some(remote.page_id.clone());
    next.notion_url = remote.page_url.clone();
    next.notion_error = None;
    next
}

fn push_local_record_to_notion(
    central_home: &Path,
    client: &Client,
    record: &mut Record,
    json_path: &Path,
    md_path: &Path,
    database_id: &str,
    notion_api_key: &str,
    action: &str,
) -> Result<NotionSyncResult, String> {
    match notion_upsert_record(database_id, notion_api_key, record, client) {
        Ok(info) => {
            record.notion_page_id = Some(info.page_id);
            record.notion_url = info.page_url;
            mark_record_synced(record, info.last_edited_time);
            persist_record_to_files(record, json_path, md_path)?;
            let _ = upsert_index_record_if_exists(central_home, record);
            Ok(build_sync_result(json_path, record, action, false))
        }
        Err(error) => {
            record.notion_sync_status = "FAILED".to_string();
            record.notion_error = Some(error);
            let _ = persist_record_to_files(record, json_path, md_path);
            let _ = upsert_index_record_if_exists(central_home, record);
            Ok(build_sync_result(json_path, record, "push_failed", false))
        }
    }
}

fn notion_client() -> Result<Client, String> {
    Client::builder()
        .timeout(StdDuration::from_secs(50))
        .build()
        .map_err(|error| error.to_string())
}

fn notion_upsert_record(
    database_id: &str,
    notion_api_key: &str,
    record: &Record,
    client: &Client,
) -> Result<NotionUpsertInfo, String> {
    let database_value = notion_fetch_database(database_id, notion_api_key, client)?;

    let properties_schema = database_value
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| "Notion database properties not found".to_string())?;
    let title_property_name =
        notion_find_title_property_name(properties_schema).ok_or_else(|| {
            "Could not find title property in target Notion database".to_string()
        })?;

    let properties = notion_build_properties(properties_schema, &title_property_name, record);

    let patch_response = if let Some(page_id) = record
        .notion_page_id
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        Some(
            client
                .patch(format!("{NOTION_API_BASE_URL}/pages/{page_id}"))
                .header("Authorization", format!("Bearer {notion_api_key}"))
                .header("Notion-Version", NOTION_API_VERSION)
                .header("Content-Type", "application/json")
                .json(&json!({ "properties": properties }))
                .send()
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };

    let create_page = if let Some(response) = patch_response {
        let status = response.status();
        let body_text = response.text().map_err(|error| error.to_string())?;
        if status.is_success() {
            Some(serde_json::from_str::<Value>(&body_text).map_err(|error| error.to_string())?)
        } else {
            let code = notion_error_code_from_body(&body_text);
            if status.as_u16() == 404 || code.as_deref() == Some("object_not_found") {
                None
            } else {
                return Err(format!("Notion API {}: {}", status.as_u16(), body_text));
            }
        }
    } else {
        None
    };

    let value = if let Some(patched) = create_page {
        patched
    } else {
        let response = client
            .post(format!("{NOTION_API_BASE_URL}/pages"))
            .header("Authorization", format!("Bearer {notion_api_key}"))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .json(&json!({
                "parent": { "database_id": database_id },
                "properties": properties,
                "children": notion_build_children(record),
            }))
            .send()
            .map_err(|error| error.to_string())?;

        let status = response.status();
        let body_text = response.text().map_err(|error| error.to_string())?;
        if !status.is_success() {
            return Err(format!("Notion API {}: {}", status.as_u16(), body_text));
        }
        serde_json::from_str::<Value>(&body_text).map_err(|error| error.to_string())?
    };

    let page_id = value
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "Notion response missing page id".to_string())?;
    let page_url = value.get("url").and_then(Value::as_str).map(str::to_string);
    let last_edited_time = value
        .get("last_edited_time")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(NotionUpsertInfo {
        page_id,
        page_url,
        last_edited_time,
    })
}

fn notion_error_code_from_body(body_text: &str) -> Option<String> {
    serde_json::from_str::<Value>(body_text)
        .ok()
        .and_then(|value| value.get("code").and_then(Value::as_str).map(str::to_string))
}

fn notion_fetch_database(database_id: &str, notion_api_key: &str, client: &Client) -> Result<Value, String> {
    let response = client
        .get(format!("{NOTION_API_BASE_URL}/databases/{database_id}"))
        .header("Authorization", format!("Bearer {notion_api_key}"))
        .header("Notion-Version", NOTION_API_VERSION)
        .send()
        .map_err(|error| error.to_string())?;
    let status = response.status();
    let body = response.text().map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("Notion database API {}: {}", status.as_u16(), body));
    }
    serde_json::from_str(&body).map_err(|error| error.to_string())
}

fn notion_query_database_pages(
    database_id: &str,
    notion_api_key: &str,
    client: &Client,
) -> Result<Vec<Value>, String> {
    let mut pages: Vec<Value> = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let body = if let Some(next_cursor) = cursor.clone() {
            json!({ "page_size": 100, "start_cursor": next_cursor })
        } else {
            json!({ "page_size": 100 })
        };

        let response = client
            .post(format!("{NOTION_API_BASE_URL}/databases/{database_id}/query"))
            .header("Authorization", format!("Bearer {notion_api_key}"))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .map_err(|error| error.to_string())?;

        let status = response.status();
        let text = response.text().map_err(|error| error.to_string())?;
        if !status.is_success() {
            return Err(format!("Notion query API {}: {}", status.as_u16(), text));
        }

        let value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
        let batch = value
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        pages.extend(batch);

        let has_more = value
            .get("has_more")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if has_more {
            cursor = value
                .get("next_cursor")
                .and_then(Value::as_str)
                .map(str::to_string);
            if cursor.is_none() {
                break;
            }
        } else {
            break;
        }
    }

    Ok(pages)
}

fn notion_fetch_remote_record(
    page_id: &str,
    notion_api_key: &str,
    client: &Client,
    include_content: bool,
) -> Result<NotionRemoteRecord, String> {
    let response = client
        .get(format!("{NOTION_API_BASE_URL}/pages/{page_id}"))
        .header("Authorization", format!("Bearer {notion_api_key}"))
        .header("Notion-Version", NOTION_API_VERSION)
        .send()
        .map_err(|error| error.to_string())?;
    let status = response.status();
    let body = response.text().map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("Notion page API {}: {}", status.as_u16(), body));
    }
    let page: Value = serde_json::from_str(&body).map_err(|error| error.to_string())?;
    notion_remote_record_from_page(&page, notion_api_key, client, include_content)
}

fn notion_remote_record_from_page(
    page: &Value,
    notion_api_key: &str,
    client: &Client,
    include_content: bool,
) -> Result<NotionRemoteRecord, String> {
    let page_id = page
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "Notion page missing id".to_string())?;
    let page_url = page.get("url").and_then(Value::as_str).map(str::to_string);
    let last_edited_time = page
        .get("last_edited_time")
        .and_then(Value::as_str)
        .map(str::to_string);

    let properties = page
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| "Notion page missing properties".to_string())?;

    let title = notion_extract_title_from_properties(properties);
    let record_type = notion_extract_record_type_from_properties(properties);
    let tags = notion_extract_tags_from_properties(properties);
    let date = notion_extract_date_from_properties(properties);
    let created_at = notion_extract_created_at_from_properties(page, properties);

    let (final_body, source_text) = if include_content {
        notion_fetch_page_content(&page_id, notion_api_key, client)?
    } else {
        (String::new(), String::new())
    };

    Ok(NotionRemoteRecord {
        page_id,
        page_url,
        last_edited_time,
        record_type,
        title,
        created_at,
        date,
        tags,
        final_body,
        source_text,
    })
}

fn notion_extract_title_from_properties(properties: &serde_json::Map<String, Value>) -> String {
    for (_, property) in properties {
        let kind = property.get("type").and_then(Value::as_str).unwrap_or_default();
        if kind == "title" {
            let text = notion_plain_text_from_rich_text(
                property.get("title").unwrap_or(&Value::Null),
            );
            if !text.trim().is_empty() {
                return text;
            }
        }
    }
    "Untitled".to_string()
}

fn notion_extract_record_type_from_properties(properties: &serde_json::Map<String, Value>) -> String {
    if let Some(property) = notion_find_page_property_by_candidates(properties, &["Type", "Record Type"]) {
        let kind = property.get("type").and_then(Value::as_str).unwrap_or_default();
        let value = match kind {
            "select" => property
                .get("select")
                .and_then(|item| item.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            "rich_text" => notion_plain_text_from_rich_text(
                property.get("rich_text").unwrap_or(&Value::Null),
            ),
            "title" => notion_plain_text_from_rich_text(
                property.get("title").unwrap_or(&Value::Null),
            ),
            _ => String::new(),
        };
        if !value.trim().is_empty() {
            return normalize_record_type(&value);
        }
    }
    "note".to_string()
}

fn notion_extract_tags_from_properties(properties: &serde_json::Map<String, Value>) -> Vec<String> {
    if let Some(property) = notion_find_page_property_by_candidates(properties, &["Tags", "Tag"]) {
        let kind = property.get("type").and_then(Value::as_str).unwrap_or_default();
        match kind {
            "multi_select" => {
                return property
                    .get("multi_select")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.get("name").and_then(Value::as_str))
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            }
            "rich_text" => {
                let text = notion_plain_text_from_rich_text(
                    property.get("rich_text").unwrap_or(&Value::Null),
                );
                return text
                    .split(',')
                    .map(|item| item.trim().to_string())
                    .filter(|item| !item.is_empty())
                    .collect::<Vec<_>>();
            }
            "select" => {
                if let Some(value) = property
                    .get("select")
                    .and_then(|item| item.get("name"))
                    .and_then(Value::as_str)
                {
                    if !value.trim().is_empty() {
                        return vec![value.trim().to_string()];
                    }
                }
            }
            _ => {}
        }
    }
    Vec::new()
}

fn notion_extract_date_from_properties(properties: &serde_json::Map<String, Value>) -> Option<String> {
    let property = notion_find_page_property_by_candidates(properties, &["Date"])?;
    let kind = property.get("type").and_then(Value::as_str).unwrap_or_default();
    match kind {
        "date" => property
            .get("date")
            .and_then(|item| item.get("start"))
            .and_then(Value::as_str)
            .map(str::to_string),
        "rich_text" => {
            let text = notion_plain_text_from_rich_text(property.get("rich_text").unwrap_or(&Value::Null));
            if text.trim().is_empty() {
                None
            } else {
                Some(text.trim().to_string())
            }
        }
        _ => None,
    }
}

fn notion_extract_created_at_from_properties(
    page: &Value,
    properties: &serde_json::Map<String, Value>,
) -> String {
    if let Some(property) =
        notion_find_page_property_by_candidates(properties, &["Created At", "Created", "Timestamp"])
    {
        let kind = property.get("type").and_then(Value::as_str).unwrap_or_default();
        let from_property = match kind {
            "date" => property
                .get("date")
                .and_then(|item| item.get("start"))
                .and_then(Value::as_str)
                .map(str::to_string),
            "rich_text" => {
                let text = notion_plain_text_from_rich_text(
                    property.get("rich_text").unwrap_or(&Value::Null),
                );
                if text.trim().is_empty() {
                    None
                } else {
                    Some(text.trim().to_string())
                }
            }
            _ => None,
        };
        if let Some(value) = from_property {
            return value;
        }
    }

    page.get("created_time")
        .and_then(Value::as_str)
        .or_else(|| page.get("last_edited_time").and_then(Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| Local::now().to_rfc3339())
}

fn notion_find_page_property_by_candidates<'a>(
    properties: &'a serde_json::Map<String, Value>,
    candidates: &[&str],
) -> Option<&'a Value> {
    for candidate in candidates {
        for (name, property) in properties {
            if name.eq_ignore_ascii_case(candidate) {
                return Some(property);
            }
        }
    }
    None
}

fn notion_plain_text_from_rich_text(value: &Value) -> String {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    item.get("plain_text")
                        .and_then(Value::as_str)
                        .or_else(|| {
                            item.get("text")
                                .and_then(|text| text.get("content"))
                                .and_then(Value::as_str)
                        })
                        .unwrap_or_default()
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

fn notion_fetch_page_content(
    page_id: &str,
    notion_api_key: &str,
    client: &Client,
) -> Result<(String, String), String> {
    let mut blocks: Vec<Value> = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let mut request = client
            .get(format!("{NOTION_API_BASE_URL}/blocks/{page_id}/children"))
            .header("Authorization", format!("Bearer {notion_api_key}"))
            .header("Notion-Version", NOTION_API_VERSION)
            .query(&[("page_size", "100")]);
        if let Some(next_cursor) = cursor.as_ref() {
            request = request.query(&[("start_cursor", next_cursor)]);
        }

        let response = request.send().map_err(|error| error.to_string())?;
        let status = response.status();
        let text = response.text().map_err(|error| error.to_string())?;
        if !status.is_success() {
            return Err(format!(
                "Notion block children API {}: {}",
                status.as_u16(),
                text
            ));
        }

        let value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
        if let Some(items) = value.get("results").and_then(Value::as_array) {
            blocks.extend(items.iter().cloned());
        }

        let has_more = value
            .get("has_more")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if has_more {
            cursor = value
                .get("next_cursor")
                .and_then(Value::as_str)
                .map(str::to_string);
            if cursor.is_none() {
                break;
            }
        } else {
            break;
        }
    }

    Ok(notion_extract_content_sections(&blocks))
}

fn notion_extract_content_sections(blocks: &[Value]) -> (String, String) {
    let mut final_lines: Vec<String> = Vec::new();
    let mut source_lines: Vec<String> = Vec::new();
    let mut fallback_lines: Vec<String> = Vec::new();
    let mut section = "";

    for block in blocks {
        let block_type = block.get("type").and_then(Value::as_str).unwrap_or_default();
        let text = notion_extract_block_text(block, block_type);
        let clean = text.trim();
        if clean.is_empty() {
            continue;
        }

        if block_type.starts_with("heading_") {
            if clean.eq_ignore_ascii_case("Final Body") {
                section = "final";
                continue;
            }
            if clean.eq_ignore_ascii_case("Source Text") {
                section = "source";
                continue;
            }
        }

        match section {
            "final" => final_lines.push(clean.to_string()),
            "source" => source_lines.push(clean.to_string()),
            _ => fallback_lines.push(clean.to_string()),
        }
    }

    let final_body = if !final_lines.is_empty() {
        final_lines.join("\n\n")
    } else {
        fallback_lines.join("\n\n")
    };
    let source_text = source_lines.join("\n\n");
    (final_body, source_text)
}

fn notion_extract_block_text(block: &Value, block_type: &str) -> String {
    let section = block.get(block_type).unwrap_or(&Value::Null);
    if let Some(rich_text) = section.get("rich_text") {
        return notion_plain_text_from_rich_text(rich_text);
    }
    String::new()
}

fn notion_find_title_property_name(properties: &serde_json::Map<String, Value>) -> Option<String> {
    for (name, schema) in properties {
        if schema.get("type").and_then(Value::as_str) == Some("title") {
            return Some(name.to_string());
        }
    }
    None
}

fn notion_find_property_by_candidates(
    properties: &serde_json::Map<String, Value>,
    candidates: &[&str],
) -> Option<(String, String)> {
    for candidate in candidates {
        for (name, schema) in properties {
            if name.eq_ignore_ascii_case(candidate) {
                let prop_type = schema
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                return Some((name.to_string(), prop_type));
            }
        }
    }
    None
}

fn notion_build_properties(
    properties_schema: &serde_json::Map<String, Value>,
    title_property_name: &str,
    record: &Record,
) -> Value {
    let mut properties = serde_json::Map::<String, Value>::new();
    properties.insert(
        title_property_name.to_string(),
        json!({
            "title": [{
                "type": "text",
                "text": { "content": record.title.chars().take(1800).collect::<String>() }
            }]
        }),
    );

    if let Some((name, kind)) = notion_find_property_by_candidates(properties_schema, &["Type", "Record Type"]) {
        match kind.as_str() {
            "select" => {
                properties.insert(name, json!({ "select": { "name": record.record_type } }));
            }
            "rich_text" => {
                properties.insert(name, json!({ "rich_text": [{ "type": "text", "text": { "content": record.record_type } }] }));
            }
            _ => {}
        }
    }

    if let Some((name, kind)) = notion_find_property_by_candidates(properties_schema, &["Tags", "Tag"]) {
        match kind.as_str() {
            "multi_select" => {
                properties.insert(
                    name,
                    json!({
                        "multi_select": record
                            .tags
                            .iter()
                            .filter(|item| !item.trim().is_empty())
                            .map(|item| json!({ "name": item.trim() }))
                            .collect::<Vec<_>>()
                    }),
                );
            }
            "rich_text" => {
                properties.insert(
                    name,
                    json!({ "rich_text": [{ "type": "text", "text": { "content": record.tags.join(", ") } }] }),
                );
            }
            _ => {}
        }
    }

    if let Some((name, kind)) = notion_find_property_by_candidates(properties_schema, &["Date"]) {
        if kind == "date" {
            let start = record
                .date
                .clone()
                .or_else(|| extract_day(&record.created_at))
                .unwrap_or_else(|| Local::now().format("%Y-%m-%d").to_string());
            properties.insert(name, json!({ "date": { "start": start } }));
        }
    }

    if let Some((name, kind)) =
        notion_find_property_by_candidates(properties_schema, &["Created At", "Created", "Timestamp"])
    {
        match kind.as_str() {
            "date" => {
                properties.insert(name, json!({ "date": { "start": record.created_at } }));
            }
            "rich_text" => {
                properties.insert(
                    name,
                    json!({ "rich_text": [{ "type": "text", "text": { "content": record.created_at } }] }),
                );
            }
            _ => {}
        }
    }

    Value::Object(properties)
}

fn notion_build_children(record: &Record) -> Vec<Value> {
    let final_body = if record.final_body.trim().is_empty() {
        "(empty)".to_string()
    } else {
        record.final_body.clone()
    };
    let source_text = if record.source_text.trim().is_empty() {
        "(empty)".to_string()
    } else {
        record.source_text.clone()
    };

    vec![
        json!({
            "object": "block",
            "type": "heading_2",
            "heading_2": {
                "rich_text": [{ "type": "text", "text": { "content": "Final Body" } }]
            }
        }),
        json!({
            "object": "block",
            "type": "paragraph",
            "paragraph": {
                "rich_text": [{ "type": "text", "text": { "content": final_body.chars().take(1800).collect::<String>() } }]
            }
        }),
        json!({
            "object": "block",
            "type": "heading_2",
            "heading_2": {
                "rich_text": [{ "type": "text", "text": { "content": "Source Text" } }]
            }
        }),
        json!({
            "object": "block",
            "type": "paragraph",
            "paragraph": {
                "rich_text": [{ "type": "text", "text": { "content": source_text.chars().take(1800).collect::<String>() } }]
            }
        }),
    ]
}

fn parse_notebook_summary(value: &Value) -> NotebookSummary {
    let id = value
        .get("id")
        .or_else(|| value.get("notebook_id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let name = value
        .get("name")
        .or_else(|| value.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("Untitled Notebook")
        .to_string();
    let source_count = value
        .get("source_count")
        .and_then(Value::as_u64)
        .map(|item| item as usize);
    let updated_at = value
        .get("updated_at")
        .and_then(Value::as_str)
        .map(str::to_string);

    NotebookSummary {
        id,
        name,
        source_count,
        updated_at,
    }
}

fn render_record_source_text(record: &Record) -> String {
    let mut lines = vec![
        format!("# {}", record.title),
        String::new(),
        format!("- Type: {}", record.record_type),
        format!("- Created At: {}", record.created_at),
        format!("- Date: {}", record.date.clone().unwrap_or_default()),
        format!("- Tags: {}", record.tags.join(", ")),
        String::new(),
        "## Final Body".to_string(),
        record.final_body.clone(),
        String::new(),
        "## Source Text".to_string(),
        record.source_text.clone(),
    ];
    lines.retain(|line| !(line.starts_with("- Date: ") && line == "- Date: "));
    lines.join("\n")
}

fn resolve_notebooklm_runtime(config: Option<NotebookLmConfig>) -> (String, Vec<String>) {
    let settings = load_settings();
    let default_command = settings.integrations.notebooklm.command.trim().to_string();
    let default_args = settings.integrations.notebooklm.args.clone();

    let command = config
        .as_ref()
        .and_then(|item| item.command.as_ref())
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .or_else(|| {
            if default_command.is_empty() {
                None
            } else {
                Some(default_command)
            }
        })
        .unwrap_or_else(default_notebooklm_command);

    let args = config
        .and_then(|item| item.args)
        .filter(|items| !items.is_empty())
        .unwrap_or_else(|| {
            if default_args.is_empty() {
                default_notebooklm_args()
            } else {
                default_args
            }
        });

    (command, args)
}

fn notebooklm_call_tool(
    tool_name: &str,
    arguments: Value,
    config: Option<NotebookLmConfig>,
) -> Result<Value, String> {
    let (command, args) = resolve_notebooklm_runtime(config);

    let mut child = Command::new(&command)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Failed to start NotebookLM MCP command `{command}`: {error}"))?;

    let result = (|| -> Result<Value, String> {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "NotebookLM MCP stdin unavailable".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "NotebookLM MCP stdout unavailable".to_string())?;

        let (tx, rx) = mpsc::channel::<Value>();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.starts_with('{') {
                            if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
                                let _ = tx.send(value);
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        write_jsonrpc_line(
            &mut stdin,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "clientInfo": { "name": "kofnote-app", "version": "0.1.0" },
                    "capabilities": {}
                }
            }),
        )?;
        wait_jsonrpc_result(&rx, 1, StdDuration::from_secs(25))?;

        write_jsonrpc_line(
            &mut stdin,
            &json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
                "params": {}
            }),
        )?;

        write_jsonrpc_line(
            &mut stdin,
            &json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": { "name": tool_name, "arguments": arguments }
            }),
        )?;

        let call_response = wait_jsonrpc_result(&rx, 2, StdDuration::from_secs(90))?;
        parse_mcp_tool_payload(&call_response)
    })();

    let _ = child.kill();
    let _ = child.wait();
    result
}

fn write_jsonrpc_line(stdin: &mut std::process::ChildStdin, value: &Value) -> Result<(), String> {
    let text = serde_json::to_string(value).map_err(|error| error.to_string())?;
    stdin
        .write_all(format!("{text}\n").as_bytes())
        .map_err(|error| error.to_string())
}

fn wait_jsonrpc_result(
    rx: &mpsc::Receiver<Value>,
    expected_id: u64,
    timeout: StdDuration,
) -> Result<Value, String> {
    let deadline = Instant::now() + timeout;
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err("NotebookLM MCP response timeout".to_string());
        }
        let wait_for = deadline.saturating_duration_since(now);
        match rx.recv_timeout(wait_for) {
            Ok(message) => {
                let id = message.get("id").and_then(Value::as_u64).unwrap_or(0);
                if id != expected_id {
                    continue;
                }

                if let Some(error) = message.get("error") {
                    return Err(format!("NotebookLM MCP error: {error}"));
                }
                return Ok(message);
            }
            Err(_) => return Err("NotebookLM MCP response timeout".to_string()),
        }
    }
}

fn parse_mcp_tool_payload(response: &Value) -> Result<Value, String> {
    let result = response
        .get("result")
        .ok_or_else(|| "NotebookLM MCP missing result".to_string())?;

    if result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(format!("NotebookLM MCP tool error: {result}"));
    }

    let text = result
        .get("content")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find_map(|item| {
                let item_type = item.get("type").and_then(Value::as_str).unwrap_or_default();
                if item_type == "text" {
                    item.get("text").and_then(Value::as_str).map(str::to_string)
                } else {
                    None
                }
            })
        })
        .unwrap_or_default();

    if text.trim().is_empty() {
        return Ok(json!({}));
    }

    let parsed = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({ "text": text }));
    if let Some(error) = parsed.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_else(|| text.as_str());
        return Err(format!("NotebookLM MCP tool error: {message}"));
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_test_home(case: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kofnote_debate_{}_{}_{}",
            case,
            std::process::id(),
            Local::now().timestamp_micros()
        ));
        fs::create_dir_all(dir.join(".agentic")).expect("create .agentic");
        fs::write(dir.join(".agentic").join("CENTRAL_LOG_MARKER"), b"ok")
            .expect("marker write");
        dir
    }

    fn cleanup_test_home(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    fn sample_request() -> DebateModeRequest {
        DebateModeRequest {
            problem: "Choose implementation strategy for local-first debate mode".to_string(),
            constraints: vec![
                "Local-first persistence is mandatory".to_string(),
                "Output must be replayable".to_string(),
            ],
            output_type: "decision".to_string(),
            participants: Vec::new(),
            max_turn_seconds: Some(10),
            max_turn_tokens: Some(512),
            writeback_record_type: Some("decision".to_string()),
        }
    }

    fn sample_packet() -> DebateFinalPacket {
        DebateFinalPacket {
            run_id: "debate_20260210_120000_123".to_string(),
            mode: "debate-v0.1".to_string(),
            problem: "Test".to_string(),
            constraints: vec!["A".to_string()],
            output_type: "decision".to_string(),
            participants: DebateRole::all()
                .iter()
                .map(|role| DebatePacketParticipant {
                    role: role.as_str().to_string(),
                    model_provider: "local".to_string(),
                    model_name: "local-heuristic-v1".to_string(),
                })
                .collect(),
            consensus: DebatePacketConsensus {
                consensus_score: 0.8,
                confidence_score: 0.75,
                key_agreements: vec!["g1".to_string()],
                key_disagreements: vec!["d1".to_string()],
            },
            decision: DebateDecision {
                selected_option: "option".to_string(),
                why_selected: vec!["why".to_string()],
                rejected_options: vec![],
            },
            risks: vec![DebateRisk {
                risk: "risk".to_string(),
                severity: "low".to_string(),
                mitigation: "mitigate".to_string(),
            }],
            next_actions: vec![DebateAction {
                id: "A1".to_string(),
                action: "act".to_string(),
                owner: "me".to_string(),
                due: "2026-02-10".to_string(),
            }],
            trace: DebateTrace {
                round_refs: vec!["round-1".to_string(), "round-2".to_string(), "round-3".to_string()],
                evidence_refs: vec!["/tmp/a.json".to_string()],
            },
            timestamps: DebatePacketTimestamps {
                started_at: "2026-02-10T10:00:00Z".to_string(),
                finished_at: "2026-02-10T10:00:10Z".to_string(),
            },
        }
    }

    fn collect_key_paths(value: &Value, prefix: &str, out: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                let mut keys = map.keys().cloned().collect::<Vec<_>>();
                keys.sort();
                for key in keys {
                    let path = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{prefix}.{key}")
                    };
                    out.push(path.clone());
                    if let Some(next) = map.get(&key) {
                        collect_key_paths(next, &path, out);
                    }
                }
            }
            Value::Array(items) => {
                if let Some(first) = items.first() {
                    let path = if prefix.is_empty() {
                        "[0]".to_string()
                    } else {
                        format!("{prefix}[0]")
                    };
                    out.push(path.clone());
                    collect_key_paths(first, &path, out);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn debate_transition_guard_matrix() {
        assert!(validate_debate_transition(None, DebateState::Intake));
        assert!(validate_debate_transition(
            Some(DebateState::Intake),
            DebateState::Round1
        ));
        assert!(validate_debate_transition(
            Some(DebateState::Round1),
            DebateState::Round2
        ));
        assert!(!validate_debate_transition(
            Some(DebateState::Round1),
            DebateState::Consensus
        ));
        assert!(!validate_debate_transition(
            Some(DebateState::Writeback),
            DebateState::Round1
        ));
    }

    #[test]
    fn final_packet_validation_bounds() {
        let mut packet = sample_packet();
        assert!(validate_final_packet(&packet).is_ok());

        packet.consensus.consensus_score = 1.4;
        assert!(validate_final_packet(&packet).is_err());
    }

    #[test]
    fn claim_extraction_reads_claim_block_without_prefix() {
        let text = "Claim: adopt isolated runner\nwith append-only events\nRationale: because replay safety";
        let claim = extract_claim_text(text).expect("claim should be parsed");
        assert_eq!(claim, "adopt isolated runner with append-only events");
    }

    #[test]
    fn packet_markdown_contains_conclusion_tldr() {
        let mut packet = sample_packet();
        packet.decision.selected_option = "Use a clear and concise decision statement for operators".to_string();

        let markdown = render_debate_packet_markdown(&packet);
        assert!(markdown.contains("## Conclusion"));
        assert!(markdown.contains("- TL;DR:"));
        assert!(markdown.contains("- Selected: Use a clear and concise decision statement for operators"));
    }

    #[test]
    fn debate_happy_path_local_persists_artifacts() {
        let home = make_test_home("happy");
        let result = run_debate_mode_internal(&home, sample_request()).expect("run debate");

        assert_eq!(result.mode, "debate-v0.1");
        assert_eq!(result.state, "Writeback");
        assert!(!result.run_id.is_empty());
        assert!(!result.final_packet.next_actions.is_empty());
        assert!(result.writeback_json_path.is_some());

        let root = home.join("records").join("debates").join(&result.run_id);
        assert!(root.join("request.json").exists());
        assert!(root.join("rounds").join("round-1.json").exists());
        assert!(root.join("rounds").join("round-2.json").exists());
        assert!(root.join("rounds").join("round-3.json").exists());
        assert!(root.join("consensus.json").exists());
        assert!(root.join("final-packet.json").exists());
        assert!(root.join("final-packet.md").exists());

        cleanup_test_home(&home);
    }

    #[test]
    fn debate_degraded_when_provider_fails() {
        let home = make_test_home("degraded");
        let mut request = sample_request();
        request.participants = vec![DebateParticipantConfig {
            role: Some("Analyst".to_string()),
            model_provider: Some("gemini".to_string()),
            model_name: Some("gemini-2.0-flash".to_string()),
        }];

        let result = run_debate_mode_internal(&home, request).expect("run debate");
        assert!(result.degraded);
        assert!(result
            .error_codes
            .iter()
            .any(|code| code.contains("DEBATE_ERR_PROVIDER_GEMINI")));

        cleanup_test_home(&home);
    }

    #[test]
    fn debate_replay_works_without_cloud() {
        let home = make_test_home("replay");
        let result = run_debate_mode_internal(&home, sample_request()).expect("run debate");
        let replay =
            replay_debate_mode_internal(&home, &result.run_id).expect("replay should load from local artifacts");

        assert_eq!(replay.run_id, result.run_id);
        assert!(replay.consistency.files_complete);
        assert_eq!(replay.rounds.len(), 3);

        cleanup_test_home(&home);
    }

    #[test]
    fn debate_provider_combinations_keep_packet_shape() {
        let home = make_test_home("shape");

        let local = run_debate_mode_internal(&home, sample_request()).expect("local run");

        let mut mixed_request = sample_request();
        mixed_request.participants = vec![
            DebateParticipantConfig {
                role: Some("Proponent".to_string()),
                model_provider: Some("openai".to_string()),
                model_name: Some("gpt-4.1-mini".to_string()),
            },
            DebateParticipantConfig {
                role: Some("Critic".to_string()),
                model_provider: Some("local".to_string()),
                model_name: Some("local-heuristic-v1".to_string()),
            },
            DebateParticipantConfig {
                role: Some("Analyst".to_string()),
                model_provider: Some("claude".to_string()),
                model_name: Some("claude-3-5-sonnet-latest".to_string()),
            },
        ];
        let mixed = run_debate_mode_internal(&home, mixed_request).expect("mixed run");

        let local_json = serde_json::to_value(local.final_packet).expect("local packet json");
        let mixed_json = serde_json::to_value(mixed.final_packet).expect("mixed packet json");

        let mut local_keys = Vec::new();
        let mut mixed_keys = Vec::new();
        collect_key_paths(&local_json, "", &mut local_keys);
        collect_key_paths(&mixed_json, "", &mut mixed_keys);
        local_keys.sort();
        mixed_keys.sort();

        assert_eq!(local_keys, mixed_keys);
        cleanup_test_home(&home);
    }

    #[test]
    fn provider_registry_defaults_are_available() {
        let settings = normalize_settings(AppSettings::default());
        let registry = DebateProviderRegistry::from_settings(&settings);

        assert!(registry.is_enabled("codex-cli"));
        assert!(registry.is_enabled("gemini-cli"));
        assert!(registry.is_enabled("claude-web"));
    }

    #[test]
    fn disabled_provider_falls_back_to_local() {
        let mut settings = AppSettings::default();
        for provider in &mut settings.provider_registry.providers {
            if provider.id == "codex-cli" {
                provider.enabled = false;
            }
        }
        let registry = DebateProviderRegistry::from_settings(&normalize_settings(settings));

        let mut request = sample_request();
        request.participants = vec![DebateParticipantConfig {
            role: Some("Proponent".to_string()),
            model_provider: Some("codex-cli".to_string()),
            model_name: Some("codex".to_string()),
        }];

        let normalized = normalize_debate_request(request, &registry).expect("normalize request");
        let proponent = normalized
            .participants
            .iter()
            .find(|item| item.role == DebateRole::Proponent)
            .expect("proponent exists");

        assert_eq!(proponent.model_provider, "local");
        assert!(normalized
            .warning_codes
            .iter()
            .any(|code| code == "DEBATE_WARN_PROVIDER_DISABLED_FALLBACK_LOCAL"));
    }

    #[test]
    fn provider_stub_matrix_matches_expectation() {
        assert!(!provider_uses_local_stub("codex-cli", "cli"));
        assert!(!provider_uses_local_stub("gemini-cli", "cli"));
        assert!(!provider_uses_local_stub("claude-cli", "cli"));
        assert!(provider_uses_local_stub("chatgpt-web", "web"));
    }

    #[test]
    fn cli_model_aliases_map_to_provider_defaults() {
        assert_eq!(crate::providers::cli::normalize_cli_model_arg("codex-cli", "codex"), None);
        assert_eq!(crate::providers::cli::normalize_cli_model_arg("gemini-cli", "gemini"), None);
        assert_eq!(crate::providers::cli::normalize_cli_model_arg("claude-cli", "claude"), None);
        assert_eq!(crate::providers::cli::normalize_cli_model_arg("codex-cli", "auto"), None);
        assert_eq!(
            crate::providers::cli::normalize_cli_model_arg("codex-cli", "gpt-5-codex"),
            Some("gpt-5-codex".to_string())
        );
    }

    #[test]
    fn cli_model_error_detector_matches_common_provider_failures() {
        assert!(crate::providers::cli::is_cli_model_error("", "inaccessible model: gpt-5.3-codex"));
        assert!(crate::providers::cli::is_cli_model_error("", "this is not a supported model for codex"));
        assert!(crate::providers::cli::is_cli_model_error(
            "",
            "The model `gpt-5.3-codex` does not exist or you do not have access to it."
        ));
        assert!(crate::providers::cli::is_cli_model_error("", "invalid model"));
        assert!(!crate::providers::cli::is_cli_model_error("network error", ""));
    }

    #[test]
    fn cli_provider_build_args_match_expected_shape() {
        let codex_args =
            crate::providers::cli::build_codex_cli_args(Some("gpt-5-codex"), "prompt", 30, 1024);
        assert!(codex_args.args.iter().any(|arg| arg == "exec"));
        assert!(codex_args.args.windows(2).any(|pair| pair == ["--model", "gpt-5-codex"]));
        assert_eq!(codex_args.stdin_payload.as_deref(), Some(""));
        assert!(codex_args.output_file.is_some());

        let gemini_args =
            crate::providers::cli::build_gemini_cli_args(Some("gemini-2.0-flash"), "hello", 30, 1024);
        assert_eq!(gemini_args.args.first().map(String::as_str), Some("hello"));
        assert!(gemini_args.args.windows(2).any(|pair| pair == ["--model", "gemini-2.0-flash"]));
        assert!(gemini_args.stdin_payload.is_none());
        assert!(gemini_args.output_file.is_none());

        let claude_args = crate::providers::cli::build_claude_cli_args(
            Some("claude-3-5-sonnet-latest"),
            "question",
            30,
            1024,
        );
        assert!(claude_args.args.windows(2).any(|pair| pair == ["--model", "claude-3-5-sonnet-latest"]));
        assert_eq!(claude_args.args.last().map(String::as_str), Some("question"));
        assert!(claude_args.stdin_payload.is_none());
        assert!(claude_args.output_file.is_none());
    }
}

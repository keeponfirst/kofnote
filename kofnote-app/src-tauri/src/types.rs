use crate::storage::index::*;
use crate::storage::records::*;
use crate::storage::settings_io::*;
use crate::util::*;
use chrono::{Duration as ChronoDuration, Local, NaiveDate};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Mutex};
use std::thread;
use tauri::Emitter;

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
    pub(crate) record_type: String,
    pub(crate) title: String,
    pub(crate) created_at: Option<String>,
    pub(crate) source_text: Option<String>,
    pub(crate) final_body: Option<String>,
    pub(crate) tags: Option<Vec<String>>,
    pub(crate) date: Option<String>,
    pub(crate) notion_page_id: Option<String>,
    pub(crate) notion_url: Option<String>,
    pub(crate) notion_sync_status: Option<String>,
    pub(crate) notion_error: Option<String>,
    pub(crate) notion_last_synced_at: Option<String>,
    pub(crate) notion_last_edited_time: Option<String>,
    pub(crate) notion_last_synced_hash: Option<String>,
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
    snippets: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebuildIndexResult {
    indexed_count: usize,
    index_path: String,
    took_ms: u128,
}

// --- Second Brain P0: Unified Memory ---

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedMemoryItem {
    pub id: String,
    pub source: String,
    pub source_type: String,
    pub title: String,
    pub snippet: String,
    pub body: String,
    pub created_at: String,
    pub tags: Vec<String>,
    pub relevance_score: Option<f64>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedSearchResult {
    pub items: Vec<UnifiedMemoryItem>,
    pub total: usize,
    pub took_ms: u128,
    pub source_counts: HashMap<String, usize>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TimelineGroup {
    pub label: String,
    pub date: String,
    pub items: Vec<UnifiedMemoryItem>,
    pub count: usize,
    pub source_counts: HashMap<String, usize>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TimelineResponse {
    pub groups: Vec<TimelineGroup>,
    pub total_groups: usize,
    pub total_items: usize,
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
    pub(crate) json_path: String,
    pub(crate) notion_page_id: Option<String>,
    pub(crate) notion_url: Option<String>,
    pub(crate) notion_sync_status: String,
    pub(crate) notion_error: Option<String>,
    pub(crate) action: String,
    pub(crate) conflict: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotionBatchSyncResult {
    pub(crate) total: usize,
    pub(crate) success: usize,
    pub(crate) failed: usize,
    pub(crate) conflicts: usize,
    pub(crate) results: Vec<NotionSyncResult>,
}

#[derive(Debug, Clone)]
pub struct NotionRemoteRecord {
    pub(crate) page_id: String,
    pub(crate) page_url: Option<String>,
    pub(crate) last_edited_time: Option<String>,
    pub(crate) record_type: String,
    pub(crate) title: String,
    pub(crate) created_at: String,
    pub(crate) date: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) final_body: String,
    pub(crate) source_text: String,
}

#[derive(Debug)]
pub struct NotionUpsertInfo {
    pub(crate) page_id: String,
    pub(crate) page_url: Option<String>,
    pub(crate) last_edited_time: Option<String>,
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
    pub(crate) run_id: String,
    pub(crate) mode: String,
    pub(crate) state: String,
    pub(crate) degraded: bool,
    pub(crate) final_packet: DebateFinalPacket,
    pub(crate) artifacts_root: String,
    pub(crate) writeback_json_path: Option<String>,
    pub(crate) error_codes: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebateReplayConsistency {
    pub(crate) files_complete: bool,
    pub(crate) sql_indexed: bool,
    pub(crate) issues: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebateReplayResponse {
    pub(crate) run_id: String,
    pub(crate) request: Value,
    pub(crate) rounds: Vec<Value>,
    pub(crate) consensus: Value,
    pub(crate) final_packet: DebateFinalPacket,
    pub(crate) writeback_record: Option<Record>,
    pub(crate) consistency: DebateReplayConsistency,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebateRunSummary {
    pub(crate) run_id: String,
    pub(crate) problem: String,
    pub(crate) provider: String,
    pub(crate) output_type: String,
    pub(crate) degraded: bool,
    pub(crate) created_at: String,
    pub(crate) artifacts_root: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateProgress {
    pub(crate) run_id: String,
    pub(crate) round: String,
    pub(crate) role: String,
    pub(crate) turn_index: usize,
    pub(crate) total_turns: usize,
    pub(crate) status: String,
}

#[derive(Debug, Clone)]
pub struct DebateRuntimeParticipant {
    pub(crate) role: DebateRole,
    pub(crate) model_provider: String,
    pub(crate) provider_type: String,
    pub(crate) model_name: String,
}

#[derive(Debug, Clone)]
pub struct DebateNormalizedRequest {
    pub(crate) problem: String,
    pub(crate) constraints: Vec<String>,
    pub(crate) output_type: String,
    pub(crate) participants: Vec<DebateRuntimeParticipant>,
    pub(crate) max_turn_seconds: u64,
    pub(crate) max_turn_tokens: u32,
    pub(crate) writeback_record_type: Option<String>,
    pub(crate) warning_codes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DebateProviderRegistry {
    providers: HashMap<String, DebateProviderConfig>,
}

impl DebateProviderRegistry {
    pub(crate) fn from_settings(settings: &AppSettings) -> Self {
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

    pub(crate) fn get(&self, provider_id: &str) -> Option<&DebateProviderConfig> {
        self.providers.get(&provider_id.trim().to_lowercase())
    }

    pub(crate) fn is_enabled(&self, provider_id: &str) -> bool {
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
    pub(crate) fn all() -> [Self; 5] {
        [
            Self::Proponent,
            Self::Critic,
            Self::Analyst,
            Self::Synthesizer,
            Self::Judge,
        ]
    }

    pub(crate) fn as_str(&self) -> &'static str {
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
    pub(crate) fn all() -> [Self; 3] {
        [Self::Round1, Self::Round2, Self::Round3]
    }

    pub(crate) fn as_str(&self) -> &'static str {
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
    pub(crate) fn as_str(&self) -> &'static str {
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
    pub(crate) source_role: String,
    pub(crate) target_role: String,
    pub(crate) question: String,
    pub(crate) response: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateTurn {
    pub(crate) role: String,
    pub(crate) round: String,
    pub(crate) model_provider: String,
    pub(crate) model_name: String,
    pub(crate) status: String,
    pub(crate) claim: String,
    pub(crate) rationale: String,
    pub(crate) risks: Vec<String>,
    pub(crate) challenges: Vec<DebateChallenge>,
    pub(crate) revisions: Vec<String>,
    pub(crate) target_role: Option<String>,
    pub(crate) duration_ms: u128,
    pub(crate) error_code: Option<String>,
    pub(crate) error_message: Option<String>,
    pub(crate) started_at: String,
    pub(crate) finished_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateRoundArtifact {
    pub(crate) round: String,
    pub(crate) turns: Vec<DebateTurn>,
    pub(crate) started_at: String,
    pub(crate) finished_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebatePacketParticipant {
    pub(crate) role: String,
    pub(crate) model_provider: String,
    pub(crate) model_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebatePacketConsensus {
    pub(crate) consensus_score: f64,
    pub(crate) confidence_score: f64,
    pub(crate) key_agreements: Vec<String>,
    pub(crate) key_disagreements: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateRejectedOption {
    pub(crate) option: String,
    pub(crate) reason: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateDecision {
    pub(crate) selected_option: String,
    pub(crate) why_selected: Vec<String>,
    pub(crate) rejected_options: Vec<DebateRejectedOption>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateRisk {
    pub(crate) risk: String,
    pub(crate) severity: String,
    pub(crate) mitigation: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateAction {
    pub(crate) id: String,
    pub(crate) action: String,
    pub(crate) owner: String,
    pub(crate) due: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateTrace {
    pub(crate) round_refs: Vec<String>,
    pub(crate) evidence_refs: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebatePacketTimestamps {
    pub(crate) started_at: String,
    pub(crate) finished_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DebateFinalPacket {
    pub(crate) run_id: String,
    pub(crate) mode: String,
    pub(crate) problem: String,
    pub(crate) constraints: Vec<String>,
    pub(crate) output_type: String,
    pub(crate) participants: Vec<DebatePacketParticipant>,
    pub(crate) consensus: DebatePacketConsensus,
    pub(crate) decision: DebateDecision,
    pub(crate) risks: Vec<DebateRisk>,
    pub(crate) next_actions: Vec<DebateAction>,
    pub(crate) trace: DebateTrace,
    pub(crate) timestamps: DebatePacketTimestamps,
}


pub struct DebateLock(pub Mutex<Option<String>>);

pub(crate) fn get_app_settings() -> Result<AppSettings, String> {
    Ok(load_settings())
}

pub(crate) fn save_app_settings(settings: AppSettings) -> Result<AppSettings, String> {
    let normalized = normalize_settings(settings);
    save_settings(&normalized)?;
    Ok(normalized)
}


fn normalize_provider_type(value: &str) -> String {
    if value.trim().eq_ignore_ascii_case("web") {
        "web".to_string()
    } else {
        "cli".to_string()
    }
}

fn dedup_non_empty_strings(items: Vec<String>) -> Vec<String> {
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

fn normalize_provider_capabilities(input: &[String]) -> Vec<String> {
    let normalized = input
        .iter()
        .map(|item| item.trim().to_lowercase())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    let deduped = dedup_non_empty_strings(normalized);
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


// ──────────────────────────────────────────────────────────────────────────────
// Prompt Service
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PromptProfile {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) display_name: String,
    pub(crate) role: String,
    pub(crate) company: String,
    pub(crate) department: String,
    pub(crate) bio: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TemplateVariable {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) placeholder: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PromptTemplate {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) content: String,
    pub(crate) variables: Vec<TemplateVariable>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PromptRunRequest {
    pub(crate) profile_id: String,
    pub(crate) template_id: String,
    pub(crate) variable_values: HashMap<String, String>,
    pub(crate) provider: Option<String>,
    pub(crate) model: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptRunResponse {
    pub(crate) result: String,
    pub(crate) resolved_prompt: String,
    pub(crate) provider: String,
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
        assert!(crate::commands::debate::validate_debate_transition(None, DebateState::Intake));
        assert!(crate::commands::debate::validate_debate_transition(
            Some(DebateState::Intake),
            DebateState::Round1
        ));
        assert!(crate::commands::debate::validate_debate_transition(
            Some(DebateState::Round1),
            DebateState::Round2
        ));
        assert!(!crate::commands::debate::validate_debate_transition(
            Some(DebateState::Round1),
            DebateState::Consensus
        ));
        assert!(!crate::commands::debate::validate_debate_transition(
            Some(DebateState::Writeback),
            DebateState::Round1
        ));
    }

    #[test]
    fn final_packet_validation_bounds() {
        let mut packet = sample_packet();
        assert!(crate::commands::debate::validate_final_packet(&packet).is_ok());

        packet.consensus.consensus_score = 1.4;
        assert!(crate::commands::debate::validate_final_packet(&packet).is_err());
    }

    #[test]
    fn claim_extraction_reads_claim_block_without_prefix() {
        let text = "Claim: adopt isolated runner\nwith append-only events\nRationale: because replay safety";
        let claim = crate::commands::debate::extract_claim_text(text).expect("claim should be parsed");
        assert_eq!(claim, "adopt isolated runner with append-only events");
    }

    #[test]
    fn packet_markdown_contains_conclusion_tldr() {
        let mut packet = sample_packet();
        packet.decision.selected_option = "Use a clear and concise decision statement for operators".to_string();

        let markdown = crate::commands::debate::render_debate_packet_markdown(&packet);
        assert!(markdown.contains("## Conclusion"));
        assert!(markdown.contains("- TL;DR:"));
        assert!(markdown.contains("- Selected: Use a clear and concise decision statement for operators"));
    }

    #[test]
    fn debate_happy_path_local_persists_artifacts() {
        let home = make_test_home("happy");
        let result = crate::commands::debate::run_debate_mode_internal(&home, sample_request()).expect("run debate");

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

        let result = crate::commands::debate::run_debate_mode_internal(&home, request).expect("run debate");
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
        let result = crate::commands::debate::run_debate_mode_internal(&home, sample_request()).expect("run debate");
        let replay =
            crate::commands::debate::replay_debate_mode_internal(&home, &result.run_id).expect("replay should load from local artifacts");

        assert_eq!(replay.run_id, result.run_id);
        assert!(replay.consistency.files_complete);
        assert_eq!(replay.rounds.len(), 3);

        cleanup_test_home(&home);
    }

    #[test]
    fn debate_provider_combinations_keep_packet_shape() {
        let home = make_test_home("shape");

        let local = crate::commands::debate::run_debate_mode_internal(&home, sample_request()).expect("local run");

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
        let mixed = crate::commands::debate::run_debate_mode_internal(&home, mixed_request).expect("mixed run");

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

        let normalized = crate::commands::debate::normalize_debate_request(request, &registry).expect("normalize request");
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
        assert!(!crate::commands::debate::provider_uses_local_stub("codex-cli", "cli"));
        assert!(!crate::commands::debate::provider_uses_local_stub("gemini-cli", "cli"));
        assert!(!crate::commands::debate::provider_uses_local_stub("claude-cli", "cli"));
        assert!(crate::commands::debate::provider_uses_local_stub("chatgpt-web", "web"));
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

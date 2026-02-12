# KOF Note 重構計畫 — Codex 執行 Prompt

## 重要前提

上一輪重構把 main.rs 整坨搬到 types.rs（7,344 行），建了一堆 1 行註解的空殼檔案，前端只是 rename AppLegacy.tsx。**這不是重構，這是搬檔案。**

本輪要求：**真正把函式移到對應的模組檔案裡**。每個模組檔案都必須包含真正的 `fn` 實作，不能只寫 `// lives in crate::types`。

---

## 硬性約束（違反任何一條即視為失敗）

1. **`types.rs` 只能包含 `struct`、`enum`、`const`、`impl Default`、serde default 函式。禁止包含任何 `fn` 帶業務邏輯或 `#[tauri::command]`。**
2. **每個 `commands/*.rs`、`providers/*.rs`、`storage/*.rs` 檔案的 `fn` 數量必須 ≥ 2。不允許 1 行註解佔位檔。**
3. **`types.rs` 行數必須 < 800 行。**
4. **`main.rs` 行數必須 < 60 行。**
5. **前端 `App.tsx` 行數必須 < 800 行。不允許 `AppLegacy.tsx` 這種 rename wrapper。**
6. **前端每個 Tab 元件（`DashboardTab.tsx` 等）行數必須 > 50 行，且必須包含真正的 JSX render。**
7. **`cargo check` 零 error。`npm run lint && npm run build` 通過。**
8. **所有現有 `#[cfg(test)]` 測試必須通過 `cargo test`（如有 linker 則 `cargo check --tests`）。**
9. **不新增任何第三方 crate 或 npm package。**
10. **不改變任何 Tauri command 的函式名稱、參數、回傳型別。**

---

## 當前檔案狀態

Codex 上一輪的產出：
- `src-tauri/src/main.rs`（13 行）— 只呼叫 `types::build_app()`
- `src-tauri/src/types.rs`（7,344 行）— 所有程式碼都在這裡
- `src-tauri/src/commands/*.rs` — 全是 1 行 `// Command implementations are currently defined in crate::types.`
- `src-tauri/src/providers/*.rs` — 全是 1 行 placeholder
- `src-tauri/src/storage/*.rs` — 全是 1 行 placeholder
- `src-tauri/src/util.rs` — 1 行 placeholder
- `src/App.tsx`（3 行）— `import AppLegacy; export default AppLegacy`
- `src/components/AppLegacy.tsx`（3,879 行）— 原 App.tsx 整份
- `src/components/*.tsx` — 全是 `return null` 空殼
- `src/hooks/useNotices.ts` — 空殼
- `src/constants.ts` — 只有 1 行

**你的工作是把 `types.rs` 裡的函式真正搬到對應模組，並把 `AppLegacy.tsx` 真正拆成元件。**

---

## Phase 1：Rust 後端拆分（從 types.rs 搬到模組）

### types.rs 最終只保留（< 800 行）

只保留以下內容，全部加 `pub` 或 `pub(crate)`：
- 常數（RECORD_TYPE_DIRS, OPENAI_SERVICE, *_USERNAME, *_URL, CODEX_MODEL_FALLBACKS 等）
- struct 定義（ResolvedHome, Record, RecordPayload, LogEntry, TagCount, DailyCount, DashboardStats, SearchResult, RebuildIndexResult, AiAnalysisResponse, WorkspaceProfile, NotionSettings, NotebookLmSettings, IntegrationsSettings, DebateProviderConfig, ProviderRegistrySettings, AppSettings, ExportReportResult, HealthDiagnostics, HomeFingerprint, NotionSyncResult, NotionBatchSyncResult, NotionRemoteRecord, NotionUpsertInfo, NotebookLmConfig, NotebookSummary, NotebookLmAskResult, DebateParticipantConfig, DebateModeRequest, DebateModeResponse, DebateReplayConsistency, DebateReplayResponse, DebateRuntimeParticipant, DebateNormalizedRequest, DebateProviderRegistry, DebateRole, DebateRound, DebateState, DebateChallenge, DebateTurn, DebateRoundArtifact, DebatePacketParticipant, DebatePacketConsensus, DebateRejectedOption, DebateDecision, DebateRisk, DebateAction, DebateTrace, DebatePacketTimestamps, DebateFinalPacket, DebateLock, CliInvocation, CliProviderConfig, DebateWritebackRef）
- impl Default（NotionSettings, NotebookLmSettings, IntegrationsSettings, ProviderRegistrySettings）
- impl DebateProviderRegistry
- serde default 函式（default_poll_interval, default_enabled_true, default_debate_provider_configs, default_notebooklm_command, default_notebooklm_args）

**types.rs 禁止包含的東西：任何 `#[tauri::command]`、任何業務邏輯 `fn`、`build_app()`、`#[cfg(test)]` 區塊。**

### main.rs（< 60 行）

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod providers;
mod storage;
pub mod types;
mod util;

use std::sync::Mutex;
use types::DebateLock;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(DebateLock(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            commands::core::resolve_central_home,
            commands::core::list_records,
            commands::core::list_logs,
            commands::core::get_dashboard_stats,
            commands::core::upsert_record,
            commands::core::delete_record,
            commands::search::rebuild_search_index,
            commands::search::search_records,
            commands::ai::run_ai_analysis,
            commands::debate::run_debate_mode,
            commands::debate::replay_debate_mode,
            commands::export::export_markdown_report,
            commands::health::get_home_fingerprint,
            commands::health::get_health_diagnostics,
            commands::settings::get_app_settings,
            commands::settings::save_app_settings,
            commands::keychain::set_openai_api_key,
            commands::keychain::has_openai_api_key,
            commands::keychain::clear_openai_api_key,
            commands::keychain::set_gemini_api_key,
            commands::keychain::has_gemini_api_key,
            commands::keychain::clear_gemini_api_key,
            commands::keychain::set_claude_api_key,
            commands::keychain::has_claude_api_key,
            commands::keychain::clear_claude_api_key,
            commands::keychain::set_notion_api_key,
            commands::keychain::has_notion_api_key,
            commands::keychain::clear_notion_api_key,
            commands::notion::sync_record_to_notion,
            commands::notion::sync_records_to_notion,
            commands::notion::sync_record_bidirectional,
            commands::notion::sync_records_bidirectional,
            commands::notion::pull_records_from_notion,
            commands::notebooklm::notebooklm_health_check,
            commands::notebooklm::notebooklm_list_notebooks,
            commands::notebooklm::notebooklm_create_notebook,
            commands::notebooklm::notebooklm_add_record_source,
            commands::notebooklm::notebooklm_ask,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### 函式分配表（精確到每個 fn）

以下列出 types.rs 中每個 fn 及其目標檔案。**你必須剪切（不是複製）每個 fn 到指定檔案。**

#### `commands/core.rs`

tauri commands（加 `pub`）：
- `resolve_central_home`（L771）
- `list_records`（L787）
- `list_logs`（L793）
- `get_dashboard_stats`（L799）
- `upsert_record`（L807）
- `delete_record`（L910）

private helpers：
- `normalized_home`（L1609）
- `compute_dashboard_stats`（L1619）

#### `commands/search.rs`

tauri commands（加 `pub`）：
- `rebuild_search_index`（L926）
- `search_records`（L940）

private helpers：
- `search_records_in_memory`（L1986）
- `count_records_in_memory`（L2006）
- `matches_record`（L2021）

#### `commands/ai.rs`

tauri commands（加 `pub`）：
- `run_ai_analysis`（L1023）

private helpers：
- `run_local_analysis`（L2064）
- `run_openai_analysis`（L2129）
- `build_context_digest`（L4526）

#### `commands/export.rs`

tauri commands（加 `pub`）：
- `export_markdown_report`（L1058）

private helpers：
- `render_report_markdown`（L4562）

#### `commands/health.rs`

tauri commands（加 `pub`）：
- `get_home_fingerprint`（L1108）
- `get_health_diagnostics`（L1144）

#### `commands/settings.rs`

tauri commands（加 `pub`）：
- `get_app_settings`（L1176）
- `save_app_settings`（L1181）

private helpers：
- `normalize_settings`（L5224）
- `normalize_provider_type`（L5168）
- `normalize_provider_capabilities`（L5176）
- `normalize_provider_registry_settings`（L5190）

#### `commands/keychain.rs`

tauri commands（共 12 個，全部加 `pub`）：
- `set_openai_api_key`, `has_openai_api_key`, `clear_openai_api_key`
- `set_gemini_api_key`, `has_gemini_api_key`, `clear_gemini_api_key`
- `set_claude_api_key`, `has_claude_api_key`, `clear_claude_api_key`
- `set_notion_api_key`, `has_notion_api_key`, `clear_notion_api_key`

private helpers：
- `keyring_entry`, `has_keyring_entry_value`
- `has_openai_api_key_internal`, `gemini_keyring_entry`, `has_gemini_api_key_internal`
- `claude_keyring_entry`, `has_claude_api_key_internal`
- `notion_keyring_entry`, `has_notion_api_key_internal`
- `resolve_api_key`, `resolve_gemini_api_key`, `resolve_claude_api_key`, `resolve_notion_api_key`

#### `commands/debate.rs`

tauri commands（加 `pub`）：
- `run_debate_mode`（L1569，async，含 `DebateLock` state 參數）
- `replay_debate_mode`（L1598，async）

private helpers（全部搬過來）：
- `run_debate_mode_internal`, `replay_debate_mode_internal`
- `normalize_debate_request`, `normalize_debate_output_type`, `normalize_provider_alias`
- `resolve_provider_type`, `normalize_debate_provider`, `normalize_debate_model_name`
- `parse_debate_role`, `validate_debate_transition`, `advance_debate_state`
- `debate_round2_target`, `execute_debate_turn`
- `provider_uses_local_stub`, `build_debate_provider_prompt`, `run_debate_provider_text`
- `generate_local_debate_text`
- `build_round2_challenges`, `build_round3_revisions`
- `build_debate_consensus`, `build_debate_decision`, `build_debate_risks`, `build_debate_actions`
- `validate_final_packet`, `write_json_artifact`, `render_debate_packet_markdown`
- `writeback_debate_result`, `select_writeback_record_type`
- `upsert_debate_index`, `count_debate_turns`, `count_debate_actions`
- `read_json_value`, `debate_error`, `parse_debate_error`, `generate_debate_run_id`
- `dedup_non_empty`, `summarize_text_line`, `trim_bullet_prefix`, `strip_claim_label`
- `extract_first_non_empty_line`, `extract_claim_text`, `extract_risk_lines`
- `find_turn`, `round_score`, `classify_risk_severity`, `due_after_days`, `round_number_from_str`

`#[cfg(test)] mod tests` 區塊也搬到 `commands/debate.rs` 底部。

#### `commands/notion.rs`

tauri commands（加 `pub`）：
- `sync_record_to_notion`, `sync_records_to_notion`
- `sync_record_bidirectional`, `sync_records_bidirectional`
- `pull_records_from_notion`

private helpers（全部 `notion_*` 前綴 + sync helpers）：
- `resolve_notion_database_id`, `load_record_by_json_path`, `infer_record_type_from_path`
- `normalize_conflict_strategy`, `record_sync_hash`, `local_has_changed_since_sync`
- `remote_has_changed`, `mark_record_synced`, `build_sync_result`
- `sync_record_to_notion_internal`, `sync_record_bidirectional_internal`, `pull_records_from_notion_internal`
- `resolve_record_paths`, `generate_unique_record_paths`
- `record_from_remote`, `apply_remote_to_local_record`, `push_local_record_to_notion`
- `notion_client`, `notion_upsert_record`, `notion_error_code_from_body`
- `notion_fetch_database`, `notion_query_database_pages`, `notion_fetch_remote_record`
- `notion_remote_record_from_page`, `notion_extract_title_from_properties`
- `notion_extract_record_type_from_properties`, `notion_extract_tags_from_properties`
- `notion_extract_date_from_properties`, `notion_extract_created_at_from_properties`
- `notion_find_page_property_by_candidates`, `notion_plain_text_from_rich_text`
- `notion_fetch_page_content`, `notion_extract_content_sections`, `notion_extract_block_text`
- `notion_find_title_property_name`, `notion_find_property_by_candidates`
- `notion_build_properties`, `notion_build_children`

#### `commands/notebooklm.rs`

tauri commands（加 `pub`）：
- `notebooklm_health_check`, `notebooklm_list_notebooks`, `notebooklm_create_notebook`
- `notebooklm_add_record_source`, `notebooklm_ask`

private helpers：
- `parse_notebook_summary`, `render_record_source_text`, `resolve_notebooklm_runtime`
- `notebooklm_call_tool`, `write_jsonrpc_line`, `wait_jsonrpc_result`, `parse_mcp_tool_payload`

#### `providers/cli.rs`

structs（已在 types.rs 中定義）+ 函式實作：
- `run_cli_command_with_timeout`
- `summarize_cli_stream`, `extract_cli_json_text`, `parse_cli_output_text`
- `normalize_cli_model_arg`, `is_cli_model_error`, `read_and_cleanup_output_file`
- `build_codex_cli_args`, `build_gemini_cli_args`, `build_claude_cli_args`
- `parse_codex_cli_output`, `parse_json_stdout_output`
- `run_cli_provider_once`, `run_cli_provider`
- `run_codex_cli_completion`, `run_gemini_cli_completion`, `run_claude_cli_completion`
- `codex_cli_failure_hint`, `gemini_cli_failure_hint`, `claude_cli_failure_hint`

#### `providers/openai.rs`

- `run_openai_text_completion`
- `extract_openai_output_text`

#### `providers/gemini.rs`

- `run_gemini_text_completion`

#### `providers/claude.rs`

- `run_claude_text_completion`

#### `storage/records.rs`

- `load_records`, `load_logs`
- `record_from_value`, `persist_record_to_files`, `render_markdown`
- `detect_central_home_path`, `is_central_home`, `ensure_structure`
- `absolute_path`, `normalize_record_type`, `record_dir_by_type`

#### `storage/index.rs`

- `open_index_connection`, `ensure_index_schema`
- `rebuild_index`, `search_records_in_index`
- `get_index_count`, `upsert_index_record_if_exists`, `delete_index_record_if_exists`, `index_db_path`

#### `storage/settings_io.rs`

- `app_settings_path`, `load_settings`, `save_settings`

#### `util.rs`

- `slugify`, `generate_filename`, `file_mtime_iso`, `extract_day`
- `write_atomic`, `value_string`, `value_string_array`
- `parse_tags`, `option_non_empty`, `compare_iso_desc`, `sanitize_date_filter`

### 跨模組引用規則

- 每個模組在頂部加 `use crate::types::*;`
- 如需呼叫其他模組的函式，透過 `use crate::storage::records::load_records;` 等明確 import
- 被其他模組呼叫的函式需要 `pub(crate)` 修飾
- 只被同檔案使用的函式保持私有（無 `pub`）

### Phase 1 驗證

完成後執行：
```bash
PATH="/Users/pershing/.rustup/toolchains/stable-x86_64-apple-darwin/bin:$PATH" cargo check --manifest-path kofnote-app/src-tauri/Cargo.toml
```
必須零 error。然後：
```bash
wc -l kofnote-app/src-tauri/src/types.rs  # 必須 < 800
wc -l kofnote-app/src-tauri/src/main.rs   # 必須 < 60
# 每個 commands/*.rs, providers/*.rs, storage/*.rs 必須 > 10 行
```

---

## Phase 2：前端拆分（從 AppLegacy.tsx 拆成真正元件）

### 步驟

1. **刪除 `src/components/AppLegacy.tsx`**
2. **將原本的 App 元件內容寫回 `src/App.tsx`**，然後從中提取元件
3. **不允許任何 `return null` 的空殼元件存在**

### App.tsx 保留內容（< 800 行）

- 所有 `useState`, `useCallback`, `useMemo`, `useEffect` 頂層 hooks
- `function App()` 主元件
- Tab 路由邏輯（`activeTab` switch）
- Sidebar render
- 鍵盤快捷鍵 `useEffect`
- `<DashboardTab ...props />` / `<RecordsTab ...props />` 等元件引用
- helper 函式（`makeProfile`, `statusTone`, `getDebateModelDefault` 等可留在 App.tsx 或搬到 util）

### 元件提取規則

每個 Tab 元件從 App.tsx 的 JSX render 函式中提取。每個元件：
- **必須接收 props**（相關 state 和 callbacks）
- **必須包含完整的 JSX render**（不是 `return null`）
- **props 介面用 `interface XxxTabProps` 定義**

具體提取：

#### `components/DashboardTab.tsx`
- 從 App.tsx 中提取 Dashboard tab 的整個 render 區塊
- 包含 KPI 卡片、類型分佈、活動趨勢、force graph（如有）
- Props: records, logs, dashboardStats, 等相關 state

#### `components/RecordsTab.tsx`
- 記錄列表、篩選、編輯表單、CRUD 操作 UI
- Props: records, filters, selectedRecord, onUpsert, onDelete, 等

#### `components/LogsTab.tsx`
- 日誌列表、detail viewer
- Props: logs, filter state

#### `components/AiTab.tsx`（最大的元件）
- AI 分析表單 + 結果顯示
- Debate Mode 完整區塊（表單、provider 選擇、結果、replay）
- Props: 所有 ai/debate 相關 state 和 callbacks

#### `components/IntegrationsTab.tsx`
- Notion 連接設定 + 同步操作
- NotebookLM 連接設定 + 問答
- Props: 相關 settings 和 callbacks

#### `components/SettingsTab.tsx`
- App 設定、workspace profiles
- Props: settings, onSave, profiles

#### `components/HealthTab.tsx`
- 健康診斷、search index 狀態
- Props: diagnostics, onRebuildIndex

#### `components/CommandPalette.tsx`
- Command palette overlay
- Props: open, query, onClose, onSelect, commands

#### `components/NoticeBar.tsx`
- 通知 toast 列表
- Props: notices, onDismiss

#### `hooks/useNotices.ts`
- 包含真正的 `notices` state（`useState`）
- `pushNotice` 函式
- auto-dismiss `useEffect`
- 回傳 `{ notices, pushNotice }`

#### `constants.ts`
- `DEFAULT_MODEL`
- `TYPE_COLORS`
- `DEFAULT_DEBATE_MODEL_BY_PROVIDER`
- `DEBATE_ROLES`
- 其他 top-level 常數

### Phase 2 驗證

```bash
cd kofnote-app
npm run lint   # 必須通過
npm run build  # 必須通過
wc -l src/App.tsx                    # 必須 < 800
wc -l src/components/AiTab.tsx       # 必須 > 50
wc -l src/components/DashboardTab.tsx # 必須 > 50
wc -l src/components/RecordsTab.tsx   # 必須 > 50
# 不能存在 AppLegacy.tsx
ls src/components/AppLegacy.tsx && echo "FAIL: AppLegacy.tsx still exists" || echo "OK"
```

---

## Phase 3：DebateLock bug fix

當前實作問題：如果 `run_debate_mode_internal` panic，`spawn_blocking` 回傳 `JoinError`，但 lock 不會被清除。

修正方式：在 acquire lock 之後、spawn 之前就設定 run_id，然後在 scope exit 時一定清除：

```rust
pub async fn run_debate_mode(
    lock: tauri::State<'_, DebateLock>,
    central_home: String,
    request: DebateModeRequest,
) -> Result<DebateModeResponse, String> {
    // Acquire lock
    {
        let guard = lock.0.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        if guard.is_some() {
            return Err("Another debate is already running".to_string());
        }
    }
    // Set lock
    {
        let mut guard = lock.0.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        *guard = Some("pending".to_string());
    }

    let home = normalized_home(&central_home)?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        run_debate_mode_internal(&home, request)
    }).await.map_err(|e| format!("Debate worker join error: {e}"))?;

    // Always clear lock, even if result is Err
    if let Ok(mut guard) = lock.0.lock() {
        *guard = None;
    }

    result
}
```

---

## Phase 4：文件更新

1. **`docs/DEBATE_MODE_V01.md`** — Provider Routing Policy 段落改為：
   ```
   Supported provider labels:
   - `local`
   - `openai`, `gemini`, `claude` (API-based)
   - `codex-cli`, `gemini-cli`, `claude-cli` (CLI-based, live execution)
   - `chatgpt-web`, `gemini-web`, `claude-web` (config-ready, local stub fallback)
   ```

2. **`docs/ARCHITECTURE.md`** — 更新 Repository Structure 段落反映新的 `src-tauri/src/` 模組結構（commands/, providers/, storage/, types.rs, util.rs）

3. **`CLAUDE.md`** — 更新「關鍵檔案」表格反映模組化後的結構

---

## 執行順序

1. Phase 1（Rust 拆分）→ 驗證 → commit
2. Phase 2（前端拆分）→ 驗證 → commit
3. Phase 3（DebateLock fix）→ 驗證 → commit
4. Phase 4（文件）→ commit

每步之間跑驗證指令確認不破壞。

# KOF Note Desktop 重構計畫（Tauri + React + TypeScript）

## 1. 目標

建立一個高品質桌面中控台，管理 `keeponfirst-local-brain` 的 Central Home：
- 視覺化瀏覽 records + central logs
- 完整 CRUD（新增/編輯/刪除）
- 儀表板與搜尋
- AI 整理與分析（可切換 provider）
- 與既有檔案格式完全相容

## 2. 技術選型（首推）

- Desktop Shell: `Tauri v2`（Rust）
- Frontend: `React + TypeScript + Vite`
- UI: `Tailwind CSS + shadcn/ui + Radix UI`
- Animation: `Framer Motion`
- State: `Zustand + TanStack Query`
- Validation: `Zod`
- Chart: `Recharts`
- 本地索引（選配，M2）：`SQLite + FTS5`（`sqlx`/`rusqlite`）

## 3. 架構總覽

### 3.1 分層

1. Presentation（React）
- Dashboard、Records、Logs、AI、Settings

2. Application（TS service layer）
- RecordService、LogService、AnalyticsService、AIService
- 統一錯誤與 loading 狀態

3. Desktop Bridge（Tauri commands）
- 用 `invoke()` 呼叫 Rust commands

4. Infrastructure（Rust）
- 檔案系統讀寫、路徑偵測、索引、AI provider adapter

### 3.2 資料來源與相容性

- `records/{decisions,worklogs,ideas,backlogs,other}/*.json|*.md`
- `.agentic/logs/*.json`
- 僅讀取白名單資料夾，忽略 `.obsidian` 等非業務檔案

## 4. 核心模組設計

### 4.1 Rust（Tauri backend）

- `core/path_resolver.rs`
  - 自動將 `records`/`records/<type>` 回推至 Central Home
- `core/record_repo.rs`
  - list/get/create/update/delete
  - JSON/Markdown 雙檔同步
- `core/log_repo.rs`
  - list/get log entries
- `core/analytics.rs`
  - type 分布、7日趨勢、熱門 tags、sync status
- `core/ai/`
  - `local_analyzer.rs`（本地 heuristic）
  - `openai_provider.rs`（Responses API）
- `core/config_store.rs`
  - 儲存 app 設定（central_home, model, key_ref）

### 4.2 Frontend（React）

- `src/pages/`
  - `DashboardPage.tsx`
  - `RecordsPage.tsx`
  - `LogsPage.tsx`
  - `AIPage.tsx`
  - `SettingsPage.tsx`
- `src/features/records/`
  - list/filter/form/editor
- `src/features/logs/`
  - table/detail viewer
- `src/features/dashboard/`
  - KPI cards + charts
- `src/features/ai/`
  - prompt editor + result renderer
- `src/lib/tauri.ts`
  - command wrappers（typed）

## 5. Command API（初版）

- `resolve_central_home(input_path) -> central_home`
- `list_records(central_home, query?) -> Record[]`
- `upsert_record(central_home, payload, previous_path?) -> Record`
- `delete_record(json_path) -> void`
- `list_logs(central_home, query?) -> LogEntry[]`
- `get_dashboard_stats(central_home) -> DashboardStats`
- `run_local_analysis(central_home, prompt) -> string`
- `run_openai_analysis(central_home, prompt, model, api_key?) -> string`

## 6. UI/UX 規格（MVP+）

- 左側全域導覽（Dashboard / Records / Logs / AI / Settings）
- Dashboard：
  - KPI（Total Records / Total Logs / Pending Sync）
  - 7日活動趨勢（折線）
  - 類型比例（圓餅/條圖）
  - 熱門 tags（chip + bar）
- Records：
  - 快速搜尋（title/body/tag）
  - type/date/tag 篩選
  - Split view（左清單 + 右編輯）
  - optimistic UI + undo delete（toast）
- Logs：
  - 表格 + JSON detail viewer
- AI：
  - Prompt 模板、結果可複製/匯出

## 7. 安全與設定

- API key 不明文寫入 records；預設只存在 app 設定（可選 Keychain）
- 錯誤訊息可讀，但不洩漏敏感資訊
- 檔案 I/O 先寫 temp 再 rename（避免中斷導致壞檔）

## 8. 里程碑

### M1（可用版）

- Tauri 專案骨架
- Central Home 選取 + path normalize
- Records/Logs 讀取
- Records CRUD
- Dashboard 基礎統計

### M2（體驗版）

- 高品質 UI（shadcn + animation）
- 進階 filter/search
- Local analysis + OpenAI analysis
- 錯誤處理與提示系統

### M3（強化版）

- SQLite FTS 索引與快取
- 大量資料效能優化（virtualized list）
- 匯出報告、快捷鍵、命令面板
- macOS `.app` 打包與簽章流程

### M7（Connector 層）

- Notion Connector
  - Keychain 儲存 Notion API Key
  - 設定 Notion Database ID
  - 單筆/批次同步 records 到 Notion，並回寫 `notion_page_id/url/status/error`
- NotebookLM Connector
  - 以 MCP stdio（`uvx kof-notebooklm-mcp`）連線
  - Notebook 列表/建立
  - 將選定 record 當 text source 加入 notebook
  - 直接在 App 內提問並顯示 answer + citations

### M7.1（Notion 雙向同步 + 衝突策略）

- Notion 雙向同步
  - 選定 record 雙向同步
  - 當前篩選視圖批次雙向同步
  - 從 Notion database 拉回最新資料到本地（含新頁面建立本地 record）
- 衝突處理策略
  - `manual`：標記 `CONFLICT`，不自動覆寫
  - `local_wins`：將本地版本推回 Notion
  - `notion_wins`：使用 Notion 版本覆寫本地
- 版本追蹤欄位
  - `notion_last_synced_at`
  - `notion_last_edited_time`
  - `notion_last_synced_hash`

## 9. 驗收標準

- 指向既有 `keeponfirst-local-brain` central home 可直接讀到資料
- 100 筆以上 records 操作流暢
- CRUD 後 JSON/Markdown 一致
- Local/OpenAI analysis 皆可運作（OpenAI 需 key）
- Notion 同步可運作（需 Notion key + database id）
- NotebookLM 問答可運作（需本機可啟動 `kof-notebooklm-mcp` 並已登入）
- UI 在 macOS 桌面觀感與互動達到產品級水準

## 10. 接下來實作順序

1. 建立 `tauri + react + ts` 新專案骨架
2. 先完成 Rust repository + commands（資料層）
3. 接 Records/Logs 頁面，保證可讀可寫
4. 接 Dashboard 圖表與統計
5. 接 AI 分析頁
6. 收尾：錯誤處理、測試、打包

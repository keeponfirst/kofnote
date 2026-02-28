# KOF Note Desktop (Tauri + React + TypeScript)

M1~M5 implementation for the desktop central log console.
Now includes M7 connectors for Notion + NotebookLM.

## Delivered scope

### M1 - Core desktop app
- Tauri shell + React/TS frontend
- Central Home normalization (`records`, `records/<type>`, `.agentic`, `.agentic/logs`)
- Records / Logs read + Records CRUD (JSON + Markdown sync)
- Dashboard metrics

### M2 - UX and AI
- Upgraded workbench UI
- Tabs: Dashboard / Records / Logs / AI / Settings / Health
- Keyboard shortcuts:
  - `Cmd/Ctrl + K`: command palette
  - `Cmd/Ctrl + S`: save record (Records tab)
  - `Cmd/Ctrl + 1..7`: switch tab
- AI provider abstraction:
  - `local` analysis
  - `openai` analysis via Responses API

### M3 - Performance and index
- SQLite FTS5 search index (`.agentic/kofnote_search.sqlite`)
- Index rebuild command and indexed search
- "Load more" record rendering for larger datasets
- Markdown report export

### M4 - Release and security baseline
- OpenAI API key stored in Keychain (not plain app config)
- Health diagnostics page
- Tauri updater config scaffold (inactive by default)

### M5 - Ecosystem integration
- Workspace profile management (multiple central homes)
- Home fingerprint polling for external updates (e.g. skill writes from IDE)
- Auto-refresh when central log changes

### M7 - Connector layer (Notion + NotebookLM)
- New `Integrations` tab for connector setup and actions
- Notion connector:
  - Keychain-based API key storage
  - Database ID config in app settings
  - Sync selected record or batch sync current record view
  - Sync writes back `notion_page_id`, `notion_url`, `notion_sync_status`, `notion_error`
- NotebookLM connector:
  - MCP stdio command integration (default: `uvx kof-notebooklm-mcp`)
  - Health check, notebook listing, notebook creation
  - Add selected record as text source
  - Ask notebook and receive answer + citations

### M7.1 - Bidirectional Notion sync + conflict policy
- Bidirectional sync actions:
  - selected record
  - current filtered record view (batch)
  - pull latest from Notion database
- Conflict strategies:
  - `manual`: keep both sides and mark local record `CONFLICT`
  - `local_wins`: push local changes to Notion
  - `notion_wins`: pull Notion changes to local record
- Sync metadata persisted per record:
  - `notion_last_synced_at`
  - `notion_last_edited_time`
  - `notion_last_synced_hash`

### Quick Capture + System Tray
- **Global hotkey** `Cmd+Shift+K` (macOS) / `Ctrl+Shift+K` (Windows/Linux) to capture clipboard content from any app
  **全域快捷鍵** `Cmd+Shift+K` 從任何 app 捕捉剪貼簿內容
- System Tray residence — closing the window hides to tray, hotkey always available
  System Tray 常駐 — 關閉視窗隱藏到系統列，快捷鍵隨時可用
- Instant save as provisional note + background AI analysis (type / title / summary / tags)
  立即存為暫定筆記 + 背景 AI 分析（類型 / 標題 / 摘要 / 標籤）
- AI provider auto-selection: Claude → OpenAI → Gemini (based on configured API keys)
  AI 供應者自動選擇：Claude → OpenAI → Gemini（依已設定的 API key）
- macOS system notification on completion
  分析完成後發送 macOS 系統通知
- Tauri plugins: `global-shortcut`, `clipboard-manager`, `notification`

### Debate Provider Registry (config layer)
- Provider abstraction added as a registry-backed settings layer (`providerRegistry.providers[]`):
  - `id`
  - `type` (`cli` | `web`)
  - `enabled`
  - `capabilities`
- Built-in configurable providers:
  - CLI: `codex-cli`, `gemini-cli`, `claude-cli`
  - Web: `chatgpt-web`, `gemini-web`, `claude-web`
- Runtime in current build:
  - `codex-cli`: wired to real `codex exec`
  - `gemini-cli`: wired to real `gemini` one-shot invocation
  - `claude-cli`: wired to real `claude --print` invocation
  - all `*-web`: config-ready, currently local stub fallback
- Debate Mode model field behavior:
  - For CLI providers (`codex-cli`, `gemini-cli`, `claude-cli`), model is optional.
  - Leave model blank (or `auto`) to use the provider's default CLI/account model.
  - If a specific model is not supported, runtime retries once without explicit model.
- CLI provider smoke checks:
  - `codex exec - --skip-git-repo-check --sandbox read-only --output-last-message /tmp/codex.out --color never <<< "Reply with one line only."`
  - `gemini "Reply with one line only." --output-format json`
  - `claude --print --output-format json "Reply with one line only."`
- Example config:
  - `examples/providers.example.json`

### Second Brain P0 — Timeline + Unified Search

Bridges `keeponfirst-local-brain` records and OpenClaw session memory into a unified knowledge layer.

- **Timeline Tab** (`TimelineTab.tsx`): chronological view of all records and memory entries
  - Group by day / week / month
  - Source filter toggles (records / memory)
  - Debounced cross-source search with highlighted snippets
  - Detail panel with full content preview
- **Unified Search** (`unified_search` command): merges `records_fts` and `memory_fts` FTS5 results at query time
- **Memory Parser** (`storage/memory.rs`): parses OpenClaw `memory/*.md` files (session format + daily summary format)
- **Memory FTS5 Index** (`memory_fts` table): read-only indexing of memory files into SQLite FTS5, rebuilt alongside records index
- **Timeline API** (`get_timeline` command): loads all sources, groups by configurable time bucket
- i18n: full EN + zh-TW translations for all timeline strings

New/modified files:
| File | Change |
|------|--------|
| `src/components/TimelineTab.tsx` | New — Timeline tab UI component |
| `src-tauri/src/storage/memory.rs` | New — Memory file parser + unit tests |
| `src-tauri/src/storage/index.rs` | Extended — `memory_fts` table, indexing, search |
| `src-tauri/src/commands/search.rs` | Extended — `unified_search` + `get_timeline` commands |
| `src/types.ts` | Extended — `UnifiedMemoryItem`, `TimelineResponse` DTOs |
| `src-tauri/src/types.rs` | Extended — Rust struct equivalents |
| `src/lib/tauri.ts` | Extended — `unifiedSearch()` + `getTimeline()` bridge |
| `src/components/AppLegacy.tsx` | Extended — Timeline tab routing |
| `src/index.css` | Extended — Timeline styles |
| `src/i18n/locales/{en,zh-TW}.ts` | Extended — Timeline i18n strings |

## Data compatibility

Reads and writes are compatible with `keeponfirst-local-brain`:

- `records/{decisions,worklogs,ideas,backlogs,other}/*.json|*.md`
- `.agentic/logs/*.json`
- `memory/*.md` (read-only, indexed for unified search and timeline)

## Requirements

- Node.js 20+
- npm 10+
- Rust toolchain (`rustup`, `cargo`, `rustc`)
- macOS: Xcode Command Line Tools (`xcode-select --install`)

If `cargo` is not in your shell PATH, run commands with:

```bash
PATH="$HOME/.rustup/toolchains/stable-x86_64-apple-darwin/bin:$PATH" <command>
```

## Install

```bash
cd kofnote-app
npm install
```

## Development

```bash
npm run tauri:dev
```

## Build

```bash
npm run tauri:build
```

For non-GUI/CI environments:

```bash
npm run tauri:build:ci
```

## Validation

```bash
npm run lint
npm run build
```

## Notes

- App settings file is stored in OS config dir under `kofnote-desktop-tauri/settings.json`.
- Search index database is stored under `<central_home>/.agentic/kofnote_search.sqlite`.
- OpenAI key management is available in Settings tab.
- Notion API key management is available in Integrations tab.
- NotebookLM integration requires local command availability for `uvx kof-notebooklm-mcp` and authenticated profile.

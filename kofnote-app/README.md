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
  - `gemini-cli`, `claude-cli`, all `*-web`: config-ready, currently local stub fallback
- Example config:
  - `examples/providers.example.json`

## Data compatibility

Reads and writes are compatible with `keeponfirst-local-brain`:

- `records/{decisions,worklogs,ideas,backlogs,other}/*.json|*.md`
- `.agentic/logs/*.json`

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
cd /Users/pershing/Documents/henry/Fun/kofnote/kofnote-app
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

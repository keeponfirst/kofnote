# KOF Note Architecture

Updated: 2026-02-09

## 1. Purpose and Scope (What / Why)

### What
KOF Note is a desktop console for a `keeponfirst-local-brain` central log workspace.  
It provides:

- Central Home discovery and normalization.
- Record CRUD for `decision/worklog/idea/backlog/note`.
- Dashboard visualization (KPIs, distributions, activity wave, interactive force graph).
- Search and indexing (SQLite FTS5).
- Integrations (Notion bidirectional sync, NotebookLM MCP bridge).
- AI analysis (local heuristic + OpenAI provider).

### Why
The goal is to give one local-first command center for knowledge capture, operational logs, and action-oriented analysis while keeping compatibility with the existing on-disk structure.

## 2. High-Level Architecture

### Runtime split

- `kofnote-app/`: primary production runtime (Tauri + React + TypeScript + Rust).
- root Python app (`main.py` + `kofnote_desktop/`): legacy MVP runtime kept for compatibility.

### Layered architecture (primary runtime)

```mermaid
flowchart LR
  U["Desktop User"] --> FE["React UI\n/src/App.tsx + /src/components/AppLegacy.tsx"]
  FE --> BR["Tauri Bridge\n/src/lib/tauri.ts"]
  BR --> CMD["Rust Commands\n/src-tauri/src/types.rs + modular folders"]

  CMD --> FS["Central Home Filesystem\nrecords/* + .agentic/logs/*"]
  CMD --> IDX["SQLite FTS\n<central_home>/.agentic/kofnote_search.sqlite"]
  CMD --> CFG["App Settings\nOS config dir/kofnote-desktop-tauri/settings.json"]
  CMD --> KC["OS Keychain\nOpenAI/Gemini/Claude/Notion keys"]

  CMD --> OAI["OpenAI Responses API"]
  CMD --> NOTION["Notion API"]
  CMD --> NB["NotebookLM MCP runtime\n(default: uvx kof-notebooklm-mcp)"]
```

### Module boundaries

- Presentation/UI: `kofnote-app/src/App.tsx` (entry), `kofnote-app/src/components/AppLegacy.tsx`, `kofnote-app/src/index.css`.
- Frontend gateway: `kofnote-app/src/lib/tauri.ts` (typed wrapper + mock runtime).
- Domain/infra backend: `kofnote-app/src-tauri/src/main.rs` (startup wiring) + `kofnote-app/src-tauri/src/types.rs` (runtime logic) + modular folders under `commands/`, `providers/`, `storage/`.
- Shared contracts: `kofnote-app/src/types.ts`.
- i18n layer: `kofnote-app/src/i18n/*` (currently `en`, `zh-TW`).

## 3. Repository Structure and Responsibilities

```text
/
├── README.md                         # legacy-root usage + links
├── plan.md                           # implementation milestones/history
├── main.py                           # legacy Python app entrypoint
├── kofnote_desktop/                  # legacy Tkinter implementation
│   ├── app.py
│   ├── repository.py
│   ├── analytics.py
│   ├── ai.py
│   └── models.py
├── tests/                            # legacy Python unit tests
│   ├── test_repository.py
│   └── test_analytics.py
├── kofnote-app/                      # main production desktop app
│   ├── src/
│   │   ├── App.tsx                   # thin app entrypoint
│   │   ├── components/AppLegacy.tsx  # current full UI orchestration
│   │   ├── components/*Tab.tsx       # planned tab extraction targets
│   │   ├── hooks/useNotices.ts       # notice hook scaffold
│   │   ├── constants.ts              # frontend constants scaffold
│   │   ├── lib/tauri.ts              # invoke wrappers + mock runtime
│   │   ├── i18n/                     # translation dictionaries
│   │   └── types.ts                  # TS DTOs
│   ├── src-tauri/
│   │   ├── src/main.rs               # Tauri startup wiring
│   │   ├── src/types.rs              # runtime logic + command implementations
│   │   ├── src/commands/             # command module namespace (scaffold)
│   │   ├── src/providers/            # provider module namespace (scaffold)
│   │   └── src/storage/              # storage module namespace (scaffold)
│   │   ├── tauri.conf.json           # Tauri app/runtime config
│   │   └── Cargo.toml                # Rust deps/features
│   ├── e2e/smoke.spec.ts             # Playwright e2e smoke
│   ├── playwright.config.ts
│   └── package.json
├── openspec/                         # OpenSpec SSOT structure
│   ├── config.yaml
│   ├── specs/
│   └── changes/
└── docs/
    ├── ARCHITECTURE.md
    └── SDD_WITH_OPENSPEC.md
```

## 4. Key Flows

### 4.1 Startup + Central Home load

```mermaid
sequenceDiagram
  participant User
  participant UI as React App
  participant Bridge as tauri.ts
  participant Rust as Tauri Command
  participant FS as Filesystem

  User->>UI: Launch app
  UI->>Bridge: getAppSettings() + has*ApiKey()
  Bridge->>Rust: invoke
  Rust-->>UI: settings + key presence
  UI->>UI: restore cached home (localStorage/profile)
  UI->>Bridge: resolveCentralHome(inputPath)
  Bridge->>Rust: resolve_central_home
  Rust->>FS: detect path + ensure records/.agentic/logs
  Rust-->>UI: { centralHome, corrected }
  UI->>Bridge: listRecords/listLogs/getDashboardStats/getHealthDiagnostics/getHomeFingerprint
  Bridge->>Rust: invoke batch
  Rust->>FS: read JSON files + index/health state
  Rust-->>UI: hydrated state
```

### 4.2 Record upsert + markdown + index maintenance

```mermaid
sequenceDiagram
  participant UI as Records Editor
  participant Rust as upsert_record
  participant FS as records/*.json|*.md
  participant IDX as SQLite FTS

  UI->>Rust: upsert_record(payload, previous_json_path?)
  Rust->>Rust: normalize type/title/path + merge sync metadata
  Rust->>FS: atomic write JSON
  Rust->>FS: atomic write Markdown
  Rust->>FS: remove old files when moved
  Rust->>IDX: upsert index row if DB exists
  Rust-->>UI: saved Record DTO
```

### 4.3 Search flow (FTS first, memory fallback)

```mermaid
flowchart TD
  A["search_records(query, filters)"] --> B{"query empty?"}
  B -- yes --> M["in-memory filter on loaded records"]
  B -- no --> C{"FTS DB exists?"}
  C -- no --> D["rebuild index (best effort)"]
  D --> E["query SQLite FTS"]
  C -- yes --> E
  E --> F{"FTS success?"}
  F -- yes --> G["return indexed=true result + bm25 ranking"]
  F -- no --> H["fallback to in-memory search"]
  H --> I["return indexed=false result"]
  M --> I
```

### 4.4 Notion + NotebookLM integration flow

```mermaid
flowchart LR
  UI["Integrations tab"] --> N1["sync_record_bidirectional / batch / pull"]
  N1 --> R["Rust Notion adapter"]
  R --> K["Keychain (Notion key)"]
  R --> S["settings.json (database_id/conflict strategy)"]
  R --> API["Notion API"]
  R --> FS["Local record files + sync metadata"]

  UI --> NB1["notebooklm_* commands"]
  NB1 --> MCP["Spawn MCP command\n(default uvx kof-notebooklm-mcp)"]
  MCP --> NBSVC["NotebookLM service"]
  NBSVC --> MCP
  MCP --> UI
```

## 5. Important Configuration and Environment Variables

### Runtime data/config locations

- Central workspace data:
  - `records/{decisions,worklogs,ideas,backlogs,other}/*.json|*.md`
  - `.agentic/logs/*.json`
- Tauri app settings: OS config dir + `kofnote-desktop-tauri/settings.json`.
- Search index: `<central_home>/.agentic/kofnote_search.sqlite`.
- Legacy Python config: `~/.kofnote-desktop/config.json`.
- API keys (Tauri): OS Keychain entries under service `com.keeponfirst.kofnote`.

### Environment variables and flags

| Name | Used by | Purpose |
| --- | --- | --- |
| `OPENAI_API_KEY` | legacy Python app | Fallback for OpenAI analysis if key not entered in UI |
| `ANTIGRAVITY_LOG_HOME` | legacy Python app | Default Central Home |
| `VITE_KOF_MOCK=1` | React/Tauri bridge | Force mock runtime (no native Tauri backend) |
| `CI=true` | build scripts / Playwright | CI behavior (`tauri build --debug`, server reuse toggle) |
| `PATH=...rustup...` | npm scripts | Ensure `cargo` is discoverable for Tauri commands |
| `OPENSPEC_TELEMETRY=0` | OpenSpec CLI | Optional telemetry opt-out |

## 6. Local Development / Test / Build / Deploy

### Legacy Python MVP (still runnable)

```bash
cd /Users/pershing/Documents/henry/Fun/kofnote
python3 main.py
python3 -m unittest discover -s tests -p 'test_*.py'
```

### Main desktop app (Tauri + React)

```bash
cd /Users/pershing/Documents/henry/Fun/kofnote/kofnote-app
npm install
npm run tauri:dev
```

Useful verification commands:

```bash
cd /Users/pershing/Documents/henry/Fun/kofnote/kofnote-app
npm run lint
npm run build
npm run test:e2e
```

Build/package desktop app:

```bash
cd /Users/pershing/Documents/henry/Fun/kofnote/kofnote-app
npm run tauri:build
```

Notes:

- When `cargo` is not on shell `PATH`, use the existing scripts in `package.json` (they prepend rustup toolchain path).
- Current repository has no explicit CI pipeline file; build/test are run manually or through local task runners.

## 7. Known Technical Debt / Risks (from current state)

1. **Large monolithic modules**
   - Runtime monolith risk still exists in `kofnote-app/src/components/AppLegacy.tsx` and `kofnote-app/src-tauri/src/types.rs`; extraction scaffolds are in place but decomposition is still ongoing.

2. **Dual runtime maintenance cost**
   - Legacy Python app and Tauri app coexist, with overlapping capabilities and duplicated logic.

3. **Test coverage gap on real backend**
   - Playwright e2e runs primarily against mock runtime (`VITE_KOF_MOCK=1`), so native Tauri command regressions may escape.

4. **Provider surface mismatch**
   - Key management supports OpenAI/Gemini/Claude, but `run_ai_analysis` currently handles only `local` + `openai`.

5. **Integration reliability dependency**
   - NotebookLM depends on an external MCP command (`uvx kof-notebooklm-mcp`) and authenticated runtime availability; failures are runtime/environment-sensitive.

6. **On-disk schema coupling**
   - App behavior depends on implicit folder + JSON field conventions. Schema drift from external writers can degrade parsing/sync behavior.

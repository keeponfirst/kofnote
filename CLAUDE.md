# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Language Preference

- 回應使用**繁體中文**
- 程式碼註解可中英文混用
- 文件撰寫使用繁體中文

## Project Overview

KOF Note 是 `keeponfirst-local-brain` 中央日誌工作區的桌面控制台。主要生產環境為 `kofnote-app/`（Tauri 2 + React 19 + TypeScript + Rust），根目錄另有舊版 Python/Tkinter MVP（`main.py` + `kofnote_desktop/`）。

## Common Commands

所有 npm 指令在 `kofnote-app/` 目錄下執行：

```bash
# 開發
cd kofnote-app && npm install
npm run tauri:dev          # 啟動桌面應用（含 hot reload）
npm run dev:mock           # 純前端 mock runtime（無需 Rust backend）

# 驗證
npm run lint               # ESLint
npm run build              # TypeScript 編譯 + Vite 打包

# 測試
npm run test:e2e           # Playwright smoke test
npm run test:e2e:ui        # Playwright 互動式 UI

# 打包
npm run tauri:build        # 建構原生應用
npm run tauri:build:ci     # CI 模式（debug build）
```

舊版 Python：
```bash
python3 main.py
python3 -m unittest discover -s tests -p 'test_*.py'
```

## Architecture

### 分層架構

```
Desktop User → React UI (App.tsx) → Tauri Bridge (lib/tauri.ts) → Rust Commands (main.rs)
                                                                    ├── Central Home 檔案系統
                                                                    ├── SQLite FTS5 搜尋索引
                                                                    ├── OS Keychain（API keys）
                                                                    ├── OpenAI / Notion API
                                                                    └── NotebookLM MCP runtime
```

### 關鍵檔案

| 檔案 | 職責 |
|------|------|
| `kofnote-app/src/App.tsx` | 主 UI 編排、tab 路由、狀態管理（大型單體檔案） |
| `kofnote-app/src/lib/tauri.ts` | Tauri invoke 型別封裝 + mock runtime fallback |
| `kofnote-app/src/types.ts` | 前後端共用 TypeScript DTO |
| `kofnote-app/src-tauri/src/main.rs` | Rust 後端：所有 Tauri commands（大型單體檔案） |
| `kofnote-app/src/i18n/` | 國際化字典（`en`, `zh-TW`） |
| `kofnote-app/src/lib/providerRegistry.ts` | Debate Mode provider 抽象層 |

### 模組邊界

- **Presentation**：`App.tsx` + `index.css`（Tabs: Dashboard / Records / Logs / AI / Integrations / Settings / Health）
- **Frontend Gateway**：`lib/tauri.ts`（typed invoke wrappers；`VITE_KOF_MOCK=1` 啟用 mock）
- **Backend**：`main.rs`（commands + repositories + integration adapters）
- **Shared Contracts**：`types.ts`

### 資料存儲

Central Home 目錄結構（與 `keeponfirst-local-brain` 相容）：
```
<central_home>/
├── records/{decisions,worklogs,ideas,backlogs,other}/*.json|*.md
├── .agentic/logs/*.json
└── .agentic/kofnote_search.sqlite
```

- App settings：OS config dir + `kofnote-desktop-tauri/settings.json`
- API keys：OS Keychain（service: `com.keeponfirst.kofnote`）

## Tech Stack

- **Frontend**：React 19, TypeScript 5.9, Vite 7, d3-force 3
- **Desktop Shell**：Tauri 2 (Rust 2021, edition 1.77.2+)
- **Storage**：rusqlite (SQLite FTS5 bundled), serde_json
- **Security**：keyring 2 (OS Keychain), reqwest (rustls)
- **Testing**：Playwright (e2e)
- **Linting**：ESLint 9, typescript-eslint

## Environment Variables

| 變數 | 用途 |
|------|------|
| `VITE_KOF_MOCK=1` | 強制 mock runtime（前端開發用） |
| `CI=true` | CI 模式（debug build, Playwright server reuse） |
| `OPENAI_API_KEY` | 舊版 Python app fallback |

## OpenSpec Workflow

本專案使用 OpenSpec 作為 SSOT（Single Source of Truth）規格管理：
```bash
openspec new change "<change-id>"
openspec instructions apply --change "<change-id>"
```
規格檔案位於 `openspec/` 目錄。

## Key Documentation

- 架構設計：`docs/ARCHITECTURE.md`
- SDD with OpenSpec：`docs/SDD_WITH_OPENSPEC.md`
- Debate Mode v0.1：`docs/DEBATE_MODE_V01.md`
- 實作里程碑：`plan.md`

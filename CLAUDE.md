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
| `kofnote-app/src/App.tsx` | 前端入口（薄層 wrapper） |
| `kofnote-app/src/components/AppLegacy.tsx` | 目前主 UI 編排、tab 路由、狀態管理（待持續拆分） |
| `kofnote-app/src/lib/tauri.ts` | Tauri invoke 型別封裝 + mock runtime fallback |
| `kofnote-app/src/types.ts` | 前後端共用 TypeScript DTO |
| `kofnote-app/src-tauri/src/main.rs` | Rust 啟動 wiring（Builder + invoke handler） |
| `kofnote-app/src-tauri/src/types.rs` | Rust 執行邏輯與 command 實作（目前主要邏輯檔） |
| `kofnote-app/src/i18n/` | 國際化字典（`en`, `zh-TW`） |
| `kofnote-app/src/lib/providerRegistry.ts` | Debate Mode provider 抽象層 |

### 模組邊界

- **Presentation**：`App.tsx`（entry）+ `components/AppLegacy.tsx` + `index.css`
- **Frontend Gateway**：`lib/tauri.ts`（typed invoke wrappers；`VITE_KOF_MOCK=1` 啟用 mock）
- **Backend**：`main.rs`（startup）+ `types.rs`（commands + repositories + integration adapters）
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

本專案使用 OpenSpec 作為 SSOT（Single Source of Truth）規格管理。規格檔案位於 `openspec/` 目錄。

### `/opsx:*` Slash Commands

| 命令 | 用途 |
|------|------|
| `/opsx:new` | 建立新 change（proposal → specs → design → tasks） |
| `/opsx:explore` | 探索現有 specs 和 changes |
| `/opsx:apply` | 根據 change 的 tasks 執行實作 |
| `/opsx:verify` | 驗證 change 實作是否符合 specs |
| `/opsx:archive` | 歸檔已完成的 change |
| `/opsx:bulk-archive` | 批次歸檔多個已完成 changes |
| `/opsx:continue` | 繼續未完成的 change |
| `/opsx:ff` | Fast-forward：快速推進簡單 change |
| `/opsx:sync` | 同步 specs 與 changes 狀態 |
| `/opsx:onboard` | 專案導覽與 OpenSpec 使用說明 |

### CLI 指令

```bash
openspec new change "<change-id>"          # 建立新 change
openspec list                              # 列出所有 changes
openspec instructions apply --change "<id>" # 產生實作指引
```

## Superpowers Skills

本專案已整合 [Superpowers](https://github.com/obra/superpowers) 工程紀律技能庫（v4.3.1）。以下技能會在 session 啟動時自動載入，於適當情境觸發：

| 技能 | 觸發時機 |
|------|---------|
| `using-superpowers` | Session 啟動時注入基礎行為 |
| `writing-plans` | 規劃實作方案時 |
| `executing-plans` | 按計畫逐步實作時 |
| `test-driven-development` | 撰寫或修改測試時 |
| `systematic-debugging` | 除錯時使用系統化方法 |
| `requesting-code-review` | 提交 PR / 請求 review 時 |
| `receiving-code-review` | 處理 review 回饋時 |
| `verification-before-completion` | 標記任務完成前的驗證 |
| `brainstorming` | 腦力激盪 / 方案探索時 |
| `dispatching-parallel-agents` | 分派平行子任務時 |
| `subagent-driven-development` | 使用 subagent 完成開發時 |
| `finishing-a-development-branch` | 完成功能分支、準備合併時 |
| `using-git-worktrees` | 使用 git worktree 隔離開發時 |
| `writing-skills` | 撰寫新 skill 定義時 |

## 推薦工作流

OpenSpec 規範驅動 + Superpowers 工程紀律的整合流程：

```
1. /opsx:new "feature-name"     ← 建立 change，撰寫 proposal
   ↓ Superpowers: writing-plans
2. 撰寫 specs/ + design.md      ← 定義規格與設計
   ↓ Superpowers: brainstorming
3. 撰寫 tasks.md                ← 拆解可執行任務（每個 ≤2hr）
   ↓ Superpowers: executing-plans
4. /opsx:apply                   ← 按 tasks 逐步實作
   ↓ Superpowers: test-driven-development, systematic-debugging
5. /opsx:verify                  ← 驗證實作符合 specs
   ↓ Superpowers: verification-before-completion
6. /opsx:archive                 ← 歸檔已完成 change
   ↓ Superpowers: finishing-a-development-branch
```

## Key Documentation

- 架構設計：`docs/ARCHITECTURE.md`
- SDD with OpenSpec：`docs/SDD_WITH_OPENSPEC.md`
- Debate Mode v0.1：`docs/DEBATE_MODE_V01.md`
- 實作里程碑：`plan.md`

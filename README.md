# KOF Note Desktop Console

A desktop control panel for `keeponfirst-local-brain` central logs.

## Documentation

- Architecture: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
- SDD with OpenSpec: [`docs/SDD_WITH_OPENSPEC.md`](docs/SDD_WITH_OPENSPEC.md)
- Debate Mode v0.1 runtime: [`docs/DEBATE_MODE_V01.md`](docs/DEBATE_MODE_V01.md)
- OpenSpec example spec: [`openspec/specs/example/spec.md`](openspec/specs/example/spec.md)
- Tauri app implementation notes: [`kofnote-app/README.md`](kofnote-app/README.md)

## SDD workflow (OpenSpec)

Use OpenSpec as SSOT before implementation changes:

```bash
# one-time setup
npm install -g @fission-ai/openspec@latest
cd /Users/pershing/Documents/henry/Fun/kofnote
openspec init

# per change
openspec new change "<change-id>"
openspec status --change "<change-id>"
openspec instructions <artifact> --change "<change-id>"
openspec instructions apply --change "<change-id>"
```

## What this app does

- Select one **Central Home** directory (the same root used by `keeponfirst-local-brain`)
- Visualize existing records and central logs
- CRUD records (`idea`, `worklog`, `decision`, `backlog`, `note`)
- Show dashboard insights (type distribution, recent activity, top tags)
- Run AI analysis:
  - Local heuristic summary (no API required)
  - OpenAI analysis (optional)
- **Quick Capture** — global hotkey to capture clipboard content from any app (see below)

## Quick Capture

Capture text from any application with a single hotkey. KOF Note lives in the **System Tray** so the shortcut is always available — no need to keep the window open.

**快速捕捉** — 在任何應用程式中，按下全域快捷鍵即可將剪貼簿內容捕捉為筆記。KOF Note 以 **System Tray 常駐**，不需要開啟視窗。

### How to use / 使用方式

1. **Copy text** in any app (browser, PDF, terminal, etc.)
   在任意應用程式中 **複製文字**
2. Press **`Cmd+Shift+K`** (macOS) or **`Ctrl+Shift+K`** (Windows/Linux)
   按下 **`Cmd+Shift+K`**（macOS）或 **`Ctrl+Shift+K`**（Windows/Linux）
3. KOF Note shows a toast: *"Captured, AI analyzing..."*
   KOF Note 顯示提示：「已捕捉，AI 分析中…」
4. The content is immediately saved as a provisional note
   內容會立即儲存為暫定筆記
5. AI analyzes in the background → assigns **type** / **title** / **summary** / **tags**
   AI 在背景分析 → 自動分類為 **類型** / **標題** / **摘要** / **標籤**
6. A **macOS system notification** appears when analysis is complete
   分析完成後會收到 **macOS 系統通知**

### AI provider priority / AI 供應者優先順序

The AI provider is selected automatically based on which API key is configured in Settings:

| Priority | Provider | Model |
|----------|----------|-------|
| 1 | Claude | `claude-sonnet-4-6` |
| 2 | OpenAI | `gpt-4o-mini` |
| 3 | Gemini | `gemini-2.0-flash` |

If no API key is configured, the note is saved as-is without AI analysis.
若未設定任何 API key，筆記會直接儲存為原始內容，不進行 AI 分析。

### System Tray / 系統列常駐

- Closing the app window **hides** it to the tray instead of quitting.
  關閉視窗時，app 會**隱藏**到系統列而非退出。
- **Left-click** the tray icon to toggle window visibility.
  **左鍵點擊** tray icon 可切換視窗顯示/隱藏。
- **Right-click** for menu: "顯示 KOF Note" / "結束".
  **右鍵點擊** 開啟選單：「顯示 KOF Note」/「結束」。

## Data compatibility

This app reads/writes the same storage layout as `keeponfirst-local-brain`:

- `records/{decisions,worklogs,ideas,backlogs,other}/*.json`
- `records/{...}/*.md`
- `.agentic/logs/*.json`

## Run

```bash
cd /Users/pershing/Documents/henry/Fun/kofnote
python3 main.py
```

No third-party dependency is required for the MVP (Tkinter + stdlib only).

## Optional OpenAI setup

In AI tab:
- Fill `API Key` and model (default `gpt-4.1-mini`)
- Click `OpenAI Analysis`

Or set env before launch:

```bash
export OPENAI_API_KEY="your_key"
python3 main.py
```

## Tests

```bash
cd /Users/pershing/Documents/henry/Fun/kofnote
python3 -m unittest discover -s tests -p 'test_*.py'
```

## Notes

- First time you pick Central Home, app persists config to:
  - `~/.kofnote-desktop/config.json`
- If your central path has no existing structure, the app will create required folders.

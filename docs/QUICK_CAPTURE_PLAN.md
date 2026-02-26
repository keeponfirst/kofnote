# Quick Capture 功能實作計劃

> 狀態：草稿 | 建立：2026-02-26

## 功能概述

讓使用者在任何 app（瀏覽器、PDF、Terminal 等）選取文字或複製 URL 後，按下全域快捷鍵，KOF Note 在背景自動：
1. 立即儲存原始內容到 central log
2. 呼叫 AI 分析內容，產出有意義的摘要、分類、標籤
3. 更新記錄，以系統通知告知完成

App 以 **System Tray 常駐**於 macOS 選單列，確保快捷鍵隨時有效，不需要開著視窗。

---

## 使用者流程

```
在任意 app 選取文字 → Cmd+C（複製）
           ↓
按下 Cmd+Shift+K（全域快捷鍵）
           ↓
KOF Note 顯示簡短 Toast：「已捕捉，AI 分析中...」
（立即存為暫定 note，不阻塞使用者）
           ↓
背景 thread 呼叫 AI（使用已設定的 API key）
AI 分析：類型 / 標題 / 摘要 / 標籤
           ↓
更新 record（覆寫 final_body、type、title、tags）
           ↓
macOS 系統通知：「已儲存為 [idea]：[AI 生成的標題]」
```

---

## 系統架構

```
全域快捷鍵觸發
    │
    ▼
tauri-plugin-global-shortcut
    │
    ▼
前端 (React) — 讀取剪貼簿
tauri-plugin-clipboard-manager
    │ invoke("quick_capture", { content, central_home, provider })
    ▼
Rust: commands::capture::quick_capture
    ├── 立即 upsert_record(type="note", source_text=raw, final_body="AI 分析中...")
    │       → 回傳 json_path 給前端（前端顯示 toast）
    └── std::thread::spawn → AI 分析
            ├── 呼叫 AI API（使用 keychain 中的 key）
            ├── 解析 JSON response
            ├── 更新磁碟上的 JSON 檔（覆寫 type/title/final_body/tags）
            └── app_handle.emit("capture_complete", payload)
                    │
                    ▼
            前端收到事件 → 發送 macOS 系統通知
            tauri-plugin-notification
```

---

## 新增 Tauri Plugins

| Plugin | 用途 | Cargo crate |
|--------|------|-------------|
| `tauri-plugin-global-shortcut` | 全域快捷鍵 | `tauri-plugin-global-shortcut = "2"` |
| `tauri-plugin-clipboard-manager` | 讀取剪貼簿 | `tauri-plugin-clipboard-manager = "2"` |
| `tauri-plugin-notification` | 系統通知 | `tauri-plugin-notification = "2"` |

System Tray 在 Tauri 2 是內建功能（不需要額外 plugin），透過 `tauri::tray` API 使用。

---

## 實作步驟

### Phase 1：依賴更新

**`src-tauri/Cargo.toml`** — 新增：
```toml
tauri-plugin-global-shortcut = "2"
tauri-plugin-clipboard-manager = "2"
tauri-plugin-notification = "2"
```

**`src-tauri/tauri.conf.json`** — 新增 permissions：
```json
{
  "app": {
    "windows": [
      {
        "label": "main",
        "closable": true
      }
    ]
  },
  "plugins": {
    "global-shortcut": {
      "shortcuts": ["CommandOrControl+Shift+K"]
    },
    "notification": {
      "permission": "request-on-use"
    }
  }
}
```

**`src-tauri/capabilities/default.json`** — 新增 capability permissions：
```json
"global-shortcut:allow-register",
"global-shortcut:allow-unregister",
"clipboard-manager:allow-read-text",
"notification:allow-send-notification",
"notification:allow-request-permission"
```

---

### Phase 2：System Tray 常駐

**`src-tauri/src/tray.rs`**（新檔案）：

```rust
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};

pub fn setup_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "顯示 KOF Note", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "結束", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, .. } = event {
                let app = tray.app_handle();
                if let Some(win) = app.get_webview_window("main") {
                    if win.is_visible().unwrap_or(false) {
                        let _ = win.hide();
                    } else {
                        let _ = win.show();
                        let _ = win.set_focus();
                    }
                }
            }
        })
        .build(app)?;

    Ok(())
}
```

**`src-tauri/src/main.rs`** — 修改 `.run()` 前加入：
- 關閉視窗改為「隱藏」而非退出（`on_window_event` 攔截 `CloseRequested`）
- 呼叫 `tray::setup_tray`
- 初始化 global shortcut plugin

```rust
.setup(|app| {
    tray::setup_tray(&app.handle())?;
    Ok(())
})
.on_window_event(|window, event| {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        window.hide().unwrap();
        api.prevent_close();
    }
})
.plugin(tauri_plugin_global_shortcut::Builder::new().build())
.plugin(tauri_plugin_clipboard_manager::init())
.plugin(tauri_plugin_notification::init())
```

---

### Phase 3：Quick Capture Rust Command

**`src-tauri/src/commands/capture.rs`**（新檔案）：

```rust
use tauri::{AppHandle, Emitter, Manager};
use crate::storage::records::upsert_record;
use crate::types::{RecordPayload, load_api_key};

/// 事件 payload，當 AI 分析完成時 emit 給前端
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CaptureCompletePayload {
    pub json_path: String,
    pub record_type: String,
    pub title: String,
    pub tags: Vec<String>,
}

#[tauri::command]
pub fn quick_capture(
    app: AppHandle,
    central_home: String,
    content: String,          // 剪貼簿原始內容
    source_hint: Option<String>, // 選填：來源 app 名稱或 URL
) -> Result<String, String> {
    // Step 1: 立即存為暫定 note
    let payload = RecordPayload {
        record_type: "note".to_string(),
        title: format!("Quick Capture: {}", truncate(&content, 60)),
        source_text: Some(content.clone()),
        final_body: Some("⏳ AI 分析中...".to_string()),
        tags: Some(vec!["quick-capture".to_string()]),
        created_at: None,
        date: None,
        ..Default::default()
    };
    let record = upsert_record(&central_home, payload, None)?;
    let json_path = record.json_path.clone().unwrap_or_default();

    // Step 2: 背景 thread 呼叫 AI
    let json_path_clone = json_path.clone();
    let central_home_clone = central_home.clone();
    std::thread::spawn(move || {
        let result = analyze_with_ai(&content, &source_hint);
        match result {
            Ok(analysis) => {
                // 更新磁碟上的 JSON 檔
                if let Ok(()) = update_record_with_analysis(
                    &central_home_clone,
                    &json_path_clone,
                    &analysis,
                ) {
                    let _ = app.emit("capture_complete", CaptureCompletePayload {
                        json_path: json_path_clone,
                        record_type: analysis.record_type,
                        title: analysis.title,
                        tags: analysis.tags,
                    });
                }
            }
            Err(e) => {
                // AI 失敗時保留原始 note，emit 錯誤事件
                let _ = app.emit("capture_failed", serde_json::json!({
                    "jsonPath": json_path_clone,
                    "error": e,
                }));
            }
        }
    });

    Ok(json_path)
}
```

---

### Phase 4：AI 分析邏輯

**固定 System Prompt**（內嵌在 Rust 中）：

```
你是 KOF Note 的智慧知識管理助理。使用者剛剛捕捉了一段文字或 URL。

請深度分析這段內容，然後以 JSON 格式回傳以下欄位：

- "type"：分類，只能是 "decision" / "idea" / "backlog" / "note" / "worklog" 其中之一
  - decision：包含選擇、判斷、決定、取捨的內容
  - idea：靈感、可能性、創意方向、假設
  - backlog：待辦、任務、需要執行的事項
  - worklog：工作記錄、進度、已完成的事
  - note：其他知識、參考資料、學習筆記
- "title"：簡潔有意義的標題（繁體中文，最多 80 字）
- "summary"：2～4 句話的深度分析（繁體中文）
  - 說明這段內容的核心含意
  - 為什麼值得記錄
  - 有什麼潛在行動或洞察
- "tags"：2～5 個相關標籤（英文小寫 kebab-case）

僅回傳合法 JSON，不要加其他文字。
```

**回傳 JSON 結構**：
```json
{
  "type": "idea",
  "title": "用剪貼簿監控做快速捕捉筆記工具",
  "summary": "這段內容描述了一個可以從剪貼簿自動擷取資訊的筆記流程...",
  "tags": ["productivity", "note-taking", "clipboard", "automation"]
}
```

**AI 選用優先順序**（依 keychain 中的 key 存在與否）：
1. Claude（`claude_api_key`）→ `claude-sonnet-4-6`
2. OpenAI（`openai_api_key`）→ `gpt-4o-mini`（低成本，捕捉場景不需要最強模型）
3. Gemini（`gemini_api_key`）→ `gemini-2.0-flash`
4. 若都無 → 儲存為 note，不進行 AI 分析，通知用戶設定 API key

---

### Phase 5：前端整合

**全域快捷鍵註冊**（在 `App.tsx` 或 `AppLegacy.tsx` 的 useEffect）：

```typescript
import { register } from '@tauri-apps/plugin-global-shortcut';
import { readText } from '@tauri-apps/plugin-clipboard-manager';
import { sendNotification } from '@tauri-apps/plugin-notification';
import { listen } from '@tauri-apps/api/event';

// 註冊快捷鍵
await register('CommandOrControl+Shift+K', async () => {
  const content = await readText();
  if (!content?.trim()) return;

  showToast('已捕捉，AI 分析中...');

  const jsonPath = await invoke('quick_capture', {
    centralHome,
    content,
  });
});

// 監聽 AI 完成事件
await listen('capture_complete', (event) => {
  const { recordType, title } = event.payload;
  sendNotification({
    title: `KOF Note — 已儲存為 ${recordType}`,
    body: title,
  });
});

await listen('capture_failed', (event) => {
  sendNotification({
    title: 'KOF Note — 捕捉失敗',
    body: '請確認已設定 AI API Key',
  });
});
```

**Toast 元件**：使用現有的 `NoticeBar.tsx` 或新增一個輕量 `CaptureToast`（右下角浮現，2 秒後自動消失）。

---

## 資料結構說明

記錄在磁碟上的 JSON 格式，Quick Capture 後的範例：

```json
{
  "recordType": "idea",
  "title": "用剪貼簿監控做快速捕捉筆記工具",
  "createdAt": "2026-02-26T14:23:00+08:00",
  "sourceText": "原始剪貼簿內容...",
  "finalBody": "## AI 分析\n\n這段內容描述了一個可以從剪貼簿自動擷取...\n\n## 核心洞察\n...",
  "tags": ["productivity", "quick-capture", "automation"],
  "notionSyncStatus": "pending"
}
```

- `source_text`：永遠保存原始內容（不修改）
- `final_body`：AI 產出的 Markdown 分析結果
- `tags` 包含 `"quick-capture"` 標籤，方便在 Records Tab 篩選

---

## 變更檔案清單

| 檔案 | 操作 | 說明 |
|------|------|------|
| `src-tauri/Cargo.toml` | 修改 | 新增 3 個 plugin crates |
| `src-tauri/tauri.conf.json` | 修改 | 新增 plugin 設定 |
| `src-tauri/capabilities/default.json` | 修改 | 新增 permissions |
| `src-tauri/src/main.rs` | 修改 | 初始化 plugins、tray、window close 行為 |
| `src-tauri/src/tray.rs` | 新增 | System Tray 設定邏輯 |
| `src-tauri/src/commands/capture.rs` | 新增 | `quick_capture` command |
| `src-tauri/src/commands/mod.rs` | 修改 | 匯出 `capture` module |
| `src/App.tsx` 或 `AppLegacy.tsx` | 修改 | 註冊快捷鍵、監聽事件 |
| `src/components/CaptureToast.tsx` | 新增（選） | 輕量捕捉狀態 Toast |

---

## 注意事項 / 邊界情況

1. **剪貼簿為空**：快捷鍵觸發但剪貼簿無文字內容 → 靜默忽略或短暫提示「剪貼簿無內容」
2. **重複捕捉同一內容**：不做去重，讓使用者自行決定（保持簡單）
3. **AI 無回應 / 格式錯誤**：JSON parse 失敗時，保留原始 note 不更新，emit `capture_failed`
4. **App 未啟動**：System Tray 常駐後應由 Login Items 自動啟動；首次安裝需引導使用者設定
5. **macOS 通知權限**：首次觸發時 `tauri-plugin-notification` 會彈出系統授權請求
6. **快捷鍵衝突**：`Cmd+Shift+K` 可能與其他 app 衝突；未來可在設定中讓使用者自訂
7. **開機自動啟動**：本 plan 不含此功能，可後續用 `tauri-plugin-autostart` 加入

---

## 未來擴充（不在本次範圍）

- 自訂快捷鍵（Settings Tab）
- 開機自動啟動 toggle（`tauri-plugin-autostart`）
- 捕捉時自動偵測並附上來源 app 名稱
- Mini capture window（捕捉後彈出小視窗讓使用者補充備註）
- 支援圖片剪貼簿（OCR → text → AI）

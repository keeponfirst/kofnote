# Codex 修復任務（Review 後必修 + 建議修）

依序執行，每完成一個跑驗證後再繼續下一個。

---

## Fix1：Search snippet column index 錯誤（必修 — 功能無效）

### 問題

`kofnote-app/src-tauri/src/storage/index.rs` L206 的 snippet 呼叫：

```sql
snippet(records_fts, 2, '<mark>', '</mark>', '...', 32) AS snippet
```

Column `2` 對應 FTS5 table 的 `record_type` 欄位（值只有 "note"、"decision" 等短字串）。使用者搜尋的關鍵字幾乎不會匹配 record_type，導致 snippet 永遠為空，搜尋 highlight 功能形同虛設。

FTS5 table schema（同檔案 L19-33）：
- Column 0: json_path (UNINDEXED)
- Column 1: md_path (UNINDEXED)
- Column 2: record_type ← 目前錯誤地用了這個
- Column 3: title
- Column 4: final_body ← 最有用的內容欄位
- Column 5: source_text

### 修正

在 `kofnote-app/src-tauri/src/storage/index.rs` L206，把 column index 從 `2` 改為 `-1`：

```sql
snippet(records_fts, -1, '<mark>', '</mark>', '...', 32) AS snippet
```

`-1` 表示讓 FTS5 自動選擇最佳匹配 column，這樣無論搜尋詞出現在 title、final_body 或 source_text，都能正確產生 snippet。

### 驗證

```bash
PATH="/Users/pershing/.rustup/toolchains/stable-x86_64-apple-darwin/bin:$PATH" cargo check --manifest-path kofnote-app/src-tauri/Cargo.toml
```

---

## Fix2：Search snippet XSS 安全修正（必修 — 安全漏洞）

### 問題

`kofnote-app/src/components/AppLegacy.tsx` L2688 使用 `dangerouslySetInnerHTML` 直接渲染 FTS5 snippet：

```tsx
<p className="search-snippet" dangerouslySetInnerHTML={{ __html: snippet }} />
```

FTS5 的 `snippet()` 函式**不做 HTML escape**。如果 record 內容包含 `<script>alert(1)</script>` 或 `<img onerror=...>`，這些會原封不動進入 snippet，透過 `dangerouslySetInnerHTML` 注入 DOM，造成 XSS。

雖然是桌面 app + 本地資料，但有 Notion 雙向同步功能，外部資料可進入資料庫。

### 修正方案：Rust 端 sanitize

在 `kofnote-app/src-tauri/src/storage/index.rs` 的 `search_records_in_index` 函式中，snippet 插入 HashMap 之前做 sanitize。在 snippets 收集迴圈（L244-247 附近）修改為：

```rust
if let Some(json_path) = &record.json_path {
    if !snippet.trim().is_empty() {
        snippets.insert(json_path.clone(), sanitize_snippet_html(&snippet));
    }
}
```

在同一個檔案中新增 sanitize 函式：

```rust
/// Strip all HTML tags from snippet except <mark> and </mark>.
/// FTS5 snippet() does not escape HTML in source content, so we must
/// sanitize before sending to the frontend's dangerouslySetInnerHTML.
fn sanitize_snippet_html(raw: &str) -> String {
    // Step 1: temporarily replace our safe <mark> tags with placeholders
    let s = raw
        .replace("<mark>", "\x00MARK_OPEN\x00")
        .replace("</mark>", "\x00MARK_CLOSE\x00");

    // Step 2: strip all remaining HTML tags
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }

    // Step 3: also escape any remaining < > & to prevent injection
    let result = result
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");

    // Step 4: restore our safe <mark> tags
    result
        .replace("\x00MARK_OPEN\x00", "<mark>")
        .replace("\x00MARK_CLOSE\x00", "</mark>")
}
```

### 驗證

```bash
PATH="/Users/pershing/.rustup/toolchains/stable-x86_64-apple-darwin/bin:$PATH" cargo check --manifest-path kofnote-app/src-tauri/Cargo.toml
```

---

## Fix3：DebateLock TOCTOU race condition（建議修）

### 問題

`kofnote-app/src-tauri/src/types.rs` L1570-1587 的 `run_debate_mode` 中，check-lock 和 set-lock 分成兩個獨立的 `{}` scope，各自做一次 `lock.0.lock()`。兩次 lock acquisition 之間有一個極小的時間窗口，理論上另一個 request 可能同時通過 check。

原先的程式碼是在同一個 MutexGuard scope 內做 check+set，新版本退化了。

### 修正

把 L1570-1587 的兩個 scope 合併為一個：

```rust
// Check and set lock atomically
{
    let mut guard = lock
        .0
        .lock()
        .map_err(|error| format!("Lock poisoned: {error}"))?;
    if let Some(run_id) = guard.as_ref() {
        return Err(format!("Another debate is already running: {run_id}"));
    }
    *guard = Some(generate_debate_run_id());
}
```

### 驗證

```bash
PATH="/Users/pershing/.rustup/toolchains/stable-x86_64-apple-darwin/bin:$PATH" cargo check --manifest-path kofnote-app/src-tauri/Cargo.toml
```

---

## Fix4：清理空殼檔案和過期文件（建議修 — 技術債）

### 問題

Codex 重構時產生了一批空殼檔案和過期文件，佔位而無實際用途。

### 修正

1. 刪除以下空殼前端 Tab 元件（全是 `export function XxxTab() { return null }`）：

```
kofnote-app/src/components/AiTab.tsx
kofnote-app/src/components/RecordsTab.tsx
kofnote-app/src/components/LogsTab.tsx
kofnote-app/src/components/DashboardTab.tsx
kofnote-app/src/components/IntegrationsTab.tsx
kofnote-app/src/components/SettingsTab.tsx
kofnote-app/src/components/HealthTab.tsx
kofnote-app/src/components/CommandPalette.tsx
kofnote-app/src/components/NoticeBar.tsx
```

2. 確認這些檔案沒有被任何地方 import。搜尋 `from.*components/(AiTab|RecordsTab|LogsTab|DashboardTab|IntegrationsTab|SettingsTab|HealthTab|CommandPalette|NoticeBar)` 確認無引用後再刪除。如果有引用，則移除該 import 語句（因為 import 的東西是 null，不可能被使用）。

3. 刪除 `REFACTOR_PLAN.md`（已過期的重構計劃文件，約束全部違反，不再適用）。

4. 刪除 `CODEX_TASK.md`（已完成的任務清單）。

### 驗證

```bash
cd kofnote-app && npm run lint && npm run build
```

---

## 執行順序總覽

```
Fix1 (snippet column, 1 行改動) → commit "fix: use auto-select column for FTS5 search snippets"
Fix2 (XSS sanitize, ~30 行新增) → commit "fix: sanitize FTS5 snippet HTML to prevent XSS"
Fix3 (TOCTOU, ~10 行改動) → commit "fix: make debate lock check-and-set atomic"
Fix4 (cleanup, 刪除檔案) → commit "chore: remove placeholder stub files and outdated docs"
```

每步完成後跑對應的驗證指令。

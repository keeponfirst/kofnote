# fix-search-snippet-bugs

## 背景（Why）

搜尋功能有兩個已知問題：

1. **FTS5 snippet 欄位索引**（已修正）：原先 `snippet()` 使用硬編碼欄位 `2`，已修正為 `-1`（自動選擇最佳匹配欄位）。此 change 記錄此修正。

2. **Snippet XSS 安全漏洞**（待修正）：FTS5 `snippet()` 函式回傳含 `<mark>` 標籤的 HTML，但前端 `AppLegacy.tsx` 使用 `dangerouslySetInnerHTML` 直接渲染。若使用者記錄中含有惡意 HTML/JS，可能透過搜尋結果觸發 XSS。需新增 `sanitize_snippet_html` 函式在 Rust 端先行淨化。

## 變更（What）

1. 在 `storage/index.rs` 新增 `sanitize_snippet_html()` 函式，僅允許 FTS5 自己的 `<mark>` 和 `</mark>` 標籤，剩餘 HTML 特殊字元一律 escape
2. 在搜尋結果回傳前套用此函式

## 影響區域

| 檔案 | 修改內容 |
|------|---------|
| `kofnote-app/src-tauri/src/storage/index.rs` | 新增 `sanitize_snippet_html` 函式並套用 |

前端無需修改（仍使用 `dangerouslySetInnerHTML`，但後端已保證安全 HTML）。

## Rollback Plan

1. 移除 `sanitize_snippet_html` 函式及其呼叫即可回退
2. 無資料庫 schema 變更
3. 回退後功能不受影響，僅恢復 XSS 風險

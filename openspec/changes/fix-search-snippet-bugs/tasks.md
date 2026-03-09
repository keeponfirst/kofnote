# fix-search-snippet-bugs 任務清單

## Task 1: 實作 sanitize_snippet_html 函式

**追溯**：specs/snippet-sanitization.md → AC-1, AC-2, AC-3, AC-4

**工時預估**：1 小時

- [ ] 在 `kofnote-app/src-tauri/src/storage/index.rs` 新增 `sanitize_snippet_html(raw: &str) -> String` 函式
  - 定義 placeholder 常數（如 `\x00MARK_OPEN\x00` / `\x00MARK_CLOSE\x00`）
  - 將 `<mark>` 和 `</mark>` 替換為 placeholder
  - 對剩餘內容進行 HTML escape（`&` → `&amp;`, `<` → `&lt;`, `>` → `&gt;`, `"` → `&quot;`）
  - 將 placeholder 還原為 `<mark>` / `</mark>`
- [ ] 在搜尋結果的 `map` 回呼中，對 `snippet` 欄位套用 `sanitize_snippet_html`
- [ ] `cargo check` 通過
- [ ] `npm run build` 通過

**驗證指令**：
```bash
cd kofnote-app
cargo check --manifest-path src-tauri/Cargo.toml
npm run build
```

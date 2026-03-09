# fix-search-snippet-bugs 設計文件

## 設計決策

### D1: Rust 端淨化（非前端）

**修改檔案**：`kofnote-app/src-tauri/src/storage/index.rs`

在 Rust 後端完成淨化，理由：
- 安全邊界應在最接近資料來源處
- 前端不需修改，減少變更範圍
- 未來若有其他消費端（CLI、API），也自動受保護

**備選方案**：
- 前端 sanitize（DOMPurify 等）→ 拒絕：每個消費端都要做，容易遺漏
- SQLite UDF → 拒絕：rusqlite 自訂函式維護成本高，且邏輯不適合在 SQL 層

### D2: Placeholder 替換策略

使用兩階段替換：
1. 將 FTS5 的 `<mark>` / `</mark>` 替換為 UUID placeholder
2. HTML escape 全部內容
3. 將 placeholder 還原為 `<mark>` / `</mark>`

此策略保證 FTS5 標記與使用者內容中的同名標籤可區分。

**備選方案**：
- 正則 whitelist → 拒絕：正則處理 HTML 不可靠
- 使用 HTML parser crate → 拒絕：過度依賴，30 行手寫即可

## 修改檔案

| 檔案 | 修改 |
|------|------|
| `kofnote-app/src-tauri/src/storage/index.rs` | 新增 `sanitize_snippet_html()` 函式 + 在搜尋結果映射中呼叫 |

## 驗證指令

```bash
cd kofnote-app
cargo check --manifest-path src-tauri/Cargo.toml
npm run build
# 手動測試：建立含 <script>alert(1)</script> 的記錄，搜尋後確認顯示為純文字
```

## Migration Plan

1. 新增 `sanitize_snippet_html` 函式（~30 行）
2. 在搜尋結果 `map` 中呼叫（1 行修改）
3. 編譯驗證

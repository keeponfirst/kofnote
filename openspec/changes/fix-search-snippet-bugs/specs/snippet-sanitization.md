# Snippet HTML 淨化規格

## 需求

FTS5 搜尋回傳的 snippet HTML 必須在後端淨化，防止前端 XSS 攻擊。

## Acceptance Criteria

1. **AC-1**：FTS5 snippet 中的 `<mark>` 和 `</mark>` 標籤保留（用於高亮顯示）
2. **AC-2**：snippet 中所有其他 HTML 標籤被 escape（`<` → `&lt;`, `>` → `&gt;`）
3. **AC-3**：`&` 字元被正確 escape 為 `&amp;`（但已有的 `&lt;` / `&gt;` / `&amp;` 不重複 escape）
4. **AC-4**：搜尋結果中含 `<script>`, `<img onerror=...>`, `<a href="javascript:...">` 等攻擊向量時，全部被安全 escape

## Edge Cases

1. **記錄含合法 HTML**：使用者記錄原文包含 `<code>` 等標籤 → snippet 中這些標籤被 escape 顯示為純文字
2. **嵌套 mark 標籤**：FTS5 不會產生嵌套 mark，但若記錄原文含 `<mark>` → 先 escape 所有內容，再還原 FTS5 的 mark placeholder
3. **空 snippet**：FTS5 回傳空字串 → 直接回傳空字串，不處理
4. **Unicode 內容**：含中日韓字元的 snippet → 正常處理，UTF-8 不受 HTML escape 影響

## 實作策略

```
原始 snippet → 先將 <mark> 替換為 placeholder → HTML escape 全部 → 還原 placeholder 為 <mark>
```

## Contract

無 API 變更。搜尋結果的 `snippet` 欄位內容從原始 FTS5 HTML 變為淨化後 HTML。

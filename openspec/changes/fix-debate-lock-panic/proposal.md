# fix-debate-lock-panic

## 背景（Why）

`run_debate_mode` 函式中的 `DebateLock` 使用 `Mutex<Option<String>>` 保護並行存取，但 check-and-set 操作分成兩個獨立的 lock scope（先 check 再 set），產生 TOCTOU（Time-of-Check to Time-of-Use）競態條件。理論上兩個併發請求可能同時通過 check，導致兩個 debate run 同時啟動或 lock 狀態不一致。

此外，若 debate 執行中 panic，雖然已有 always-clear 邏輯，但 Mutex 會進入 poisoned 狀態。目前的 `map_err` 處理會回傳錯誤而非嘗試恢復。

## 變更（What）

1. **合併 check-and-set 為原子操作**：將 `types.rs` 中的兩個 lock scope 合併為單一 scope，在同一個 guard 下完成「檢查是否已鎖 → 設定新 run_id」
2. **改善 poisoned lock 恢復**：使用 `lock().unwrap_or_else(|e| e.into_inner())` 模式，在 Mutex poisoned 時仍可恢復 guard 並清除 lock

## 影響區域

| 檔案 | 修改內容 |
|------|---------|
| `kofnote-app/src-tauri/src/types.rs` | `run_debate_mode` 函式 L1570-1587 |

無 API 變更，無前端影響，無資料庫 schema 變更。

## Rollback Plan

1. 此為純邏輯修改，可直接 `git revert` 回退
2. 不涉及 additive schema 或持久化格式變更
3. 回退後行為回到原始兩段式 lock（功能正常，但保留 TOCTOU 理論風險）

# Debate Lock 原子性修正規格

## 需求

`DebateLock` 的 check-and-set 必須在單一 Mutex guard 生命週期內完成，消除 TOCTOU 競態條件。

## Acceptance Criteria

1. **AC-1**：check（是否已有 active run）和 set（設定新 run_id）在同一個 `MutexGuard` scope 內完成
2. **AC-2**：Mutex poisoned 時不 panic，而是透過 `into_inner()` 恢復 guard 並清除 lock 狀態
3. **AC-3**：always-clear 邏輯（debate 結束或錯誤後清除 lock）保持不變
4. **AC-4**：並行呼叫 `run_debate_mode` 時，第二個呼叫收到明確的「debate already in progress」錯誤

## Edge Cases

1. **Poisoned Mutex**：前一次 debate panic 導致 Mutex poisoned → 應恢復 guard、清除 lock、允許新 debate 啟動
2. **同時兩個請求**：兩個前端 tab 同時觸發 debate → 第一個成功、第二個收到錯誤訊息（含 active run_id）
3. **Lock 清除失敗**：debate 結束時 `lock()` 也失敗 → 使用 `unwrap_or_else` 確保清除仍執行
4. **Guard 持有時間**：合併後 guard 持有時間稍長（包含 run_id 生成）→ `generate_debate_run_id()` 為純 CPU 運算，不構成效能問題

## Contract

無 API 變更。內部行為改變：
- Before: 兩次 lock acquisition（check scope + set scope）
- After: 一次 lock acquisition（atomic check-and-set scope）

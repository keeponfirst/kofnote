# fix-debate-lock-panic 設計文件

## 設計決策

### D1: 合併 check-and-set 為單一 scope

**修改檔案**：`kofnote-app/src-tauri/src/types.rs`（L1570-1587）

**Before（兩段式）**：
```rust
// Check lock
{
    let guard = lock.0.lock().map_err(|e| format!("Lock error: {}", e))?;
    if let Some(run_id) = guard.as_ref() {
        return Err(format!("Debate already in progress: {}", run_id));
    }
}
// Set lock
{
    let mut guard = lock.0.lock().map_err(|e| format!("Lock error: {}", e))?;
    *guard = Some(generate_debate_run_id());
}
```

**After（原子式）**：
```rust
let current_run_id = {
    let mut guard = lock.0.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(run_id) = guard.as_ref() {
        return Err(format!("Debate already in progress: {}", run_id));
    }
    let run_id = generate_debate_run_id();
    *guard = Some(run_id.clone());
    run_id
};
```

**備選方案**：
- 使用 `try_lock` + retry loop → 拒絕：過度複雜，非必要
- 使用 `RwLock` → 拒絕：單一寫入者場景，Mutex 足夠
- 使用 `AtomicBool` + `Mutex` 雙重保護 → 拒絕：增加複雜度無額外收益

### D2: Poisoned lock 恢復策略

**修改檔案**：同上

所有 `lock()` 呼叫改用 `unwrap_or_else(|e| e.into_inner())` 模式，包含 always-clear 區段。

## 驗證指令

```bash
# 1. 編譯確認
cd kofnote-app && cargo check --manifest-path src-tauri/Cargo.toml

# 2. 前端 build 確認無破壞
npm run build

# 3. 手動測試：啟動 app 後連續快速點擊 debate 按鈕兩次，確認第二次收到錯誤而非 panic
```

## Migration Plan

1. 修改 `types.rs` 中 `run_debate_mode` 的 lock 邏輯（1 個函式，~15 行變更）
2. 編譯驗證
3. 完成

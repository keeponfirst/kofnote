# fix-debate-lock-panic 任務清單

## Task 1: 合併 DebateLock check-and-set 為原子操作

**追溯**：specs/debate-lock-atomicity.md → AC-1, AC-2

**工時預估**：30 分鐘

- [ ] 修改 `kofnote-app/src-tauri/src/types.rs` 中 `run_debate_mode` 函式
  - 合併兩個 lock scope 為單一 `{ let mut guard = ... }` scope
  - 在 guard 持有期間完成 check → generate_run_id → set
  - 所有 `lock()` 改用 `unwrap_or_else(|e| e.into_inner())`
- [ ] 確認 always-clear 區段也使用相同的 poisoned recovery 模式
- [ ] `cargo check` 通過
- [ ] `npm run build` 通過

**驗證指令**：
```bash
cd kofnote-app
cargo check --manifest-path src-tauri/Cargo.toml
npm run build
```

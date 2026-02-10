## Why

目前決策流程仰賴手動輪流詢問多個 AI，再由使用者自行整合，缺少一致流程、可重播機制與可追溯證據鏈。  
KOF Note 需要一個內建的 Debate Mode，將多 AI 協作標準化為可執行、可審計、可本地累積的認知流程。

## What Changes

- 在 KOF Note 定義內建 `Debate Mode v0.1` 的行為規格（非外掛、非獨立 SaaS）。
- 定義固定辯論 state machine（Intake -> Round1 -> Round2 -> Round3 -> Consensus -> Judge -> Packetize -> Writeback）。
- 定義固定角色、固定回合、固定限制條件，排除動態角色與自我進化 prompt。
- 定義 `Final Packet` 固定 JSON 資料契約，確保輸出可執行與可驗證。
- 定義 local-first 落地：檔案結構、SQLite 索引、Local Brain writeback 與 replay 能力。
- 定義 v0.1 驗收標準（Definition of Done）與 v0.2/v0.3 演進方向。

## Capabilities

### New Capabilities
- `debate-mode`: KOF Note 內建多模型多角色辯論流程，產出可執行 Final Packet，並完整寫回本地記憶層以支援回放與審計。

### Modified Capabilities
- (none)

## Impact

- Affected domain/spec: `openspec/changes/add-debate-mode-v01/specs/debate-mode/spec.md`
- Affected runtime areas (for later implementation planning only):
  - Tauri command contract and orchestration flow
  - Data contracts in frontend/backend shared types
  - Local storage/indexing strategy for debate run artifacts
- No implementation in this change phase (specification only).

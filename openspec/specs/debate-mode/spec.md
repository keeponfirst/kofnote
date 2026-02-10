## 1. 目標與非目標（明確邊界）

### 目標

- 本能力 MUST 作為 `KOF Note` 內建 `Debate Mode v0.1`，不是外掛工具。
- 本能力 MUST 將單次輸入轉為多角色、多模型的結構化辯論流程，並產出可直接執行的結果包（Final Packet）。
- 本能力 MUST 支援 local-first 寫回與完整 replay，讓每次 run 可回溯、可觀測、可理解。

### 非目標

- 不設計獨立 SaaS、獨立後端服務或外掛式子系統。
- 不設計動態角色生成、自我進化 prompt、多人協作。
- 不討論 UI 視覺細節與互動版面，只定義行為與資料契約。

## 2. v0.1 Scope（必做 / 不做）

### 必做（MUST）

- 固定 state machine 辯論流程與停條件。
- 固定角色數與固定回合數。
- 固定 Final Packet schema（JSON）。
- 全程 local-first 落地（檔案 + SQLite + Local Brain writeback）。
- 每次 run 具備 replay 與審計資訊（問題、各角色輸出、收斂理由、風險、下一步）。

### 不做（MUST NOT）

- 動態擴縮角色與回合策略。
- Prompt 自主改寫與自我進化迴圈。
- 多使用者協作與權限模型。
- UI 動畫、圖形視覺語言等介面層實作細節。

## 3. Debate Protocol（固定 state machine）

狀態必須固定為：

`Intake -> Round1 -> Round2 -> Round3 -> Consensus -> Judge -> Packetize -> Writeback`

### 狀態定義

- `Intake`: 正規化問題、約束、輸出型別、run metadata。
- `Round1`: 各角色提出初始立場與主張。
- `Round2`: 各角色交叉質疑他角至少一項主張。
- `Round3`: 各角色基於質疑修正主張，提交最終立場。
- `Consensus`: 計算共識強度、主要分歧與信心分數。
- `Judge`: 裁決採納方案、拒絕理由與風險摘要。
- `Packetize`: 產生固定 schema 的 Final Packet。
- `Writeback`: 將完整 run artifacts 與 Final Packet 寫回本地記憶層。

### 停條件

- MUST 在完成 `Round3` 後進入 `Consensus`，不得無限回圈。
- 若任一角色輸出失敗，MUST 記錄失敗並以降級策略完成流程（不可黑箱中止）。

## 4. 固定角色數、回合數、限制條件

### 角色（固定 5 角）

- `Proponent`
- `Critic`
- `Analyst`
- `Synthesizer`
- `Judge`

### 回合（固定 3 輪）

- `Round1`: Opening Statements
- `Round2`: Cross-Examination
- `Round3`: Revised Positions

### 限制條件

- 每角色每輪 MUST 以結構化欄位輸出（claim, rationale, risks, challenge/revision）。
- `Round2` MUST 包含跨角色質疑關係（來源角色 -> 目標角色 -> 挑戰內容）。
- `Judge` MUST 產出採納與拒絕依據，不得只給單句結論。
- 本版本 MUST 限制 token/時間預算並記錄實際消耗（供審計與優化）。

## 5. Final Packet 固定 schema（JSON）

```json
{
  "run_id": "debate_YYYYMMDD_HHMMSS_xxx",
  "mode": "debate-v0.1",
  "problem": "string",
  "constraints": ["string"],
  "output_type": "decision|writing|architecture|planning|evaluation",
  "participants": [
    {
      "role": "Proponent|Critic|Analyst|Synthesizer|Judge",
      "model_provider": "openai|gemini|claude|local",
      "model_name": "string"
    }
  ],
  "consensus": {
    "consensus_score": 0.0,
    "confidence_score": 0.0,
    "key_agreements": ["string"],
    "key_disagreements": ["string"]
  },
  "decision": {
    "selected_option": "string",
    "why_selected": ["string"],
    "rejected_options": [
      {
        "option": "string",
        "reason": "string"
      }
    ]
  },
  "risks": [
    {
      "risk": "string",
      "severity": "high|medium|low",
      "mitigation": "string"
    }
  ],
  "next_actions": [
    {
      "id": "A1",
      "action": "string",
      "owner": "string",
      "due": "YYYY-MM-DD"
    }
  ],
  "trace": {
    "round_refs": ["round-1", "round-2", "round-3"],
    "evidence_refs": ["string"]
  },
  "timestamps": {
    "started_at": "ISO-8601",
    "finished_at": "ISO-8601"
  }
}
```

## 6. Local-first 落地設計（檔案結構 + SQLite 索引 + Local Brain writeback）

### 檔案落地（MUST）

每次 run MUST 寫入：

- `records/debates/<run_id>/request.json`
- `records/debates/<run_id>/rounds/round-1.json`
- `records/debates/<run_id>/rounds/round-2.json`
- `records/debates/<run_id>/rounds/round-3.json`
- `records/debates/<run_id>/consensus.json`
- `records/debates/<run_id>/final-packet.json`
- `records/debates/<run_id>/final-packet.md`

### SQLite 索引（MUST）

至少包含可查詢索引（資料表命名可調整，但語義不可缺）：

- `debate_runs`（run metadata, scores, selected option, timestamps）
- `debate_turns`（role, round, claim/challenge/revision references）
- `debate_actions`（next actions, status, due）

### Local Brain writeback（MUST）

- 完成 `Writeback` 後 MUST 生成至少一筆 Local Brain 記錄（`decision` 或 `worklog`），引用 `run_id` 與 `final-packet` 路徑。
- 雲端模型輸出僅為參與訊號，MUST NOT 成為唯一記憶來源。

## 7. 驗收標準（Definition of Done）

- 一次輸入可完整跑完 8 個狀態，不跳步、不無限迴圈。
- 每個 run 均可從本地檔案重建完整辯論脈絡（問題、角色輸出、收斂理由、風險、下一步）。
- Final Packet 必須符合固定 schema 且包含可執行 `next_actions`。
- 在至少兩種不同 provider 組合下可完成流程並產生一致格式輸出。
- 任一角色失敗時，流程可降級完成且有可讀錯誤紀錄。
- 最終結果可被寫回 Local Brain，並可由 `run_id` 反查來源 artifacts。

## 8. v0.2 / v0.3 演進路線（僅方向，不做設計）

### v0.2 方向

- 提升共識機制（更細緻的衝突分類與信心校準）。
- 增加跨 run 對照（相同問題不同策略的比較視圖）。
- 增加 provider policy（成本/延遲/穩定性）設定。

### v0.3 方向

- 增加長期記憶回饋迴路（用歷史決策結果校正後續辯論權重）。
- 增加更嚴格的可驗證決策協議（evidence quality gates）。
- 增加可配置輸出包型別（Decision/Architecture/Writing 的擴充模板）。

## ADDED Requirements

### Requirement: Built-in Debate Mode v0.1 state machine
The system SHALL execute a fixed debate workflow as an internal KOF Note mode: `Intake -> Round1 -> Round2 -> Round3 -> Consensus -> Judge -> Packetize -> Writeback`.

#### Scenario: Full run succeeds
- **WHEN** a user submits one debate request with valid constraints
- **THEN** the engine completes all fixed states in order and produces one Final Packet

#### Scenario: Participant failure is degraded, not hidden
- **WHEN** any participant role fails during a round
- **THEN** the run is completed with degraded output and failure traces are persisted locally

### Requirement: Local-first persistence and replay
The system SHALL persist all debate artifacts locally and SHALL allow replay of one run from local data only.

#### Scenario: Replay from local storage
- **WHEN** a run has finished and local files/index are available
- **THEN** the system can reconstruct the problem, each round outputs, consensus rationale, and final actions without cloud dependency

### Requirement: Fixed Final Packet contract
The system SHALL emit a Final Packet conforming to the fixed JSON schema defined in this spec.

#### Scenario: Packet validation
- **WHEN** packetization is completed
- **THEN** required fields (`run_id`, `problem`, `consensus`, `decision`, `risks`, `next_actions`, `trace`, `timestamps`) exist and are machine-parseable

### Requirement: Local Brain writeback as source of truth linkage
The system SHALL write at least one Local Brain record linked to the debate `run_id` and final packet path.

#### Scenario: Writeback linkability
- **WHEN** writeback is executed
- **THEN** the resulting Local Brain record can be traced back to the exact run artifacts via `run_id`
